//! Versioned contracts shared by write-side commands, read-side queries,
//! workers, and client adapters. Transport adapters may encode these values
//! in protobuf, but the domain representation remains transport-independent.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const CONTRACT_VERSION: u16 = 1;

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
    /// Returns whether `self` authorizes every record represented by `child`.
    /// A branch never implies its company, parent holding, or siblings.
    pub fn contains(&self, child: &Self) -> bool {
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
                },
            ) => parent == child,
            (
                Self::Holding { holding_id: parent },
                Self::Subtree {
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
                },
            ) => parent_holding == child_holding && parent_company == child_company,
            (
                Self::Company {
                    holding_id: parent_holding,
                    company_id: parent_company,
                },
                Self::Branch {
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
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorKind {
    PlatformUser,
    TenantUser,
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
    pub tenant_scope: TenantScope,
    pub audit: AuditContext,
    pub expected_aggregate_version: Option<u64>,
    pub subscription_id: Option<Uuid>,
}

impl CommandContext {
    pub fn try_new(
        idempotency_key: impl Into<String>,
        actor: ActorAttribution,
        tenant_scope: TenantScope,
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
            tenant_scope,
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
    pub tenant_scope: TenantScope,
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
    pub tenant_scope: TenantScope,
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
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[test]
    fn tenant_scope_never_leaks_to_parent_or_sibling() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let parent = TenantScope::Company {
            holding_id: holding,
            company_id: company,
        };
        assert!(parent.contains(&TenantScope::Branch {
            holding_id: holding,
            company_id: company,
            branch_id: branch,
        }));
        assert!(!parent.contains(&TenantScope::Company {
            holding_id: holding,
            company_id: sibling,
        }));
        assert!(
            !TenantScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            }
            .contains(&parent)
        );
    }

    #[test]
    fn command_context_carries_idempotency_and_correlated_audit() {
        let context = CommandContext::try_new("command-1", actor(), TenantScope::Platform, "test")
            .expect("valid context");
        assert_eq!(context.contract_version, CONTRACT_VERSION);
        assert_eq!(context.audit.correlation_id, context.correlation_id);
        assert_eq!(context.idempotency_key, "command-1");
        context.validate().expect("context validation");
        assert_eq!(
            CommandContext::try_new(" ", actor(), TenantScope::Platform, "test"),
            Err(ContractError::EmptyIdempotencyKey)
        );
        assert_eq!(
            CommandContext::try_new("command-2", actor(), TenantScope::Platform, " "),
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
            tenant_scope: TenantScope::Platform,
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
        let restricted = TenantScope::Subtree {
            holding_id: holding,
            company_ids: vec![company],
            branch_ids: vec![branch],
        };
        assert!(!restricted.contains(&TenantScope::Company {
            holding_id: holding,
            company_id: company,
        }));
    }
}
