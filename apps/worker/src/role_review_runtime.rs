//! Durable role-review snapshot evaluation.
//!
//! The role-assignment rows remain authoritative. This handler reads them from
//! the control write database, evaluates the domain review classifier for one
//! explicitly configured scope, and persists the bounded snapshot plus expiry
//! notification queue atomically. It never changes role assignments.

use std::env;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::access::{RoleAssignment, Scope, build_role_review_queue};
use erpx_infrastructure::RoleReviewPersistence;
use erpx_migration::g13::content_hash;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const ROLE_REVIEW_SNAPSHOT_HANDLER_NAME: &str = "role_review_snapshot";
const MAX_ROLE_REVIEW_ASSIGNMENTS: i64 = 10_000;
const ROLE_REVIEW_ASSIGNMENT_PAGE_SIZE: i64 = 500;
const MAX_ROLE_REVIEW_HORIZON: Duration = Duration::days(365);

#[derive(Debug, Clone)]
pub struct RoleReviewSnapshotRuntimeHandler {
    pool: PgPool,
    persistence: RoleReviewPersistence,
    scope: Scope,
    actor_ref: String,
    horizon: Duration,
}

impl RoleReviewSnapshotRuntimeHandler {
    pub fn from_env(pool: PgPool) -> Result<Self, WorkerRuntimeError> {
        let scope = parse_scope(&env::var("ERPX_ROLE_REVIEW_SCOPE").map_err(|_| {
            WorkerRuntimeError::Configuration(
                "ERPX_ROLE_REVIEW_SCOPE is required for role-review snapshots".into(),
            )
        })?)?;
        let actor_ref = env::var("ERPX_ROLE_REVIEW_ACTOR_REF")
            .unwrap_or_else(|_| "erpx-role-review-snapshot".into());
        if actor_ref.trim().is_empty()
            || actor_ref.len() > 255
            || actor_ref.chars().any(char::is_control)
        {
            return Err(WorkerRuntimeError::Configuration(
                "ERPX_ROLE_REVIEW_ACTOR_REF is invalid".into(),
            ));
        }
        let horizon_seconds = env::var("ERPX_ROLE_REVIEW_HORIZON_SECONDS")
            .unwrap_or_else(|_| "604800".into())
            .parse::<i64>()
            .map_err(|_| {
                WorkerRuntimeError::Configuration(
                    "ERPX_ROLE_REVIEW_HORIZON_SECONDS must be an integer".into(),
                )
            })?;
        let horizon = Duration::seconds(horizon_seconds);
        if horizon <= Duration::zero() || horizon > MAX_ROLE_REVIEW_HORIZON {
            return Err(WorkerRuntimeError::Configuration(
                "ERPX_ROLE_REVIEW_HORIZON_SECONDS is outside the permitted range".into(),
            ));
        }
        Ok(Self {
            persistence: RoleReviewPersistence::new(pool.clone()),
            pool,
            scope,
            actor_ref,
            horizon,
        })
    }

    async fn load_assignments(&self) -> Result<Vec<RoleAssignment>, WorkerRuntimeError> {
        let mut assignments = Vec::new();
        let mut last_valid_until: Option<DateTime<Utc>> = None;
        let mut last_id: Option<Uuid> = None;

        loop {
            let rows = sqlx::query(
                "SELECT id, principal_ref, role_key, parent_assignment_id,
                        valid_from, valid_until, assignment_version, payload
                 FROM erpx_control.role_assignments
                 WHERE ($1::timestamptz IS NULL
                    OR (valid_until, id) > ($1::timestamptz, $2::uuid))
                 ORDER BY valid_until, id
                 LIMIT $3",
            )
            .bind(last_valid_until)
            .bind(last_id)
            .bind(ROLE_REVIEW_ASSIGNMENT_PAGE_SIZE)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("role-review assignment read failed".into())
            })?;
            if rows.is_empty() {
                break;
            }

            for row in rows {
                last_valid_until = Some(row.try_get("valid_until").map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "role-review assignment metadata is invalid".into(),
                    )
                })?);
                last_id = Some(row.try_get("id").map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "role-review assignment metadata is invalid".into(),
                    )
                })?);
                let payload: Value = row.try_get("payload").map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "role-review assignment payload is invalid".into(),
                    )
                })?;
                let assignment: RoleAssignment = serde_json::from_value(payload).map_err(|_| {
                    WorkerRuntimeError::Configuration(
                        "role-review assignment payload is invalid".into(),
                    )
                })?;
                validate_assignment_row_binding(
                    row.try_get("id").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("principal_ref").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("role_key").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("parent_assignment_id").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("valid_from").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("valid_until").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    row.try_get("assignment_version").map_err(|_| {
                        WorkerRuntimeError::Configuration(
                            "role-review assignment metadata is invalid".into(),
                        )
                    })?,
                    &assignment,
                )?;
                append_scoped_assignment(&mut assignments, &self.scope, assignment)?;
            }
        }
        Ok(assignments)
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_assignment_row_binding(
    row_id: Uuid,
    row_principal_ref: String,
    row_role_key: String,
    row_parent_assignment_id: Option<Uuid>,
    row_valid_from: DateTime<Utc>,
    row_valid_until: DateTime<Utc>,
    row_assignment_version: i32,
    assignment: &RoleAssignment,
) -> Result<(), WorkerRuntimeError> {
    let row_assignment_version = u32::try_from(row_assignment_version).map_err(|_| {
        WorkerRuntimeError::Configuration("role-review assignment metadata is invalid".into())
    })?;
    if row_id != assignment.id
        || row_principal_ref != assignment.principal_ref
        || row_role_key != assignment.role_key
        || row_parent_assignment_id != assignment.parent_assignment_id
        || row_valid_from != assignment.valid_from
        || row_valid_until != assignment.valid_until
        || row_assignment_version != assignment.assignment_version
    {
        return Err(WorkerRuntimeError::Configuration(
            "role-review assignment payload is not bound to its authoritative row".into(),
        ));
    }
    Ok(())
}

fn append_scoped_assignment(
    assignments: &mut Vec<RoleAssignment>,
    scope: &Scope,
    assignment: RoleAssignment,
) -> Result<(), WorkerRuntimeError> {
    if !scope.contains(&assignment.scope) {
        return Ok(());
    }
    if assignments.len() >= MAX_ROLE_REVIEW_ASSIGNMENTS as usize {
        return Err(WorkerRuntimeError::Configuration(
            "role-review assignment volume exceeds the permitted scope bound".into(),
        ));
    }
    assignments.push(assignment);
    Ok(())
}

#[async_trait::async_trait]
impl NamedWorkerHandler for RoleReviewSnapshotRuntimeHandler {
    fn name(&self) -> &str {
        ROLE_REVIEW_SNAPSHOT_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "role-review snapshot worker identity is required".into(),
            ));
        }
        let assignments = self
            .load_assignments()
            .await?
            .into_iter()
            .filter(|assignment| self.scope.contains(&assignment.scope))
            .collect::<Vec<_>>();
        let now = Utc::now();
        let queue = build_role_review_queue(&assignments, now, self.horizon).map_err(|_| {
            WorkerRuntimeError::Configuration("role-review queue evaluation failed".into())
        })?;
        let queue_json = serde_json::to_value(&queue).map_err(|_| {
            WorkerRuntimeError::Configuration("role-review queue serialization failed".into())
        })?;
        let request_hash = content_hash(&queue_json).map_err(|_| {
            WorkerRuntimeError::Configuration("role-review queue hash failed".into())
        })?;
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "role-review-snapshot:{}:{request_hash}",
                self.scope.canonical_key()
            )
            .as_bytes(),
        );
        let receipt = self
            .persistence
            .persist_snapshot(
                &self.scope,
                &queue,
                &self.actor_ref,
                correlation_id,
                &format!(
                    "role-review-snapshot:{}:{request_hash}",
                    self.scope.canonical_key()
                ),
                &request_hash,
            )
            .await
            .map_err(|_| {
                WorkerRuntimeError::Configuration("role-review snapshot persistence failed".into())
            })?;
        Ok(WorkerHandlerReport {
            claimed: assignments.len() as u64,
            processed: receipt.item_count as u64,
            published: receipt.item_count as u64,
            has_work: !assignments.is_empty(),
            ..WorkerHandlerReport::default()
        })
    }
}

fn parse_scope(value: &str) -> Result<Scope, WorkerRuntimeError> {
    let parts = value.split(':').collect::<Vec<_>>();
    let parse_id = |field: &str| {
        Uuid::parse_str(field).map_err(|_| {
            WorkerRuntimeError::Configuration("role-review scope UUID is invalid".into())
        })
    };
    match parts.as_slice() {
        ["platform"] => Ok(Scope::Platform),
        ["holding", holding] => Ok(Scope::Holding {
            holding_id: parse_id(holding)?,
        }),
        ["company", holding, company] => Ok(Scope::Company {
            holding_id: parse_id(holding)?,
            company_id: parse_id(company)?,
        }),
        ["branch", holding, company, branch] => Ok(Scope::Branch {
            holding_id: parse_id(holding)?,
            company_id: parse_id(company)?,
            branch_id: parse_id(branch)?,
        }),
        _ => Err(WorkerRuntimeError::Configuration(
            "ERPX_ROLE_REVIEW_SCOPE must be platform, holding:<uuid>, company:<uuid>:<uuid>, or branch:<uuid>:<uuid>:<uuid>".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use erpx_domain::access::{RiskTier, RoleAssignment, Scope};
    use uuid::Uuid;

    use super::{append_scoped_assignment, parse_scope, validate_assignment_row_binding};

    fn assignment(scope: Scope, id: u128) -> RoleAssignment {
        let valid_from = Utc::now();
        RoleAssignment {
            id: Uuid::from_u128(id),
            principal_ref: format!("principal-{id}"),
            role_key: "role:review".into(),
            permissions: vec!["access.review".into()],
            functional_domain: "identity".into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope,
            valid_from,
            valid_until: valid_from + Duration::days(1),
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "test".into(),
            ticket_reference: None,
            grantor: "test".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        }
    }

    #[test]
    fn scope_parser_requires_an_explicit_supported_scope() {
        assert!(matches!(parse_scope("platform"), Ok(Scope::Platform)));
        let holding = Uuid::from_u128(1);
        assert!(matches!(
            parse_scope(&format!("holding:{holding}")),
            Ok(Scope::Holding { holding_id }) if holding_id == holding
        ));
        assert!(parse_scope("holding:not-a-uuid").is_err());
        assert!(parse_scope("all").is_err());
    }

    #[test]
    fn scoped_assignment_collection_does_not_let_unrelated_rows_consume_the_bound() {
        let scope = Scope::Holding {
            holding_id: Uuid::from_u128(1),
        };
        let mut assignments = Vec::new();
        append_scoped_assignment(
            &mut assignments,
            &scope,
            assignment(
                Scope::Holding {
                    holding_id: Uuid::from_u128(2),
                },
                1,
            ),
        )
        .expect("unrelated assignments are skipped");
        append_scoped_assignment(
            &mut assignments,
            &scope,
            assignment(
                Scope::Company {
                    holding_id: Uuid::from_u128(1),
                    company_id: Uuid::from_u128(3),
                },
                2,
            ),
        )
        .expect("in-scope assignment is retained");
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].id, Uuid::from_u128(2));
    }

    #[test]
    fn assignment_payload_must_match_authoritative_row_identity_and_window() {
        let scope = Scope::Platform;
        let assignment = assignment(scope, 1);
        let error = validate_assignment_row_binding(
            assignment.id,
            assignment.principal_ref.clone(),
            assignment.role_key.clone(),
            assignment.parent_assignment_id,
            assignment.valid_from,
            assignment.valid_until + Duration::seconds(1),
            assignment.assignment_version as i32,
            &assignment,
        )
        .expect_err("a forged authoritative expiry must fail closed");
        assert!(matches!(
            error,
            super::WorkerRuntimeError::Configuration(message)
                if message.contains("not bound to its authoritative row")
        ));
    }

    #[test]
    fn assignment_payload_must_match_authoritative_parent_and_version() {
        let assignment = assignment(Scope::Platform, 2);
        let error = validate_assignment_row_binding(
            assignment.id,
            assignment.principal_ref.clone(),
            assignment.role_key.clone(),
            Some(Uuid::from_u128(99)),
            assignment.valid_from,
            assignment.valid_until,
            2,
            &assignment,
        )
        .expect_err("a forged parent or version must fail closed");
        assert!(matches!(
            error,
            super::WorkerRuntimeError::Configuration(message)
                if message.contains("not bound to its authoritative row")
        ));
    }
}
