//! Application port and pure projection for the native follow-up workspace.
use crate::adapters::native_r3_followup_workspace::domain::{
    WorkspaceQuery, request_invalid,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type WorkspaceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Scalar identities and permissions resolved by the caller's authority.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceInput<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub order_id: Option<&'a str>,
    pub user_id: &'a str,
    pub can_read: bool,
    pub can_sell: bool,
    pub can_manage: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WorkspaceRows {
    pub order: Option<Value>,
    pub items: Vec<Value>,
    pub recipients: Vec<Value>,
}

/// Composition injects the owner pool; the port never opens a transaction and
/// accepts no SQL from the wire.
pub trait FollowupWorkspaceStore: Send + Sync {
    fn read<'a>(
        &'a self,
        input: WorkspaceInput<'a>,
    ) -> WorkspaceFuture<'a, WorkspaceRows>;
}

/// Validate, execute the owner reads and render the exact retired envelope.
pub async fn read_workspace(
    store: &dyn FollowupWorkspaceStore,
    query: &WorkspaceQuery,
    input: WorkspaceInput<'_>,
) -> Result<Value, BusinessError> {
    query.validate()?;
    if !input.can_read {
        return Err(BusinessError::new(
            erpx_platform_shared_server::public::FailureKind::Forbidden,
            "FOLLOWUP_ACCESS_REQUIRED",
            "The current follow-up assignment is not authorized.",
        ));
    }
    if input.company_id.is_empty() || input.user_id.is_empty() {
        return Err(request_invalid());
    }
    let rows = store.read(input).await?;
    Ok(json!({
        "order": rows.order,
        "items": rows.items,
        "recipients": rows.recipients,
        "canCreate": input.can_sell,
        "canManage": input.can_manage,
        "userId": input.user_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Store {
        seen: Arc<Mutex<Option<String>>>,
    }
    impl FollowupWorkspaceStore for Store {
        fn read<'a>(&'a self, input: WorkspaceInput<'a>) -> WorkspaceFuture<'a, WorkspaceRows> {
            let seen = self.seen.clone();
            let order = input.order_id.map(str::to_owned);
            Box::pin(async move {
                *seen.lock().unwrap() = order;
                Ok(WorkspaceRows {
                    order: Some(json!({"id":"order-1","kind":"sales"})),
                    items: vec![json!({"id":"f-1","owner_eligible":1})],
                    recipients: vec![json!({"id":"u-1","name":"Ada"})],
                })
            })
        }
    }

    #[tokio::test]
    async fn renders_the_exact_retired_workspace_envelope() {
        let seen = Arc::new(Mutex::new(None));
        let store = Store { seen: seen.clone() };
        let query = WorkspaceQuery { order_id: Some("order-1".into()) };
        let value = read_workspace(
            &store,
            &query,
            WorkspaceInput {
                company_id: "co-1",
                branch_id: None,
                department_id: None,
                order_id: Some("order-1"),
                user_id: "u-2",
                can_read: true,
                can_sell: true,
                can_manage: false,
            },
        )
        .await
        .expect("workspace read");
        assert_eq!(value["order"]["kind"], "sales");
        assert_eq!(value["items"][0]["owner_eligible"], 1);
        assert_eq!(value["recipients"][0]["name"], "Ada");
        assert_eq!(value["canCreate"], true);
        assert_eq!(value["canManage"], false);
        assert_eq!(value["userId"], "u-2");
        assert_eq!(seen.lock().unwrap().as_deref(), Some("order-1"));
    }

    #[tokio::test]
    async fn unauthorized_and_bad_queries_fail_closed() {
        let store = Store { seen: Arc::new(Mutex::new(None)) };
        let base = WorkspaceInput {
            company_id: "co-1",
            branch_id: None,
            department_id: None,
            order_id: None,
            user_id: "u-2",
            can_read: false,
            can_sell: false,
            can_manage: false,
        };
        assert_eq!(
            read_workspace(&store, &WorkspaceQuery::default(), base)
                .await
                .unwrap_err()
                .code,
            "FOLLOWUP_ACCESS_REQUIRED"
        );
        let allowed = WorkspaceInput { can_read: true, ..base };
        assert_eq!(
            read_workspace(
                &store,
                &WorkspaceQuery { order_id: Some("../bad".into()) },
                allowed,
            )
            .await
            .unwrap_err()
            .code,
            "CUSTOMER_FOLLOWUP_REQUEST_INVALID"
        );
    }
}
