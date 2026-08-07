mod principal;
mod role;

pub use principal::{
    AccessPrincipal, PrincipalId, PrincipalStatus, RiskTier, RoleAssignment, RoleAssignmentId,
};
pub use role::{AccessRole, PermissionKey, RoleCode, RoleId};

pub const MAX_IDENTITY_ACCESS_TEXT_BYTES: usize = 256;

use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum IdentityAccessAggregate {
    Principal(AccessPrincipal),
    Role(AccessRole),
}

impl IdentityAccessAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::Principal(value) => value.version(),
            Self::Role(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &TenantScope {
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

pub(crate) fn validate_scope(scope: &TenantScope) -> Result<(), IdentityAccessDomainError> {
    scope
        .validate()
        .map_err(|_| IdentityAccessDomainError::InvalidScope)
}

pub(crate) fn next_version(version: u64) -> Result<u64, IdentityAccessDomainError> {
    version
        .checked_add(1)
        .ok_or(IdentityAccessDomainError::VersionOverflow)
}
