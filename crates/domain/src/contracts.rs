//! Contracts shared by write-side commands, read-side queries,
//! workers, and client adapters. Transport adapters may encode these values
//! in protobuf, but the domain representation remains transport-independent.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
pub use erpx_kernel::{AuthorityScope, HoldingId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const CONTRACT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorKind {
    PlatformUser,
    HoldingUser,
    Service,
    SystemWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorAttribution {
    pub principal_ref: String,
    pub kind: ActorKind,
    pub session_id: Option<Uuid>,
    pub represented_principal_ref: Option<String>,
    pub support_capability_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditContext {
    pub purpose: String,
    pub customer_visible: bool,
    pub module: Option<String>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContext {
    pub contract_version: u16,
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub actor: ActorAttribution,
    pub active_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_aggregate_version: Option<u64>,
    pub subscription_id: Option<Uuid>,
}

impl CommandContext {
    pub fn try_new(
        idempotency_key: impl Into<String>,
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        purpose: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let idempotency_key = idempotency_key.into();
        let purpose = purpose.into();
        if idempotency_key.trim().is_empty() {
            return Err(ContractError::EmptyIdempotencyKey);
        }
        if purpose.trim().is_empty() {
            return Err(ContractError::EmptyAuditPurpose);
        }
        let correlation_id = Uuid::new_v4();
        Ok(Self {
            contract_version: CONTRACT_VERSION,
            command_id: Uuid::new_v4(),
            idempotency_key,
            correlation_id,
            causation_id: None,
            actor,
            active_scope,
            audit: AuditContext {
                purpose,
                customer_visible: false,
                module: None,
                correlation_id,
            },
            expected_aggregate_version: None,
            subscription_id: None,
        })
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.idempotency_key.trim().is_empty() {
            return Err(ContractError::EmptyIdempotencyKey);
        }
        if self.audit.purpose.trim().is_empty() {
            return Err(ContractError::EmptyAuditPurpose);
        }
        if self.audit.correlation_id != self.correlation_id {
            return Err(ContractError::CorrelationMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryConsistency {
    Eventual,
    ReadYourWrites {
        command_id: Uuid,
    },
    AtLeastAggregateVersion {
        aggregate_type: String,
        aggregate_id: Uuid,
        version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryContext {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub correlation_id: Uuid,
    pub actor: ActorAttribution,
    pub active_scope: AuthorityScope,
    pub consistency: QueryConsistency,
    pub audit: AuditContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionFreshness {
    pub projection_name: String,
    pub last_event_id: Option<Uuid>,
    pub last_projected_at: Option<DateTime<Utc>>,
    pub lag_seconds: u64,
    pub source_aggregate_version: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub contract_version: u16,
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: u16,
    pub occurred_at: DateTime<Utc>,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub active_scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    Unauthenticated,
    Forbidden,
    NotFound,
    Conflict,
    InvalidArgument,
    FailedPrecondition,
    RateLimited,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorContext {
    pub code: ErrorCode,
    pub message: String,
    pub correlation_id: Uuid,
    pub retryable: bool,
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("idempotency key cannot be empty")]
    EmptyIdempotencyKey,
    #[error("audit purpose cannot be empty")]
    EmptyAuditPurpose,
    #[error("audit and request correlation IDs must match")]
    CorrelationMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "user-1".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn holding_scope_never_leaks_to_parent_or_sibling() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let parent = AuthorityScope::Company {
            holding_id: holding,
            company_id: company,
        };
        assert!(parent.contains(&AuthorityScope::Branch {
            holding_id: holding,
            company_id: company,
            branch_id: branch,
        }));
        assert!(!parent.contains(&AuthorityScope::Company {
            holding_id: holding,
            company_id: sibling,
        }));
        assert!(
            !AuthorityScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            }
            .contains(&parent)
        );
    }

    #[test]
    fn command_context_carries_idempotency_and_correlated_audit() {
        let context =
            CommandContext::try_new("command-1", actor(), AuthorityScope::Platform, "test")
                .expect("valid context");
        assert_eq!(context.contract_version, CONTRACT_VERSION);
        assert_eq!(context.audit.correlation_id, context.correlation_id);
        assert_eq!(context.idempotency_key, "command-1");
        context.validate().expect("context validation");
        assert_eq!(
            CommandContext::try_new(" ", actor(), AuthorityScope::Platform, "test"),
            Err(ContractError::EmptyIdempotencyKey)
        );
        assert_eq!(
            CommandContext::try_new("command-2", actor(), AuthorityScope::Platform, " "),
            Err(ContractError::EmptyAuditPurpose)
        );
    }

    #[test]
    fn event_envelope_serializes_version_and_causation() {
        let event = EventEnvelope {
            contract_version: CONTRACT_VERSION,
            event_id: Uuid::new_v4(),
            event_type: "customer.created".into(),
            event_version: 1,
            occurred_at: Utc::now(),
            aggregate_type: "customer".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            active_scope: AuthorityScope::Platform,
            actor: actor(),
            correlation_id: Uuid::new_v4(),
            causation_id: Some(Uuid::new_v4()),
            idempotency_key: Some("customer-create-1".into()),
            payload: serde_json::json!({"name": "fixture"}),
        };
        let json = serde_json::to_string(&event).expect("event JSON");
        assert!(json.contains("customer.created"));
        assert!(json.contains("aggregate_version"));
    }

    #[test]
    fn branch_restricted_subtree_cannot_authorize_a_whole_company() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let restricted = AuthorityScope::Subtree {
            holding_id: holding,
            organization_node_id: branch,
        };
        assert!(!restricted.contains(&AuthorityScope::Company {
            holding_id: holding,
            company_id: company,
        }));
    }
}
