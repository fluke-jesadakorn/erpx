mod authority;
mod principal;
mod role;

pub use authority::{
    AccessCertification, BreakGlassSession, BreakGlassStatus, CertificationDecision,
    CertificationItem, FRESH_MFA_WINDOW, HighRiskAuthority, MAX_BREAK_GLASS_DURATION,
    NonHumanPrincipalGovernance, PrincipalKind, PrincipalRealm,
};
pub use principal::{
    AccessPrincipal, AccessPrincipalState, GroupMembership, GroupMembershipState, PrincipalId,
    PrincipalStatus, RiskTier, RoleAssignment, RoleAssignmentId, RoleAssignmentState,
    principal_activation_decision_ref,
};
pub use role::{AccessRole, AccessRoleState, PermissionKey, RoleCode, RoleId};

pub const MAX_IDENTITY_ACCESS_TEXT_BYTES: usize = 256;

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum IdentityAccessAggregate {
    Principal(AccessPrincipal),
    Role(AccessRole),
}

/// Standalone governance records are first-class lifecycle roots, separate
/// from principals and roles so review/break-glass evidence cannot be hidden
/// inside an identity snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", content = "state", rename_all = "snake_case")]
pub enum IdentityGovernanceRecord {
    AccessReview(AccessCertification),
    BreakGlass(BreakGlassSession),
}

impl IdentityGovernanceRecord {
    pub const fn id_uuid(&self) -> uuid::Uuid {
        match self {
            Self::AccessReview(value) => value.id(),
            Self::BreakGlass(value) => value.id(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::AccessReview(value) => value.version(),
            Self::BreakGlass(value) => value.version(),
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::AccessReview(value) => value.scope(),
            Self::BreakGlass(value) => value.scope(),
        }
    }
    pub const fn record_type(&self) -> &'static str {
        match self {
            Self::AccessReview(_) => "identity.access-review",
            Self::BreakGlass(_) => "identity.break-glass-session",
        }
    }
    pub const fn lifecycle_state(&self) -> &'static str {
        match self {
            Self::AccessReview(value) if value.completed_at().is_some() => "completed",
            Self::AccessReview(_) => "open",
            Self::BreakGlass(value) => match value.status() {
                BreakGlassStatus::Requested => "requested",
                BreakGlassStatus::Active => "active",
                BreakGlassStatus::Closed => "closed",
                BreakGlassStatus::Expired => "expired",
            },
        }
    }
    pub fn validate_rehydrated(&self) -> Result<(), IdentityAccessDomainError> {
        match self {
            Self::AccessReview(value) => value.validate_rehydrated(),
            Self::BreakGlass(value) => value.validate_rehydrated(),
        }
    }
}

impl IdentityAccessAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Principal(value) => value.version(),
            Self::Role(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Principal(value) => value.scope(),
            Self::Role(value) => value.scope(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Principal(_) => "access_principal",
            Self::Role(_) => "access_role",
        }
    }

    pub const fn aggregate_uuid(&self) -> uuid::Uuid {
        match self {
            Self::Principal(value) => value.id().as_uuid(),
            Self::Role(value) => value.id().as_uuid(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Principal(value) => value.take_uncommitted_events(),
            Self::Role(value) => value.take_uncommitted_events(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentityAccessDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("identity access scope is invalid")]
    InvalidScope,
    #[error("principal transition is not allowed: {from:?} -> {to:?}")]
    InvalidPrincipalTransition {
        from: PrincipalStatus,
        to: PrincipalStatus,
    },
    #[error("disabled principals cannot receive role assignments")]
    PrincipalDisabled,
    #[error("role assignment scope is outside the principal scope")]
    AssignmentScopeOutsidePrincipal,
    #[error("role assignment window must be finite and end after it starts")]
    InvalidAssignmentWindow,
    #[error("role assignment exceeds the {risk_tier:?} maximum duration")]
    AssignmentDurationExceeded { risk_tier: RiskTier },
    #[error("high-risk role assignments require an independent approver")]
    IndependentApproverRequired,
    #[error("an equivalent active role assignment already exists")]
    DuplicateActiveAssignment,
    #[error("role assignment was not found")]
    AssignmentNotFound,
    #[error("role assignment was already revoked")]
    AssignmentAlreadyRevoked,
    #[error("a role must contain at least one unique permission")]
    InvalidPermissionSet,
    #[error("{field} contains characters outside the canonical key alphabet")]
    InvalidKey { field: &'static str },
    #[error("identity access aggregate version overflowed")]
    VersionOverflow,
    #[error("identity access timestamp window is invalid")]
    InvalidTimestamp,
    #[error("fresh MFA evidence is required")]
    FreshMfaRequired,
    #[error("break-glass duration or timing is invalid")]
    InvalidBreakGlassWindow,
    #[error("break-glass lifecycle transition is not allowed")]
    InvalidBreakGlassTransition,
    #[error("break-glass monitoring must be available")]
    BreakGlassMonitoringRequired,
    #[error("break-glass after-review requires an independent reviewer")]
    IndependentAfterReviewRequired,
    #[error("access certification must contain at least one item")]
    EmptyCertification,
    #[error("access certification contains a duplicate assignment")]
    DuplicateCertificationItem,
    #[error("access certification is closed or overdue")]
    CertificationClosed,
    #[error("the certification reviewer has a conflict of interest")]
    ConflictedCertificationReviewer,
    #[error("a certification decision is final")]
    CertificationDecisionFinal,
    #[error("authority version conflict (expected {expected}, actual {actual})")]
    AuthorityVersionConflict { expected: u64, actual: u64 },
    #[error("principal kind and non-human governance do not match")]
    InvalidPrincipalGovernance,
    #[error("group membership is invalid for this principal")]
    InvalidGroupMembership,
    #[error("an active group membership already exists")]
    DuplicateGroupMembership,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    PrincipalAccessChanged {
        principal_id: PrincipalId,
        display_name: String,
        status: PrincipalStatus,
        assigned_roles: Vec<RoleId>,
        at: DateTime<Utc>,
    },
    PrincipalDisabled {
        principal_id: PrincipalId,
        at: DateTime<Utc>,
    },
    RoleDefinitionChanged {
        role_id: RoleId,
        code: String,
        at: DateTime<Utc>,
    },
}

pub(crate) fn validate_text(
    value: &str,
    field: &'static str,
) -> Result<(), IdentityAccessDomainError> {
    if value.trim().is_empty() {
        return Err(IdentityAccessDomainError::Required { field });
    }
    if value.len() > MAX_IDENTITY_ACCESS_TEXT_BYTES {
        return Err(IdentityAccessDomainError::TooLong {
            field,
            maximum: MAX_IDENTITY_ACCESS_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(IdentityAccessDomainError::ControlCharacter { field });
    }
    Ok(())
}

pub(crate) fn validate_scope(scope: &AuthorityScope) -> Result<(), IdentityAccessDomainError> {
    scope
        .validate()
        .map_err(|_| IdentityAccessDomainError::InvalidScope)
}

pub(crate) fn next_version(version: u64) -> Result<u64, IdentityAccessDomainError> {
    version
        .checked_add(1)
        .ok_or(IdentityAccessDomainError::VersionOverflow)
}
