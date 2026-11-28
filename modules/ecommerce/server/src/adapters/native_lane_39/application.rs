//! Lane 39 Ecommerce application: the SQL-free query port and handler.
//!
//! Composition injects the owner reader. The handler validates the workspace
//! scope, projects the rows and owns the exact response shape consumed by the
//! retained repository; the reader owns both owner-local queries.

use crate::adapters::native_lane_39::domain::{
    WorkspaceLists, WorkspaceScope, project, validate_scope,
};
use erpx_platform_runtime_server::public::http_support::serde_json::{json, Value};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin};

pub type WorkspaceListFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait WorkspaceListRead: Send + Sync {
    fn lists<'a>(
        &'a self,
        scope: WorkspaceScope<'a>,
    ) -> WorkspaceListFuture<'a, WorkspaceLists>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_WORKSPACE_LIST_UNAVAILABLE",
        "The native Ecommerce workspace service is unavailable.",
    )
}

/// Validate, read and render the bounded owner-local workspace lists.
pub async fn read_workspace_lists(
    reader: &dyn WorkspaceListRead,
    company_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
) -> Result<Value, BusinessError> {
    let scope = WorkspaceScope {
        company_id: &company_id,
        branch_id: branch_id.as_deref(),
        department_id: department_id.as_deref(),
    };
    validate_scope(&scope)?;
    let lists = reader.lists(scope).await.map_err(|error| {
        if error.kind == FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    let lists = project(lists)?;
    let channels = lists
        .channels
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "company_id": row.company_id,
                "name": row.name,
                "channel_type": row.channel_type,
                "configuration_json": row.configuration_json,
                "status": row.status,
                "created_by": row.created_by,
                "reviewed_by": row.reviewed_by,
                "version": row.version,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
            })
        })
        .collect::<Vec<Value>>();
    let carts = lists
        .carts
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "company_id": row.company_id,
                "branch_id": row.branch_id,
                "department_id": row.department_id,
                "channel_id": row.channel_id,
                "party_id": row.party_id,
                "warehouse_id": row.warehouse_id,
                "currency": row.currency,
                "status": row.status,
                "created_by": row.created_by,
                "version": row.version,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
            })
        })
        .collect::<Vec<Value>>();
    Ok(json!({ "channels": channels, "carts": carts }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_39::domain::{CartRow, ChannelRow};
    use std::sync::{Arc, Mutex};

    struct Fake {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl WorkspaceListRead for Fake {
        fn lists<'a>(
            &'a self,
            scope: WorkspaceScope<'a>,
        ) -> WorkspaceListFuture<'a, WorkspaceLists> {
            *self.seen.lock().unwrap() = vec![
                scope.company_id.to_owned(),
                scope.branch_id.unwrap_or("NULL").to_owned(),
                scope.department_id.unwrap_or("NULL").to_owned(),
            ];
            Box::pin(async {
                Ok(WorkspaceLists {
                    channels: vec![ChannelRow {
                        id: "channel-1".into(),
                        company_id: "co-1".into(),
                        name: "Web".into(),
                        channel_type: "web".into(),
                        configuration_json: "{}".into(),
                        status: "active".into(),
                        created_by: "user-1".into(),
                        reviewed_by: None,
                        version: 1,
                        created_at: 1,
                        updated_at: 2,
                    }],
                    carts: vec![CartRow {
                        id: "cart-1".into(),
                        company_id: "co-1".into(),
                        branch_id: "branch-1".into(),
                        department_id: None,
                        channel_id: "channel-1".into(),
                        party_id: "party-1".into(),
                        warehouse_id: "warehouse-1".into(),
                        currency: "THB".into(),
                        status: "open".into(),
                        created_by: "user-1".into(),
                        version: 1,
                        created_at: 1,
                        updated_at: 2,
                    }],
                })
            })
        }
    }

    #[tokio::test]
    async fn handler_pins_the_response_shape_and_binds_scope_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone() };
        let value = read_workspace_lists(
            &reader,
            "co-1".into(),
            Some("branch-1".into()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            vec!["co-1".to_owned(), "branch-1".to_owned(), "NULL".to_owned()]
        );
        assert_eq!(
            value["channels"][0]["id"],
            json!("channel-1")
        );
        assert_eq!(value["carts"][0]["branch_id"], json!("branch-1"));
        assert_eq!(value["carts"][0]["department_id"], json!(null));
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn handler_fails_closed_on_invalid_scope_before_reading() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone() };
        assert_eq!(
            read_workspace_lists(&reader, String::new(), None, None)
                .await
                .unwrap_err()
                .code,
            "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID"
        );
        assert!(seen.lock().unwrap().is_empty());
    }
}
