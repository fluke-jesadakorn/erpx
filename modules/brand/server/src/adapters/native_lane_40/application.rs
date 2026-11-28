//! Native lane 40 Brand application: owner query ports and SQL-free handlers.
//!
//! Composition injects the pool-backed reader behind `ManagementGuardRead`.
//! The handlers own validation and the exact retired response shape; no SQL
//! and no HTTP live here.

use crate::adapters::native_lane_40::domain::{validate_target, ActiveBranch};
use erpx_platform_runtime_server::public::http_support::serde_json::{json, Value};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin};

pub type GuardFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Query for the active-Branch management guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchGuardQuery {
    pub id: String,
    pub company_id: String,
}

/// Query for the active-Company management guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyGuardQuery {
    pub id: String,
}

/// Owner query port. The Brand crate owns the statement, the Company scope and
/// the projection; the axum router only transports bounded request facts.
pub trait ManagementGuardRead: Send + Sync {
    fn active_branch<'a>(
        &'a self,
        query: &'a BranchGuardQuery,
    ) -> GuardFuture<'a, Option<ActiveBranch>>;

    fn company_exists<'a>(&'a self, query: &'a CompanyGuardQuery) -> GuardFuture<'a, bool>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRAND_MANAGEMENT_READ_UNAVAILABLE",
        "The native Brand management read is unavailable.",
    )
}

fn map_read_error(error: BusinessError) -> BusinessError {
    if error.kind == FailureKind::InvalidInput {
        error
    } else {
        unavailable()
    }
}

/// Validate, read and render the active-Branch guard. `null` keeps the retired
/// reader's no-row semantics for a missing or inactive Branch.
pub async fn read_active_branch(
    reader: &dyn ManagementGuardRead,
    query: BranchGuardQuery,
) -> Result<Value, BusinessError> {
    validate_target(&query.id, Some(&query.company_id))?;
    let row = reader.active_branch(&query).await.map_err(map_read_error)?;
    let branch = match row {
        Some(row) => json!({ "id": row.id, "display_name": row.display_name }),
        None => Value::Null,
    };
    Ok(json!({ "branch": branch }))
}

/// Validate, read and render the active-Company existence guard.
pub async fn read_company_exists(
    reader: &dyn ManagementGuardRead,
    query: CompanyGuardQuery,
) -> Result<Value, BusinessError> {
    validate_target(&query.id, None)?;
    let exists = reader.company_exists(&query).await.map_err(map_read_error)?;
    Ok(json!({ "exists": exists }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_40::domain::ActiveBranch;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Fake {
        branch_calls: Arc<Mutex<Vec<BranchGuardQuery>>>,
        company_calls: Arc<Mutex<Vec<CompanyGuardQuery>>>,
        branch: Option<ActiveBranch>,
        exists: bool,
    }

    impl ManagementGuardRead for Fake {
        fn active_branch<'a>(
            &'a self,
            query: &'a BranchGuardQuery,
        ) -> GuardFuture<'a, Option<ActiveBranch>> {
            self.branch_calls.lock().unwrap().push(query.clone());
            let branch = self.branch.clone();
            Box::pin(async move { Ok(branch) })
        }

        fn company_exists<'a>(&'a self, query: &'a CompanyGuardQuery) -> GuardFuture<'a, bool> {
            self.company_calls.lock().unwrap().push(query.clone());
            let exists = self.exists;
            Box::pin(async move { Ok(exists) })
        }
    }

    #[tokio::test]
    async fn branch_guard_renders_the_exact_retired_shape() {
        let reader = Fake {
            branch: Some(ActiveBranch {
                id: "br-1".into(),
                display_name: "North Branch".into(),
            }),
            ..Default::default()
        };
        let value = read_active_branch(
            &reader,
            BranchGuardQuery {
                id: "br-1".into(),
                company_id: "co-1".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            value,
            json!({"branch":{"id":"br-1","display_name":"North Branch"}})
        );
        assert_eq!(reader.branch_calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn no_row_keeps_the_null_branch_semantics() {
        let reader = Fake::default();
        let value = read_active_branch(
            &reader,
            BranchGuardQuery {
                id: "br-9".into(),
                company_id: "co-1".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(value, json!({"branch":null}));
    }

    #[tokio::test]
    async fn company_guard_renders_the_exact_retired_boolean() {
        let reader = Fake { exists: true, ..Default::default() };
        let value = read_company_exists(
            &reader,
            CompanyGuardQuery { id: "co-1".into() },
        )
        .await
        .unwrap();
        assert_eq!(value, json!({"exists":true}));
    }

    #[tokio::test]
    async fn malformed_targets_fail_before_the_reader_runs() {
        let reader = Fake::default();
        assert_eq!(
            read_active_branch(
                &reader,
                BranchGuardQuery {
                    id: "bad/id".into(),
                    company_id: "co-1".into(),
                },
            )
            .await
            .unwrap_err()
            .code,
            "BRAND_MANAGEMENT_REQUEST_INVALID"
        );
        assert!(reader.branch_calls.lock().unwrap().is_empty());
        assert_eq!(
            read_company_exists(
                &reader,
                CompanyGuardQuery { id: "".into() },
            )
            .await
            .unwrap_err()
            .code,
            "BRAND_MANAGEMENT_REQUEST_INVALID"
        );
        assert!(reader.company_calls.lock().unwrap().is_empty());
    }
}
