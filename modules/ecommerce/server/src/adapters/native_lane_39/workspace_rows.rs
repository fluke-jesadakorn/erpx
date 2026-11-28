//! Lane 39 workspace listings/checkouts native read family.
//!
//! The two retained owner statements are rendered here byte-for-byte (with the
//! retired `?` binds) and executed on the owner pool. Cross-owner Commerce
//! projection text is inlined as the retained read-only source; no lock, write
//! or receipt authority is involved. Rows are returned as JSON objects so the
//! strict TS transport emits no SQL and keeps the caller's row shape.

use crate::adapters::native_lane_39::domain::{WorkspaceScope, validate_scope};
use erpx_platform_runtime_server::public::http_support::serde_json::{Map, Value};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{QueryBuilder, Row};
use std::{future::Future, pin::Pin};

/// Retained Commerce-owned projections, inlined byte-for-byte at the join.
pub const COMMERCE_PRODUCTS: &str =
    "SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products";
pub const COMMERCE_ORDERS: &str =
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders";

/// Retired TS literal: `ecommerceOrderedList('l.*,p.name product_name,c.name channel_name', ...)`.
pub const LISTINGS_SQL: &str = "SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC";

/// Retired TS literal: `ecommerceOrderedList('x.*,o.number,o.state order_state', ... cartScope ...)`.
pub const CHECKOUTS_SQL: &str = "SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=x.sales_order_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY x.updated_at DESC";

pub const MAX_ROWS: usize = 5000;

pub type WorkspaceRowFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRows {
    pub listings: Vec<Value>,
    pub checkouts: Vec<Value>,
}

pub trait WorkspaceRowRead: Send + Sync {
    fn rows<'a>(&'a self, scope: WorkspaceScope<'a>) -> WorkspaceRowFuture<'a, WorkspaceRows>;
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_WORKSPACE_ROW_RESPONSE_INVALID",
        "The native Ecommerce workspace rows were invalid.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_WORKSPACE_ROWS_UNAVAILABLE",
        "The native Ecommerce workspace row service is unavailable.",
    )
}

/// Validate, read and render both bounded workspace row lists.
pub async fn read_workspace_rows(
    reader: &dyn WorkspaceRowRead,
    scope: WorkspaceScope<'_>,
) -> Result<Value, BusinessError> {
    validate_scope(&scope)?;
    let rows = reader.rows(scope).await.map_err(|error| {
        if error.kind == FailureKind::InvalidInput {
            error
        } else {
            unavailable()
        }
    })?;
    if rows.listings.len() > MAX_ROWS || rows.checkouts.len() > MAX_ROWS {
        return Err(invalid());
    }
    let listings = rows.listings.into_iter().map(json_row).collect::<Result<Vec<_>, _>>()?;
    let checkouts = rows.checkouts.into_iter().map(json_row).collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({ "listings": listings, "checkouts": checkouts }))
}

fn json_row(value: Value) -> Result<Value, BusinessError> {
    let object = value.as_object().ok_or_else(invalid)?;
    let mut bounded = Map::new();
    for (key, item) in object {
        if key.len() > 64 || !matches!(item, Value::String(_) | Value::Number(_) | Value::Null) {
            return Err(invalid());
        }
        bounded.insert(key.clone(), item.clone());
    }
    if !matches!(object.get("id"), Some(Value::String(_)))
        || !matches!(object.get("company_id"), Some(Value::String(_)))
    {
        return Err(invalid());
    }
    Ok(Value::Object(bounded))
}

pub struct PgWorkspaceRowRead {
    pool: DatabasePool,
}

impl PgWorkspaceRowRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

async fn fetch_rows(
    pool: DatabasePool,
    statement: &str,
    scope: &WorkspaceScope<'_>,
) -> Result<Vec<Value>, BusinessError> {
    let mut parts = statement.split('?');
    let mut query = QueryBuilder::new("SELECT to_jsonb(source) FROM (");
    query.push(parts.next().unwrap_or(""));
    let binds: [Option<&str>; 5] = [
        Some(scope.company_id),
        scope.branch_id,
        scope.branch_id,
        scope.department_id,
        scope.department_id,
    ];
    let count = statement.matches('?').count();
    for (tail, bind) in parts.zip(binds.into_iter().take(count)) {
        query.push_bind(bind);
        query.push(tail);
    }
    query.push(") source");
    let rows = query
        .build()
        .fetch_all(pool.raw())
        .await
        .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| row.try_get::<Value, _>(0).map_err(|_| unavailable()))
        .collect()
}

impl WorkspaceRowRead for PgWorkspaceRowRead {
    fn rows<'a>(&'a self, scope: WorkspaceScope<'a>) -> WorkspaceRowFuture<'a, WorkspaceRows> {
        let pool = self.pool.clone();
        let company = scope.company_id.to_owned();
        let branch = scope.branch_id.map(str::to_owned);
        let department = scope.department_id.map(str::to_owned);
        Box::pin(async move {
            let checked = WorkspaceScope {
                company_id: &company,
                branch_id: branch.as_deref(),
                department_id: department.as_deref(),
            };
            validate_scope(&checked)?;
            let listings = fetch_rows(pool.clone(), LISTINGS_SQL, &checked).await?;
            let checkouts = fetch_rows(pool, CHECKOUTS_SQL, &checked).await?;
            Ok(WorkspaceRows { listings, checkouts })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn retired_statements_and_projection_text_are_byte_identical() {
        assert_eq!(
            LISTINGS_SQL,
            format!("SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN ({COMMERCE_PRODUCTS}) p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC")
        );
        assert_eq!(
            CHECKOUTS_SQL,
            format!("SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN ({COMMERCE_ORDERS}) o ON o.id=x.sales_order_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY x.updated_at DESC")
        );
        assert_eq!(LISTINGS_SQL.matches('?').count(), 1);
        assert_eq!(CHECKOUTS_SQL.matches('?').count(), 5);
        for sql in [LISTINGS_SQL, CHECKOUTS_SQL] {
            assert!(sql.starts_with("SELECT "));
            assert!(!sql.contains(';'));
            let upper = sql.to_uppercase();
            assert!(!upper.contains("FOR UPDATE") && !upper.contains("FOR SHARE") && !upper.contains("LOCK TABLE"));
        }
    }

    struct Fake {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl WorkspaceRowRead for Fake {
        fn rows<'a>(&'a self, scope: WorkspaceScope<'a>) -> WorkspaceRowFuture<'a, WorkspaceRows> {
            *self.seen.lock().unwrap() = vec![
                scope.company_id.to_owned(),
                scope.branch_id.unwrap_or("NULL").to_owned(),
                scope.department_id.unwrap_or("NULL").to_owned(),
            ];
            Box::pin(async {
                Ok(WorkspaceRows {
                    listings: vec![serde_json::json!({"id": "l-1", "company_id": "co-1", "status": "published", "version": 1})],
                    checkouts: vec![serde_json::json!({"id": "x-1", "company_id": "co-1", "status": "confirmed"})],
                })
            })
        }
    }

    #[tokio::test]
    async fn handler_pins_reply_shape_and_scope_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Fake { seen: seen.clone() };
        let value = read_workspace_rows(
            &reader,
            WorkspaceScope { company_id: "co-1", branch_id: Some("b-1"), department_id: None },
        )
        .await
        .unwrap();
        assert_eq!(*seen.lock().unwrap(), vec!["co-1", "b-1", "NULL"]);
        assert_eq!(value["listings"][0]["id"], serde_json::json!("l-1"));
        assert_eq!(value["checkouts"][0]["status"], serde_json::json!("confirmed"));
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn handler_fails_closed_on_drifted_rows() {
        struct Drift;
        impl WorkspaceRowRead for Drift {
            fn rows<'a>(&'a self, _: WorkspaceScope<'a>) -> WorkspaceRowFuture<'a, WorkspaceRows> {
                Box::pin(async {
                    Ok(WorkspaceRows {
                        listings: vec![serde_json::json!({"id": "l-1", "company_id": 5})],
                        checkouts: vec![],
                    })
                })
            }
        }
        let error = read_workspace_rows(
            &Drift,
            WorkspaceScope { company_id: "co-1", branch_id: None, department_id: None },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "ECOMMERCE_WORKSPACE_ROW_RESPONSE_INVALID");
    }
}
