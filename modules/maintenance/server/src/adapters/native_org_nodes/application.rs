//! Query port and handler for the native Maintenance organization-node read.

use super::domain::{NodeScope, OrgNode};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json, to_value};
use std::{future::Future, pin::Pin};

pub type OrgNodeReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner reader owns the statement, projection and Company/branch/department
/// scope. A query is read-only and carries no transaction or idempotency key.
pub trait OrgNodeRead: Send + Sync {
    fn nodes<'a>(&'a self, scope: &'a NodeScope) -> OrgNodeReadFuture<'a, Vec<OrgNode>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_ORG_NODES_READ_UNAVAILABLE",
        "The native Maintenance organization-node read is unavailable.",
    )
}

pub async fn handle(reader: &dyn OrgNodeRead, scope: NodeScope) -> Result<Value, BusinessError> {
    let rows = reader.nodes(&scope).await.map_err(|_| unavailable())?;
    let data = to_value(rows).map_err(|_| unavailable())?;
    Ok(json!({"data": data}))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        rows: Vec<OrgNode>,
        fail: bool,
    }

    impl OrgNodeRead for Fake {
        fn nodes<'a>(&'a self, _scope: &'a NodeScope) -> OrgNodeReadFuture<'a, Vec<OrgNode>> {
            let rows = self.rows.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(BusinessError::new(FailureKind::Unavailable, "BOOM", "boom"))
                } else {
                    Ok(rows)
                }
            })
        }
    }

    #[tokio::test]
    async fn pins_complete_envelope_and_empty_result() {
        let scope = NodeScope::new("company-1", None, None).unwrap();
        let reader = Fake {
            rows: vec![OrgNode {
                id: "node-1".into(),
                kind: "branch".into(),
                display_name: "Bangkok".into(),
                branch_id: None,
            }],
            fail: false,
        };
        assert_eq!(
            handle(&reader, scope.clone()).await.unwrap(),
            json!({"data":[{"id":"node-1","kind":"branch","display_name":"Bangkok","branch_id":null}]})
        );
        let empty = Fake {
            rows: Vec::new(),
            fail: false,
        };
        assert_eq!(handle(&empty, scope).await.unwrap(), json!({"data":[]}));
    }

    #[tokio::test]
    async fn maps_reader_failure_to_unavailable() {
        let scope = NodeScope::new("company-1", None, None).unwrap();
        let reader = Fake {
            rows: Vec::new(),
            fail: true,
        };
        assert_eq!(
            handle(&reader, scope).await.unwrap_err().code,
            "MAINTENANCE_ORG_NODES_READ_UNAVAILABLE"
        );
    }
}
