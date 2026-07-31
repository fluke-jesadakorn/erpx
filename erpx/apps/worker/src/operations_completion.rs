//! G7 Operations completion worker.
//!
//! Integration seam for the worker composition root. The existing registered
//! Operations projector remains the only component that mutates the read
//! model; this pass performs bounded outbox repair, applies authority proofs
//! supplied by the control-plane evaluator, then drains the real projector and
//! returns reconciliation evidence. It deliberately does not resolve roles or
//! invent a local authorization fallback.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_domain::{
    access::Scope,
    contracts::{ActorAttribution, TenantScope},
    operations::completion::{
        OPERATIONS_PROJECTION_FRESHNESS_SECONDS, OperationsAuthorizationProof,
        OperationsDetectionPolicy, OperationsDetectionRequest,
        OperationsExpiredPrincipalReassignment, OperationsManagerChainProof,
        OperationsSlaEscalationRequest, OperationsWorkflowAction,
    },
    operations::{
        ExceptionCase, ExceptionPrincipalReference, ExceptionStatus, OperationsSourceDomain,
    },
};
use erpx_infrastructure::{
    FolioOperationsBaseline, MAX_OPERATIONS_EXCEPTION_PAGE_SIZE,
    MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE, OPERATIONS_READ_PROJECTION, OperationsEventCursor,
    OperationsExceptionCursor, OperationsFolioReconciliation, OperationsReadRow,
    OperationsSourceEventCursor, OperationsSourceOutboxCursor, RoutedDatabaseSide,
    SqlxOperationsStore, TenantDatabaseRouter,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{OperationsProjectionRequest, OperationsProjectionRunner};

#[path = "operations_reconciliation_runtime.rs"]
pub(crate) mod reconciliation_runtime;

pub const OPERATIONS_COMPLETION_HANDLER_NAME: &str = "operations_completion";
pub const MAX_OPERATIONS_COMPLETION_PAGES: u32 = 1_000;
pub const MAX_OPERATIONS_COMPLETION_ATTEMPTS: u32 = 5;
pub const MAX_OPERATIONS_DETECTION_RULES: usize = 100;
pub const MAX_OPERATIONS_DETECTION_SOURCE_TYPES_PER_RULE: usize = 64;
pub const MAX_OPERATIONS_REASSIGNMENT_RULES: usize = 100;
pub const MAX_OPERATIONS_ESCALATION_RULES: usize = 100;
const OPERATIONS_COMPLETION_RETRY_BASE_SECONDS: i64 = 2;
const OPERATIONS_COMPLETION_RETRY_MAX_SECONDS: i64 = 60;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationsCompletionWorkerError {
    #[error("Operations completion configuration error: {0}")]
    Configuration(String),
    #[error("Operations completion pass failed: {operation}")]
    Failed {
        operation: &'static str,
        attempts: u32,
        retryable: bool,
        dead_lettered: bool,
    },
}

impl OperationsCompletionWorkerError {
    fn retryable(operation: &'static str) -> Self {
        Self::Failed {
            operation,
            attempts: 1,
            retryable: true,
            dead_lettered: false,
        }
    }

    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                retryable: true,
                ..
            }
        )
    }

    fn dead_letter(self, attempts: u32) -> Self {
        match self {
            Self::Failed { operation, .. } => Self::Failed {
                operation,
                attempts,
                retryable: false,
                dead_lettered: true,
            },
            configuration => configuration,
        }
    }
}

fn valid_operations_reference(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && value.chars().all(|character| {
            !character.is_control() && !matches!(character, '/' | '\\' | ':' | ' ')
        })
}

fn valid_operations_idempotency_key(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && value
            .chars()
            .all(|character| !character.is_control() && !matches!(character, '/' | '\\' | ' '))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsDetectionScheduleRule {
    pub source_domain: OperationsSourceDomain,
    pub source_types: BTreeSet<String>,
    pub policy: OperationsDetectionPolicy,
}

impl OperationsDetectionScheduleRule {
    fn validate(&self) -> Result<(), OperationsCompletionWorkerError> {
        if self.source_types.is_empty()
            || self.source_types.len() > MAX_OPERATIONS_DETECTION_SOURCE_TYPES_PER_RULE
            || !self.policy.source_domains.contains(&self.source_domain)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule rule is invalid".into(),
            ));
        }
        self.policy.validate().map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule policy is invalid".into(),
            )
        })?;
        if self
            .source_types
            .iter()
            .any(|source_type| !valid_operations_reference(source_type))
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule source type is invalid".into(),
            ));
        }
        Ok(())
    }
}

/// Inputs supplied by the authoritative policy/authorization evaluator for a
/// scheduled Operations sweep. The worker selects only matching source facts
/// and derives stable request identities; it never resolves roles, invents an
/// assignee, or broadens a manager proof from local state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsDetectionSchedule {
    pub rules: Vec<OperationsDetectionScheduleRule>,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub assignee: ExceptionPrincipalReference,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
    pub at: DateTime<Utc>,
}

impl OperationsDetectionSchedule {
    fn validate(
        &self,
        parent_authorization: &OperationsAuthorizationProof,
        requested_scope: &Scope,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionWorkerError> {
        if self.at != at
            || self.rules.is_empty()
            || self.rules.len() > MAX_OPERATIONS_DETECTION_RULES
            || self.authorization.principal_ref != parent_authorization.principal_ref
            || self.actor_attribution.principal_ref != self.authorization.principal_ref
            || !requested_scope.contains(&self.authorization.scope)
            || !requested_scope.contains(&self.manager_chain.scope)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule authority is invalid".into(),
            ));
        }
        self.authorization
            .validate_for(
                OperationsWorkflowAction::DetectException,
                requested_scope,
                at,
            )
            .map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule authorization is invalid".into(),
                )
            })?;
        self.assignee.validate(at, "assignee").map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule assignee is invalid".into(),
            )
        })?;
        self.manager.validate(at, "manager").map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule manager is invalid".into(),
            )
        })?;
        if self.manager_chain.subject_principal_ref != self.assignee.principal_ref
            || self.manager_chain.manager_principal_ref != self.manager.principal_ref
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule manager proof does not match".into(),
            ));
        }
        self.manager_chain.validate(at).map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule manager chain is invalid".into(),
            )
        })?;
        let mut unique_rules = BTreeSet::new();
        for rule in &self.rules {
            rule.validate()?;
            for source_type in &rule.source_types {
                // The source-reference key is globally unique per source
                // event, so one source fact cannot fan out into multiple
                // policy-specific Operations aggregates in this scheduler.
                if !unique_rules.insert((rule.source_domain, source_type.clone())) {
                    return Err(OperationsCompletionWorkerError::Configuration(
                        "Operations detection schedule contains duplicate rules".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn request_for(
        &self,
        source_event: erpx_domain::operations::OperationsSourceEvent,
        rule: &OperationsDetectionScheduleRule,
    ) -> Result<OperationsDetectionRequest, OperationsCompletionWorkerError> {
        let idempotency_key = format!(
            "operations-detection:{}:{}",
            source_event.source_event_id, rule.policy.policy_ref
        );
        if !valid_operations_idempotency_key(&idempotency_key) {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule idempotency key is invalid".into(),
            ));
        }
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{idempotency_key}:correlation").as_bytes(),
        );
        Ok(OperationsDetectionRequest {
            source_event,
            exception_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes()),
            policy: rule.policy.clone(),
            authorization: self.authorization.clone(),
            actor_attribution: self.actor_attribution.clone(),
            assignee: self.assignee.clone(),
            manager: self.manager.clone(),
            manager_chain: self.manager_chain.clone(),
            idempotency_key,
            correlation_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsReassignmentScheduleRule {
    pub expired_principal_ref: String,
    pub replacement: ExceptionPrincipalReference,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
}

impl OperationsReassignmentScheduleRule {
    fn validate(&self, at: DateTime<Utc>) -> Result<(), OperationsCompletionWorkerError> {
        if !valid_operations_reference(&self.expired_principal_ref)
            || self.replacement.validate(at, "replacement").is_err()
            || self.manager.validate(at, "manager").is_err()
            || self.manager_chain.validate(at).is_err()
            || self.expired_principal_ref == self.replacement.principal_ref
            || self.manager_chain.subject_principal_ref != self.replacement.principal_ref
            || self.manager_chain.manager_principal_ref != self.manager.principal_ref
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations reassignment schedule rule is invalid".into(),
            ));
        }
        Ok(())
    }
}

/// Inputs supplied by the authoritative identity/role evaluator for expired
/// assignees. The worker reads only persisted active exceptions and matches
/// these explicit rules; it never discovers expired users or invents a
/// replacement/manager chain from the tenant database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsReassignmentSchedule {
    pub rules: Vec<OperationsReassignmentScheduleRule>,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub at: DateTime<Utc>,
}

impl OperationsReassignmentSchedule {
    fn validate(
        &self,
        parent_authorization: &OperationsAuthorizationProof,
        requested_scope: &Scope,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionWorkerError> {
        if self.at != at
            || self.rules.is_empty()
            || self.rules.len() > MAX_OPERATIONS_REASSIGNMENT_RULES
            || self.authorization.principal_ref != parent_authorization.principal_ref
            || self.actor_attribution.principal_ref != self.authorization.principal_ref
            || !requested_scope.contains(&self.authorization.scope)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations reassignment schedule authority is invalid".into(),
            ));
        }
        self.authorization
            .validate_for(
                OperationsWorkflowAction::ReassignExpiredPrincipal,
                requested_scope,
                at,
            )
            .map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment schedule authorization is invalid".into(),
                )
            })?;
        let mut expired_principals = BTreeSet::new();
        for rule in &self.rules {
            rule.validate(at)?;
            if !expired_principals.insert(rule.expired_principal_ref.clone()) {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment schedule contains duplicate expired principals".into(),
                ));
            }
            if !requested_scope.contains(&rule.manager_chain.scope) {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment schedule manager scope is outside the pass".into(),
                ));
            }
        }
        Ok(())
    }

    fn request_for(
        &self,
        exception: &OperationsReadRow<ExceptionCase>,
        rule: &OperationsReassignmentScheduleRule,
    ) -> Result<Option<OperationsExpiredPrincipalReassignment>, OperationsCompletionWorkerError>
    {
        if exception.value.assigned_to.as_deref() != Some(rule.expired_principal_ref.as_str()) {
            return Ok(None);
        }
        if !self
            .authorization
            .scope
            .contains(&exception.value.meta.scope)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations reassignment exception escaped the authenticated scope".into(),
            ));
        }
        let idempotency_key = format!(
            "operations-reassignment:{}:{}:{}:{}",
            exception.aggregate_id,
            exception.aggregate_version,
            rule.expired_principal_ref,
            rule.replacement.principal_ref
        );
        if !valid_operations_idempotency_key(&idempotency_key) {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations reassignment schedule idempotency key is invalid".into(),
            ));
        }
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{idempotency_key}:correlation").as_bytes(),
        );
        let request = OperationsExpiredPrincipalReassignment {
            exception_id: exception.aggregate_id,
            expected_version: exception.aggregate_version,
            expired_principal_ref: rule.expired_principal_ref.clone(),
            replacement: rule.replacement.clone(),
            manager: rule.manager.clone(),
            manager_chain: rule.manager_chain.clone(),
            authorization: self.authorization.clone(),
            actor_attribution: self.actor_attribution.clone(),
            at: self.at,
            idempotency_key,
            correlation_id,
        };
        request.validate(&exception.value.meta.scope).map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations reassignment schedule request is invalid".into(),
            )
        })?;
        Ok(Some(request))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsEscalationScheduleRule {
    pub assigned_principal_ref: String,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
}

impl OperationsEscalationScheduleRule {
    fn validate(&self, at: DateTime<Utc>) -> Result<(), OperationsCompletionWorkerError> {
        if !valid_operations_reference(&self.assigned_principal_ref)
            || self.manager.validate(at, "manager").is_err()
            || self.manager_chain.validate(at).is_err()
            || self.manager_chain.subject_principal_ref != self.assigned_principal_ref
            || self.manager_chain.manager_principal_ref != self.manager.principal_ref
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations escalation schedule rule is invalid".into(),
            ));
        }
        Ok(())
    }
}

/// Inputs supplied by the authoritative SLA/manager evaluator for a bounded
/// escalation sweep. Due-time evaluation is performed against the persisted
/// active-exception projection, while manager authority and MFA remain
/// explicit proof inputs and are never resolved from tenant rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsEscalationSchedule {
    pub rules: Vec<OperationsEscalationScheduleRule>,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub at: DateTime<Utc>,
}

impl OperationsEscalationSchedule {
    fn validate(
        &self,
        parent_authorization: &OperationsAuthorizationProof,
        requested_scope: &Scope,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionWorkerError> {
        if self.at != at
            || self.rules.is_empty()
            || self.rules.len() > MAX_OPERATIONS_ESCALATION_RULES
            || self.authorization.principal_ref != parent_authorization.principal_ref
            || self.actor_attribution.principal_ref != self.authorization.principal_ref
            || !requested_scope.contains(&self.authorization.scope)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations escalation schedule authority is invalid".into(),
            ));
        }
        self.authorization
            .validate_for(OperationsWorkflowAction::EscalateSla, requested_scope, at)
            .map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations escalation schedule authorization is invalid".into(),
                )
            })?;
        let mut assigned_principals = BTreeSet::new();
        for rule in &self.rules {
            rule.validate(at)?;
            if !assigned_principals.insert(rule.assigned_principal_ref.clone()) {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations escalation schedule contains duplicate assignees".into(),
                ));
            }
            if !requested_scope.contains(&rule.manager_chain.scope) {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations escalation schedule manager scope is outside the pass".into(),
                ));
            }
        }
        Ok(())
    }

    fn request_for(
        &self,
        exception: &OperationsReadRow<ExceptionCase>,
        rule: &OperationsEscalationScheduleRule,
    ) -> Result<Option<OperationsSlaEscalationRequest>, OperationsCompletionWorkerError> {
        if exception.value.status == ExceptionStatus::Escalated
            || exception.value.assigned_to.as_deref() != Some(rule.assigned_principal_ref.as_str())
            || exception.value.due_at.is_none_or(|due_at| due_at > self.at)
        {
            return Ok(None);
        }
        if !self
            .authorization
            .scope
            .contains(&exception.value.meta.scope)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations escalation exception escaped the authenticated scope".into(),
            ));
        }
        let due_at = exception.value.due_at.ok_or_else(|| {
            OperationsCompletionWorkerError::Configuration(
                "Operations escalation exception has no persisted due time".into(),
            )
        })?;
        let idempotency_key = format!(
            "operations-escalation:{}:{}:{}",
            exception.aggregate_id,
            exception.aggregate_version,
            due_at.to_rfc3339()
        );
        if !valid_operations_idempotency_key(&idempotency_key) {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations escalation schedule idempotency key is invalid".into(),
            ));
        }
        let request = OperationsSlaEscalationRequest {
            exception_id: exception.aggregate_id,
            expected_version: exception.aggregate_version,
            manager: rule.manager.clone(),
            manager_chain: rule.manager_chain.clone(),
            authorization: self.authorization.clone(),
            actor_attribution: self.actor_attribution.clone(),
            at: self.at,
            correlation_id: Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("{idempotency_key}:correlation").as_bytes(),
            ),
            idempotency_key,
        };
        request.validate(&exception.value).map_err(|_| {
            OperationsCompletionWorkerError::Configuration(
                "Operations escalation schedule request is invalid".into(),
            )
        })?;
        Ok(Some(request))
    }
}

fn access_scope(tenant_scope: &TenantScope) -> Scope {
    match tenant_scope {
        TenantScope::Platform => Scope::Platform,
        TenantScope::Holding { holding_id } => Scope::Holding {
            holding_id: *holding_id,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree {
            holding_id: *holding_id,
            company_ids: company_ids.clone(),
            branch_ids: branch_ids.clone(),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_nested_authorization(
    parent: &OperationsAuthorizationProof,
    scope: &Scope,
    at: DateTime<Utc>,
    nested_at: DateTime<Utc>,
    nested: &OperationsAuthorizationProof,
    actor_principal_ref: &str,
    manager_scope: &Scope,
    action: OperationsWorkflowAction,
) -> Result<(), OperationsCompletionWorkerError> {
    if nested_at != at
        || nested.principal_ref != parent.principal_ref
        || actor_principal_ref != nested.principal_ref
        || manager_scope != scope
    {
        return Err(OperationsCompletionWorkerError::Configuration(
            "Operations completion nested authorization is outside the pass scope".into(),
        ));
    }
    nested.validate_for(action, scope, at).map_err(|_| {
        OperationsCompletionWorkerError::Configuration(
            "Operations completion nested authorization is invalid".into(),
        )
    })
}

fn validate_detection_authorization(
    parent: &OperationsAuthorizationProof,
    pass_scope: &Scope,
    at: DateTime<Utc>,
    request: &OperationsDetectionRequest,
) -> Result<(), OperationsCompletionWorkerError> {
    if parent.principal_ref != request.authorization.principal_ref
        || request.actor_attribution.principal_ref != parent.principal_ref
        || !pass_scope.contains(&request.source_event.scope)
        || !pass_scope.contains(&request.authorization.scope)
        || !pass_scope.contains(&request.manager_chain.scope)
    {
        return Err(OperationsCompletionWorkerError::Configuration(
            "Operations detection authorization is outside the pass scope".into(),
        ));
    }
    request.validate_at(at).map_err(|_| {
        OperationsCompletionWorkerError::Configuration(
            "Operations detection request authorization is invalid".into(),
        )
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsCompletionPass {
    pub tenant_id: String,
    pub holding_ref: String,
    pub scope: Scope,
    pub authorization: OperationsAuthorizationProof,
    pub at: DateTime<Utc>,
    pub worker_id: Uuid,
    pub outbox_repair_limit: u32,
    pub projection_page_size: u32,
    pub max_projection_pages: u32,
    #[serde(default)]
    pub detections: Vec<OperationsDetectionRequest>,
    #[serde(default)]
    pub detection_schedule: Option<OperationsDetectionSchedule>,
    #[serde(default)]
    pub reassignment_schedule: Option<OperationsReassignmentSchedule>,
    #[serde(default)]
    pub escalation_schedule: Option<OperationsEscalationSchedule>,
    pub reassignments: Vec<OperationsExpiredPrincipalReassignment>,
    pub escalations: Vec<OperationsSlaEscalationRequest>,
    /// Optional PII-free evidence emitted by the read-only Folio bridge. When
    /// present, completion must prove parity before reporting a healthy pass.
    #[serde(default)]
    pub folio_operations_baseline: Option<FolioOperationsBaseline>,
}

impl OperationsCompletionPass {
    pub(crate) fn validate_shape(&self) -> Result<(), OperationsCompletionWorkerError> {
        if !valid_operations_reference(&self.tenant_id)
            || !valid_operations_reference(&self.holding_ref)
            || self.worker_id.is_nil()
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "tenant, holding, and worker identity are required".into(),
            ));
        }
        if !matches!(self.scope, Scope::Holding { .. }) {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations completion requires an explicit holding scope".into(),
            ));
        }
        if self.outbox_repair_limit == 0
            || self.projection_page_size == 0
            || self.max_projection_pages == 0
            || self.max_projection_pages > MAX_OPERATIONS_COMPLETION_PAGES
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations completion bounds are invalid".into(),
            ));
        }
        if self.outbox_repair_limit > MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations outbox repair limit exceeds the authoritative SQLx page bound".into(),
            ));
        }
        if self.reassignments.len()
            > usize::try_from(self.outbox_repair_limit).unwrap_or(usize::MAX)
            || self.detections.len()
                > usize::try_from(self.outbox_repair_limit).unwrap_or(usize::MAX)
            || self.escalations.len()
                > usize::try_from(self.outbox_repair_limit).unwrap_or(usize::MAX)
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations completion lifecycle batch is unbounded".into(),
            ));
        }
        if let Some(baseline) = &self.folio_operations_baseline {
            baseline
                .validate_for_holding(&self.holding_ref)
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                    "Folio Operations reconciliation baseline must be bound to the pass holding"
                        .into(),
                )
                })?;
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), OperationsCompletionWorkerError> {
        self.validate_shape()?;
        self.authorization
            .validate_for(OperationsWorkflowAction::Reconcile, &self.scope, self.at)
            .map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations completion authorization is invalid".into(),
                )
            })?;
        for request in &self.detections {
            validate_detection_authorization(&self.authorization, &self.scope, self.at, request)?;
        }
        if let Some(schedule) = &self.detection_schedule {
            schedule.validate(&self.authorization, &self.scope, self.at)?;
        }
        if let Some(schedule) = &self.reassignment_schedule {
            schedule.validate(&self.authorization, &self.scope, self.at)?;
        }
        if let Some(schedule) = &self.escalation_schedule {
            schedule.validate(&self.authorization, &self.scope, self.at)?;
        }
        for request in &self.reassignments {
            validate_nested_authorization(
                &self.authorization,
                &self.scope,
                self.at,
                request.at,
                &request.authorization,
                &request.actor_attribution.principal_ref,
                &request.manager_chain.scope,
                OperationsWorkflowAction::ReassignExpiredPrincipal,
            )?;
        }
        for request in &self.escalations {
            validate_nested_authorization(
                &self.authorization,
                &self.scope,
                self.at,
                request.at,
                &request.authorization,
                &request.actor_attribution.principal_ref,
                &request.manager_chain.scope,
                OperationsWorkflowAction::EscalateSla,
            )?;
        }
        Ok(())
    }

    /// A serialized pass is a short-lived execution capability, not a
    /// durable command authorization. Bounding its scheduler timestamp to the
    /// same five-minute contract used by Operations freshness prevents a
    /// caller from replaying an old, otherwise still-valid proof against a
    /// current write database.
    fn validate_execution_time(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionWorkerError> {
        let window = chrono::Duration::seconds(OPERATIONS_PROJECTION_FRESHNESS_SECONDS);
        if self.at < now - window || self.at > now + window {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations completion pass timestamp is outside the five-minute execution window"
                    .into(),
            ));
        }
        Ok(())
    }

    fn validate_current_authorization(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionWorkerError> {
        self.authorization
            .validate_for(OperationsWorkflowAction::Reconcile, &self.scope, now)
            .map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations completion authorization is not valid at worker execution time"
                        .into(),
                )
            })?;
        for request in &self.detections {
            request.validate_at(now).map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations detection proof is not valid at worker execution time".into(),
                )
            })?;
        }
        if let Some(schedule) = &self.detection_schedule {
            schedule
                .authorization
                .validate_for(OperationsWorkflowAction::DetectException, &self.scope, now)
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations detection schedule authorization is not valid at worker execution time"
                            .into(),
                    )
                })?;
            schedule.assignee.validate(now, "assignee").map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule assignee is not valid at worker execution time"
                        .into(),
                )
            })?;
            schedule.manager.validate(now, "manager").map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule manager is not valid at worker execution time"
                        .into(),
                )
            })?;
            schedule.manager_chain.validate(now).map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule manager proof is not valid at worker execution time"
                        .into(),
                )
            })?;
        }
        if let Some(schedule) = &self.reassignment_schedule {
            schedule
                .authorization
                .validate_for(
                    OperationsWorkflowAction::ReassignExpiredPrincipal,
                    &self.scope,
                    now,
                )
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment schedule authorization is not valid at worker execution time"
                            .into(),
                    )
                })?;
            for rule in &schedule.rules {
                rule.replacement.validate(now, "replacement").map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment schedule replacement is not valid at worker execution time"
                            .into(),
                    )
                })?;
                rule.manager.validate(now, "manager").map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment schedule manager is not valid at worker execution time"
                            .into(),
                    )
                })?;
                rule.manager_chain.validate(now).map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment schedule manager proof is not valid at worker execution time"
                            .into(),
                    )
                })?;
            }
        }
        if let Some(schedule) = &self.escalation_schedule {
            schedule
                .authorization
                .validate_for(OperationsWorkflowAction::EscalateSla, &self.scope, now)
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations escalation schedule authorization is not valid at worker execution time"
                            .into(),
                    )
                })?;
            for rule in &schedule.rules {
                rule.manager.validate(now, "manager").map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations escalation schedule manager is not valid at worker execution time"
                            .into(),
                    )
                })?;
                rule.manager_chain.validate(now).map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations escalation schedule manager proof is not valid at worker execution time"
                            .into(),
                    )
                })?;
            }
        }
        for request in &self.reassignments {
            request
                .authorization
                .validate_for(
                    OperationsWorkflowAction::ReassignExpiredPrincipal,
                    &self.scope,
                    now,
                )
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment authorization is not valid at worker execution time"
                            .into(),
                    )
                })?;
            request
                .replacement
                .validate(now, "replacement")
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment replacement is not valid at worker execution time"
                            .into(),
                    )
                })?;
            request.manager.validate(now, "manager").map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment manager is not valid at worker execution time".into(),
                )
            })?;
            request.manager_chain.validate(now).map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment manager proof is not valid at worker execution time"
                        .into(),
                )
            })?;
        }
        for request in &self.escalations {
            request
                .authorization
                .validate_for(OperationsWorkflowAction::EscalateSla, &self.scope, now)
                .map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations escalation authorization is not valid at worker execution time"
                            .into(),
                    )
                })?;
            request.manager.validate(now, "manager").map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations escalation manager is not valid at worker execution time".into(),
                )
            })?;
            request.manager_chain.validate(now).map_err(|_| {
                OperationsCompletionWorkerError::Configuration(
                    "Operations escalation manager proof is not valid at worker execution time"
                        .into(),
                )
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsCompletionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    #[serde(default)]
    pub detections_applied: u64,
    #[serde(default)]
    pub detections_scheduled: u64,
    #[serde(default)]
    pub reassignments_scheduled: u64,
    #[serde(default)]
    pub escalations_scheduled: u64,
    pub outbox_rows_repaired: u64,
    pub reassignments_applied: u64,
    pub escalations_applied: u64,
    pub projection_pages: u64,
    pub projection_events_seen: u64,
    pub reconciliation: OperationsCompletionReconciliationSummary,
    #[serde(default)]
    pub retry_attempts: u32,
    #[serde(default)]
    pub dead_lettered: bool,
    #[serde(default)]
    pub rollback_required: bool,
    #[serde(default)]
    pub rollback_performed: bool,
    #[serde(default)]
    pub rebuild_projection: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsCompletionReconciliationSummary {
    pub healthy: bool,
    pub missing_outbox_events: u64,
    pub stale_aggregate_count: u64,
    pub projection_lag_seconds: Option<i64>,
    #[serde(default)]
    pub source_references_checked: u64,
    #[serde(default)]
    pub source_event_links_checked: u64,
    #[serde(default)]
    pub source_event_link_gaps: u64,
    #[serde(default)]
    pub source_scope_violations: u64,
    #[serde(default)]
    pub source_stream_moved: bool,
    #[serde(default)]
    pub source_event_reconciliation_healthy: bool,
    /// Number of source-context outbox envelopes validated during this pass.
    /// The feed is source-owned; this counter proves bounded inspection, not
    /// that an Operations exception was invented for every source fact.
    #[serde(default)]
    pub source_outbox_events_checked: u64,
    #[serde(default)]
    pub source_outbox_scope_violations: u64,
    #[serde(default)]
    pub folio_reconciliation: Option<OperationsFolioReconciliation>,
}

#[derive(Debug, Clone)]
pub struct OperationsCompletionWorker {
    router: TenantDatabaseRouter,
    projector: OperationsProjectionRunner,
}

impl OperationsCompletionWorker {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self {
            projector: OperationsProjectionRunner::new(router.clone()),
            router,
        }
    }

    pub async fn run_once(
        &self,
        pass: OperationsCompletionPass,
    ) -> Result<OperationsCompletionRunReport, OperationsCompletionWorkerError> {
        let mut attempts = 0_u32;
        loop {
            attempts = attempts.saturating_add(1);
            match self.run_once_attempt(pass.clone()).await {
                Ok(mut report) => {
                    report.retry_attempts = attempts.saturating_sub(1);
                    return Ok(report);
                }
                Err(error)
                    if error.is_retryable() && attempts < MAX_OPERATIONS_COMPLETION_ATTEMPTS =>
                {
                    tokio::time::sleep(retry_delay(attempts)).await;
                }
                Err(error) if error.is_retryable() => {
                    return Err(error.dead_letter(attempts));
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn run_once_attempt(
        &self,
        pass: OperationsCompletionPass,
    ) -> Result<OperationsCompletionRunReport, OperationsCompletionWorkerError> {
        pass.validate()?;
        let now = Utc::now();
        pass.validate_execution_time(now)?;
        pass.validate_current_authorization(now)?;
        let write = self
            .router
            .route(
                &pass.tenant_id,
                &pass.holding_ref,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| OperationsCompletionWorkerError::retryable("write route"))?;
        let read = match self
            .router
            .route(&pass.tenant_id, &pass.holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(route) => route,
            Err(_) => {
                write.close().await;
                return Err(OperationsCompletionWorkerError::retryable("read route"));
            }
        };
        let result = async {
            let store = SqlxOperationsStore::from_pools(
                write.database_pool(),
                read.database_pool(),
                pass.holding_ref.clone(),
            )
            .map_err(|_| OperationsCompletionWorkerError::retryable("store construction"))?;
            let (detections_scheduled, scheduled_detections_applied) =
                if let Some(schedule) = &pass.detection_schedule {
                    Self::apply_scheduled_detections(
                        &store,
                        &pass.scope,
                        schedule,
                        source_reconciliation_page_size(pass.outbox_repair_limit),
                        pass.max_projection_pages,
                    )
                    .await?
                } else {
                    (0, 0)
                };
            let mut outbox_rows_repaired = store
                .repair_missing_outbox_events(
                    &pass.authorization,
                    &pass.scope,
                    pass.at,
                    pass.outbox_repair_limit,
                )
                .await
                .map_err(|_| OperationsCompletionWorkerError::retryable("outbox repair"))?;
            let notification_handoffs_repaired = store
                .repair_missing_sla_notification_handoff_events(
                    &pass.authorization,
                    &pass.scope,
                    pass.at,
                    &pass.authorization.principal_ref,
                    pass.outbox_repair_limit,
                )
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable("notification handoff repair")
                })?;
            outbox_rows_repaired = outbox_rows_repaired
                .checked_add(notification_handoffs_repaired)
                .ok_or_else(|| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations outbox repair count overflow".into(),
                    )
                })?;
            let mut detections_applied = scheduled_detections_applied;
            for request in &pass.detections {
                let result = store
                    .detect_exception_at(request, pass.at)
                    .await
                    .map_err(|_| {
                        OperationsCompletionWorkerError::retryable("Operations detection")
                    })?;
                if result.ingestion.ingestion.applied || result.assignment.applied {
                    detections_applied = detections_applied.saturating_add(1);
                }
            }
            let mut reassignments_applied = 0_u64;
            for request in &pass.reassignments {
                let result = store
                    .reassign_expired_principal(request)
                    .await
                    .map_err(|_| {
                        OperationsCompletionWorkerError::retryable("expired-principal reassignment")
                    })?;
                if result.lifecycle.applied {
                    reassignments_applied = reassignments_applied.saturating_add(1);
                }
            }
            let mut escalations_applied = 0_u64;
            for request in &pass.escalations {
                let result = store
                    .escalate_due_exception(request)
                    .await
                    .map_err(|_| OperationsCompletionWorkerError::retryable("SLA escalation"))?;
                if result.lifecycle.applied {
                    escalations_applied = escalations_applied.saturating_add(1);
                }
            }

            let mut projection_pages = 0_u64;
            let mut projection_events_seen = 0_u64;
            self.project_operations_pages(
                &pass,
                None,
                &mut projection_pages,
                &mut projection_events_seen,
            )
            .await?;
            let scheduled_reassignments = if let Some(schedule) = &pass.reassignment_schedule {
                Self::schedule_reassignments(
                    &store,
                    &pass.scope,
                    schedule,
                    pass.projection_page_size
                        .min(MAX_OPERATIONS_EXCEPTION_PAGE_SIZE),
                    pass.max_projection_pages,
                )
                .await?
            } else {
                Vec::new()
            };
            let reassignments_scheduled =
                u64::try_from(scheduled_reassignments.len()).map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations reassignment schedule count is outside platform range".into(),
                    )
                })?;
            for request in &scheduled_reassignments {
                let result = store
                    .reassign_expired_principal(request)
                    .await
                    .map_err(|_| {
                        OperationsCompletionWorkerError::retryable(
                            "scheduled expired-principal reassignment",
                        )
                    })?;
                if result.lifecycle.applied {
                    reassignments_applied = reassignments_applied.saturating_add(1);
                }
            }
            if !scheduled_reassignments.is_empty() {
                self.project_operations_pages(
                    &pass,
                    None,
                    &mut projection_pages,
                    &mut projection_events_seen,
                )
                .await?;
            }
            let scheduled_escalations = if let Some(schedule) = &pass.escalation_schedule {
                Self::schedule_escalations(
                    &store,
                    &pass.scope,
                    schedule,
                    pass.projection_page_size
                        .min(MAX_OPERATIONS_EXCEPTION_PAGE_SIZE),
                    pass.max_projection_pages,
                )
                .await?
            } else {
                Vec::new()
            };
            let escalations_scheduled =
                u64::try_from(scheduled_escalations.len()).map_err(|_| {
                    OperationsCompletionWorkerError::Configuration(
                        "Operations escalation schedule count is outside platform range".into(),
                    )
                })?;
            for request in &scheduled_escalations {
                let result = store.escalate_due_exception(request).await.map_err(|_| {
                    OperationsCompletionWorkerError::retryable(
                        "scheduled Operations SLA escalation",
                    )
                })?;
                if result.lifecycle.applied {
                    escalations_applied = escalations_applied.saturating_add(1);
                }
            }
            if !scheduled_escalations.is_empty() {
                self.project_operations_pages(
                    &pass,
                    None,
                    &mut projection_pages,
                    &mut projection_events_seen,
                )
                .await?;
            }
            let source_reconciliation = Self::reconcile_source_events(
                &store,
                &pass.scope,
                source_reconciliation_page_size(pass.projection_page_size),
                pass.max_projection_pages,
            )
            .await?;
            let reconciliation = store
                .reconciliation_report(&pass.authorization, &pass.scope, pass.at)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable("Operations reconciliation")
                })?;
            let folio_reconciliation = pass
                .folio_operations_baseline
                .as_ref()
                .map(|baseline| reconciliation.reconcile_folio_baseline(baseline))
                .transpose()
                .map_err(|_| OperationsCompletionWorkerError::retryable("Folio reconciliation"))?;
            let reconciliation = OperationsCompletionReconciliationSummary {
                healthy: reconciliation.healthy
                    && source_reconciliation.healthy
                    && folio_reconciliation
                        .as_ref()
                        .is_none_or(|evidence| evidence.healthy),
                missing_outbox_events: reconciliation.missing_outbox_events,
                stale_aggregate_count: reconciliation.stale_aggregate_count,
                projection_lag_seconds: reconciliation.projection_lag_seconds,
                source_references_checked: source_reconciliation.source_references_checked,
                source_event_links_checked: source_reconciliation.source_event_links_checked,
                source_event_link_gaps: source_reconciliation.source_event_link_gaps,
                source_scope_violations: source_reconciliation.source_scope_violations,
                source_stream_moved: source_reconciliation.source_stream_moved,
                source_event_reconciliation_healthy: source_reconciliation
                    .source_event_reconciliation_healthy,
                source_outbox_events_checked: source_reconciliation.source_outbox_events_checked,
                source_outbox_scope_violations: source_reconciliation
                    .source_outbox_scope_violations,
                folio_reconciliation,
            };
            let rollback_required = !reconciliation.healthy;
            Ok(OperationsCompletionRunReport {
                tenant_id: pass.tenant_id.clone(),
                holding_ref: pass.holding_ref.clone(),
                detections_applied,
                detections_scheduled,
                reassignments_scheduled,
                escalations_scheduled,
                outbox_rows_repaired,
                reassignments_applied,
                escalations_applied,
                projection_pages,
                projection_events_seen,
                reconciliation,
                retry_attempts: 0,
                dead_lettered: false,
                rollback_required,
                rollback_performed: false,
                rebuild_projection: erpx_infrastructure::OPERATIONS_READ_PROJECTION.into(),
            })
        }
        .await;
        write.close().await;
        read.close().await;
        result
    }

    async fn project_operations_pages(
        &self,
        pass: &OperationsCompletionPass,
        initial_cursor: Option<OperationsEventCursor>,
        projection_pages: &mut u64,
        projection_events_seen: &mut u64,
    ) -> Result<(), OperationsCompletionWorkerError> {
        let mut cursor = initial_cursor;
        loop {
            if *projection_pages >= u64::from(pass.max_projection_pages) {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations completion projection page budget exhausted".into(),
                ));
            }
            let report = self
                .projector
                .project_page(
                    OperationsProjectionRequest::new(
                        pass.tenant_id.clone(),
                        pass.holding_ref.clone(),
                        cursor,
                        pass.projection_page_size,
                    )
                    .map_err(|_| {
                        OperationsCompletionWorkerError::Configuration(
                            "Operations projection request is invalid".into(),
                        )
                    })?,
                )
                .await
                .map_err(|_| OperationsCompletionWorkerError::retryable("Operations projection"))?;
            *projection_pages = projection_pages.saturating_add(1);
            *projection_events_seen = projection_events_seen.saturating_add(report.events_seen);
            if !report.has_more {
                break;
            }
            cursor = report.next_cursor;
            if cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "Operations projection continuation",
                ));
            }
        }
        Ok(())
    }

    /// Read the bounded source-owned outbox and build only policy-matched,
    /// authorization-bound detection requests. Replaying a page produces the
    /// same exception, correlation, and idempotency identities; persistence
    /// remains the authority for whether the request changes state.
    pub async fn schedule_detections(
        store: &SqlxOperationsStore,
        requested_scope: &Scope,
        schedule: &OperationsDetectionSchedule,
        page_size: u32,
        max_pages: u32,
    ) -> Result<Vec<OperationsDetectionRequest>, OperationsCompletionWorkerError> {
        schedule.validate(&schedule.authorization, requested_scope, schedule.at)?;
        if page_size == 0
            || page_size > MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE
            || max_pages == 0
            || max_pages > MAX_OPERATIONS_COMPLETION_PAGES
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule bounds are invalid".into(),
            ));
        }
        let mut requests = Vec::new();
        let mut cursor: Option<OperationsSourceOutboxCursor> = None;
        let mut pages = 0_u32;
        loop {
            if pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule page budget exhausted".into(),
                ));
            }
            let page = store
                .read_source_outbox_event_page(cursor, page_size)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable("Operations detection source feed")
                })?;
            pages = pages.saturating_add(1);
            for event in page.events {
                let source_event = event.source_event;
                if !requested_scope.contains(&source_event.scope) {
                    return Err(OperationsCompletionWorkerError::Configuration(
                        "Operations detection source event escaped the authenticated scope".into(),
                    ));
                }
                for rule in &schedule.rules {
                    if rule.source_domain == source_event.source_domain
                        && rule.source_types.contains(&source_event.source_type)
                    {
                        let request = schedule.request_for(source_event.clone(), rule)?;
                        validate_detection_authorization(
                            &schedule.authorization,
                            requested_scope,
                            schedule.at,
                            &request,
                        )?;
                        requests.push(request);
                        if requests.len()
                            > erpx_domain::operations::completion::OPERATIONS_MAX_QUEUE_ITEMS
                        {
                            return Err(OperationsCompletionWorkerError::Configuration(
                                "Operations detection schedule queue bound exhausted".into(),
                            ));
                        }
                    }
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "Operations detection source feed continuation",
                ));
            }
        }
        Ok(requests)
    }

    /// Consume the source-owned outbox from its durable high-water mark. Each
    /// page is applied before its cursor is committed; a crash between those
    /// transactions therefore replays idempotent requests instead of skipping
    /// source facts. Unlike the pure request builder above, this method is the
    /// authoritative persistence boundary used by the completion pass.
    pub async fn apply_scheduled_detections(
        store: &SqlxOperationsStore,
        requested_scope: &Scope,
        schedule: &OperationsDetectionSchedule,
        page_size: u32,
        max_pages: u32,
    ) -> Result<(u64, u64), OperationsCompletionWorkerError> {
        schedule.validate(&schedule.authorization, requested_scope, schedule.at)?;
        if page_size == 0
            || page_size > MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE
            || max_pages == 0
            || max_pages > MAX_OPERATIONS_COMPLETION_PAGES
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations detection schedule bounds are invalid".into(),
            ));
        }

        let mut cursor = store.source_detection_cursor().await.map_err(|_| {
            OperationsCompletionWorkerError::retryable("Operations detection cursor")
        })?;
        let mut pages = 0_u32;
        let mut scheduled = 0_u64;
        let mut applied = 0_u64;
        loop {
            if pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations detection schedule page budget exhausted".into(),
                ));
            }
            let page = store
                .read_source_outbox_event_page(cursor, page_size)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable("Operations detection source feed")
                })?;
            pages = pages.saturating_add(1);
            let page_cursor = page
                .events
                .last()
                .map(|event| OperationsSourceOutboxCursor {
                    occurred_at: event.source_event.occurred_at,
                    source_event_id: event.source_event.source_event_id,
                });
            let mut page_scheduled = 0_usize;

            for event in page.events {
                let source_event = event.source_event;
                if !requested_scope.contains(&source_event.scope) {
                    return Err(OperationsCompletionWorkerError::Configuration(
                        "Operations detection source event escaped the authenticated scope".into(),
                    ));
                }
                for rule in &schedule.rules {
                    if rule.source_domain == source_event.source_domain
                        && rule.source_types.contains(&source_event.source_type)
                    {
                        let request = schedule.request_for(source_event.clone(), rule)?;
                        validate_detection_authorization(
                            &schedule.authorization,
                            requested_scope,
                            schedule.at,
                            &request,
                        )?;
                        page_scheduled = page_scheduled.saturating_add(1);
                        if page_scheduled
                            > erpx_domain::operations::completion::OPERATIONS_MAX_QUEUE_ITEMS
                        {
                            return Err(OperationsCompletionWorkerError::Configuration(
                                "Operations detection schedule page queue bound exhausted".into(),
                            ));
                        }
                        scheduled = scheduled.saturating_add(1);
                        let result = store
                            .detect_exception_at(&request, schedule.at)
                            .await
                            .map_err(|_| {
                                OperationsCompletionWorkerError::retryable("Operations detection")
                            })?;
                        if result.ingestion.ingestion.applied || result.assignment.applied {
                            applied = applied.saturating_add(1);
                        }
                    }
                }
            }

            if let Some(page_cursor) = page_cursor {
                store
                    .save_source_detection_cursor(page_cursor)
                    .await
                    .map_err(|_| {
                        OperationsCompletionWorkerError::retryable(
                            "Operations detection cursor checkpoint",
                        )
                    })?;
                cursor = Some(page_cursor);
            }
            if !page.has_more {
                break;
            }
            if cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "Operations detection source feed continuation",
                ));
            }
        }
        Ok((scheduled, applied))
    }

    /// Read the bounded active-exception projection after the current pass has
    /// drained Operations events and build only explicit expired-principal
    /// reassignment requests. The write-side expected version and assignee
    /// guard remain authoritative when the request is applied, so a concurrent
    /// reassignment cannot be reported as a successful scheduler decision.
    pub async fn schedule_reassignments(
        store: &SqlxOperationsStore,
        requested_scope: &Scope,
        schedule: &OperationsReassignmentSchedule,
        page_size: u32,
        max_pages: u32,
    ) -> Result<Vec<OperationsExpiredPrincipalReassignment>, OperationsCompletionWorkerError> {
        schedule.validate(&schedule.authorization, requested_scope, schedule.at)?;
        if page_size == 0
            || page_size > MAX_OPERATIONS_EXCEPTION_PAGE_SIZE
            || max_pages == 0
            || max_pages > MAX_OPERATIONS_COMPLETION_PAGES
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations reassignment schedule bounds are invalid".into(),
            ));
        }
        let mut requests = Vec::new();
        let mut cursor: Option<OperationsExceptionCursor> = None;
        let mut pages = 0_u32;
        loop {
            if pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations reassignment schedule page budget exhausted".into(),
                ));
            }
            let page = store
                .read_active_exception_page(OPERATIONS_READ_PROJECTION, cursor, page_size)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable(
                        "Operations reassignment exception queue",
                    )
                })?;
            pages = pages.saturating_add(1);
            for exception in page.cases {
                for rule in &schedule.rules {
                    if let Some(request) = schedule.request_for(&exception, rule)? {
                        requests.push(request);
                        if requests.len()
                            > erpx_domain::operations::completion::OPERATIONS_MAX_QUEUE_ITEMS
                        {
                            return Err(OperationsCompletionWorkerError::Configuration(
                                "Operations reassignment schedule queue bound exhausted".into(),
                            ));
                        }
                    }
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "Operations reassignment exception queue continuation",
                ));
            }
        }
        Ok(requests)
    }

    /// Read the bounded active-exception projection after the current event
    /// projection pass and build due SLA escalation requests from explicit
    /// manager proofs. The write-side due-time and expected-version checks
    /// remain authoritative when the request is applied.
    pub async fn schedule_escalations(
        store: &SqlxOperationsStore,
        requested_scope: &Scope,
        schedule: &OperationsEscalationSchedule,
        page_size: u32,
        max_pages: u32,
    ) -> Result<Vec<OperationsSlaEscalationRequest>, OperationsCompletionWorkerError> {
        schedule.validate(&schedule.authorization, requested_scope, schedule.at)?;
        if page_size == 0
            || page_size > MAX_OPERATIONS_EXCEPTION_PAGE_SIZE
            || max_pages == 0
            || max_pages > MAX_OPERATIONS_COMPLETION_PAGES
        {
            return Err(OperationsCompletionWorkerError::Configuration(
                "Operations escalation schedule bounds are invalid".into(),
            ));
        }
        let mut requests = Vec::new();
        let mut cursor: Option<OperationsExceptionCursor> = None;
        let mut pages = 0_u32;
        loop {
            if pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "Operations escalation schedule page budget exhausted".into(),
                ));
            }
            let page = store
                .read_active_exception_page(OPERATIONS_READ_PROJECTION, cursor, page_size)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable(
                        "Operations escalation exception queue",
                    )
                })?;
            pages = pages.saturating_add(1);
            for exception in page.cases {
                for rule in &schedule.rules {
                    if let Some(request) = schedule.request_for(&exception, rule)? {
                        requests.push(request);
                        if requests.len()
                            > erpx_domain::operations::completion::OPERATIONS_MAX_QUEUE_ITEMS
                        {
                            return Err(OperationsCompletionWorkerError::Configuration(
                                "Operations escalation schedule queue bound exhausted".into(),
                            ));
                        }
                    }
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "Operations escalation exception queue continuation",
                ));
            }
        }
        Ok(requests)
    }

    /// Reconcile source provenance against the immutable Operations event
    /// stream. A source reference is healthy only when its durable link to the
    /// derived Operations application event is present, points at the same
    /// aggregate/version, and carries the immutable source event as causation.
    /// The linked event scope must remain inside the authenticated holding
    /// scope; a branch or sibling event can never be certified by a pass for
    /// another holding.
    ///
    /// This method is intentionally read-only. It returns an unhealthy typed
    /// report for persisted drift so the caller can request the existing
    /// disposable projection rebuild; it never edits source references or
    /// immutable Operations events as a recovery shortcut.
    pub async fn reconcile_source_events(
        store: &SqlxOperationsStore,
        requested_scope: &Scope,
        page_size: u32,
        max_pages: u32,
    ) -> Result<OperationsCompletionReconciliationSummary, OperationsCompletionWorkerError> {
        if !matches!(requested_scope, Scope::Holding { .. }) {
            return Err(OperationsCompletionWorkerError::Configuration(
                "source reconciliation requires an explicit holding scope".into(),
            ));
        }
        if page_size == 0 || max_pages == 0 || max_pages > MAX_OPERATIONS_COMPLETION_PAGES {
            return Err(OperationsCompletionWorkerError::Configuration(
                "source reconciliation bounds are invalid".into(),
            ));
        }
        if page_size > MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE {
            return Err(OperationsCompletionWorkerError::Configuration(
                "source reconciliation page size exceeds the authoritative SQLx page bound".into(),
            ));
        }

        let mut event_scopes = BTreeMap::new();
        let mut event_cursor: Option<OperationsEventCursor> = None;
        let mut event_pages = 0_u32;
        loop {
            if event_pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "source reconciliation event page budget exhausted".into(),
                ));
            }
            let page = store
                .read_event_page(event_cursor, page_size)
                .await
                .map_err(|_| OperationsCompletionWorkerError::retryable("source event stream"))?;
            event_pages = event_pages.saturating_add(1);
            for event in page.events {
                event_scopes.insert(
                    event.envelope.event_id,
                    (
                        event.envelope.tenant_scope,
                        event.envelope.aggregate_type,
                        event.envelope.aggregate_id,
                        event.envelope.aggregate_version,
                        event.envelope.causation_id,
                    ),
                );
            }
            if !page.has_more {
                break;
            }
            event_cursor = page.next_cursor;
            if event_cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "source event stream continuation",
                ));
            }
        }

        let mut source_cursor: Option<OperationsSourceEventCursor> = None;
        let mut source_pages = 0_u32;
        let mut source_references_checked = 0_u64;
        let mut source_event_links_checked = 0_u64;
        let mut source_event_link_gaps = 0_u64;
        let mut source_scope_violations = 0_u64;
        loop {
            if source_pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "source reconciliation reference page budget exhausted".into(),
                ));
            }
            let page = store
                .read_source_event_page(None, source_cursor, page_size)
                .await
                .map_err(|_| {
                    OperationsCompletionWorkerError::retryable("source reference stream")
                })?;
            source_pages = source_pages.saturating_add(1);
            for reference in page.references {
                source_references_checked = source_references_checked.saturating_add(1);
                let mut link_gap = false;
                let Some(operations_event_id) = reference.operations_event_id else {
                    source_event_link_gaps = source_event_link_gaps.saturating_add(1);
                    continue;
                };
                source_event_links_checked = source_event_links_checked.saturating_add(1);
                match event_scopes.get(&operations_event_id) {
                    Some((
                        event_scope,
                        aggregate_type,
                        aggregate_id,
                        aggregate_version,
                        causation_id,
                    )) => {
                        if aggregate_type != &reference.aggregate_type
                            || *aggregate_id != reference.aggregate_id
                            || *aggregate_version != reference.aggregate_version
                            || *causation_id != Some(reference.source.source_event_id)
                        {
                            link_gap = true;
                        }
                        if !requested_scope.contains(&access_scope(event_scope)) {
                            source_scope_violations = source_scope_violations.saturating_add(1);
                        }
                    }
                    None => link_gap = true,
                }
                if link_gap {
                    source_event_link_gaps = source_event_link_gaps.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            source_cursor = page.next_cursor;
            if source_cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "source reference stream continuation",
                ));
            }
        }

        // Validate the source-owned feed independently from the Operations
        // reference table. This catches an out-of-scope or malformed source
        // envelope before a detection scheduler can turn it into an
        // exception. The page reader validates the complete shared envelope
        // and keeps the payload in the owning context.
        let source_outbox_fence_before = store
            .source_outbox_fence()
            .await
            .map_err(|_| OperationsCompletionWorkerError::retryable("source outbox fence"))?;
        let mut source_outbox_cursor = None;
        let mut source_outbox_pages = 0_u32;
        let mut source_outbox_events_checked = 0_u64;
        let mut source_outbox_scope_violations = 0_u64;
        loop {
            if source_outbox_pages >= max_pages {
                return Err(OperationsCompletionWorkerError::Configuration(
                    "source outbox feed page budget exhausted".into(),
                ));
            }
            let page = store
                .read_source_outbox_event_page(source_outbox_cursor, page_size)
                .await
                .map_err(|_| OperationsCompletionWorkerError::retryable("source outbox feed"))?;
            source_outbox_pages = source_outbox_pages.saturating_add(1);
            for event in page.events {
                source_outbox_events_checked = source_outbox_events_checked.saturating_add(1);
                if !requested_scope.contains(&event.source_event.scope) {
                    source_outbox_scope_violations =
                        source_outbox_scope_violations.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            source_outbox_cursor = page.next_cursor;
            if source_outbox_cursor.is_none() {
                return Err(OperationsCompletionWorkerError::retryable(
                    "source outbox feed continuation",
                ));
            }
        }
        let source_outbox_fence_after = store
            .source_outbox_fence()
            .await
            .map_err(|_| OperationsCompletionWorkerError::retryable("source outbox fence"))?;
        let source_stream_moved =
            source_feed_moved(source_outbox_fence_before, source_outbox_fence_after);
        let source_event_reconciliation_healthy = source_event_link_gaps == 0
            && source_scope_violations == 0
            && source_outbox_scope_violations == 0
            && !source_stream_moved;
        Ok(OperationsCompletionReconciliationSummary {
            healthy: source_event_reconciliation_healthy,
            missing_outbox_events: 0,
            stale_aggregate_count: 0,
            projection_lag_seconds: None,
            source_references_checked,
            source_event_links_checked,
            source_event_link_gaps,
            source_scope_violations,
            source_stream_moved,
            source_event_reconciliation_healthy,
            source_outbox_events_checked,
            source_outbox_scope_violations,
            folio_reconciliation: None,
        })
    }
}

fn source_feed_moved(
    source_outbox_before: (u64, Option<OperationsSourceOutboxCursor>),
    source_outbox_after: (u64, Option<OperationsSourceOutboxCursor>),
) -> bool {
    source_outbox_before != source_outbox_after
}

fn retry_delay(attempt: u32) -> std::time::Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    let multiplier = 1_i64 << exponent;
    let seconds = OPERATIONS_COMPLETION_RETRY_BASE_SECONDS
        .saturating_mul(multiplier)
        .min(OPERATIONS_COMPLETION_RETRY_MAX_SECONDS);
    std::time::Duration::from_secs(u64::try_from(seconds).unwrap_or(1))
}

fn source_reconciliation_page_size(projection_page_size: u32) -> u32 {
    projection_page_size.min(MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Duration;
    use erpx_domain::{
        access::{RiskTier, RoleAssignment},
        contracts::{ActorAttribution, ActorKind},
        operations::ExceptionStatus,
        operations::OperationsSourceEvent,
        operations::completion::OperationsPermission,
    };

    use super::*;

    fn schedule_fixture(at: DateTime<Utc>) -> (OperationsDetectionSchedule, Scope) {
        let holding_id = Uuid::from_u128(0x7801);
        let scope = Scope::Holding { holding_id };
        let manager_assignment_id = Uuid::from_u128(0x7802);
        let manager = RoleAssignment {
            id: manager_assignment_id,
            principal_ref: "manager".into(),
            role_key: "operations_manager".into(),
            permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
            functional_domain: "operations".into(),
            authority_level: 2,
            approval_limit_minor: 0,
            scope: scope.clone(),
            valid_from: at - Duration::minutes(5),
            valid_until: at + Duration::hours(1),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "scheduler test".into(),
            ticket_reference: None,
            grantor: "root".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let assignee = RoleAssignment {
            id: Uuid::from_u128(0x7803),
            principal_ref: "assignee".into(),
            role_key: "operations_assignee".into(),
            permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
            functional_domain: "operations".into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope: scope.clone(),
            valid_from: at - Duration::minutes(5),
            valid_until: at + Duration::hours(1),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "scheduler test".into(),
            ticket_reference: None,
            grantor: "manager".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: Some(manager_assignment_id),
            assignment_version: 1,
        };
        let authorization = OperationsAuthorizationProof {
            principal_ref: "scheduler".into(),
            permission: OperationsPermission::ExceptionManage,
            scope: scope.clone(),
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(10),
            mfa_verified: true,
            break_glass: None,
        };
        let schedule = OperationsDetectionSchedule {
            rules: vec![OperationsDetectionScheduleRule {
                source_domain: OperationsSourceDomain::Sales,
                source_types: BTreeSet::from(["sales.order".into()]),
                policy: OperationsDetectionPolicy {
                    policy_ref: "sales-delivery-breach".into(),
                    source_domains: BTreeSet::from([OperationsSourceDomain::Sales]),
                    category: "delivery_breach".into(),
                    severity: "high".into(),
                    due_after_minutes: 30,
                    escalation_after_minutes: 60,
                },
            }],
            authorization,
            actor_attribution: ActorAttribution {
                principal_ref: "scheduler".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            assignee: ExceptionPrincipalReference {
                principal_ref: "assignee".into(),
                valid_from: at - Duration::minutes(5),
                valid_until: at + Duration::hours(1),
            },
            manager: ExceptionPrincipalReference {
                principal_ref: "manager".into(),
                valid_from: at - Duration::minutes(5),
                valid_until: at + Duration::hours(1),
            },
            manager_chain: OperationsManagerChainProof {
                subject_principal_ref: "assignee".into(),
                manager_principal_ref: "manager".into(),
                scope: scope.clone(),
                evaluated_at: at,
                assignments: vec![assignee, manager],
                authority_source_ref: "control-plane-role-evaluator".into(),
            },
            at,
        };
        (schedule, scope)
    }

    fn source_event(at: DateTime<Utc>, source_type: &str) -> OperationsSourceEvent {
        OperationsSourceEvent {
            source_domain: OperationsSourceDomain::Sales,
            source_type: source_type.into(),
            source_id: Uuid::from_u128(0x7804),
            source_event_id: Uuid::from_u128(0x7805),
            source_version: 1,
            event_type: "order.delivery_breached".into(),
            event_version: 1,
            holding_ref: "holding-scheduler-test".into(),
            scope: Scope::Holding {
                holding_id: Uuid::from_u128(0x7801),
            },
            actor: ActorAttribution {
                principal_ref: "sales-worker".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::from_u128(0x7806),
            causation_id: None,
            occurred_at: at - Duration::minutes(1),
            payload: serde_json::json!({"order_ref": "scheduler-test"}),
        }
    }

    fn completion_pass_with_holding(holding_ref: &str) -> OperationsCompletionPass {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let holding_id = Uuid::from_u128(0x7810);
        OperationsCompletionPass {
            tenant_id: "tenant-operations-test".into(),
            holding_ref: holding_ref.into(),
            scope: Scope::Holding { holding_id },
            authorization: OperationsAuthorizationProof {
                principal_ref: "operations-reconciliation-worker".into(),
                permission: OperationsPermission::Reconcile,
                scope: Scope::Holding { holding_id },
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::minutes(10),
                mfa_verified: true,
                break_glass: None,
            },
            at,
            worker_id: Uuid::from_u128(0x7811),
            outbox_repair_limit: 10,
            projection_page_size: 10,
            max_projection_pages: 10,
            detections: Vec::new(),
            detection_schedule: None,
            reassignment_schedule: None,
            escalation_schedule: None,
            reassignments: Vec::new(),
            escalations: Vec::new(),
            folio_operations_baseline: None,
        }
    }

    fn folio_baseline_for(holding_ref: &str) -> FolioOperationsBaseline {
        FolioOperationsBaseline::new("folio-operations-snapshot-v1", 0)
            .expect("valid Folio baseline")
            .bind_to_holding(holding_ref)
            .expect("valid holding binding")
    }

    #[test]
    fn completion_rejects_global_or_unscoped_folio_baseline() {
        let mut pass = completion_pass_with_holding("holding-operations-test");
        pass.folio_operations_baseline = Some(
            FolioOperationsBaseline::new("folio-operations-snapshot-v1", 0)
                .expect("valid unscoped Folio baseline"),
        );

        assert!(matches!(
            pass.validate(),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("bound to the pass holding")
        ));
    }

    #[test]
    fn completion_rejects_folio_baseline_bound_to_another_holding() {
        let mut pass = completion_pass_with_holding("holding-operations-test");
        pass.folio_operations_baseline = Some(folio_baseline_for("holding-other-operations-test"));

        assert!(matches!(
            pass.validate(),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("bound to the pass holding")
        ));
    }

    #[test]
    fn completion_accepts_only_matching_holding_folio_evidence() {
        let mut pass = completion_pass_with_holding("holding-operations-test");
        pass.folio_operations_baseline = Some(folio_baseline_for("holding-operations-test"));

        pass.validate().expect("matching holding evidence is valid");
    }

    #[test]
    fn dead_letter_preserves_failed_operation_and_attempt_count() {
        let error = OperationsCompletionWorkerError::retryable("Operations projection")
            .dead_letter(MAX_OPERATIONS_COMPLETION_ATTEMPTS);
        assert!(matches!(
            error,
            OperationsCompletionWorkerError::Failed {
                operation: "Operations projection",
                attempts: MAX_OPERATIONS_COMPLETION_ATTEMPTS,
                retryable: false,
                dead_lettered: true,
            }
        ));
    }

    #[test]
    fn historically_valid_pass_is_rejected_after_authorization_expiry() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let holding_id = Uuid::from_u128(0x7810);
        let pass = OperationsCompletionPass {
            tenant_id: "tenant-completion-test".into(),
            holding_ref: "holding-completion-test".into(),
            scope: Scope::Holding { holding_id },
            authorization: OperationsAuthorizationProof {
                principal_ref: "scheduler".into(),
                permission: OperationsPermission::Reconcile,
                scope: Scope::Holding { holding_id },
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::minutes(5),
                mfa_verified: true,
                break_glass: None,
            },
            at,
            worker_id: Uuid::from_u128(0x7811),
            outbox_repair_limit: 10,
            projection_page_size: 10,
            max_projection_pages: 10,
            detections: Vec::new(),
            detection_schedule: None,
            reassignment_schedule: None,
            escalation_schedule: None,
            reassignments: Vec::new(),
            escalations: Vec::new(),
            folio_operations_baseline: None,
        };

        pass.validate()
            .expect("pass is valid at its historical timestamp");
        assert!(matches!(
            pass.validate_current_authorization(at + Duration::hours(1)),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("worker execution time")
        ));
    }

    #[test]
    fn worker_rejects_stale_schedule_manager_proof_at_execution_time() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (mut schedule, scope) = schedule_fixture(at);
        schedule.authorization.valid_until = at + Duration::hours(2);
        schedule.assignee.valid_until = at + Duration::hours(2);
        schedule.manager.valid_until = at + Duration::minutes(5);
        schedule.manager_chain.assignments[0].valid_until = at + Duration::minutes(5);
        schedule.manager_chain.assignments[1].valid_until = at + Duration::minutes(5);

        let pass = OperationsCompletionPass {
            tenant_id: "tenant-completion-test".into(),
            holding_ref: "holding-completion-test".into(),
            scope: scope.clone(),
            authorization: OperationsAuthorizationProof {
                principal_ref: "scheduler".into(),
                permission: OperationsPermission::Reconcile,
                scope,
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::hours(2),
                mfa_verified: true,
                break_glass: None,
            },
            at,
            worker_id: Uuid::from_u128(0x7811),
            outbox_repair_limit: 10,
            projection_page_size: 10,
            max_projection_pages: 10,
            detections: Vec::new(),
            detection_schedule: Some(schedule),
            reassignment_schedule: None,
            escalation_schedule: None,
            reassignments: Vec::new(),
            escalations: Vec::new(),
            folio_operations_baseline: None,
        };

        pass.validate()
            .expect("pass is valid at its proof timestamp");
        assert!(matches!(
            pass.validate_current_authorization(at + Duration::hours(1)),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("schedule manager")
        ));
    }

    #[test]
    fn source_reconciliation_uses_the_authoritative_source_page_bound() {
        assert_eq!(
            source_reconciliation_page_size(MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE + 1),
            MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE
        );
        assert_eq!(source_reconciliation_page_size(1), 1);
    }

    #[test]
    fn source_reconciliation_rejects_a_moving_source_outbox_feed() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let first = OperationsSourceOutboxCursor {
            occurred_at: at,
            source_event_id: Uuid::from_u128(0x7808),
        };
        let second = OperationsSourceOutboxCursor {
            occurred_at: at + Duration::seconds(1),
            source_event_id: Uuid::from_u128(0x7809),
        };
        assert!(source_feed_moved((0, None), (1, Some(first))));
        assert!(source_feed_moved((1, Some(first)), (2, Some(second))));
        // A backdated source insertion can leave the latest tuple unchanged;
        // the count component still fences the scan.
        assert!(source_feed_moved((1, Some(first)), (2, Some(first))));
        assert!(!source_feed_moved((1, Some(first)), (1, Some(first))));
    }

    #[test]
    fn detection_schedule_derives_stable_requests_only_for_explicit_source_types() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (schedule, scope) = schedule_fixture(at);
        schedule
            .validate(&schedule.authorization, &scope, at)
            .expect("valid scheduler authority");
        let first = schedule
            .request_for(source_event(at, "sales.order"), &schedule.rules[0])
            .expect("matching request");
        let second = schedule
            .request_for(source_event(at, "sales.order"), &schedule.rules[0])
            .expect("replayed matching request");
        assert_eq!(first.exception_id, second.exception_id);
        assert_eq!(first.correlation_id, second.correlation_id);
        assert_eq!(first.idempotency_key, second.idempotency_key);
        assert!(schedule.rules[0].source_types.contains("sales.order"));
        assert!(!schedule.rules[0].source_types.contains("sales.invoice"));
    }

    #[test]
    fn detection_schedule_rejects_duplicate_rules_and_expired_authority() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (mut schedule, scope) = schedule_fixture(at);
        schedule.rules.push(schedule.rules[0].clone());
        assert!(matches!(
            schedule.validate(&schedule.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("duplicate")
        ));
        let (mut overlapping, scope) = schedule_fixture(at);
        overlapping.rules[0].policy.policy_ref = "same-source-different-policy".into();
        overlapping
            .rules
            .push(schedule_fixture(at).0.rules[0].clone());
        assert!(matches!(
            overlapping.validate(&overlapping.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("duplicate")
        ));
        let (mut expired, scope) = schedule_fixture(at);
        expired.at = at + Duration::minutes(1);
        assert!(matches!(
            expired.validate(&expired.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("authority")
        ));
    }

    #[test]
    fn reassignment_schedule_derives_stable_requests_from_active_projection_rows() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (detection, scope) = schedule_fixture(at);
        let mut manager_chain = detection.manager_chain.clone();
        manager_chain.subject_principal_ref = "replacement".into();
        manager_chain.assignments[0].principal_ref = "replacement".into();
        let schedule = OperationsReassignmentSchedule {
            rules: vec![OperationsReassignmentScheduleRule {
                expired_principal_ref: "expired".into(),
                replacement: ExceptionPrincipalReference {
                    principal_ref: "replacement".into(),
                    valid_from: at - Duration::minutes(5),
                    valid_until: at + Duration::hours(1),
                },
                manager: detection.manager.clone(),
                manager_chain,
            }],
            authorization: detection.authorization.clone(),
            actor_attribution: detection.actor_attribution.clone(),
            at,
        };
        schedule
            .validate(&schedule.authorization, &scope, at)
            .expect("valid reassignment authority");

        let source = source_event(at, "sales.order");
        let mut exception = ExceptionCase::from_source_event(
            &source,
            Uuid::from_u128(0x7807),
            "delivery_breach",
            "high",
        )
        .expect("valid exception");
        exception.meta.version = 2;
        exception.meta.updated_at = at;
        exception.assigned_to = Some("expired".into());
        exception.status = ExceptionStatus::Assigned;
        let row = OperationsReadRow {
            projection_name: OPERATIONS_READ_PROJECTION.into(),
            aggregate_type: "operations.exception_case".into(),
            aggregate_id: exception.meta.id,
            holding_ref: exception.meta.holding_ref.clone(),
            aggregate_version: exception.meta.version,
            lifecycle_status: "assigned".into(),
            value: exception,
            updated_at: at,
        };
        let first = schedule
            .request_for(&row, &schedule.rules[0])
            .expect("request derivation")
            .expect("expired assignee matches");
        let second = schedule
            .request_for(&row, &schedule.rules[0])
            .expect("request replay")
            .expect("expired assignee matches");
        assert_eq!(first.idempotency_key, second.idempotency_key);
        assert_eq!(first.correlation_id, second.correlation_id);
        assert_eq!(first.expected_version, 2);
        assert_eq!(first.replacement.principal_ref, "replacement");

        let mut unmatched = row;
        unmatched.value.assigned_to = Some("still-active".into());
        assert!(
            schedule
                .request_for(&unmatched, &schedule.rules[0])
                .expect("unmatched rule")
                .is_none()
        );
    }

    #[test]
    fn reassignment_schedule_rejects_duplicate_rules_and_expired_proof() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (detection, scope) = schedule_fixture(at);
        let rule = OperationsReassignmentScheduleRule {
            expired_principal_ref: "expired".into(),
            replacement: detection.assignee.clone(),
            manager: detection.manager.clone(),
            manager_chain: detection.manager_chain.clone(),
        };
        let mut schedule = OperationsReassignmentSchedule {
            rules: vec![rule.clone(), rule],
            authorization: detection.authorization.clone(),
            actor_attribution: detection.actor_attribution.clone(),
            at,
        };
        assert!(matches!(
            schedule.validate(&schedule.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("duplicate")
        ));
        schedule.rules.truncate(1);
        schedule.at = at + Duration::minutes(1);
        assert!(matches!(
            schedule.validate(&schedule.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("authority")
        ));
    }

    #[test]
    fn escalation_schedule_derives_stable_requests_only_for_due_assigned_cases() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (detection, scope) = schedule_fixture(at);
        let schedule = OperationsEscalationSchedule {
            rules: vec![OperationsEscalationScheduleRule {
                assigned_principal_ref: "assignee".into(),
                manager: detection.manager.clone(),
                manager_chain: detection.manager_chain.clone(),
            }],
            authorization: detection.authorization.clone(),
            actor_attribution: detection.actor_attribution.clone(),
            at,
        };
        schedule
            .validate(&schedule.authorization, &scope, at)
            .expect("valid escalation authority");

        let source = source_event(at, "sales.order");
        let mut exception = ExceptionCase::from_source_event(
            &source,
            Uuid::from_u128(0x7808),
            "delivery_breach",
            "high",
        )
        .expect("valid exception");
        exception.meta.version = 2;
        exception.meta.updated_at = at;
        exception.assigned_to = Some("assignee".into());
        exception.due_at = Some(at - Duration::minutes(1));
        exception.status = ExceptionStatus::Assigned;
        let row = OperationsReadRow {
            projection_name: OPERATIONS_READ_PROJECTION.into(),
            aggregate_type: "operations.exception_case".into(),
            aggregate_id: exception.meta.id,
            holding_ref: exception.meta.holding_ref.clone(),
            aggregate_version: exception.meta.version,
            lifecycle_status: "assigned".into(),
            value: exception,
            updated_at: at,
        };
        let first = schedule
            .request_for(&row, &schedule.rules[0])
            .expect("escalation request derivation")
            .expect("due assigned exception matches");
        let second = schedule
            .request_for(&row, &schedule.rules[0])
            .expect("escalation request replay")
            .expect("due assigned exception matches");
        assert_eq!(first.idempotency_key, second.idempotency_key);
        assert_eq!(first.correlation_id, second.correlation_id);
        assert_eq!(first.expected_version, 2);

        let mut not_due = row.clone();
        not_due.value.due_at = Some(at + Duration::minutes(1));
        assert!(
            schedule
                .request_for(&not_due, &schedule.rules[0])
                .expect("not-due case")
                .is_none()
        );
        let mut already_escalated = row;
        already_escalated.value.status = ExceptionStatus::Escalated;
        assert!(
            schedule
                .request_for(&already_escalated, &schedule.rules[0])
                .expect("already escalated case")
                .is_none()
        );
    }

    #[test]
    fn escalation_schedule_rejects_duplicate_assignees_and_expired_authority() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(10);
        let (detection, scope) = schedule_fixture(at);
        let rule = OperationsEscalationScheduleRule {
            assigned_principal_ref: "assignee".into(),
            manager: detection.manager.clone(),
            manager_chain: detection.manager_chain.clone(),
        };
        let mut schedule = OperationsEscalationSchedule {
            rules: vec![rule.clone(), rule],
            authorization: detection.authorization.clone(),
            actor_attribution: detection.actor_attribution.clone(),
            at,
        };
        assert!(matches!(
            schedule.validate(&schedule.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("duplicate")
        ));
        schedule.rules.truncate(1);
        schedule.at = at + Duration::minutes(1);
        assert!(matches!(
            schedule.validate(&schedule.authorization, &scope, at),
            Err(OperationsCompletionWorkerError::Configuration(message))
                if message.contains("authority")
        ));
    }
}
