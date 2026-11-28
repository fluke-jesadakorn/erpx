//! Postgres execution for the native follow-up workspace read.
//!
//! The owner pool is injected by composition; each statement is a fixed owner
//! literal from `domain` with only scalar identity binds. All failures map to
//! one named owner error so the TypeScript client fails closed.
use crate::adapters::native_r3_followup_workspace::application::{
    FollowupWorkspaceStore, WorkspaceFuture, WorkspaceInput, WorkspaceRows,
};
use crate::adapters::native_r3_followup_workspace::domain::{
    items_sql, order_sql, recipients_sql, request_invalid,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Row, types::Json};

pub struct PgFollowupWorkspaceStore {
    pool: DatabasePool,
}

impl PgFollowupWorkspaceStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_FOLLOWUP_WORKSPACE_UNAVAILABLE",
        "The native customer follow-up workspace read is unavailable.",
    )
}

async fn values(
    pool: &DatabasePool,
    sql: &'static str,
    binds: Vec<Option<&str>>,
) -> Result<Vec<Value>, BusinessError> {
    let mut query = sqlx::query(sql);
    for bind in binds {
        query = query.bind(bind);
    }
    let rows = query.fetch_all(pool.raw()).await.map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            let value: Json<Value> = row.try_get("value").map_err(|_| unavailable())?;
            Ok(value.0)
        })
        .collect()
}

impl FollowupWorkspaceStore for PgFollowupWorkspaceStore {
    fn read<'a>(&'a self, input: WorkspaceInput<'a>) -> WorkspaceFuture<'a, WorkspaceRows> {
        let pool = self.pool.clone();
        let (company, branch, department, order, user) = (
            input.company_id.to_owned(),
            input.branch_id.map(str::to_owned),
            input.department_id.map(str::to_owned),
            input.order_id.map(str::to_owned),
            input.user_id.to_owned(),
        );
        Box::pin(async move {
            // Every identity is bounded by the HTTP ingress before it reaches
            // the SQL layer; a missing bound is a malformed request, not a
            // wildcard that could widen the Company scope.
            if company.is_empty() || user.is_empty() {
                return Err(request_invalid());
            }
            let scope: Vec<Option<&str>> = vec![
                Some(company.as_str()),
                branch.as_deref(),
                department.as_deref(),
            ];

            let mut item_binds = scope.clone();
            item_binds.push(order.as_deref());
            let items = values(&pool, items_sql(), item_binds).await?;

            let selected = match &order {
                None => None,
                Some(id) => {
                    let found = values(
                        &pool,
                        order_sql(),
                        vec![Some(id.as_str()), Some(company.as_str())],
                    )
                    .await?;
                    found.into_iter().next()
                }
            };
            // A purchase source is never a follow-up source: the retired
            // `source()` guard rejected it before rendering the workspace.
            if let Some(value) = &selected
                && value.get("kind").and_then(Value::as_str) == Some("purchase")
            {
                return Err(request_invalid());
            }

            let recipients = match &order {
                None => Vec::new(),
                Some(id) => {
                    let mut binds = scope.clone();
                    binds.push(Some(id.as_str()));
                    values(&pool, recipients_sql(), binds).await?
                }
            };
            Ok(WorkspaceRows {
                order: selected,
                items,
                recipients,
            })
        })
    }
}
