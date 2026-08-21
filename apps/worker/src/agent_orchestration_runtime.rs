use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::Duration;
use erpx_agent_orchestration::{
    AgentExperienceUnitOfWork, AgentNotificationPort, AssignmentFailure, AssignmentId,
    AssignmentState, BudgetRequest, ClaimedAssignment, InvocationEvidenceCommit, PortError,
    ProviderInvocationEvidence, ProviderInvocationId, ProviderInvocationRequest,
    ProviderInvocationResponse, ProviderPolicyPort, UsageReconciliation, WorkerAssignmentPort,
};
use erpx_kernel::{ActorAttribution, ActorKind, AuditContext, AuthorityScope, Clock};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    NamedWorkerHandler, OutboxEnvelope, OutboxHandler, WorkerError, WorkerHandlerReport,
    WorkerRuntimeError,
    agent_provider::{GovernedAgentProvider, HoldingBoundAgentProviderPort},
};

pub const AGENT_ASSIGNMENT_HANDLER_NAME: &str = "agent_assignment";
const MAX_PROVIDER_OUTPUT_BYTES: usize = 1024 * 1024;

/// Encrypts/decrypts private prompt and output material outside the Agent
/// Orchestration core. Implementations must return opaque references and must
/// never write raw material to ordinary logs, projections, or audit payloads.
#[async_trait]
pub trait ProtectedAgentMaterialPort: Send + Sync {
    /// Resolve the exact, authorization-bound payload referenced by the
    /// durable claim. Implementations must fail closed when the reference,
    /// assignment, holding, or payload digest does not match.
    async fn resolve_prompt(&self, claim: &ClaimedAssignment) -> Result<String, PortError>;

    async fn protect_output(
        &self,
        claim: &ClaimedAssignment,
        material: &str,
    ) -> Result<String, PortError>;
}

/// Rechecks lease, cancellation, pinned version, and reserved layered budget
/// immediately before any protected material or provider egress. The SQLx
/// implementation remains the authority; the worker cannot infer permission
/// from the in-memory claim alone.
#[async_trait]
pub trait AgentExecutionGuardPort: Send + Sync {
    async fn authorize_execution(
        &self,
        claim: &ClaimedAssignment,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), PortError>;
}

#[async_trait]
impl AgentExecutionGuardPort for erpx_agent_orchestration_sqlx::SqlxWorkerQueue {
    async fn authorize_execution(
        &self,
        claim: &ClaimedAssignment,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), PortError> {
        self.verify_execution(claim, at).await
    }
}

/// Provider boundary that receives the durable reservation stop condition.
/// Adapters must translate `stop_after_units` into their provider-native
/// output limit and must never expose a tool capable of directly mutating an
/// ERPX business context.
#[async_trait]
pub trait BudgetedAgentProviderPort: Send + Sync {
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError>;
}

/// Persists digest-only invocation evidence before any provider egress and
/// completes it after the bounded response. A missing or failing evidence UoW
/// therefore fails closed before network access.
#[derive(Debug, Clone)]
struct EvidencedAgentProvider<P, U> {
    provider: P,
    evidence: U,
    holding_id: uuid::Uuid,
}

impl<P, U> EvidencedAgentProvider<P, U> {
    const fn new(provider: P, evidence: U, holding_id: uuid::Uuid) -> Self {
        Self {
            provider,
            evidence,
            holding_id,
        }
    }
}

#[async_trait]
impl<P, U> BudgetedAgentProviderPort for EvidencedAgentProvider<P, U>
where
    P: BudgetedAgentProviderPort,
    U: AgentExperienceUnitOfWork,
{
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError> {
        let invocation_id =
            provider_invocation_id(self.holding_id, request.assignment_id, request.attempt)?;
        let mut evidence = ProviderInvocationEvidence::begin(
            invocation_id,
            request.assignment_id,
            request.assignment_version,
            request.attempt,
            &request.pin,
            &request.payload_manifest,
            &request.policy_decision,
            request.policy_decision.authorized_at,
        )
        .map_err(|_| PortError::ProviderDenied)?;
        let actor = ActorAttribution {
            principal_ref: request.pin.agent_principal_ref.clone(),
            kind: ActorKind::Agent,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let audit = AuditContext {
            purpose: "provider invocation provenance".into(),
            customer_visible: false,
            module: Some("agent-orchestration".into()),
        };
        self.evidence
            .commit_invocation_evidence(InvocationEvidenceCommit {
                actor: actor.clone(),
                authority_scope: request.authority_scope.clone(),
                audit: audit.clone(),
                expected_version: None,
                idempotency_key: format!(
                    "provider-invocation:{}:{}:started",
                    request.assignment_id.as_uuid(),
                    request.attempt
                ),
                evidence: evidence.clone(),
            })
            .await?;
        let result = self.provider.invoke_bounded(request.clone(), budget).await;
        match result {
            Ok(response) => {
                let completed_at = std::cmp::max(chrono::Utc::now(), evidence.invoked_at());
                if format!("{:x}", Sha256::digest(response.output_material.as_bytes()))
                    != response.output_digest.to_ascii_lowercase()
                {
                    evidence
                        .fail(completed_at)
                        .map_err(|_| PortError::ProviderPermanent)?;
                    self.evidence
                        .commit_invocation_evidence(InvocationEvidenceCommit {
                            actor,
                            authority_scope: request.authority_scope,
                            audit,
                            expected_version: Some(1),
                            idempotency_key: format!(
                                "provider-invocation:{}:{}:failed",
                                request.assignment_id.as_uuid(),
                                request.attempt
                            ),
                            evidence,
                        })
                        .await?;
                    return Err(PortError::ProviderPermanent);
                }
                evidence
                    .complete(
                        response.output_digest.clone(),
                        response.provider_units,
                        response.provider_cost_micros,
                        completed_at,
                    )
                    .map_err(|_| PortError::ProviderPermanent)?;
                self.evidence
                    .commit_invocation_evidence(InvocationEvidenceCommit {
                        actor,
                        authority_scope: request.authority_scope,
                        audit,
                        expected_version: Some(1),
                        idempotency_key: format!(
                            "provider-invocation:{}:{}:completed",
                            request.assignment_id.as_uuid(),
                            request.attempt
                        ),
                        evidence,
                    })
                    .await?;
                Ok(response)
            }
            Err(error) => {
                let completed_at = std::cmp::max(chrono::Utc::now(), evidence.invoked_at());
                evidence
                    .fail(completed_at)
                    .map_err(|_| PortError::ProviderPermanent)?;
                self.evidence
                    .commit_invocation_evidence(InvocationEvidenceCommit {
                        actor,
                        authority_scope: request.authority_scope,
                        audit,
                        expected_version: Some(1),
                        idempotency_key: format!(
                            "provider-invocation:{}:{}:failed",
                            request.assignment_id.as_uuid(),
                            request.attempt
                        ),
                        evidence,
                    })
                    .await?;
                Err(error)
            }
        }
    }
}

fn provider_invocation_id(
    holding_id: uuid::Uuid,
    assignment_id: AssignmentId,
    attempt: u32,
) -> Result<ProviderInvocationId, PortError> {
    if holding_id.is_nil() || attempt == 0 {
        return Err(PortError::Denied);
    }
    let mut digest = Sha256::new();
    digest.update(b"erpx.agent.provider-invocation.v1");
    digest.update(holding_id.as_bytes());
    digest.update(assignment_id.as_uuid().as_bytes());
    digest.update(attempt.to_be_bytes());
    let bytes = digest.finalize();
    let mut uuid_bytes = [0_u8; 16];
    uuid_bytes.copy_from_slice(&bytes[..16]);
    uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x50;
    uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80;
    ProviderInvocationId::try_from_uuid(uuid::Uuid::from_bytes(uuid_bytes))
        .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWorkerOutcome {
    Idle,
    Completed,
    BudgetStopped,
    CancelledOrSuperseded,
    Failed,
}

#[derive(Debug, Error)]
pub enum AgentWorkerError {
    #[error("agent worker configuration is invalid")]
    InvalidConfiguration,
    #[error(transparent)]
    Port(#[from] PortError),
}

/// One durable worker pass. Claiming, terminal transitions, checkpoints, and
/// usage reconciliation remain transactionally fenced by the queue adapter.
/// The worker can only persist text output and typed action-card proposals; it
/// has no port capable of mutating a business-context repository.
pub struct AgentAssignmentWorker<Q, Y, G, P, M, N, C> {
    queue: Q,
    policies: Y,
    execution_guard: G,
    provider: P,
    protected_material: M,
    notifications: N,
    clock: C,
    worker_ref: String,
    lease_duration: StdDuration,
}

impl<Q, Y, G, P, M, N, C> AgentAssignmentWorker<Q, Y, G, P, M, N, C> {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        queue: Q,
        policies: Y,
        execution_guard: G,
        provider: P,
        protected_material: M,
        notifications: N,
        clock: C,
        worker_ref: String,
        lease_duration: StdDuration,
    ) -> Result<Self, AgentWorkerError> {
        if worker_ref.trim().is_empty()
            || lease_duration.is_zero()
            || lease_duration > StdDuration::from_secs(15 * 60)
        {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        Ok(Self {
            queue,
            policies,
            execution_guard,
            provider,
            protected_material,
            notifications,
            clock,
            worker_ref,
            lease_duration,
        })
    }
}

impl<Q, Y, G, P, M, N, C> AgentAssignmentWorker<Q, Y, G, P, M, N, C>
where
    Q: WorkerAssignmentPort,
    Y: ProviderPolicyPort,
    G: AgentExecutionGuardPort,
    P: BudgetedAgentProviderPort,
    M: ProtectedAgentMaterialPort,
    N: AgentNotificationPort,
    C: Clock,
{
    pub async fn run_once(&self) -> Result<AgentWorkerOutcome, AgentWorkerError> {
        let at = self.clock.now();
        let lease_duration = Duration::from_std(self.lease_duration)
            .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
        let Some(claim) = self
            .queue
            .claim_due(&self.worker_ref, at, at + lease_duration)
            .await?
        else {
            return Ok(AgentWorkerOutcome::Idle);
        };
        let assignment_id = claim.assignment.id();
        let expected_version = claim.assignment.version();
        let initiating_principal = claim
            .assignment
            .root_invocation()
            .initiating_principal_ref
            .clone();
        let result = self.execute_claim(&claim).await;
        match result {
            Ok((mut response, encrypted_output_ref)) => {
                let usage = UsageReconciliation {
                    actual_units: response.provider_units,
                    actual_cost_micros: response.provider_cost_micros,
                    exceeded_reservation: response.provider_units
                        > claim.budget_request.estimated_units
                        || response.provider_cost_micros
                            > claim.budget_request.estimated_cost_micros
                        || response.provider_units > claim.budget_request.stop_after_units,
                };
                if usage.exceeded_reservation {
                    // Preserve usage/output evidence for reconciliation, but
                    // an over-budget response cannot publish action cards.
                    response.proposed_actions.clear();
                }
                let completion = self
                    .queue
                    .complete(
                        assignment_id,
                        expected_version,
                        claim.lease_token,
                        &response.output_digest,
                        &encrypted_output_ref,
                        &response.proposed_actions,
                        &usage,
                        self.clock.now(),
                    )
                    .await;
                match completion {
                    Ok(()) => {}
                    // Cancellation, reclaim, or lease expiry may win while a
                    // bounded provider invocation is in flight. The queue is
                    // authoritative and fences the stale worker; do not turn
                    // that expected race into a runtime/configuration error.
                    Err(PortError::VersionConflict) => {
                        return Ok(AgentWorkerOutcome::CancelledOrSuperseded);
                    }
                    Err(error) => return Err(error.into()),
                }
                self.notifications
                    .publish_state(
                        claim.assignment.scope(),
                        assignment_id,
                        expected_version.saturating_add(1),
                        &initiating_principal,
                        AssignmentState::Completed,
                        self.clock.now(),
                    )
                    .await?;
                if usage.exceeded_reservation {
                    Ok(AgentWorkerOutcome::BudgetStopped)
                } else {
                    Ok(AgentWorkerOutcome::Completed)
                }
            }
            Err(PortError::VersionConflict) => Ok(AgentWorkerOutcome::CancelledOrSuperseded),
            Err(error) => {
                let retryable = matches!(error, PortError::ProviderRetryable);
                let failure = AssignmentFailure {
                    code: provider_failure_code(&error).into(),
                    retryable,
                    detail_digest: format!("{:x}", Sha256::digest(provider_failure_code(&error))),
                    failed_at: self.clock.now(),
                };
                let persisted = self
                    .queue
                    .fail(
                        assignment_id,
                        expected_version,
                        claim.lease_token,
                        &failure,
                        failure.failed_at,
                    )
                    .await;
                match persisted {
                    Ok(()) => {}
                    // A concurrent cancel/reclaim is already the durable
                    // outcome and must not be overwritten by a stale worker.
                    Err(PortError::VersionConflict) => {
                        return Ok(AgentWorkerOutcome::CancelledOrSuperseded);
                    }
                    Err(error) => return Err(error.into()),
                }
                self.notifications
                    .publish_state(
                        claim.assignment.scope(),
                        assignment_id,
                        expected_version.saturating_add(1),
                        &initiating_principal,
                        AssignmentState::Failed,
                        failure.failed_at,
                    )
                    .await?;
                Ok(AgentWorkerOutcome::Failed)
            }
        }
    }

    async fn execute_claim(
        &self,
        claim: &ClaimedAssignment,
    ) -> Result<(ProviderInvocationResponse, String), PortError> {
        self.execution_guard
            .authorize_execution(claim, self.clock.now())
            .await?;
        let prompt_material = self.protected_material.resolve_prompt(claim).await?;
        if format!("{:x}", Sha256::digest(prompt_material.as_bytes()))
            != claim.payload_manifest.payload_digest.to_ascii_lowercase()
        {
            return Err(PortError::ProtectedMaterialUnavailable);
        }
        let pin = claim.assignment.pin().clone();
        let policy = self
            .policies
            .load_policy(claim.assignment.scope(), &pin, self.clock.now())
            .await?;
        let decision = policy
            .authorize(&pin, &claim.payload_manifest, self.clock.now())
            .map_err(|_| PortError::ProviderDenied)?;
        // A cancellation, security revocation, policy change, or budget stop
        // may race material resolution. Recheck at the last safe point before
        // provider egress.
        self.execution_guard
            .authorize_execution(claim, self.clock.now())
            .await?;
        let response = self
            .provider
            .invoke_bounded(
                ProviderInvocationRequest {
                    assignment_id: claim.assignment.id(),
                    assignment_version: claim.assignment.version(),
                    attempt: u32::from(claim.assignment.attempt()),
                    authority_scope: claim.assignment.scope().clone(),
                    lease_token: claim.lease_token,
                    pin,
                    prompt_material,
                    payload_manifest: claim.payload_manifest.clone(),
                    policy_decision: decision,
                },
                &claim.budget_request,
            )
            .await?;
        if response.output_material.len() > MAX_PROVIDER_OUTPUT_BYTES
            || format!("{:x}", Sha256::digest(response.output_material.as_bytes()))
                != response.output_digest.to_ascii_lowercase()
        {
            return Err(PortError::ProviderPermanent);
        }
        if response.proposed_actions.iter().any(|action| {
            action.assignment_id != claim.assignment.id()
                || action.agent_principal_ref != claim.assignment.pin().agent_principal_ref
        }) {
            return Err(PortError::ProviderPermanent);
        }
        let encrypted_output_ref = self
            .protected_material
            .protect_output(claim, &response.output_material)
            .await?;
        Ok((response, encrypted_output_ref))
    }
}

#[async_trait]
pub trait AgentAssignmentPass: Send + Sync {
    async fn run_assignment_pass(&self) -> Result<AgentWorkerOutcome, AgentWorkerError>;
}

#[async_trait]
impl<Q, Y, G, P, M, N, C> AgentAssignmentPass for AgentAssignmentWorker<Q, Y, G, P, M, N, C>
where
    Q: WorkerAssignmentPort,
    Y: ProviderPolicyPort,
    G: AgentExecutionGuardPort,
    P: BudgetedAgentProviderPort,
    M: ProtectedAgentMaterialPort,
    N: AgentNotificationPort,
    C: Clock,
{
    async fn run_assignment_pass(&self) -> Result<AgentWorkerOutcome, AgentWorkerError> {
        self.run_once().await
    }
}

/// Runtime registry adapter. Missing production dependencies are represented
/// as an explicit unavailable handler, so deployments never silently omit the
/// accepted handler name and never claim work with partial composition.
pub struct AgentAssignmentRuntimeHandler {
    runner: Option<std::sync::Arc<dyn AgentAssignmentPass>>,
}

/// Routes the authoritative holding outbox fact through the existing
/// notification authority. It owns no notification records and relies on the
/// port's assignment/version/state idempotency identity for safe replay.
pub struct AgentAssignmentNotificationOutboxHandler<N> {
    notifications: std::sync::Arc<N>,
}

#[async_trait]
pub trait AgentNotificationOutboxPass: Send + Sync {
    async fn run_notification_pass(&self) -> Result<bool, AgentWorkerError>;
}

/// Holding-local notification publisher. It claims only the exact Agent event
/// type from a holding-bound queue, persists only a redacted failure digest,
/// and fences acknowledgement with the durable claim token.
pub struct AgentNotificationOutboxWorker {
    queue: erpx_agent_orchestration_sqlx::SqlxWorkerQueue,
    handler: std::sync::Arc<dyn OutboxHandler>,
    lease_duration: StdDuration,
}

impl AgentNotificationOutboxWorker {
    pub fn try_new(
        queue: erpx_agent_orchestration_sqlx::SqlxWorkerQueue,
        handler: std::sync::Arc<dyn OutboxHandler>,
        lease_duration: StdDuration,
    ) -> Result<Self, AgentWorkerError> {
        if lease_duration.is_zero() || lease_duration > StdDuration::from_secs(15 * 60) {
            return Err(AgentWorkerError::InvalidConfiguration);
        }
        Ok(Self {
            queue,
            handler,
            lease_duration,
        })
    }

    pub async fn run_once(&self) -> Result<bool, AgentWorkerError> {
        const MAX_ATTEMPTS: u32 = 8;
        let at = chrono::Utc::now();
        let lease = chrono::Duration::from_std(self.lease_duration)
            .map_err(|_| AgentWorkerError::InvalidConfiguration)?;
        let Some(fact) = self.queue.claim_notification_fact(at, at + lease).await? else {
            return Ok(false);
        };
        let envelope = OutboxEnvelope {
            id: fact.event_id,
            event_type: "agent.assignment.state_notification.requested".into(),
            event_version: fact.event_version,
            aggregate_type: fact.aggregate_type,
            aggregate_id: fact.aggregate_id,
            aggregate_version: fact.aggregate_version,
            active_scope: Some(fact.active_scope),
            actor: Some(fact.actor),
            correlation_id: fact.correlation_id,
            causation_id: fact.causation_id,
            idempotency_key: None,
            payload: fact.payload,
            occurred_at: fact.occurred_at,
            available_at: fact.available_at,
            attempts: fact.attempts,
            max_attempts: MAX_ATTEMPTS,
            claimed_by: None,
            claimed_at: Some(at),
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        match self.handler.handle(&envelope).await {
            Ok(()) => {
                self.queue
                    .complete_notification_fact(
                        fact.global_sequence,
                        fact.claim_token,
                        chrono::Utc::now(),
                    )
                    .await?;
            }
            Err(error) => {
                let retryable = matches!(error, WorkerError::Retryable(_));
                let terminal = !retryable || fact.attempts >= MAX_ATTEMPTS;
                let error_kind = if retryable { "retryable" } else { "permanent" };
                let error_digest = format!(
                    "{:x}",
                    Sha256::digest(format!("agent-notification:{error_kind}").as_bytes())
                );
                let retry_seconds = 1_i64 << fact.attempts.min(8);
                let now = chrono::Utc::now();
                self.queue
                    .fail_notification_fact(
                        fact.global_sequence,
                        fact.claim_token,
                        &error_digest,
                        now + chrono::Duration::seconds(retry_seconds),
                        now,
                        terminal,
                    )
                    .await?;
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl AgentNotificationOutboxPass for AgentNotificationOutboxWorker {
    async fn run_notification_pass(&self) -> Result<bool, AgentWorkerError> {
        self.run_once().await
    }
}

impl<N> AgentAssignmentNotificationOutboxHandler<N>
where
    N: AgentNotificationPort,
{
    pub fn new(notifications: std::sync::Arc<N>) -> Self {
        Self { notifications }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentStateNotificationPayload {
    assignment_id: uuid::Uuid,
    root_assignment_id: uuid::Uuid,
    recipient_principal_ref: String,
    agent_principal_ref: String,
    state: String,
    aggregate_version: u64,
}

#[async_trait]
impl<N> OutboxHandler for AgentAssignmentNotificationOutboxHandler<N>
where
    N: AgentNotificationPort,
{
    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        if event.event_type != "agent.assignment.state_notification.requested"
            || event.event_version != 1
            || event.aggregate_type != "assignment"
            || event.active_scope.is_none()
            || event.actor.is_none()
        {
            return Err(WorkerError::Permanent(
                "invalid agent assignment notification contract".into(),
            ));
        }
        let payload: AssignmentStateNotificationPayload =
            serde_json::from_value(event.payload.clone()).map_err(|_| {
                WorkerError::Permanent("invalid agent assignment notification payload".into())
            })?;
        if payload.assignment_id != event.aggregate_id
            || payload.aggregate_version != event.aggregate_version
            || payload.root_assignment_id.is_nil()
            || !safe_principal_ref(&payload.recipient_principal_ref)
            || !safe_principal_ref(&payload.agent_principal_ref)
        {
            return Err(WorkerError::Permanent(
                "agent assignment notification identity mismatch".into(),
            ));
        }
        let state = parse_assignment_state(&payload.state).ok_or_else(|| {
            WorkerError::Permanent("invalid agent assignment notification state".into())
        })?;
        self.notifications
            .publish_state(
                &serde_json::from_value(event.active_scope.clone().ok_or_else(|| {
                    WorkerError::Permanent("missing agent notification holding scope".into())
                })?)
                .map_err(|_| {
                    WorkerError::Permanent("invalid agent notification holding scope".into())
                })?,
                AssignmentId::try_from_uuid(payload.assignment_id).map_err(|_| {
                    WorkerError::Permanent("invalid agent assignment notification id".into())
                })?,
                payload.aggregate_version,
                &payload.recipient_principal_ref,
                state,
                event.occurred_at,
            )
            .await
            .map_err(notification_worker_error)
    }
}

fn parse_assignment_state(value: &str) -> Option<AssignmentState> {
    match value {
        "queued" => Some(AssignmentState::Queued),
        "running" => Some(AssignmentState::Running),
        "waiting" => Some(AssignmentState::Waiting),
        "completed" => Some(AssignmentState::Completed),
        "failed" => Some(AssignmentState::Failed),
        "cancelled" => Some(AssignmentState::Cancelled),
        _ => None,
    }
}

fn safe_principal_ref(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

fn notification_worker_error(error: PortError) -> WorkerError {
    match error {
        PortError::Persistence | PortError::Internal | PortError::ProviderRetryable => {
            WorkerError::Retryable("agent notification delivery unavailable".into())
        }
        _ => WorkerError::Permanent("agent notification delivery denied".into()),
    }
}

/// Constructs one production runner for exactly one current holding data
/// plane. The queue and last-moment execution guard share the same immutable
/// holding identity and bind it through transaction-local RLS before querying.
/// Every egress-capable dependency is supplied explicitly; there
/// is deliberately no environment fallback that could install a permissive
/// policy, plaintext material resolver, notification sink, or network client.
#[allow(clippy::too_many_arguments)]
pub fn compose_sqlx_agent_assignment_runner<Y, M, N>(
    holding_write_pool: sqlx::PgPool,
    holding_id: uuid::Uuid,
    policies: Y,
    provider: GovernedAgentProvider,
    protected_material: M,
    notifications: N,
    worker_ref: String,
    lease_duration: StdDuration,
) -> Result<std::sync::Arc<dyn AgentAssignmentPass>, AgentWorkerError>
where
    Y: ProviderPolicyPort + 'static,
    M: ProtectedAgentMaterialPort + 'static,
    N: AgentNotificationPort + 'static,
{
    if provider.holding_id() != holding_id {
        return Err(AgentWorkerError::InvalidConfiguration);
    }
    let evidence = erpx_agent_orchestration_sqlx::SqlxAgentExperienceStore::try_new(
        holding_write_pool.clone(),
        holding_id,
    )?;
    let provider = EvidencedAgentProvider::new(provider, evidence, holding_id);
    let queue =
        erpx_agent_orchestration_sqlx::SqlxWorkerQueue::try_new(holding_write_pool, holding_id)?;
    Ok(std::sync::Arc::new(AgentAssignmentWorker::try_new(
        queue.clone(),
        policies,
        queue,
        provider,
        protected_material,
        notifications,
        erpx_kernel::SystemClock,
        worker_ref,
        lease_duration,
    )?))
}

impl AgentAssignmentRuntimeHandler {
    pub fn available(runner: std::sync::Arc<dyn AgentAssignmentPass>) -> Self {
        Self {
            runner: Some(runner),
        }
    }

    pub const fn unavailable() -> Self {
        Self { runner: None }
    }
}

#[async_trait]
impl NamedWorkerHandler for AgentAssignmentRuntimeHandler {
    fn name(&self) -> &str {
        AGENT_ASSIGNMENT_HANDLER_NAME
    }

    async fn run_once(
        &self,
        _worker_id: uuid::Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let Some(runner) = &self.runner else {
            return Err(WorkerRuntimeError::Configuration(
                "agent assignment dependencies are unavailable".into(),
            ));
        };
        let outcome = runner.run_assignment_pass().await.map_err(|error| {
            WorkerRuntimeError::Configuration(format!("agent assignment pass failed: {error}"))
        })?;
        Ok(match outcome {
            AgentWorkerOutcome::Idle | AgentWorkerOutcome::CancelledOrSuperseded => {
                WorkerHandlerReport::default()
            }
            AgentWorkerOutcome::Completed | AgentWorkerOutcome::BudgetStopped => {
                WorkerHandlerReport {
                    claimed: 1,
                    processed: 1,
                    has_work: true,
                    ..WorkerHandlerReport::default()
                }
            }
            AgentWorkerOutcome::Failed => WorkerHandlerReport {
                claimed: 1,
                processed: 1,
                has_work: true,
                ..WorkerHandlerReport::default()
            },
        })
    }
}

fn provider_failure_code(error: &PortError) -> &'static str {
    match error {
        PortError::ProviderRetryable => "provider_retryable",
        PortError::ProviderPermanent => "provider_permanent",
        PortError::ProviderDenied | PortError::Denied => "provider_policy_denied",
        PortError::ProtectedMaterialUnavailable => "protected_material_unavailable",
        PortError::BudgetUnavailable => "budget_unavailable",
        PortError::VersionConflict => "lease_or_version_conflict",
        PortError::IdempotencyConflict => "idempotency_conflict",
        PortError::NotFound => "dependency_not_found",
        PortError::Persistence | PortError::Internal => "orchestration_internal",
    }
}

#[allow(dead_code)]
fn _scope_is_never_widened(parent: &AuthorityScope, child: &AuthorityScope) -> bool {
    parent.contains(child)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use chrono::{TimeZone, Utc};
    use erpx_agent_orchestration::{
        ActionCardProposal, AgentDefinitionId, AgentManifest, AgentPin, AgentVersionId, Assignment,
        BudgetReservationId, Checkpoint, InvocationOrigin, PayloadManifest, PrincipalKind,
        ProviderPolicy, RootInvocation,
    };
    use erpx_kernel::{DataClassification, FixedClock};
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct QueueState {
        claim: Option<ClaimedAssignment>,
        completion: Option<UsageReconciliation>,
        failure: Option<AssignmentFailure>,
        complete_error: Option<PortError>,
        fail_error: Option<PortError>,
    }

    #[derive(Clone, Default)]
    struct FakeQueue(Arc<Mutex<QueueState>>);

    #[async_trait]
    impl WorkerAssignmentPort for FakeQueue {
        async fn claim_due(
            &self,
            _worker_ref: &str,
            _at: chrono::DateTime<Utc>,
            _lease_until: chrono::DateTime<Utc>,
        ) -> Result<Option<ClaimedAssignment>, PortError> {
            Ok(self.0.lock().expect("queue").claim.take())
        }

        async fn load_prompt_material(
            &self,
            _claim: &ClaimedAssignment,
        ) -> Result<String, PortError> {
            Err(PortError::ProtectedMaterialUnavailable)
        }

        async fn save_checkpoint(
            &self,
            _assignment_id: AssignmentId,
            _expected_version: u64,
            _lease_token: Uuid,
            _checkpoint: Checkpoint,
        ) -> Result<(), PortError> {
            Ok(())
        }

        async fn complete(
            &self,
            _assignment_id: AssignmentId,
            _expected_version: u64,
            _lease_token: Uuid,
            _output_digest: &str,
            _encrypted_output_ref: &str,
            _proposed_actions: &[ActionCardProposal],
            usage: &UsageReconciliation,
            _at: chrono::DateTime<Utc>,
        ) -> Result<(), PortError> {
            if let Some(error) = self.0.lock().expect("queue").complete_error.clone() {
                return Err(error);
            }
            self.0.lock().expect("queue").completion = Some(usage.clone());
            Ok(())
        }

        async fn fail(
            &self,
            _assignment_id: AssignmentId,
            _expected_version: u64,
            _lease_token: Uuid,
            failure: &AssignmentFailure,
            _at: chrono::DateTime<Utc>,
        ) -> Result<(), PortError> {
            if let Some(error) = self.0.lock().expect("queue").fail_error.clone() {
                return Err(error);
            }
            self.0.lock().expect("queue").failure = Some(failure.clone());
            Ok(())
        }
    }

    struct FakePolicy {
        policy: Option<ProviderPolicy>,
    }

    #[async_trait]
    impl ProviderPolicyPort for FakePolicy {
        async fn load_policy(
            &self,
            _authority_scope: &AuthorityScope,
            _pin: &AgentPin,
            _at: chrono::DateTime<Utc>,
        ) -> Result<ProviderPolicy, PortError> {
            self.policy.clone().ok_or(PortError::ProviderDenied)
        }
    }

    struct FakeGuard {
        calls: AtomicUsize,
        reject_call: Option<usize>,
    }

    #[async_trait]
    impl AgentExecutionGuardPort for FakeGuard {
        async fn authorize_execution(
            &self,
            _claim: &ClaimedAssignment,
            _at: chrono::DateTime<Utc>,
        ) -> Result<(), PortError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.reject_call == Some(call) {
                Err(PortError::VersionConflict)
            } else {
                Ok(())
            }
        }
    }

    struct FakeProvider {
        calls: AtomicUsize,
        response: ProviderInvocationResponse,
        observed_stop: Mutex<Option<u64>>,
    }

    #[derive(Clone, Default)]
    struct FakeEvidence {
        commits: Arc<Mutex<Vec<InvocationEvidenceCommit>>>,
        fail_first: bool,
    }

    #[async_trait]
    impl AgentExperienceUnitOfWork for FakeEvidence {
        async fn commit_pool(
            &self,
            _commit: erpx_agent_orchestration::PoolCommit,
        ) -> Result<erpx_agent_orchestration::AgentPool, PortError> {
            Err(PortError::Internal)
        }

        async fn commit_pool_routing(
            &self,
            _commit: erpx_agent_orchestration::PoolRoutingCommit,
        ) -> Result<erpx_agent_orchestration::PoolRoutingDecision, PortError> {
            Err(PortError::Internal)
        }

        async fn commit_session(
            &self,
            _commit: erpx_agent_orchestration::SessionCommit,
        ) -> Result<erpx_agent_orchestration::SessionMutationReceipt, PortError> {
            Err(PortError::Internal)
        }

        async fn commit_invocation_evidence(
            &self,
            commit: InvocationEvidenceCommit,
        ) -> Result<ProviderInvocationEvidence, PortError> {
            if self.fail_first && self.commits.lock().expect("evidence").is_empty() {
                return Err(PortError::Persistence);
            }
            let result = commit.evidence.clone();
            self.commits.lock().expect("evidence").push(commit);
            Ok(result)
        }
    }

    #[async_trait]
    impl BudgetedAgentProviderPort for FakeProvider {
        async fn invoke_bounded(
            &self,
            _request: ProviderInvocationRequest,
            budget: &BudgetRequest,
        ) -> Result<ProviderInvocationResponse, PortError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.observed_stop.lock().expect("provider budget") = Some(budget.stop_after_units);
            Ok(self.response.clone())
        }
    }

    struct FakeMaterial {
        prompt: String,
    }

    #[async_trait]
    impl ProtectedAgentMaterialPort for FakeMaterial {
        async fn resolve_prompt(&self, _claim: &ClaimedAssignment) -> Result<String, PortError> {
            Ok(self.prompt.clone())
        }

        async fn protect_output(
            &self,
            _claim: &ClaimedAssignment,
            _material: &str,
        ) -> Result<String, PortError> {
            Ok("protected:output:v1".into())
        }
    }

    #[derive(Clone, Default)]
    struct FakeNotifications(Arc<Mutex<Vec<(u64, AssignmentState)>>>);

    #[async_trait]
    impl AgentNotificationPort for FakeNotifications {
        async fn publish_state(
            &self,
            _scope: &AuthorityScope,
            _assignment_id: AssignmentId,
            aggregate_version: u64,
            _recipient_principal_ref: &str,
            state: AssignmentState,
            _at: chrono::DateTime<Utc>,
        ) -> Result<(), PortError> {
            self.0
                .lock()
                .expect("notifications")
                .push((aggregate_version, state));
            Ok(())
        }
    }

    struct Fixture {
        claim: ClaimedAssignment,
        policy: ProviderPolicy,
        prompt: String,
        response: ProviderInvocationResponse,
        at: chrono::DateTime<Utc>,
    }

    fn id<T>(
        value: u128,
        constructor: fn(Uuid) -> Result<T, erpx_agent_orchestration::AgentDomainError>,
    ) -> T {
        constructor(Uuid::from_u128(value)).expect("id")
    }

    fn fixture() -> Fixture {
        let at = Utc.with_ymd_and_hms(2026, 8, 16, 10, 0, 0).unwrap();
        let scope = AuthorityScope::Holding {
            holding_id: Uuid::from_u128(10),
        };
        let policy_digest = "a".repeat(64);
        let manifest = AgentManifest {
            version_id: id(2, AgentVersionId::try_from_uuid),
            version_number: 7,
            purpose: "Summarize an authorized ERPX record".into(),
            provider_ref: "provider-approved".into(),
            model_ref: "model-pinned".into(),
            use_case: "record-summary".into(),
            prompt_digest: "b".repeat(64),
            policy_digest: policy_digest.clone(),
            capabilities: BTreeSet::from(["summarize".into()]),
            tool_refs: BTreeSet::new(),
            knowledge_scopes: BTreeSet::from(["current-record".into()]),
            organization_scopes: vec![scope.clone()],
            maximum_classification: DataClassification::Confidential,
            security_review_ref: "security-review-1".into(),
            privacy_review_ref: "privacy-review-1".into(),
            evaluation_refs: BTreeSet::from(["evaluation-1".into()]),
            max_attempts: 3,
            timeout_seconds: 60,
        };
        let definition_id = id(1, AgentDefinitionId::try_from_uuid);
        let version_id = manifest.version_id;
        let approved_evaluation_refs = manifest.evaluation_refs.clone();
        let pin = AgentPin {
            definition_id,
            agent_principal_ref: "agent:summary".into(),
            manifest,
        };
        let mut assignment = Assignment::start_root(
            id(3, AssignmentId::try_from_uuid),
            RootInvocation {
                initiating_principal_ref: "human:requester".into(),
                initiating_principal_kind: PrincipalKind::Human,
                origin: InvocationOrigin::Run,
                source_action_ref: "run:record-summary".into(),
                idempotency_key: "idem:record-summary:1".into(),
            },
            pin,
            scope,
            id(4, BudgetReservationId::try_from_uuid),
            at,
        )
        .expect("assignment");
        let lease_token = Uuid::from_u128(5);
        assignment
            .claim(
                lease_token,
                "agent-worker:test".into(),
                at + Duration::minutes(5),
                at,
            )
            .expect("claim");
        let prompt = "authorized protected payload".to_owned();
        let payload_digest = format!("{:x}", Sha256::digest(prompt.as_bytes()));
        let response_material = "bounded provider response".to_owned();
        let response = ProviderInvocationResponse {
            output_digest: format!("{:x}", Sha256::digest(response_material.as_bytes())),
            output_material: response_material,
            provider_units: 80,
            provider_cost_micros: 800,
            proposed_actions: Vec::new(),
        };
        Fixture {
            claim: ClaimedAssignment {
                assignment,
                lease_token,
                prompt_ref: "protected:prompt:v1".into(),
                payload_manifest: PayloadManifest {
                    payload_digest,
                    classification: DataClassification::Confidential,
                    field_refs: BTreeSet::from(["invoice.total".into()]),
                    document_refs: BTreeSet::new(),
                    authorization_checked_at: at,
                    contains_secret: false,
                    residency: "TH".into(),
                },
                budget_request: BudgetRequest {
                    estimated_units: 100,
                    estimated_cost_micros: 1_000,
                    queue_priority: 10,
                    stop_after_units: 120,
                },
            },
            policy: ProviderPolicy {
                id: erpx_agent_orchestration::ProviderPolicyId::try_from_uuid(Uuid::from_u128(8))
                    .expect("policy id"),
                agent_definition_id: definition_id,
                agent_version_id: version_id,
                policy_version: 1,
                provider_ref: "provider-approved".into(),
                external: true,
                approved: true,
                dpa_ref: Some("dpa:approved".into()),
                allowed_residencies: BTreeSet::from(["TH".into()]),
                allowed_models: BTreeSet::from(["model-pinned".into()]),
                allowed_use_cases: BTreeSet::from(["record-summary".into()]),
                maximum_classification: DataClassification::Confidential,
                policy_digest,
                approved_evaluation_refs,
                freshness_checked_at: at,
                fresh_until: at + Duration::minutes(5),
                expires_at: at + Duration::hours(1),
            },
            prompt,
            response,
            at,
        }
    }

    type TestWorker = AgentAssignmentWorker<
        FakeQueue,
        FakePolicy,
        FakeGuard,
        Arc<FakeProvider>,
        FakeMaterial,
        FakeNotifications,
        FixedClock,
    >;

    fn worker(
        fixture: Fixture,
        reject_guard_call: Option<usize>,
    ) -> (TestWorker, FakeQueue, Arc<FakeProvider>, FakeNotifications) {
        let queue = FakeQueue::default();
        queue.0.lock().expect("queue").claim = Some(fixture.claim);
        let provider = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            response: fixture.response,
            observed_stop: Mutex::new(None),
        });
        let notifications = FakeNotifications::default();
        let worker = AgentAssignmentWorker::try_new(
            queue.clone(),
            FakePolicy {
                policy: Some(fixture.policy),
            },
            FakeGuard {
                calls: AtomicUsize::new(0),
                reject_call: reject_guard_call,
            },
            Arc::clone(&provider),
            FakeMaterial {
                prompt: fixture.prompt,
            },
            notifications.clone(),
            FixedClock::new(fixture.at),
            "agent-worker:test".into(),
            StdDuration::from_secs(60),
        )
        .expect("worker");
        (worker, queue, provider, notifications)
    }

    #[async_trait]
    impl BudgetedAgentProviderPort for Arc<FakeProvider> {
        async fn invoke_bounded(
            &self,
            request: ProviderInvocationRequest,
            budget: &BudgetRequest,
        ) -> Result<ProviderInvocationResponse, PortError> {
            self.as_ref().invoke_bounded(request, budget).await
        }
    }

    #[test]
    fn worker_surface_has_no_business_mutation_port() {
        let source = include_str!("agent_orchestration_runtime.rs");
        assert!(!source.contains(concat!("CommandService", "Client")));
        assert!(!source.contains(concat!("business_", "repository")));
        assert!(source.contains("ProviderPolicyPort"));
        assert!(source.contains("proposed_actions"));
    }

    fn provider_request(fixture: &Fixture) -> ProviderInvocationRequest {
        let decision = fixture
            .policy
            .authorize(
                fixture.claim.assignment.pin(),
                &fixture.claim.payload_manifest,
                fixture.at,
            )
            .expect("policy");
        ProviderInvocationRequest {
            assignment_id: fixture.claim.assignment.id(),
            assignment_version: fixture.claim.assignment.version(),
            attempt: u32::from(fixture.claim.assignment.attempt()),
            authority_scope: fixture.claim.assignment.scope().clone(),
            lease_token: fixture.claim.lease_token,
            pin: fixture.claim.assignment.pin().clone(),
            prompt_material: fixture.prompt.clone(),
            payload_manifest: fixture.claim.payload_manifest.clone(),
            policy_decision: decision,
        }
    }

    #[tokio::test]
    async fn provider_egress_is_denied_when_started_evidence_cannot_commit() {
        let fixture = fixture();
        let provider = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            response: fixture.response.clone(),
            observed_stop: Mutex::new(None),
        });
        let evidence = FakeEvidence {
            commits: Arc::default(),
            fail_first: true,
        };
        let governed =
            EvidencedAgentProvider::new(Arc::clone(&provider), evidence, Uuid::from_u128(10));
        assert_eq!(
            governed
                .invoke_bounded(provider_request(&fixture), &fixture.claim.budget_request)
                .await,
            Err(PortError::Persistence)
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn provider_invocation_persists_started_and_terminal_digest_only_evidence() {
        let fixture = fixture();
        let provider = Arc::new(FakeProvider {
            calls: AtomicUsize::new(0),
            response: fixture.response.clone(),
            observed_stop: Mutex::new(None),
        });
        let evidence = FakeEvidence::default();
        let commits = Arc::clone(&evidence.commits);
        let governed =
            EvidencedAgentProvider::new(Arc::clone(&provider), evidence, Uuid::from_u128(10));
        governed
            .invoke_bounded(provider_request(&fixture), &fixture.claim.budget_request)
            .await
            .expect("provider response");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let commits = commits.lock().expect("evidence");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].expected_version, None);
        assert_eq!(commits[0].evidence.version(), 1);
        assert_eq!(commits[1].expected_version, Some(1));
        assert_eq!(commits[1].evidence.version(), 2);
        let encoded = serde_json::to_string(&commits[1].evidence).expect("evidence JSON");
        assert!(!encoded.contains(&fixture.prompt));
        assert!(!encoded.contains(&fixture.response.output_material));
    }

    #[tokio::test]
    async fn production_composition_rejects_provider_from_another_holding_before_pool_use() {
        let fixture = fixture();
        let provider = crate::AgentNetworkProviderConfig::Ollama(
            crate::OllamaProviderConfig::try_new(
                "http://127.0.0.1:11434",
                crate::AgentProviderApproval::try_new(
                    Uuid::from_u128(10),
                    "provider-approved".into(),
                    BTreeSet::from(["model-pinned".into()]),
                    BTreeSet::from(["record-summary".into()]),
                )
                .expect("approval"),
                crate::AgentProviderLimits::default(),
            )
            .expect("provider configuration"),
        )
        .build()
        .expect("governed provider");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .expect("lazy pool");
        let result = compose_sqlx_agent_assignment_runner(
            pool,
            Uuid::from_u128(11),
            FakePolicy { policy: None },
            provider,
            FakeMaterial {
                prompt: fixture.prompt,
            },
            FakeNotifications::default(),
            "agent-worker:test".into(),
            StdDuration::from_secs(60),
        );
        assert!(matches!(
            result,
            Err(AgentWorkerError::InvalidConfiguration)
        ));
    }

    #[test]
    fn invalid_worker_configuration_is_rejected_before_claiming() {
        let result = AgentAssignmentWorker::try_new(
            (),
            (),
            (),
            (),
            (),
            (),
            erpx_kernel::FixedClock::new(chrono::Utc::now()),
            String::new(),
            StdDuration::from_secs(60),
        );
        assert!(matches!(
            result,
            Err(AgentWorkerError::InvalidConfiguration)
        ));
    }

    #[tokio::test]
    async fn success_rechecks_authority_and_passes_the_durable_stop_to_provider() {
        let (worker, queue, provider, notifications) = worker(fixture(), None);
        assert_eq!(
            worker.run_once().await.unwrap(),
            AgentWorkerOutcome::Completed
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(*provider.observed_stop.lock().unwrap(), Some(120));
        assert!(
            !queue
                .0
                .lock()
                .unwrap()
                .completion
                .as_ref()
                .unwrap()
                .exceeded_reservation
        );
        assert_eq!(
            *notifications.0.lock().unwrap(),
            [(
                fixture().claim.assignment.version() + 1,
                AssignmentState::Completed
            )]
        );
    }

    #[tokio::test]
    async fn cancellation_race_is_rechecked_before_egress_and_never_overwritten() {
        let (worker, queue, provider, notifications) = worker(fixture(), Some(2));
        assert_eq!(
            worker.run_once().await.unwrap(),
            AgentWorkerOutcome::CancelledOrSuperseded
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        let state = queue.0.lock().unwrap();
        assert!(state.completion.is_none());
        assert!(state.failure.is_none());
        assert!(notifications.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reclaim_race_during_provider_call_fences_stale_completion() {
        let (worker, queue, provider, notifications) = worker(fixture(), None);
        queue.0.lock().unwrap().complete_error = Some(PortError::VersionConflict);
        assert_eq!(
            worker.run_once().await.unwrap(),
            AgentWorkerOutcome::CancelledOrSuperseded
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(queue.0.lock().unwrap().completion.is_none());
        assert!(notifications.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancellation_race_fences_a_stale_failure_write() {
        let mut fixture = fixture();
        fixture.policy.approved = false;
        let (worker, queue, provider, notifications) = worker(fixture, None);
        queue.0.lock().unwrap().fail_error = Some(PortError::VersionConflict);
        assert_eq!(
            worker.run_once().await.unwrap(),
            AgentWorkerOutcome::CancelledOrSuperseded
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert!(queue.0.lock().unwrap().failure.is_none());
        assert!(notifications.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn policy_denial_fails_before_provider_egress() {
        let mut fixture = fixture();
        fixture.policy.approved = false;
        let (worker, queue, provider, _) = worker(fixture, None);
        assert_eq!(worker.run_once().await.unwrap(), AgentWorkerOutcome::Failed);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            queue.0.lock().unwrap().failure.as_ref().unwrap().code,
            "provider_policy_denied"
        );
    }

    #[tokio::test]
    async fn protected_payload_digest_mismatch_blocks_provider_egress() {
        let mut fixture = fixture();
        fixture.prompt.push_str(" tampered");
        let (worker, queue, provider, _) = worker(fixture, None);
        assert_eq!(worker.run_once().await.unwrap(), AgentWorkerOutcome::Failed);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            queue.0.lock().unwrap().failure.as_ref().unwrap().code,
            "protected_material_unavailable"
        );
    }

    #[tokio::test]
    async fn provider_response_digest_mismatch_is_a_permanent_failure() {
        let mut fixture = fixture();
        fixture.response.output_digest = "c".repeat(64);
        let (worker, queue, provider, _) = worker(fixture, None);
        assert_eq!(worker.run_once().await.unwrap(), AgentWorkerOutcome::Failed);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            queue.0.lock().unwrap().failure.as_ref().unwrap().code,
            "provider_permanent"
        );
    }

    #[tokio::test]
    async fn provider_overage_is_reconciled_and_stops_the_assignment_tree_pass() {
        let mut fixture = fixture();
        fixture.response.provider_units = 121;
        let (worker, queue, provider, _) = worker(fixture, None);
        assert_eq!(
            worker.run_once().await.unwrap(),
            AgentWorkerOutcome::BudgetStopped
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(
            queue
                .0
                .lock()
                .unwrap()
                .completion
                .as_ref()
                .unwrap()
                .exceeded_reservation
        );
    }

    #[tokio::test]
    async fn unavailable_runtime_handler_is_registered_but_never_claims_or_egresses() {
        let handler = AgentAssignmentRuntimeHandler::unavailable();
        assert_eq!(handler.name(), AGENT_ASSIGNMENT_HANDLER_NAME);
        let error = handler.run_once(Uuid::from_u128(9)).await.unwrap_err();
        assert!(error.to_string().contains("dependencies are unavailable"));
    }

    struct FixedPass(AgentWorkerOutcome);

    #[async_trait]
    impl AgentAssignmentPass for FixedPass {
        async fn run_assignment_pass(&self) -> Result<AgentWorkerOutcome, AgentWorkerError> {
            Ok(self.0)
        }
    }

    #[tokio::test]
    async fn available_runtime_handler_maps_a_durable_pass_into_worker_progress() {
        let handler = AgentAssignmentRuntimeHandler::available(Arc::new(FixedPass(
            AgentWorkerOutcome::Completed,
        )));
        let report = handler.run_once(Uuid::from_u128(9)).await.unwrap();
        assert_eq!(report.claimed, 1);
        assert_eq!(report.processed, 1);
        assert!(report.has_work);
        assert_eq!(report.published, 0);
        assert_eq!(report.dead_lettered, 0);
    }

    fn notification_event() -> OutboxEnvelope {
        let occurred_at = Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0).unwrap();
        let assignment_id = Uuid::from_u128(701);
        OutboxEnvelope {
            id: Uuid::from_u128(700),
            event_type: "agent.assignment.state_notification.requested".into(),
            event_version: 1,
            aggregate_type: "assignment".into(),
            aggregate_id: assignment_id,
            aggregate_version: 9,
            active_scope: Some(
                serde_json::to_value(AuthorityScope::Holding {
                    holding_id: Uuid::from_u128(10),
                })
                .expect("scope"),
            ),
            actor: Some(serde_json::json!({"principal_ref": "agent:summary"})),
            correlation_id: Uuid::from_u128(702),
            causation_id: None,
            idempotency_key: None,
            payload: serde_json::json!({
                "assignment_id": assignment_id,
                "root_assignment_id": assignment_id,
                "recipient_principal_ref": "human:requester",
                "agent_principal_ref": "agent:summary",
                "state": "failed",
                "aggregate_version": 9,
            }),
            occurred_at,
            available_at: occurred_at,
            attempts: 0,
            max_attempts: 5,
            claimed_by: Some(Uuid::from_u128(703)),
            claimed_at: Some(occurred_at),
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        }
    }

    #[tokio::test]
    async fn durable_notification_fact_routes_through_the_existing_idempotent_port() {
        let notifications = Arc::new(FakeNotifications::default());
        let handler = AgentAssignmentNotificationOutboxHandler::new(Arc::clone(&notifications));
        handler.handle(&notification_event()).await.unwrap();
        assert_eq!(
            *notifications.0.lock().unwrap(),
            [(9, AssignmentState::Failed)]
        );
    }

    #[tokio::test]
    async fn notification_fact_rejects_identity_drift_before_delivery() {
        let notifications = Arc::new(FakeNotifications::default());
        let handler = AgentAssignmentNotificationOutboxHandler::new(Arc::clone(&notifications));
        let mut event = notification_event();
        event.payload["assignment_id"] = serde_json::json!(Uuid::from_u128(999));
        assert!(matches!(
            handler.handle(&event).await,
            Err(WorkerError::Permanent(_))
        ));
        assert!(notifications.0.lock().unwrap().is_empty());
    }
}
