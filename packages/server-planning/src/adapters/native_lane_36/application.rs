//! Query port and handler for the native Planning lane-36 scoped reads.
//!
//! No SQL lives here. The owner reader behind [`LaneRead`] executes the query,
//! projects the closed rows and enforces Company/branch scope; this handler
//! only validates the envelope and applies the response bound.

use super::domain::{LaneData, LaneScope};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

/// Maximum rows any single scoped list read may return. The calendar source
/// was already bounded to 500 rows in TypeScript; the same ceiling is applied
/// to the other two lists so an unbounded response cannot cross the port.
pub const MAX_ROWS: usize = 500;

pub type LaneReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner owns the
/// statement, the projection and the authorization-safe scope; the router only
/// transports the validated facts.
pub trait LaneRead: Send + Sync {
    fn read<'a>(&'a self, scope: &'a LaneScope) -> LaneReadFuture<'a, LaneData>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLANNING_LANE36_READ_UNAVAILABLE",
        "The native Planning scoped read is unavailable.",
    )
}

/// Handle one validated scoped read and pin the wire envelope. The caller's
/// Company and branch are echoed so the TS transport can assert scope parity.
pub async fn handle(reader: &dyn LaneRead, scope: LaneScope) -> Result<Value, BusinessError> {
    let data = reader.read(&scope).await.map_err(|_| unavailable())?;
    if data.len() > MAX_ROWS {
        return Err(unavailable());
    }
    Ok(json!({
        "data": data.value(),
        "kind": scope.kind,
        "companyId": scope.company_id,
        "branchId": scope.branch,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fake {
        rows: Vec<Value>,
        fail: bool,
    }

    impl LaneRead for Fake {
        fn read<'a>(&'a self, scope: &'a LaneScope) -> LaneReadFuture<'a, LaneData> {
            let rows = if scope.kind == "resources" {
                self.rows.clone()
            } else {
                Vec::new()
            };
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "BOOM",
                        "boom",
                    ))
                } else {
                    Ok(LaneData::List(rows))
                }
            })
        }
    }

    #[tokio::test]
    async fn envelope_pins_kind_and_company_scope() {
        let scope = LaneScope::new_list("resources", "company-1", Some("branch-1")).unwrap();
        let reader = Fake {
            rows: vec![json!({"id": "r1"})],
            fail: false,
        };
        let value = handle(&reader, scope).await.unwrap();
        assert_eq!(
            value,
            json!({
                "data": [{"id": "r1"}],
                "kind": "resources",
                "companyId": "company-1",
                "branchId": "branch-1",
            })
        );
    }

    #[tokio::test]
    async fn rejects_an_oversized_response() {
        let scope = LaneScope::new_list("resources", "company-1", None).unwrap();
        let reader = Fake {
            rows: (0..=MAX_ROWS).map(|_| json!({})).collect(),
            fail: false,
        };
        let error = handle(&reader, scope).await.unwrap_err();
        assert_eq!(error.code, "PLANNING_LANE36_READ_UNAVAILABLE");
    }

    #[tokio::test]
    async fn maps_owner_failure_to_unavailable() {
        let scope = LaneScope::new_list("resources", "company-1", None).unwrap();
        let reader = Fake {
            rows: Vec::new(),
            fail: true,
        };
        let error = handle(&reader, scope).await.unwrap_err();
        assert_eq!(error.code, "PLANNING_LANE36_READ_UNAVAILABLE");
    }
}
