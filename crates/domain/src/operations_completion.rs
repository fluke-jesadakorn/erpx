// G7 Operations completion contracts.
//
// These types are deliberately transport- and persistence-neutral. They bind
// the operational control loop to an authoritative authorization proof,
// active principal windows, and a delegated manager chain. The infrastructure
// adapter persists the resulting commands through the existing Operations
// snapshots/events and holding outbox; no client-supplied role state is trusted.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::{
    ExceptionCase, ExceptionLifecycleAction, ExceptionLifecycleCommand,
    ExceptionPrincipalReference, ExceptionSlaMetadata, MAX_EXCEPTION_REFERENCE_BYTES,
    MAX_EXCEPTION_TEXT_BYTES, MAX_OPERATIONS_POLICY_DOMAINS, OperationsSourceDomain,
    OperationsSourceEvent, SourceTransactionRef,
};
use crate::{
    access::RoleAssignment,
    contracts::{ActorAttribution, AuthorityScope},
};

pub const OPERATIONS_COMPLETION_CONTRACT_VERSION: u16 = 1;
pub const OPERATIONS_MAX_MANAGER_CHAIN_DEPTH: usize = 8;
pub const OPERATIONS_MAX_DETECTION_POLICY_DOMAINS: usize = MAX_OPERATIONS_POLICY_DOMAINS;
pub const OPERATIONS_MAX_QUEUE_ITEMS: usize = 100;
pub const OPERATIONS_PROJECTION_FRESHNESS_SECONDS: i64 = 300;
pub const OPERATIONS_MAX_SLA_MINUTES: u32 = 7 * 24 * 60;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OperationsCompletionError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("operation identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("authorization proof is not active at command time")]
    AuthorizationExpired,
    #[error("authorization permission {actual} does not satisfy {required}")]
    PermissionDenied {
        required: &'static str,
        actual: String,
    },
    #[error("authorization scope does not contain the requested Operations scope")]
    ScopeDenied,
    #[error("MFA is required for high-risk Operations access")]
    MfaRequired,
    #[error("break-glass fallback must be local-only")]
    BreakGlassMustBeLocal,
    #[error("break-glass fallback is expired or incomplete")]
    InvalidBreakGlass,
    #[error("source domain is not allowed by the detection policy")]
    SourceDomainNotAllowed,
    #[error("source event cannot be detected before it has occurred")]
    SourceEventFromFuture,
    #[error("detection policy is invalid")]
    InvalidDetectionPolicy,
    #[error("manager chain is invalid: {0}")]
    InvalidManagerChain(String),
    #[error("replacement principal must differ from the expired principal")]
    SameReplacementPrincipal,
    #[error("actor attribution does not match the authorization proof")]
    ActorMismatch,
    #[error("SLA is outside the bounded operational window")]
    InvalidSla,
    #[error("expected exception version must be positive")]
    InvalidExpectedVersion,
    #[error("Operations queue item field {field} is invalid")]
    InvalidQueueItem { field: &'static str },
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), OperationsCompletionError> {
    if value.trim().is_empty() {
        return Err(OperationsCompletionError::EmptyField { field });
    }
    if value.len() > max {
        return Err(OperationsCompletionError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(OperationsCompletionError::ControlCharacter { field });
    }
    Ok(())
}

fn identifier(value: Uuid, field: &'static str) -> Result<(), OperationsCompletionError> {
    if value.is_nil() {
        return Err(OperationsCompletionError::NilIdentifier { field });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsPermission {
    Read,
    Manage,
    ExceptionManage,
    Reconcile,
    BreakGlass,
}

impl OperationsPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "operations.read",
            Self::Manage => "operations.manage",
            Self::ExceptionManage => "operations.exception.manage",
            Self::Reconcile => "operations.reconcile",
            Self::BreakGlass => "operations.break_glass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationsWorkflowAction {
    ReadQueue,
    DetectException,
    ReassignExpiredPrincipal,
    EscalateSla,
    Reconcile,
    RebuildProjection,
}

impl OperationsWorkflowAction {
    pub const fn required_permission(self) -> OperationsPermission {
        match self {
            Self::ReadQueue => OperationsPermission::Read,
            Self::DetectException => OperationsPermission::ExceptionManage,
            Self::ReassignExpiredPrincipal | Self::EscalateSla => {
                OperationsPermission::ExceptionManage
            }
            Self::Reconcile | Self::RebuildProjection => OperationsPermission::Reconcile,
        }
    }

    pub const fn high_risk(self) -> bool {
        matches!(
            self,
            Self::ReassignExpiredPrincipal
                | Self::EscalateSla
                | Self::Reconcile
                | Self::RebuildProjection
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsBreakGlassProof {
    pub ticket_ref: String,
    pub reason: String,
    pub expires_at: DateTime<Utc>,
    pub local_only: bool,
}

impl OperationsBreakGlassProof {
    fn validate(&self, at: DateTime<Utc>) -> Result<(), OperationsCompletionError> {
        bounded(&self.ticket_ref, "break_glass.ticket_ref", 255)?;
        bounded(&self.reason, "break_glass.reason", 1_024)?;
        if !self.local_only || self.expires_at <= at {
            return Err(OperationsCompletionError::InvalidBreakGlass);
        }
        Ok(())
    }
}

/// Proof returned by the shared authorization evaluator. Operations checks it
/// but never resolves roles itself. The only fallback permitted by this
/// contract is an explicitly local, expiring, MFA-backed break-glass proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsAuthorizationProof {
    pub principal_ref: String,
    pub permission: OperationsPermission,
    pub scope: AuthorityScope,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub mfa_verified: bool,
    pub break_glass: Option<OperationsBreakGlassProof>,
}

impl OperationsAuthorizationProof {
    pub fn validate_for(
        &self,
        action: OperationsWorkflowAction,
        requested_scope: &AuthorityScope,
        at: DateTime<Utc>,
    ) -> Result<(), OperationsCompletionError> {
        bounded(&self.principal_ref, "authorization.principal_ref", 255)?;
        if self.valid_until <= self.valid_from || at < self.valid_from || at >= self.valid_until {
            return Err(OperationsCompletionError::AuthorizationExpired);
        }
        if !self.scope.contains(requested_scope) {
            return Err(OperationsCompletionError::ScopeDenied);
        }
        let required = action.required_permission();
        if self.permission != required && self.permission != OperationsPermission::BreakGlass {
            return Err(OperationsCompletionError::PermissionDenied {
                required: required.as_str(),
                actual: self.permission.as_str().into(),
            });
        }
        if self.permission == OperationsPermission::BreakGlass && self.break_glass.is_none() {
            return Err(OperationsCompletionError::InvalidBreakGlass);
        }
        if self.permission != OperationsPermission::BreakGlass && self.break_glass.is_some() {
            return Err(OperationsCompletionError::InvalidBreakGlass);
        }
        if action.high_risk() && !self.mfa_verified {
            return Err(OperationsCompletionError::MfaRequired);
        }
        if let Some(break_glass) = &self.break_glass {
            break_glass.validate(at)?;
            if !break_glass.local_only {
                return Err(OperationsCompletionError::BreakGlassMustBeLocal);
            }
        }
        Ok(())
    }

    pub fn actor_reference(&self) -> ExceptionPrincipalReference {
        ExceptionPrincipalReference {
            principal_ref: self.principal_ref.clone(),
            valid_from: self.valid_from,
            valid_until: self.valid_until,
        }
    }
}

/// The policy is captured on the exception snapshot so later SLA sweeps do
/// not depend on a mutable external policy lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsDetectionPolicy {
    pub policy_ref: String,
    pub source_domains: BTreeSet<OperationsSourceDomain>,
    pub category: String,
    pub severity: String,
    pub due_after_minutes: u32,
    pub escalation_after_minutes: u32,
}

impl OperationsDetectionPolicy {
    pub fn validate(&self) -> Result<(), OperationsCompletionError> {
        bounded(&self.policy_ref, "policy_ref", 128)?;
        bounded(&self.category, "category", 128)?;
        bounded(&self.severity, "severity", 128)?;
        if self.source_domains.is_empty()
            || self.source_domains.len() > OPERATIONS_MAX_DETECTION_POLICY_DOMAINS
            || self.due_after_minutes == 0
            || self.due_after_minutes > OPERATIONS_MAX_SLA_MINUTES
            || self.escalation_after_minutes == 0
            || self.escalation_after_minutes > OPERATIONS_MAX_SLA_MINUTES
        {
            return Err(OperationsCompletionError::InvalidDetectionPolicy);
        }
        Ok(())
    }

    pub fn sla(
        &self,
        created_at: DateTime<Utc>,
    ) -> Result<ExceptionSlaMetadata, OperationsCompletionError> {
        self.validate()?;
        let due_at = created_at + Duration::minutes(i64::from(self.due_after_minutes));
        Ok(ExceptionSlaMetadata {
            policy_ref: self.policy_ref.clone(),
            due_at,
            escalation_after_minutes: self.escalation_after_minutes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsManagerChainProof {
    pub subject_principal_ref: String,
    pub manager_principal_ref: String,
    pub scope: AuthorityScope,
    pub evaluated_at: DateTime<Utc>,
    pub assignments: Vec<RoleAssignment>,
    pub authority_source_ref: String,
}

impl OperationsManagerChainProof {
    pub fn validate(&self, at: DateTime<Utc>) -> Result<(), OperationsCompletionError> {
        bounded(&self.subject_principal_ref, "manager_chain.subject", 255)?;
        bounded(&self.manager_principal_ref, "manager_chain.manager", 255)?;
        bounded(
            &self.authority_source_ref,
            "manager_chain.authority_source_ref",
            255,
        )?;
        if self.subject_principal_ref == self.manager_principal_ref
            || self.assignments.is_empty()
            || self.assignments.len() < 2
            || self.assignments.len() > OPERATIONS_MAX_MANAGER_CHAIN_DEPTH
            || self.evaluated_at != at
        {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "chain shape or evaluation time is invalid".into(),
            ));
        }
        let first = self.assignments.first().ok_or_else(|| {
            OperationsCompletionError::InvalidManagerChain("chain is empty".into())
        })?;
        let last = self.assignments.last().ok_or_else(|| {
            OperationsCompletionError::InvalidManagerChain("chain is empty".into())
        })?;
        let mut seen_ids = BTreeSet::new();
        let mut seen_principals = BTreeSet::new();
        for assignment in &self.assignments {
            identifier(assignment.id, "manager_chain.assignment_id")?;
            bounded(
                &assignment.principal_ref,
                "manager_chain.assignment_principal",
                255,
            )?;
            if !seen_ids.insert(assignment.id) {
                return Err(OperationsCompletionError::InvalidManagerChain(
                    "chain contains a repeated assignment".into(),
                ));
            }
            if !seen_principals.insert(assignment.principal_ref.clone()) {
                return Err(OperationsCompletionError::InvalidManagerChain(
                    "chain contains a repeated principal".into(),
                ));
            }
        }
        if first.principal_ref != self.subject_principal_ref
            || last.principal_ref != self.manager_principal_ref
            || last.parent_assignment_id.is_some()
            || !first.scope.contains(&self.scope)
            || !last.scope.contains(&self.scope)
            || !first.is_active_at(at)
            || !last.is_active_at(at)
        {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "principal, scope, or active window does not match".into(),
            ));
        }
        if !last.permissions.iter().any(|permission| {
            permission == OperationsPermission::ExceptionManage.as_str()
                || permission == OperationsPermission::Manage.as_str()
        }) {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "manager lacks Operations exception authority".into(),
            ));
        }
        for pair in self.assignments.windows(2) {
            let child = &pair[0];
            let parent = &pair[1];
            // `effective_at` proves the edge is active now; delegation
            // validation additionally proves that the child never starts
            // before, outlives, exceeds the scope/authority of, or requests
            // permissions outside its authoritative parent grant.
            if !child.effective_at(at, Some(parent)) || child.validate_delegation(parent).is_err() {
                return Err(OperationsCompletionError::InvalidManagerChain(
                    "delegation link is not effective or contained".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationsDetectionRequest {
    pub source_event: OperationsSourceEvent,
    pub exception_id: Uuid,
    pub policy: OperationsDetectionPolicy,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub assignee: ExceptionPrincipalReference,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
}

impl OperationsDetectionRequest {
    pub fn validate(&self) -> Result<(), OperationsCompletionError> {
        self.validate_at(self.source_event.occurred_at)
    }

    /// Validate a detection request at the scheduler's execution time. The
    /// source event remains the authoritative occurrence timestamp, but the
    /// Operations permission, assignee window, and manager-chain proof must
    /// still be active when a scheduled sweep performs the write.
    pub fn validate_at(&self, at: DateTime<Utc>) -> Result<(), OperationsCompletionError> {
        identifier(self.exception_id, "exception_id")?;
        identifier(self.correlation_id, "correlation_id")?;
        bounded(&self.idempotency_key, "idempotency_key", 255)?;
        if self.source_event.occurred_at > at {
            return Err(OperationsCompletionError::SourceEventFromFuture);
        }
        self.source_event
            .validate()
            .map_err(|_| OperationsCompletionError::InvalidDetectionPolicy)?;
        self.policy.validate()?;
        if !self
            .policy
            .source_domains
            .contains(&self.source_event.source_domain)
        {
            return Err(OperationsCompletionError::SourceDomainNotAllowed);
        }
        self.authorization.validate_for(
            OperationsWorkflowAction::DetectException,
            &self.source_event.scope,
            at,
        )?;
        if self.actor_attribution.principal_ref != self.authorization.principal_ref {
            return Err(OperationsCompletionError::ActorMismatch);
        }
        self.assignee
            .validate(at, "assignee")
            .map_err(|_| OperationsCompletionError::AuthorizationExpired)?;
        self.manager
            .validate(at, "manager")
            .map_err(|_| OperationsCompletionError::AuthorizationExpired)?;
        if self.manager_chain.manager_principal_ref != self.manager.principal_ref
            || self.manager_chain.subject_principal_ref != self.assignee.principal_ref
        {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "manager proof does not match assignment principals".into(),
            ));
        }
        self.manager_chain.validate(at)
    }

    pub fn exception(&self) -> Result<ExceptionCase, OperationsCompletionError> {
        self.exception_at(self.source_event.occurred_at)
    }

    pub fn exception_at(
        &self,
        at: DateTime<Utc>,
    ) -> Result<ExceptionCase, OperationsCompletionError> {
        self.validate_at(at)?;
        let exception = ExceptionCase::from_source_event(
            &self.source_event,
            self.exception_id,
            self.policy.category.clone(),
            self.policy.severity.clone(),
        )
        .map_err(|_| OperationsCompletionError::InvalidDetectionPolicy)?;
        // Detection persists a valid, unassigned version-one snapshot. The
        // SLA and assignment facts are applied by the subsequent lifecycle
        // command in the same authoritative write flow; placing them on a
        // `Detected` snapshot violates ExceptionCase's state invariant.
        Ok(exception)
    }

    pub fn assignment_command(
        &self,
    ) -> Result<ExceptionLifecycleCommand, OperationsCompletionError> {
        self.assignment_command_at(self.source_event.occurred_at)
    }

    pub fn assignment_command_at(
        &self,
        at: DateTime<Utc>,
    ) -> Result<ExceptionLifecycleCommand, OperationsCompletionError> {
        self.validate_at(at)?;
        let sla = self.policy.sla(self.source_event.occurred_at)?;
        Ok(ExceptionLifecycleCommand {
            exception_id: self.exception_id,
            expected_version: 1,
            at,
            actor: self.authorization.actor_reference(),
            actor_attribution: self.actor_attribution.clone(),
            idempotency_key: format!("{}:assign", self.idempotency_key),
            correlation_id: self.correlation_id,
            causation_id: Some(self.source_event.source_event_id),
            action: ExceptionLifecycleAction::Assign {
                assignee: self.assignee.clone(),
                manager: self.manager.clone(),
                sla,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsExpiredPrincipalReassignment {
    pub exception_id: Uuid,
    pub expected_version: u64,
    pub expired_principal_ref: String,
    pub replacement: ExceptionPrincipalReference,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub at: DateTime<Utc>,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
}

impl OperationsExpiredPrincipalReassignment {
    pub fn validate(
        &self,
        exception_scope: &AuthorityScope,
    ) -> Result<(), OperationsCompletionError> {
        identifier(self.exception_id, "exception_id")?;
        identifier(self.correlation_id, "correlation_id")?;
        if self.expected_version == 0 {
            return Err(OperationsCompletionError::InvalidExpectedVersion);
        }
        bounded(&self.expired_principal_ref, "expired_principal_ref", 255)?;
        bounded(&self.idempotency_key, "idempotency_key", 255)?;
        if self.expired_principal_ref == self.replacement.principal_ref {
            return Err(OperationsCompletionError::SameReplacementPrincipal);
        }
        if self.authorization.principal_ref != self.actor_attribution.principal_ref {
            return Err(OperationsCompletionError::ActorMismatch);
        }
        self.authorization.validate_for(
            OperationsWorkflowAction::ReassignExpiredPrincipal,
            exception_scope,
            self.at,
        )?;
        self.replacement
            .validate(self.at, "replacement")
            .map_err(|_| OperationsCompletionError::AuthorizationExpired)?;
        self.manager
            .validate(self.at, "manager")
            .map_err(|_| OperationsCompletionError::AuthorizationExpired)?;
        if self.manager_chain.subject_principal_ref != self.replacement.principal_ref
            || self.manager_chain.manager_principal_ref != self.manager.principal_ref
        {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "reassignment manager proof does not match".into(),
            ));
        }
        self.manager_chain.validate(self.at)
    }

    pub fn command(&self) -> ExceptionLifecycleCommand {
        ExceptionLifecycleCommand {
            exception_id: self.exception_id,
            expected_version: self.expected_version,
            at: self.at,
            actor: self.authorization.actor_reference(),
            actor_attribution: self.actor_attribution.clone(),
            idempotency_key: self.idempotency_key.clone(),
            correlation_id: self.correlation_id,
            causation_id: None,
            action: ExceptionLifecycleAction::Reassign {
                assignee: self.replacement.clone(),
                manager: self.manager.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsSlaEscalationRequest {
    pub exception_id: Uuid,
    pub expected_version: u64,
    pub manager: ExceptionPrincipalReference,
    pub manager_chain: OperationsManagerChainProof,
    pub authorization: OperationsAuthorizationProof,
    pub actor_attribution: ActorAttribution,
    pub at: DateTime<Utc>,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
}

impl OperationsSlaEscalationRequest {
    pub fn validate(&self, exception: &ExceptionCase) -> Result<(), OperationsCompletionError> {
        identifier(self.exception_id, "exception_id")?;
        identifier(self.correlation_id, "correlation_id")?;
        if self.expected_version == 0 || exception.meta.id != self.exception_id {
            return Err(OperationsCompletionError::InvalidExpectedVersion);
        }
        bounded(&self.idempotency_key, "idempotency_key", 255)?;
        self.authorization.validate_for(
            OperationsWorkflowAction::EscalateSla,
            &exception.meta.scope,
            self.at,
        )?;
        if self.authorization.principal_ref != self.actor_attribution.principal_ref {
            return Err(OperationsCompletionError::ActorMismatch);
        }
        self.manager
            .validate(self.at, "manager")
            .map_err(|_| OperationsCompletionError::AuthorizationExpired)?;
        if self.manager_chain.subject_principal_ref
            != exception.assigned_to.as_deref().unwrap_or_default()
            || self.manager_chain.manager_principal_ref != self.manager.principal_ref
        {
            return Err(OperationsCompletionError::InvalidManagerChain(
                "escalation manager proof does not match exception assignment".into(),
            ));
        }
        self.manager_chain.validate(self.at)
    }

    pub fn command(&self) -> ExceptionLifecycleCommand {
        ExceptionLifecycleCommand {
            exception_id: self.exception_id,
            expected_version: self.expected_version,
            at: self.at,
            actor: self.authorization.actor_reference(),
            actor_attribution: self.actor_attribution.clone(),
            idempotency_key: self.idempotency_key.clone(),
            correlation_id: self.correlation_id,
            causation_id: None,
            action: ExceptionLifecycleAction::Escalate {
                manager: self.manager.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsProjectionFreshnessState {
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_projected_at: Option<DateTime<Utc>>,
    pub lag_seconds: Option<i64>,
    pub healthy: bool,
}

impl OperationsProjectionFreshnessState {
    pub fn at(
        last_event_at: Option<DateTime<Utc>>,
        last_projected_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Self {
        let lag_seconds = match (last_event_at, last_projected_at) {
            (Some(event), Some(projected)) => Some((event - projected).num_seconds().max(0)),
            (Some(event), None) => Some((now - event).num_seconds().max(0)),
            _ => None,
        };
        let healthy = lag_seconds.is_none_or(|lag| lag <= OPERATIONS_PROJECTION_FRESHNESS_SECONDS);
        Self {
            last_event_at,
            last_projected_at,
            lag_seconds,
            healthy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsQueueItemState {
    pub exception_id: Uuid,
    pub version: u64,
    pub status: String,
    pub category: String,
    pub severity: String,
    pub assigned_to: Option<String>,
    pub manager_ref: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub source: SourceTransactionRef,
}

impl OperationsQueueItemState {
    fn validate(&self) -> Result<(), OperationsCompletionError> {
        identifier(self.exception_id, "queue_item.exception_id")?;
        if self.version == 0 {
            return Err(OperationsCompletionError::InvalidQueueItem { field: "version" });
        }
        if !matches!(
            self.status.as_str(),
            "detected" | "assigned" | "acknowledged" | "escalated" | "resolved" | "closed"
        ) {
            return Err(OperationsCompletionError::InvalidQueueItem { field: "status" });
        }
        bounded(&self.status, "queue_item.status", 32)?;
        bounded(
            &self.category,
            "queue_item.category",
            MAX_EXCEPTION_TEXT_BYTES,
        )?;
        bounded(
            &self.severity,
            "queue_item.severity",
            MAX_EXCEPTION_TEXT_BYTES,
        )?;
        if let Some(assigned_to) = &self.assigned_to {
            bounded(
                assigned_to,
                "queue_item.assigned_to",
                MAX_EXCEPTION_REFERENCE_BYTES,
            )?;
        }
        if let Some(manager_ref) = &self.manager_ref {
            bounded(
                manager_ref,
                "queue_item.manager_ref",
                MAX_EXCEPTION_REFERENCE_BYTES,
            )?;
        }
        self.source
            .validate()
            .map_err(|_| OperationsCompletionError::InvalidQueueItem { field: "source" })?;
        if OperationsSourceDomain::parse(&self.source.source_domain).is_err() {
            return Err(OperationsCompletionError::InvalidQueueItem {
                field: "source_domain",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsQueueState {
    pub data_profile_key: String,
    pub projection_name: String,
    pub as_of: DateTime<Utc>,
    pub freshness: OperationsProjectionFreshnessState,
    pub items: Vec<OperationsQueueItemState>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl OperationsQueueState {
    pub fn validate(&self) -> Result<(), OperationsCompletionError> {
        bounded(&self.data_profile_key, "data_profile_key", 255)?;
        bounded(&self.projection_name, "projection_name", 255)?;
        if self.items.len() > OPERATIONS_MAX_QUEUE_ITEMS {
            return Err(OperationsCompletionError::FieldTooLong {
                field: "items",
                max: OPERATIONS_MAX_QUEUE_ITEMS,
            });
        }
        for item in &self.items {
            item.validate()?;
        }
        if self.has_more != self.next_cursor.is_some() {
            return Err(OperationsCompletionError::InvalidQueueItem {
                field: "next_cursor",
            });
        }
        if let Some(cursor) = &self.next_cursor {
            bounded(cursor, "queue.next_cursor", 255)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::RiskTier;
    use crate::contracts::{ActorKind, AuthorityScope};

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding {
            holding_id: Uuid::from_u128(0x7100),
        }
    }

    fn proof(permission: OperationsPermission, mfa_verified: bool) -> OperationsAuthorizationProof {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        OperationsAuthorizationProof {
            principal_ref: "operator".into(),
            permission,
            scope: scope(),
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::hours(1),
            mfa_verified,
            break_glass: None,
        }
    }

    #[test]
    fn high_risk_operations_requires_mfa_and_scope() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let requested = AuthorityScope::Branch {
            holding_id: Uuid::from_u128(0x7100),
            company_id: Uuid::from_u128(0x7101),
            branch_id: Uuid::from_u128(0x7102),
        };
        assert!(
            proof(OperationsPermission::ExceptionManage, false)
                .validate_for(
                    OperationsWorkflowAction::ReassignExpiredPrincipal,
                    &requested,
                    at
                )
                .is_err()
        );
        assert!(
            proof(OperationsPermission::ExceptionManage, true)
                .validate_for(
                    OperationsWorkflowAction::ReassignExpiredPrincipal,
                    &requested,
                    at
                )
                .is_ok()
        );
    }

    fn detection_request_for_time_checks(
        source_occurred_at: DateTime<Utc>,
    ) -> OperationsDetectionRequest {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        OperationsDetectionRequest {
            source_event: OperationsSourceEvent {
                source_domain: OperationsSourceDomain::Sales,
                source_type: "sales.order".into(),
                source_id: Uuid::from_u128(0x7110),
                source_event_id: Uuid::from_u128(0x7111),
                source_version: 1,
                event_type: "order.delivery_breached".into(),
                event_version: 1,
                data_profile_key: "holding-operations-test".into(),
                scope: scope(),
                actor: ActorAttribution {
                    principal_ref: "sales-source".into(),
                    kind: ActorKind::SystemWorker,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                correlation_id: Uuid::from_u128(0x7112),
                causation_id: None,
                occurred_at: source_occurred_at,
                payload: serde_json::json!({"order_ref": "operations-test"}),
            },
            exception_id: Uuid::from_u128(0x7113),
            policy: OperationsDetectionPolicy {
                policy_ref: "sales-delivery-breach".into(),
                source_domains: BTreeSet::from([OperationsSourceDomain::Sales]),
                category: "delivery_breach".into(),
                severity: "high".into(),
                due_after_minutes: 30,
                escalation_after_minutes: 60,
            },
            authorization: proof(OperationsPermission::ExceptionManage, true),
            actor_attribution: ActorAttribution {
                principal_ref: "operator".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            assignee: ExceptionPrincipalReference {
                principal_ref: "assignee".into(),
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::hours(1),
            },
            manager: ExceptionPrincipalReference {
                principal_ref: "manager".into(),
                valid_from: at - Duration::minutes(1),
                valid_until: at + Duration::hours(1),
            },
            manager_chain: OperationsManagerChainProof {
                subject_principal_ref: "assignee".into(),
                manager_principal_ref: "manager".into(),
                scope: scope(),
                evaluated_at: at,
                assignments: Vec::new(),
                authority_source_ref: "control-plane-test".into(),
            },
            idempotency_key: "operations-detection:test".into(),
            correlation_id: Uuid::from_u128(0x7114),
        }
    }

    #[test]
    fn scheduled_detection_rejects_future_source_facts() {
        let sweep_at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let request = detection_request_for_time_checks(sweep_at + Duration::minutes(1));
        assert_eq!(
            request.validate_at(sweep_at),
            Err(OperationsCompletionError::SourceEventFromFuture)
        );
    }

    #[test]
    fn scheduled_detection_rechecks_authority_at_sweep_time() {
        let source_at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let request = detection_request_for_time_checks(source_at);
        let expired_sweep = source_at + Duration::hours(1);
        assert_eq!(
            request.validate_at(expired_sweep),
            Err(OperationsCompletionError::AuthorizationExpired)
        );
    }

    #[test]
    fn detection_rejects_a_truncated_manager_chain() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let request = detection_request_for_time_checks(at);
        assert!(matches!(
            request.validate_at(at),
            Err(OperationsCompletionError::InvalidManagerChain(_))
        ));
    }

    fn manager_assignment(
        id: u128,
        principal_ref: &str,
        permissions: &[&str],
        authority_level: u8,
        parent_assignment_id: Option<Uuid>,
        at: DateTime<Utc>,
    ) -> RoleAssignment {
        RoleAssignment {
            id: Uuid::from_u128(id),
            principal_ref: principal_ref.into(),
            role_key: "operations-test".into(),
            permissions: permissions.iter().map(|value| (*value).into()).collect(),
            functional_domain: "operations".into(),
            authority_level,
            approval_limit_minor: 0,
            scope: scope(),
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::hours(1),
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "test".into(),
            ticket_reference: Some("test-ticket".into()),
            grantor: "test-grantor".into(),
            approver: Some("test-approver".into()),
            renewal_reviewer: Some("test-reviewer".into()),
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    #[test]
    fn manager_chain_requires_authoritative_root_permission() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let root_id = Uuid::from_u128(0x7130);
        let proof = OperationsManagerChainProof {
            subject_principal_ref: "assignee".into(),
            manager_principal_ref: "manager".into(),
            scope: scope(),
            evaluated_at: at,
            assignments: vec![
                manager_assignment(
                    0x7131,
                    "assignee",
                    &["operations.read"],
                    1,
                    Some(root_id),
                    at,
                ),
                manager_assignment(
                    0x7130,
                    "manager",
                    &["operations.read", "operations.exception.manage"],
                    2,
                    None,
                    at,
                ),
            ],
            authority_source_ref: "role-assignment-chain:7130".into(),
        };
        assert!(proof.validate(at).is_ok());

        let mut read_only = proof.clone();
        read_only.assignments[1].permissions = vec!["operations.read".into()];
        assert!(matches!(
            read_only.validate(at),
            Err(OperationsCompletionError::InvalidManagerChain(_))
        ));
    }

    #[test]
    fn manager_chain_requires_delegation_containment_and_unique_principals() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let root_id = Uuid::from_u128(0x7140);
        let proof = OperationsManagerChainProof {
            subject_principal_ref: "assignee".into(),
            manager_principal_ref: "manager".into(),
            scope: scope(),
            evaluated_at: at,
            assignments: vec![
                manager_assignment(
                    0x7141,
                    "assignee",
                    &["operations.read"],
                    1,
                    Some(root_id),
                    at,
                ),
                manager_assignment(
                    0x7140,
                    "manager",
                    &["operations.read", "operations.exception.manage"],
                    2,
                    None,
                    at,
                ),
            ],
            authority_source_ref: "role-assignment-chain:7140".into(),
        };
        assert!(proof.validate(at).is_ok());

        let mut outliving_child = proof.clone();
        outliving_child.assignments[0].valid_until = at + Duration::hours(2);
        assert!(matches!(
            outliving_child.validate(at),
            Err(OperationsCompletionError::InvalidManagerChain(message))
                if message.contains("contained")
        ));

        let mut overprivileged_child = proof.clone();
        overprivileged_child.assignments[0].permissions = vec!["operations.manage".into()];
        assert!(matches!(
            overprivileged_child.validate(at),
            Err(OperationsCompletionError::InvalidManagerChain(message))
                if message.contains("contained")
        ));

        let mut repeated_principal = proof;
        repeated_principal.assignments[0].principal_ref = "manager".into();
        assert!(matches!(
            repeated_principal.validate(at),
            Err(OperationsCompletionError::InvalidManagerChain(message))
                if message.contains("repeated principal")
        ));
    }

    #[test]
    fn local_break_glass_is_explicitly_bounded() {
        let at = DateTime::UNIX_EPOCH + Duration::hours(1);
        let mut authorization = proof(OperationsPermission::BreakGlass, true);
        assert!(
            authorization
                .validate_for(OperationsWorkflowAction::Reconcile, &scope(), at)
                .is_err()
        );
        authorization.break_glass = Some(OperationsBreakGlassProof {
            ticket_ref: "ticket-1".into(),
            reason: "expired manager".into(),
            expires_at: at + Duration::minutes(5),
            local_only: true,
        });
        assert!(
            authorization
                .validate_for(OperationsWorkflowAction::Reconcile, &scope(), at)
                .is_ok()
        );
        authorization.break_glass.as_mut().unwrap().local_only = false;
        assert!(
            authorization
                .validate_for(OperationsWorkflowAction::Reconcile, &scope(), at)
                .is_err()
        );
    }

    #[test]
    fn queue_state_does_not_accept_unbounded_items() {
        let state = OperationsQueueState {
            data_profile_key: "holding".into(),
            projection_name: "operations_read_model".into(),
            as_of: Utc::now(),
            freshness: OperationsProjectionFreshnessState::at(None, None, Utc::now()),
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
        };
        assert!(state.validate().is_ok());
    }

    #[test]
    fn queue_cursor_presence_must_match_the_has_more_flag() {
        let mut state = OperationsQueueState {
            data_profile_key: "holding".into(),
            projection_name: "operations_read_model".into(),
            as_of: Utc::now(),
            freshness: OperationsProjectionFreshnessState::at(None, None, Utc::now()),
            items: Vec::new(),
            next_cursor: None,
            has_more: true,
        };
        assert_eq!(
            state.validate(),
            Err(OperationsCompletionError::InvalidQueueItem {
                field: "next_cursor"
            })
        );
        state.next_cursor = Some("cursor-1".into());
        state.has_more = false;
        assert_eq!(
            state.validate(),
            Err(OperationsCompletionError::InvalidQueueItem {
                field: "next_cursor"
            })
        );
    }

    fn valid_queue_item() -> OperationsQueueItemState {
        OperationsQueueItemState {
            exception_id: Uuid::from_u128(0x7120),
            version: 1,
            status: "assigned".into(),
            category: "late_delivery".into(),
            severity: "high".into(),
            assigned_to: Some("assignee".into()),
            manager_ref: Some("manager".into()),
            due_at: Some(Utc::now() + Duration::minutes(30)),
            updated_at: Utc::now(),
            source: SourceTransactionRef::new(
                OperationsSourceDomain::Sales.as_str(),
                "sales.order",
                Uuid::from_u128(0x7121),
                Uuid::from_u128(0x7122),
            )
            .unwrap(),
        }
    }

    #[test]
    fn queue_state_rejects_unversioned_or_untrusted_items() {
        let mut state = OperationsQueueState {
            data_profile_key: "holding".into(),
            projection_name: "operations_read_model".into(),
            as_of: Utc::now(),
            freshness: OperationsProjectionFreshnessState::at(None, None, Utc::now()),
            items: vec![valid_queue_item()],
            next_cursor: None,
            has_more: false,
        };

        state.items[0].version = 0;
        assert_eq!(
            state.validate(),
            Err(OperationsCompletionError::InvalidQueueItem { field: "version" })
        );

        state.items[0].version = 1;
        state.items[0].source.source_domain = "unknown".into();
        assert_eq!(
            state.validate(),
            Err(OperationsCompletionError::InvalidQueueItem {
                field: "source_domain"
            })
        );
    }

    #[allow(dead_code)]
    fn _actor_shape_is_serializable() {
        let _ = ActorAttribution {
            principal_ref: "operator".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let _ = AuthorityScope::Holding {
            holding_id: Uuid::from_u128(0x7100),
        };
    }
}
