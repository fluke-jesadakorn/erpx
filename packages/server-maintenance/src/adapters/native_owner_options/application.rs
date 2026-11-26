//! Query port and handler for the native Maintenance owner-option read.

use super::domain::{OwnerOption, OwnerScope};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, to_value};
use std::{future::Future, pin::Pin};

pub type OwnerOptionReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner reader owns the statement, projection and Company scope. A query
/// is read-only and carries no transaction or idempotency key.
pub trait OwnerOptionRead: Send + Sync {
    fn owners<'a>(&'a self, scope: &'a OwnerScope) -> OwnerOptionReadFuture<'a, Vec<OwnerOption>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_OWNER_OPTIONS_READ_UNAVAILABLE",
        "The native Maintenance owner-option read is unavailable.",
    )
}

pub async fn handle(
    reader: &dyn OwnerOptionRead,
    scope: OwnerScope,
) -> Result<Value, BusinessError> {
    let rows = reader.owners(&scope).await.map_err(|_| unavailable())?;
    let data = to_value(rows).map_err(|_| unavailable())?;
    Ok(serde_json::json!({"data": data}))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        rows: Vec<OwnerOption>,
        fail: bool,
    }

    impl OwnerOptionRead for Fake {
        fn owners<'a>(
            &'a self,
            _scope: &'a OwnerScope,
        ) -> OwnerOptionReadFuture<'a, Vec<OwnerOption>> {
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
        let scope = OwnerScope::new("company-1").unwrap();
        let reader = Fake {
            rows: vec![OwnerOption {
                id: "user-1".into(),
                name: "Ada".into(),
            }],
            fail: false,
        };
        assert_eq!(
            handle(&reader, scope.clone()).await.unwrap(),
            serde_json::json!({"data":[{"id":"user-1","name":"Ada"}]})
        );
        let empty = Fake {
            rows: Vec::new(),
            fail: false,
        };
        assert_eq!(
            handle(&empty, scope).await.unwrap(),
            serde_json::json!({"data":[]})
        );
    }

    #[tokio::test]
    async fn maps_owner_failure_to_unavailable() {
        let scope = OwnerScope::new("company-1").unwrap();
        let reader = Fake {
            rows: Vec::new(),
            fail: true,
        };
        assert_eq!(
            handle(&reader, scope).await.unwrap_err().code,
            "MAINTENANCE_OWNER_OPTIONS_READ_UNAVAILABLE"
        );
    }
}
