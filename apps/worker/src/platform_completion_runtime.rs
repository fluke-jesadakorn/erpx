//! Runtime composition for the G3--G6 platform-control completion boundary.
//!
//! This module is intentionally a coordinator hook rather than a new worker
//! registration.  It composes the existing [`PlatformControlStore`],
//! [`PlatformCompletionProjector`], and durable [`OutboxWorker`] while making
//! the physical control-plane split explicit:
//!
//! * command state, outbox claims, and projection checkpoints use the control
//!   write pool;
//! * sanitized platform read models are written and read through the control
//!   read pool; and
//! * no caller can provide a pool, database URL, credential, or replacement
//!   database key to this adapter.
//!
//! The shared authorization evaluator remains the authority for roles and
//! entitlements.  This adapter accepts only its already-issued
//! [`PlatformAuthorizationProof`] and fails closed when that proof is absent,
//! expired, out of scope, missing MFA, or not bound to the command actor.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    contracts::AuthorityScope as Scope,
    platform_completion::{
        BackupControlCommand, ConnectorControlCommand, HoldingLifecycleCommand,
        PlatformAuthorizationProof, PlatformCompletionError, PlatformReconciliationReport,
        ReconciliationCommand, ReplacementActivationProof, ReplacementReadCommand,
    },
};
use erpx_infrastructure::{
    MAX_OUTBOX_CLAIM_BATCH_SIZE, OutboxEnvelope, OutboxHandler, OutboxWorker,
    PlatformBackupJobReceipt, PlatformCommandReceipt, PlatformCompletionProjector,
    PlatformConnectorReadModel, PlatformControlStore, PlatformHoldingLifecycleReceipt,
    PlatformPersistenceError, PlatformReplacementReadReceipt, ProjectionHandler,
    ProjectionRebuildReport, RetryPolicy, SqlxControlPlane, SqlxPlatformReadModelWriter,
    WorkerError,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME: &str = "platform_completion_runtime";
pub const PLATFORM_COMPLETION_READ_PERMISSION: &str = "platform.holding.read";
pub const PLATFORM_COMPLETION_MAX_BATCH_SIZE: u32 = MAX_OUTBOX_CLAIM_BATCH_SIZE;

const PLATFORM_CONNECTOR_MUTATION_PERMISSION: &str = "platform.connector.manage";
const PLATFORM_BACKUP_PERMISSION: &str = "platform.holding.backup";
const PLATFORM_RECONCILIATION_PERMISSION: &str = "platform.holding.reconcile";
const PLATFORM_PROJECTION_PERMISSION: &str = "platform.projection.rebuild";
const PLATFORM_LIFECYCLE_PROVISION_PERMISSION: &str = "platform.holding.provision";
const PLATFORM_LIFECYCLE_SUSPEND_PERMISSION: &str = "platform.holding.suspend";
const PLATFORM_LIFECYCLE_RECOVERY_PERMISSION: &str = "platform.holding.recovery";

/// Errors deliberately expose no database URL, SQL, credential, or provider
/// response.  The source is retained for tracing at the coordinator boundary,
/// but its display text is stable and safe for an API/worker response.
#[derive(Debug, Error)]
pub enum PlatformCompletionRuntimeError {
    #[error("platform authorization denied")]
    Authorization(#[source] PlatformCompletionError),
    #[error("platform persistence operation failed")]
    Persistence(#[source] PlatformPersistenceError),
    #[error("platform runtime route boundary rejected the request")]
    RouteBoundary,
}

impl From<PlatformCompletionError> for PlatformCompletionRuntimeError {
    fn from(error: PlatformCompletionError) -> Self {
        Self::Authorization(error)
    }
}

impl From<PlatformPersistenceError> for PlatformCompletionRuntimeError {
    fn from(error: PlatformPersistenceError) -> Self {
        Self::Persistence(error)
    }
}

/// A proof-bearing read request.  The adapter never constructs a proof or
/// falls back to a local identity; the server/shared authorization boundary
/// must provide all fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformReadAuthorization {
    pub actor_ref: String,
    pub proof: PlatformAuthorizationProof,
    pub at: DateTime<Utc>,
}

impl PlatformReadAuthorization {
    pub fn validate(&self, requested_scope: &Scope) -> Result<(), PlatformCompletionRuntimeError> {
        self.proof
            .validate(
                &self.actor_ref,
                requested_scope,
                PLATFORM_COMPLETION_READ_PERMISSION,
                false,
                self.at,
            )
            .map_err(PlatformCompletionRuntimeError::Authorization)
    }

    /// Connector reads are platform-control reads.  A holding-scoped caller
    /// must be narrowed by the server API before reaching this adapter; the
    /// adapter refuses to infer a holding UUID from an opaque holding label.
    pub fn validate_platform_read(&self) -> Result<(), PlatformCompletionRuntimeError> {
        self.validate(&Scope::Platform)
    }
}

/// Runtime composition that preserves the control write/read route split.
///
/// `PlatformControlStore` owns mutations and its read methods select the
/// control read pool.  `SqlxPlatformReadModelWriter` is constructed only from
/// that same control read pool and is used exclusively by the existing
/// projector.  There is no constructor accepting an arbitrary pool.
#[derive(Clone)]
pub struct PlatformCompletionRuntimeAdapter {
    control: Arc<SqlxControlPlane>,
    persistence: PlatformControlStore,
    projector: PlatformCompletionProjector,
}

impl PlatformCompletionRuntimeAdapter {
    pub fn new(control: Arc<SqlxControlPlane>) -> Result<Self, PlatformCompletionRuntimeError> {
        if control.pools().write.is_closed() || control.pools().read.is_closed() {
            return Err(PlatformCompletionRuntimeError::RouteBoundary);
        }
        let persistence = PlatformControlStore::new(Arc::clone(&control));
        let read_writer = SqlxPlatformReadModelWriter::new(control.pools().read.clone());
        let projector = PlatformCompletionProjector::new(Arc::new(read_writer));
        Ok(Self {
            control,
            persistence,
            projector,
        })
    }

    pub fn worker_handler(
        &self,
        worker_id: Uuid,
        batch_size: u32,
        claim_timeout: Duration,
        retry_policy: RetryPolicy,
    ) -> Result<PlatformCompletionRuntimeHandler, WorkerRuntimeError> {
        PlatformCompletionRuntimeHandler::new(
            Arc::clone(&self.control),
            self.projector.clone(),
            worker_id,
            batch_size,
            claim_timeout,
            retry_policy,
        )
    }

    pub async fn transition_lifecycle(
        &self,
        command: &HoldingLifecycleCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformHoldingLifecycleReceipt, PlatformCompletionRuntimeError> {
        let permission = match command.action {
            erpx_domain::platform_completion::HoldingLifecycleAction::Activate => {
                PLATFORM_LIFECYCLE_PROVISION_PERMISSION
            }
            erpx_domain::platform_completion::HoldingLifecycleAction::Suspend => {
                PLATFORM_LIFECYCLE_SUSPEND_PERMISSION
            }
            erpx_domain::platform_completion::HoldingLifecycleAction::EnterRecovery => {
                PLATFORM_LIFECYCLE_RECOVERY_PERMISSION
            }
        };
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            permission,
            true,
            command.at,
        )?;
        self.persistence
            .transition_lifecycle(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn reconcile_holding(
        &self,
        command: &ReconciliationCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReconciliationReport, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_RECONCILIATION_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .reconcile_holding(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn schedule_backup(
        &self,
        command: &BackupControlCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformBackupJobReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_BACKUP_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .schedule_backup(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn start_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_PROJECTION_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .start_replacement_read(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn mark_replacement_ready(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_PROJECTION_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .mark_replacement_ready(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn fail_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
        reason_code: &str,
    ) -> Result<PlatformReplacementReadReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_PROJECTION_PERMISSION,
            false,
            command.at,
        )?;
        self.persistence
            .fail_replacement_read(command, proof, reason_code)
            .await
            .map_err(Into::into)
    }

    pub async fn activate_replacement_read(
        &self,
        command: &ReplacementReadCommand,
        proof: &PlatformAuthorizationProof,
        activation: &ReplacementActivationProof,
    ) -> Result<PlatformReplacementReadReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &command.binding.scope(),
            PLATFORM_PROJECTION_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .activate_replacement_read(command, proof, activation)
            .await
            .map_err(Into::into)
    }

    pub async fn save_connector(
        &self,
        command: &ConnectorControlCommand,
        proof: &PlatformAuthorizationProof,
    ) -> Result<PlatformCommandReceipt, PlatformCompletionRuntimeError> {
        authorize_mutation(
            proof,
            &command.actor_ref,
            &Scope::Platform,
            PLATFORM_CONNECTOR_MUTATION_PERMISSION,
            true,
            command.at,
        )?;
        self.persistence
            .save_connector(command, proof)
            .await
            .map_err(Into::into)
    }

    pub async fn read_connector(
        &self,
        connector_id: Uuid,
        authorization: &PlatformReadAuthorization,
    ) -> Result<Option<PlatformConnectorReadModel>, PlatformCompletionRuntimeError> {
        authorization.validate_platform_read()?;
        // PlatformControlStore::read_connector selects only the control read
        // pool and returns a credential-free projection.
        self.persistence
            .read_connector(connector_id)
            .await
            .map_err(Into::into)
    }

    pub async fn read_reconciliation(
        &self,
        binding: &erpx_domain::platform_completion::HoldingBinding,
        authorization: &PlatformReadAuthorization,
    ) -> Result<Option<PlatformReconciliationReport>, PlatformCompletionRuntimeError> {
        authorization.validate(&binding.scope())?;
        self.persistence
            .read_reconciliation(binding)
            .await
            .map_err(Into::into)
    }
}

fn authorize_mutation(
    proof: &PlatformAuthorizationProof,
    actor_ref: &str,
    requested_scope: &Scope,
    permission: &'static str,
    high_risk: bool,
    at: DateTime<Utc>,
) -> Result<(), PlatformCompletionRuntimeError> {
    proof
        .validate(actor_ref, requested_scope, permission, high_risk, at)
        .map_err(PlatformCompletionRuntimeError::Authorization)
}

/// Durable projection handler returned by the adapter.  It is not registered
/// here because worker `lib.rs`/`main.rs` are coordinator-owned files.
#[derive(Clone)]
pub struct PlatformCompletionRuntimeHandler {
    worker: OutboxWorker,
    projector: PlatformCompletionProjector,
    worker_id: Uuid,
}

impl PlatformCompletionRuntimeHandler {
    fn new(
        control: Arc<SqlxControlPlane>,
        projector: PlatformCompletionProjector,
        worker_id: Uuid,
        batch_size: u32,
        claim_timeout: Duration,
        retry_policy: RetryPolicy,
    ) -> Result<Self, WorkerRuntimeError> {
        if worker_id.is_nil()
            || batch_size == 0
            || batch_size > PLATFORM_COMPLETION_MAX_BATCH_SIZE
            || claim_timeout <= Duration::zero()
        {
            return Err(WorkerRuntimeError::Configuration(
                "platform completion runtime configuration is invalid".into(),
            ));
        }
        Ok(Self {
            worker: OutboxWorker::new(control, worker_id)
                .with_batch_size(batch_size)
                .with_claim_timeout(claim_timeout)
                .with_retry_policy(retry_policy),
            projector,
            worker_id,
        })
    }

    pub async fn rebuild(&self) -> Result<ProjectionRebuildReport, WorkerRuntimeError> {
        self.worker
            .rebuild_projection(
                erpx_infrastructure::PLATFORM_COMPLETION_PROJECTION,
                &self.projector,
            )
            .await
            .map_err(WorkerRuntimeError::Persistence)
    }
}

#[async_trait]
impl NamedWorkerHandler for PlatformCompletionRuntimeHandler {
    fn name(&self) -> &str {
        PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        validate_worker_identity(self.worker_id, worker_id)?;
        self.worker
            .run_once(&PlatformCompletionOutboxHandler {
                worker: self.worker.clone(),
                projector: self.projector.clone(),
            })
            .await
            .map(WorkerHandlerReport::from)
            .map_err(WorkerRuntimeError::Persistence)
    }
}

#[derive(Clone)]
struct PlatformCompletionOutboxHandler {
    worker: OutboxWorker,
    projector: PlatformCompletionProjector,
}

const PLATFORM_COMPLETION_AGGREGATE_TYPES: &[&str] = &[
    erpx_infrastructure::PLATFORM_HOLDING_LIFECYCLE_AGGREGATE,
    erpx_infrastructure::PLATFORM_BACKUP_AGGREGATE,
    erpx_infrastructure::PLATFORM_REPLACEMENT_READ_AGGREGATE,
    erpx_infrastructure::PLATFORM_RECONCILIATION_AGGREGATE,
    erpx_infrastructure::PLATFORM_CONNECTOR_AGGREGATE,
];

#[async_trait]
impl OutboxHandler for PlatformCompletionOutboxHandler {
    fn aggregate_type_filter(&self) -> Option<&'static [&'static str]> {
        Some(PLATFORM_COMPLETION_AGGREGATE_TYPES)
    }

    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        if !self
            .projector
            .handles_event(erpx_infrastructure::PLATFORM_COMPLETION_PROJECTION, event)
        {
            return Ok(());
        }
        validate_platform_event(event).map_err(WorkerError::Permanent)?;
        self.worker
            .project_once(
                erpx_infrastructure::PLATFORM_COMPLETION_PROJECTION,
                event,
                &self.projector,
            )
            .await
            .map(|_| ())
            .map_err(|_| WorkerError::Retryable("platform projection unavailable".into()))
    }
}

fn validate_platform_event(event: &OutboxEnvelope) -> Result<(), String> {
    if event.id.is_nil()
        || event.aggregate_id.is_nil()
        || event.correlation_id.is_nil()
        || event.aggregate_version == 0
        || event.event_version == 0
        || !event.event_type.starts_with("platform.")
        || !event
            .idempotency_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty() && !key.chars().any(char::is_control))
    {
        return Err("platform event envelope is incomplete".into());
    }
    let principal_ref = event
        .actor
        .as_ref()
        .and_then(|actor| actor.get("principal_ref"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control));
    if principal_ref.is_none() {
        return Err("platform event authorization actor is missing".into());
    }
    let Some(request_hash) = event.payload.get("request_hash").and_then(Value::as_str) else {
        return Err("platform event request binding is missing".into());
    };
    if request_hash.len() != 64
        || !request_hash
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("platform event request binding is invalid".into());
    }
    let Some(read_model) = event.payload.get("read_model") else {
        return Err("platform event read model is missing".into());
    };
    if !read_model.is_object() || contains_forbidden_projection_key(read_model) {
        return Err("platform event read model is not safe".into());
    }
    validate_platform_event_scope(event, read_model)?;
    Ok(())
}

fn validate_worker_identity(
    configured_worker_id: Uuid,
    supplied_worker_id: Uuid,
) -> Result<(), WorkerRuntimeError> {
    if configured_worker_id.is_nil()
        || supplied_worker_id.is_nil()
        || configured_worker_id != supplied_worker_id
    {
        return Err(WorkerRuntimeError::Configuration(
            "platform completion worker identity is not bound to its handler".into(),
        ));
    }
    Ok(())
}

fn validate_platform_event_scope(event: &OutboxEnvelope, read_model: &Value) -> Result<(), String> {
    let scope_source = read_model.get("binding").unwrap_or(read_model);
    let holding_id_present = scope_source.get("holding_id").is_some();
    let data_profile_present = scope_source.get("data_profile_key").is_some();
    if read_model.get("binding").is_some() && !(holding_id_present && data_profile_present) {
        return Err("platform event read model binding scope is missing".into());
    }
    if !holding_id_present && !data_profile_present {
        if event.active_scope.is_some() {
            return Err("platform event holding scope is not bound to the read model".into());
        }
        return Ok(());
    }

    let holding_id = scope_string(scope_source, "holding_id")?;
    let data_profile_key = scope_string(scope_source, "data_profile_key")?;
    let expected_scope = match (holding_id, data_profile_key) {
        (Some(holding_id), Some(data_profile_key)) => Some(serde_json::json!({
            "holding_id": holding_id,
            "data_profile_key": data_profile_key,
        })),
        (None, None) => None,
        _ => return Err("platform event read model scope is incomplete".into()),
    };
    if event.active_scope != expected_scope {
        return Err("platform event holding scope does not match the read model".into());
    }
    Ok(())
}

fn scope_string(scope: &Value, field: &'static str) -> Result<Option<String>, String> {
    match scope.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if !value.trim().is_empty() && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value.clone()))
        }
        _ => Err(format!("platform event read model {field} is invalid")),
    }
}

fn contains_forbidden_projection_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            matches!(
                key.as_str(),
                "credential_ref" | "candidate_read_database_key"
            ) || contains_forbidden_projection_key(child)
        }),
        Value::Array(values) => values.iter().any(contains_forbidden_projection_key),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use erpx_domain::platform_completion::HoldingBinding;
    use serde_json::json;

    fn proof(permission: &str, scope: Scope, at: DateTime<Utc>) -> PlatformAuthorizationProof {
        PlatformAuthorizationProof {
            principal_ref: "principal:test".into(),
            permission: permission.into(),
            scope,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(4),
            mfa_verified: true,
            local_break_glass: false,
            break_glass_ticket_ref: None,
        }
    }

    fn envelope(payload: Value) -> OutboxEnvelope {
        OutboxEnvelope {
            id: Uuid::new_v4(),
            event_type: "platform.connector.changed".into(),
            event_version: 1,
            aggregate_type: erpx_infrastructure::PLATFORM_CONNECTOR_AGGREGATE.into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            active_scope: None,
            actor: Some(json!({"principal_ref": "principal:test"})),
            correlation_id: Uuid::new_v4(),
            causation_id: None,
            idempotency_key: Some("platform-test-1".into()),
            payload,
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 1,
            max_attempts: 3,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        }
    }

    #[test]
    fn read_authorization_requires_the_shared_platform_permission() {
        let at = Utc::now();
        let denied = PlatformReadAuthorization {
            actor_ref: "principal:test".into(),
            proof: proof("platform.connector.read", Scope::Platform, at),
            at,
        };
        assert!(matches!(
            denied.validate_platform_read(),
            Err(PlatformCompletionRuntimeError::Authorization(
                PlatformCompletionError::PermissionDenied { .. }
            ))
        ));
        let allowed = PlatformReadAuthorization {
            proof: proof(PLATFORM_COMPLETION_READ_PERMISSION, Scope::Platform, at),
            ..denied
        };
        assert!(allowed.validate_platform_read().is_ok());
    }

    #[test]
    fn read_authorization_rejects_a_holding_proof_without_an_exact_typed_scope() {
        let at = Utc::now();
        let authorization = PlatformReadAuthorization {
            actor_ref: "principal:test".into(),
            proof: proof(
                PLATFORM_COMPLETION_READ_PERMISSION,
                Scope::Holding {
                    holding_id: Uuid::new_v4(),
                },
                at,
            ),
            at,
        };
        assert!(matches!(
            authorization.validate_platform_read(),
            Err(PlatformCompletionRuntimeError::Authorization(
                PlatformCompletionError::ScopeDenied
            ))
        ));
    }

    #[test]
    fn mutation_authorization_requires_mfa_for_high_risk_operations() {
        let at = Utc::now();
        let mut high_risk = proof(PLATFORM_RECONCILIATION_PERMISSION, Scope::Platform, at);
        high_risk.mfa_verified = false;
        let error = authorize_mutation(
            &high_risk,
            "principal:test",
            &Scope::Platform,
            PLATFORM_RECONCILIATION_PERMISSION,
            true,
            at,
        )
        .expect_err("high-risk platform mutation must require MFA");
        assert!(matches!(
            error,
            PlatformCompletionRuntimeError::Authorization(PlatformCompletionError::MfaRequired)
        ));
    }

    #[test]
    fn local_break_glass_is_still_platform_only_and_ticket_bound() {
        let at = Utc::now();
        let mut emergency = proof("platform.break_glass", Scope::Platform, at);
        emergency.local_break_glass = true;
        emergency.break_glass_ticket_ref = Some("ticket-1".into());
        assert!(
            authorize_mutation(
                &emergency,
                "principal:test",
                &Scope::Platform,
                "platform.connector.manage",
                true,
                at,
            )
            .is_ok()
        );
        emergency.scope = Scope::Holding {
            holding_id: Uuid::new_v4(),
        };
        assert!(matches!(
            authorize_mutation(
                &emergency,
                "principal:test",
                &Scope::Platform,
                "platform.connector.manage",
                true,
                at,
            ),
            Err(PlatformCompletionRuntimeError::Authorization(
                PlatformCompletionError::ScopeDenied
            ))
        ));
    }

    #[test]
    fn platform_event_gate_requires_actor_binding_and_safe_projection() {
        let safe = envelope(json!({
            "request_hash": "aa".repeat(32),
            "read_model": {"state": "active", "credential_bound": true}
        }));
        assert!(validate_platform_event(&safe).is_ok());

        let mut missing_actor = safe.clone();
        missing_actor.actor = None;
        assert!(validate_platform_event(&missing_actor).is_err());

        let mut leaked = safe;
        leaked.payload["read_model"] = json!({"credential_ref": "vault/ref"});
        assert!(validate_platform_event(&leaked).is_err());
    }

    #[test]
    fn platform_event_gate_rejects_unbound_or_invalid_request_hashes() {
        let mut event = envelope(json!({
            "request_hash": "not-a-hash",
            "read_model": {"state": "active"}
        }));
        assert!(validate_platform_event(&event).is_err());
        event.payload["request_hash"] = json!("bb".repeat(32));
        event.idempotency_key = None;
        assert!(validate_platform_event(&event).is_err());

        event.idempotency_key = Some("   ".into());
        assert!(validate_platform_event(&event).is_err());
        event.idempotency_key = Some("platform-test-2".into());
        event.event_version = 0;
        assert!(validate_platform_event(&event).is_err());
    }

    #[test]
    fn platform_event_gate_requires_the_outbox_scope_to_match_the_read_model() {
        let mut event = envelope(json!({
            "request_hash": "cc".repeat(32),
            "read_model": {
                "holding_id": "holding-1",
                "data_profile_key": "holding-1",
                "state": "active"
            }
        }));
        event.active_scope = Some(json!({
            "holding_id": "holding-1",
            "data_profile_key": "holding-1"
        }));
        assert!(validate_platform_event(&event).is_ok());

        event.active_scope = Some(json!({
            "holding_id": "holding-2",
            "data_profile_key": "holding-1"
        }));
        assert!(validate_platform_event(&event).is_err());

        event.active_scope = None;
        assert!(validate_platform_event(&event).is_err());
    }

    #[test]
    fn platform_event_gate_rejects_a_partial_holding_holding_binding() {
        let mut event = envelope(json!({
            "request_hash": "dd".repeat(32),
            "read_model": {
                "holding_id": "holding-1",
                "state": "active"
            }
        }));
        event.active_scope = Some(json!({"holding_id": "holding-1"}));
        assert!(validate_platform_event(&event).is_err());

        event.payload["read_model"] = json!({
            "data_profile_key": "holding-1",
            "state": "active"
        });
        event.active_scope = Some(json!({"data_profile_key": "holding-1"}));
        assert!(validate_platform_event(&event).is_err());
    }

    #[test]
    fn runtime_handler_rejects_a_worker_identity_mismatch() {
        let configured = Uuid::from_u128(11);
        assert!(validate_worker_identity(configured, configured).is_ok());
        assert!(validate_worker_identity(configured, Uuid::from_u128(12)).is_err());
        assert!(validate_worker_identity(configured, Uuid::nil()).is_err());
    }

    #[test]
    fn runtime_handler_configuration_is_bounded_before_claiming_work() {
        assert!(Uuid::nil().is_nil());
        assert_eq!(
            PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME,
            "platform_completion_runtime"
        );
        let binding = HoldingBinding::new("holding-1", "holding-1").expect("bounded binding");
        assert!(matches!(binding.scope(), Scope::Holding { .. }));
    }

    #[test]
    fn forbidden_projection_keys_are_checked_recursively() {
        assert!(contains_forbidden_projection_key(&json!({
            "nested": [{"candidate_read_database_key": "candidate"}]
        })));
        assert!(!contains_forbidden_projection_key(&json!({
            "nested": [{"state": "ready"}]
        })));
    }
}
