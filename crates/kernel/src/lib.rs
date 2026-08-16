#![doc = "Transport- and persistence-neutral primitives shared by ERPX contexts."]

use std::{collections::BTreeMap, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_REFERENCE_LENGTH: usize = 256;
pub const MAX_AUDIT_PURPOSE_LENGTH: usize = 256;
pub const MAX_ERROR_CONTEXT_ENTRIES: usize = 16;
pub const MAX_ERROR_CONTEXT_VALUE_LENGTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum IdentifierError {
    #[error("identifier must not be nil")]
    Nil,
}

macro_rules! uuid_identifier {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, IdentifierError> {
                if value.is_nil() {
                    return Err(IdentifierError::Nil);
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = IdentifierError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_identifier!(AggregateId);
uuid_identifier!(BranchId);
uuid_identifier!(CompanyId);
uuid_identifier!(CommandId);
uuid_identifier!(CorrelationId);
uuid_identifier!(DataPlaneId);
uuid_identifier!(EventId);
uuid_identifier!(HoldingId);
uuid_identifier!(PolicyId);
uuid_identifier!(PrincipalId);
uuid_identifier!(QueryId);
uuid_identifier!(RecordId);
uuid_identifier!(SessionId);
uuid_identifier!(TenantId);

/// Classification carried by records, fields, events, and provider payloads.
///
/// Ordering is intentional: a larger value is always at least as restrictive
/// as a smaller value. Adapters may narrow access further, but must never use a
/// less restrictive classification than the source value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl DataClassification {
    /// Returns the stricter classification of two independently classified
    /// values. This is the propagation rule for envelopes and projections.
    pub const fn stricter(self, other: Self) -> Self {
        if (self as u8) >= (other as u8) {
            self
        } else {
            other
        }
    }

    /// Returns whether a policy ceiling permits this classification.
    pub const fn is_permitted_by(self, ceiling: Self) -> bool {
        (self as u8) <= (ceiling as u8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScopeError {
    #[error("tenant scope contains a nil identifier")]
    NilIdentifier,
    #[error("tenant subtree must select at least one company or branch")]
    EmptySubtree,
    #[error("tenant subtree contains duplicate identifiers")]
    DuplicateIdentifier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantScope {
    Platform,
    Holding {
        holding_id: Uuid,
    },
    Company {
        holding_id: Uuid,
        company_id: Uuid,
    },
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Uuid,
    },
    Subtree {
        holding_id: Uuid,
        company_ids: Vec<Uuid>,
        branch_ids: Vec<Uuid>,
    },
}

impl TenantScope {
    pub fn validate(&self) -> Result<(), ScopeError> {
        let non_nil = |value: &Uuid| !value.is_nil();
        match self {
            Self::Platform => Ok(()),
            Self::Holding { holding_id } if non_nil(holding_id) => Ok(()),
            Self::Company {
                holding_id,
                company_id,
            } if non_nil(holding_id) && non_nil(company_id) => Ok(()),
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } if non_nil(holding_id) && non_nil(company_id) && non_nil(branch_id) => Ok(()),
            Self::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => {
                if !non_nil(holding_id)
                    || !company_ids.iter().all(non_nil)
                    || !branch_ids.iter().all(non_nil)
                {
                    return Err(ScopeError::NilIdentifier);
                }
                if company_ids.is_empty() && branch_ids.is_empty() {
                    return Err(ScopeError::EmptySubtree);
                }
                if contains_duplicate(company_ids) || contains_duplicate(branch_ids) {
                    return Err(ScopeError::DuplicateIdentifier);
                }
                Ok(())
            }
            _ => Err(ScopeError::NilIdentifier),
        }
    }

    /// Returns whether this authority contains every record selected by `child`.
    pub fn contains(&self, child: &Self) -> bool {
        if self.validate().is_err() || child.validate().is_err() {
            return false;
        }
        match (self, child) {
            (Self::Platform, _) => true,
            (Self::Holding { holding_id: parent }, Self::Holding { holding_id: child }) => {
                parent == child
            }
            (
                Self::Holding { holding_id: parent },
                Self::Company {
                    holding_id: child, ..
                }
                | Self::Branch {
                    holding_id: child, ..
                }
                | Self::Subtree {
                    holding_id: child, ..
                },
            ) => parent == child,
            (
                Self::Company {
                    holding_id: parent_holding,
                    company_id: parent_company,
                },
                Self::Company {
                    holding_id: child_holding,
                    company_id: child_company,
                }
                | Self::Branch {
                    holding_id: child_holding,
                    company_id: child_company,
                    ..
                },
            ) => parent_holding == child_holding && parent_company == child_company,
            (
                Self::Company {
                    holding_id: parent_holding,
                    company_id: parent_company,
                },
                Self::Subtree {
                    holding_id: child_holding,
                    company_ids,
                    branch_ids,
                },
            ) => {
                parent_holding == child_holding
                    && company_ids.iter().all(|id| id == parent_company)
                    && branch_ids.is_empty()
            }
            (
                Self::Branch {
                    holding_id: parent_holding,
                    company_id: parent_company,
                    branch_id: parent_branch,
                },
                Self::Branch {
                    holding_id: child_holding,
                    company_id: child_company,
                    branch_id: child_branch,
                },
            ) => {
                parent_holding == child_holding
                    && parent_company == child_company
                    && parent_branch == child_branch
            }
            (
                Self::Subtree {
                    holding_id: parent_holding,
                    company_ids: parent_companies,
                    branch_ids: parent_branches,
                },
                Self::Company {
                    holding_id: child_holding,
                    company_id: child_company,
                },
            ) => {
                parent_holding == child_holding
                    && parent_branches.is_empty()
                    && parent_companies.contains(child_company)
            }
            (
                Self::Subtree {
                    holding_id: parent_holding,
                    company_ids: parent_companies,
                    branch_ids: parent_branches,
                },
                Self::Branch {
                    holding_id: child_holding,
                    company_id: child_company,
                    branch_id: child_branch,
                },
            ) => {
                parent_holding == child_holding
                    && parent_companies.contains(child_company)
                    && parent_branches.contains(child_branch)
            }
            (
                Self::Subtree {
                    holding_id: parent_holding,
                    company_ids: parent_companies,
                    branch_ids: parent_branches,
                },
                Self::Subtree {
                    holding_id: child_holding,
                    company_ids: child_companies,
                    branch_ids: child_branches,
                },
            ) => {
                parent_holding == child_holding
                    && (parent_branches.is_empty() || !child_branches.is_empty())
                    && child_companies
                        .iter()
                        .all(|id| parent_companies.contains(id))
                    && child_branches.iter().all(|id| parent_branches.contains(id))
            }
            _ => self == child,
        }
    }
}

fn contains_duplicate(values: &[Uuid]) -> bool {
    let unique = values.iter().collect::<std::collections::BTreeSet<_>>();
    unique.len() != values.len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    PlatformUser,
    TenantUser,
    Agent,
    Service,
    SystemWorker,
}

/// Principal categories are identities, not authorization grants. In
/// particular, an agent or group mention never increases effective authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    Human,
    Agent,
    Service,
    HumanGroup,
    AgentPool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorAttribution {
    pub principal_ref: String,
    pub kind: ActorKind,
    pub session_id: Option<SessionId>,
    pub represented_principal_ref: Option<String>,
    pub support_capability_id: Option<Uuid>,
}

impl ActorAttribution {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_reference(&self.principal_ref, "principal_ref")?;
        if let Some(reference) = &self.represented_principal_ref {
            validate_reference(reference, "represented_principal_ref")?;
        }
        if self.support_capability_id.is_some_and(|id| id.is_nil()) {
            return Err(ValidationError::InvalidIdentifier("support_capability_id"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditContext {
    pub purpose: String,
    pub customer_visible: bool,
    pub module: Option<String>,
}

impl AuditContext {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_bounded_text(&self.purpose, "audit purpose", MAX_AUDIT_PURPOSE_LENGTH)?;
        if let Some(module) = &self.module {
            validate_reference(module, "audit module")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("{0} must not be empty")]
    Empty(&'static str),
    #[error("{0} exceeds its maximum length")]
    TooLong(&'static str),
    #[error("{0} contains an invalid identifier")]
    InvalidIdentifier(&'static str),
}

pub fn validate_bounded_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::Empty(field));
    }
    if value.len() > maximum {
        return Err(ValidationError::TooLong(field));
    }
    Ok(())
}

fn validate_reference(value: &str, field: &'static str) -> Result<(), ValidationError> {
    validate_bounded_text(value, field, MAX_REFERENCE_LENGTH)
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    pub const fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Invalid,
    Denied,
    NotFound,
    Conflict,
    NotReady,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorContext {
    values: BTreeMap<String, String>,
}

impl ErrorContext {
    pub fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub fn try_insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), ErrorContextError> {
        let key = key.into();
        let value = value.into();
        if self.values.len() >= MAX_ERROR_CONTEXT_ENTRIES && !self.values.contains_key(&key) {
            return Err(ErrorContextError::TooManyEntries);
        }
        if key.trim().is_empty() || key.len() > MAX_REFERENCE_LENGTH {
            return Err(ErrorContextError::InvalidKey);
        }
        if value.len() > MAX_ERROR_CONTEXT_VALUE_LENGTH {
            return Err(ErrorContextError::ValueTooLong);
        }
        self.values.insert(key, value);
        Ok(())
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ErrorContextError {
    #[error("error context has too many entries")]
    TooManyEntries,
    #[error("error context key is invalid")]
    InvalidKey,
    #[error("error context value is too long")]
    ValueTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelError {
    pub code: String,
    pub category: ErrorCategory,
    pub safe_message: String,
    pub retryable: bool,
    pub correlation_id: CorrelationId,
    pub context: ErrorContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn nil_typed_identifiers_fail_closed() {
        assert_eq!(
            CommandId::try_from_uuid(Uuid::nil()),
            Err(IdentifierError::Nil)
        );
        assert!(CommandId::try_from_uuid(id(1)).is_ok());
    }

    #[test]
    fn branch_authority_does_not_imply_siblings_or_parent() {
        let branch = TenantScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        };
        let sibling = TenantScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(4),
        };
        let company = TenantScope::Company {
            holding_id: id(1),
            company_id: id(2),
        };
        assert!(branch.contains(&branch));
        assert!(!branch.contains(&sibling));
        assert!(!branch.contains(&company));
        assert!(company.contains(&branch));
    }

    #[test]
    fn invalid_subtrees_never_authorize() {
        let invalid = TenantScope::Subtree {
            holding_id: id(1),
            company_ids: vec![],
            branch_ids: vec![],
        };
        assert_eq!(invalid.validate(), Err(ScopeError::EmptySubtree));
        assert!(!invalid.contains(&TenantScope::Platform));
    }

    #[test]
    fn error_context_is_bounded() {
        let mut context = ErrorContext::empty();
        for index in 0..MAX_ERROR_CONTEXT_ENTRIES {
            context.try_insert(format!("key-{index}"), "value").unwrap();
        }
        assert_eq!(
            context.try_insert("overflow", "value"),
            Err(ErrorContextError::TooManyEntries)
        );
    }

    #[test]
    fn data_classification_propagates_the_strictest_value() {
        assert_eq!(
            DataClassification::Internal.stricter(DataClassification::Restricted),
            DataClassification::Restricted
        );
        assert!(DataClassification::Confidential.is_permitted_by(DataClassification::Restricted));
        assert!(!DataClassification::Restricted.is_permitted_by(DataClassification::Confidential));
    }
}
