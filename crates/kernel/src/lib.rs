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

pub mod feed;

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
    #[error("authority scope contains a nil identifier")]
    NilIdentifier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorityScope {
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
        organization_node_id: Uuid,
    },
}

impl AuthorityScope {
    /// Stable representation for persistence keys and audited capability
    /// bindings. Transport adapters must still encode the structured scope.
    pub fn canonical_key(&self) -> String {
        match self {
            Self::Platform => "platform".into(),
            Self::Holding { holding_id } => format!("holding:{holding_id}"),
            Self::Company {
                holding_id,
                company_id,
            } => format!("company:{holding_id}:{company_id}"),
            Self::Branch {
                holding_id,
                company_id,
                branch_id,
            } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
            Self::Subtree {
                holding_id,
                organization_node_id,
            } => format!("subtree:{holding_id}:{organization_node_id}"),
        }
    }

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
                organization_node_id,
            } if non_nil(holding_id) && non_nil(organization_node_id) => Ok(()),
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
                    organization_node_id: parent_node,
                },
                Self::Subtree {
                    holding_id: child_holding,
                    organization_node_id: child_node,
                },
            ) => parent_holding == child_holding && parent_node == child_node,
            _ => self == child,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    PlatformUser,
    HoldingUser,
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
    /// Returns whether this attribution represents a directly authenticated
    /// human actor. Agent, service, and worker identities never become human
    /// merely because they carry a represented principal reference.
    pub const fn is_human(&self) -> bool {
        matches!(self.kind, ActorKind::PlatformUser | ActorKind::HoldingUser)
    }

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

/// In-process evidence required by a business decision that only an
/// authenticated human may make.
///
/// This proof does not grant authority by itself. Command boundaries must
/// still evaluate role, scope, policy, separation-of-duties, and approval
/// limits before constructing it, and must never populate `ActorAttribution`
/// from an unverified transport payload. This is a type-level domain guard,
/// not a cryptographic authentication token. Private fields prevent callers
/// from bypassing its human/session/scope validation with an actor string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanDecisionProof {
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    decision_ref: String,
    decided_at: DateTime<Utc>,
}

impl HumanDecisionProof {
    pub fn try_new(
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        decision_ref: impl Into<String>,
        decided_at: DateTime<Utc>,
    ) -> Result<Self, HumanDecisionError> {
        actor.validate().map_err(HumanDecisionError::InvalidActor)?;
        authority_scope
            .validate()
            .map_err(HumanDecisionError::InvalidScope)?;
        let decision_ref = decision_ref.into();
        validate_reference(&decision_ref, "human decision reference")
            .map_err(|_| HumanDecisionError::InvalidDecisionReference)?;
        if !actor.is_human() {
            return Err(HumanDecisionError::HumanActorRequired);
        }
        if actor.session_id.is_none() {
            return Err(HumanDecisionError::AuthenticatedSessionRequired);
        }
        if actor.represented_principal_ref.is_some() || actor.support_capability_id.is_some() {
            return Err(HumanDecisionError::RepresentedDecisionDenied);
        }
        Ok(Self {
            actor,
            authority_scope,
            decision_ref,
            decided_at,
        })
    }

    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }

    pub fn principal_ref(&self) -> &str {
        &self.actor.principal_ref
    }

    pub const fn authority_scope(&self) -> &AuthorityScope {
        &self.authority_scope
    }

    pub fn decision_ref(&self) -> &str {
        &self.decision_ref
    }

    pub const fn decided_at(&self) -> DateTime<Utc> {
        self.decided_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HumanDecisionError {
    #[error("human decision actor is invalid: {0}")]
    InvalidActor(ValidationError),
    #[error("human decision scope is invalid: {0}")]
    InvalidScope(ScopeError),
    #[error("human decision reference is invalid")]
    InvalidDecisionReference,
    #[error("business decisions require a human actor")]
    HumanActorRequired,
    #[error("business decisions require an authenticated human session")]
    AuthenticatedSessionRequired,
    #[error("represented or support-session actors cannot make business decisions")]
    RepresentedDecisionDenied,
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
        let branch = AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        };
        let sibling = AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(4),
        };
        let company = AuthorityScope::Company {
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
        let invalid = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: Uuid::nil(),
        };
        assert_eq!(invalid.validate(), Err(ScopeError::NilIdentifier));
        assert!(!invalid.contains(&AuthorityScope::Platform));
    }

    #[test]
    fn subtree_ids_are_never_reinterpreted_as_company_or_branch_lineage() {
        let company = AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        };
        let branch = AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        };
        let company_shaped_subtree = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: id(2),
        };
        let branch_shaped_subtree = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: id(3),
        };

        assert!(!company.contains(&company_shaped_subtree));
        assert!(!company_shaped_subtree.contains(&company));
        assert!(!company_shaped_subtree.contains(&branch));
        assert!(!branch_shaped_subtree.contains(&branch));
        assert!(company_shaped_subtree.contains(&company_shaped_subtree));
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

    fn actor(kind: ActorKind, with_session: bool) -> ActorAttribution {
        ActorAttribution {
            principal_ref: "principal:reviewer".into(),
            kind,
            session_id: with_session
                .then(|| SessionId::try_from_uuid(id(7)).expect("non-nil session")),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn human_decision_proof_requires_a_direct_authenticated_human() {
        let scope = AuthorityScope::Holding { holding_id: id(1) };
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();

        let proof = HumanDecisionProof::try_new(
            actor(ActorKind::HoldingUser, true),
            scope.clone(),
            "command:approve:1",
            at,
        )
        .unwrap();
        assert_eq!(proof.principal_ref(), "principal:reviewer");
        assert_eq!(proof.authority_scope(), &scope);
        assert_eq!(proof.decision_ref(), "command:approve:1");
        assert_eq!(proof.decided_at(), at);

        assert_eq!(
            HumanDecisionProof::try_new(
                actor(ActorKind::Agent, true),
                scope.clone(),
                "command:approve:2",
                at,
            ),
            Err(HumanDecisionError::HumanActorRequired)
        );
        assert_eq!(
            HumanDecisionProof::try_new(
                actor(ActorKind::HoldingUser, false),
                scope,
                "command:approve:3",
                at,
            ),
            Err(HumanDecisionError::AuthenticatedSessionRequired)
        );
    }

    #[test]
    fn represented_human_session_cannot_create_a_business_decision_proof() {
        let mut represented = actor(ActorKind::PlatformUser, true);
        represented.represented_principal_ref = Some("principal:customer".into());
        represented.support_capability_id = Some(id(8));

        assert_eq!(
            HumanDecisionProof::try_new(
                represented,
                AuthorityScope::Platform,
                "command:approve:4",
                DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            ),
            Err(HumanDecisionError::RepresentedDecisionDenied)
        );
    }
}
