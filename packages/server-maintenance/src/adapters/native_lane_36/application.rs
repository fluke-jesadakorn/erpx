//! Query port and handler for the native Maintenance lane-36 descriptor read.

use super::domain::DescriptorScope;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

pub type DescriptorReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner reader owns the statement, projection and Company scope.
pub trait DescriptorRead: Send + Sync {
    fn descriptor<'a>(
        &'a self,
        scope: &'a DescriptorScope,
    ) -> DescriptorReadFuture<'a, Option<Value>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_LANE36_READ_UNAVAILABLE",
        "The native Maintenance descriptor read is unavailable.",
    )
}

pub async fn handle(
    reader: &dyn DescriptorRead,
    scope: DescriptorScope,
) -> Result<Value, BusinessError> {
    let row = reader.descriptor(&scope).await.map_err(|_| unavailable())?;
    Ok(json!({
        "data": row,
        "targetId": scope.target_id,
        "companyId": scope.company_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::FailureKind;

    struct Fake {
        row: Option<Value>,
        fail: bool,
    }

    impl DescriptorRead for Fake {
        fn descriptor<'a>(
            &'a self,
            _scope: &'a DescriptorScope,
        ) -> DescriptorReadFuture<'a, Option<Value>> {
            let row = self.row.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(BusinessError::new(FailureKind::Unavailable, "BOOM", "boom"))
                } else {
                    Ok(row)
                }
            })
        }
    }

    #[tokio::test]
    async fn pins_envelope_and_missing_row() {
        let scope = DescriptorScope::new("maintenance_work", "work-1", "company-1").unwrap();
        let reader = Fake {
            row: Some(json!({"company_id":"company-1"})),
            fail: false,
        };
        assert_eq!(
            handle(&reader, scope.clone()).await.unwrap(),
            json!({"data":{"company_id":"company-1"},"targetId":"work-1","companyId":"company-1"})
        );
        let empty = Fake { row: None, fail: false };
        assert!(handle(&empty, scope).await.unwrap()["data"].is_null());
    }

    #[tokio::test]
    async fn maps_owner_failure_to_unavailable() {
        let scope = DescriptorScope::new("maintenance_work", "work-1", "company-1").unwrap();
        let reader = Fake { row: None, fail: true };
        assert_eq!(
            handle(&reader, scope).await.unwrap_err().code,
            "MAINTENANCE_LANE36_READ_UNAVAILABLE"
        );
    }
}
