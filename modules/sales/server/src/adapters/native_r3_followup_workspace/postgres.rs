//! PostgreSQL reads bound to a caller-owned SERIALIZABLE transaction.
use crate::adapters::native_r3_followup_workspace::application::{FollowupWorkspaceStore, WorkspaceFuture, WorkspaceInput, WorkspaceRows};
use crate::adapters::native_r3_followup_workspace::domain::{items_sql, order_sql, recipients_sql, request_invalid};
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row, types::Json};

/// Kept for source compatibility; unbound pool reads must not become authority.
pub struct PgFollowupWorkspaceStore;
impl PgFollowupWorkspaceStore { pub fn new(_pool: DatabasePool) -> Self { Self } }
fn unavailable() -> BusinessError { BusinessError::new(FailureKind::Unavailable, "CUSTOMER_FOLLOWUP_WORKSPACE_UNAVAILABLE", "The native customer follow-up workspace read is unavailable.") }
fn numeric_overflow() -> BusinessError { BusinessError::new(FailureKind::Unavailable, "DATABASE_NUMBER_OUT_OF_RANGE", "The request could not be completed.") }
/// `to_jsonb(row)` bypasses the TypeScript adapter's INT8/NUMERIC parsers.
/// Match its safe-number boundary before a JSON number can be rounded by JS.
/// Inspect only top-level columns: JSON/JSONB columns are returned as serialized
/// strings by that adapter, rather than recursively parsed there.
fn safe_row(value: Value) -> Result<Value, BusinessError> {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    let object = value.as_object().ok_or_else(unavailable)?;
    for value in object.values() {
        if let Value::Number(number) = value {
            let n = number.as_f64().ok_or_else(numeric_overflow)?;
            if !n.is_finite() || (n.fract() == 0.0 && n.abs() > MAX_SAFE) {
                return Err(numeric_overflow());
            }
        }
    }
    Ok(value)
}
impl FollowupWorkspaceStore for PgFollowupWorkspaceStore {
    fn read<'a>(&'a self, _input: WorkspaceInput<'a>) -> WorkspaceFuture<'a, WorkspaceRows> {
        Box::pin(async { Err(unavailable()) })
    }
}

/// Caller must establish and retain selected IAM and per-kind Commercial
/// history admission on this same connection before invoking this function.
pub async fn read_workspace_on_connection(c: &mut PgConnection, input: WorkspaceInput<'_>) -> Result<WorkspaceRows, PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c).await? != "serializable" {
        return Err(unavailable().into());
    }
    if !input.can_read || input.company_id.is_empty() || input.user_id.is_empty() {
        return Err(request_invalid().into());
    }
    let selected = if let Some(id) = input.order_id.filter(|id| !id.is_empty()) {
        let row = sqlx::query(order_sql()).bind(id).bind(input.company_id).bind(input.branch_id).bind(input.department_id)
            .fetch_optional(&mut *c).await?
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND", "The customer order is unavailable in this workspace."))?;
        let value: Json<Value> = row.try_get("value")?;
        let value = safe_row(value.0)?;
        match value.get("kind").and_then(Value::as_str) {
            Some("purchase") => return Err(BusinessError::invalid("CUSTOMER_SOURCE_REQUIRED", "Use a customer order or quotation.").into()),
            Some("quote") if !input.quote_history => return Err(BusinessError::new(FailureKind::Forbidden, "PACKAGE_REQUIRED", "Quotation history is required.").into()),
            Some("sales") if !input.sales_history => return Err(BusinessError::new(FailureKind::Forbidden, "PACKAGE_REQUIRED", "Sales order history is required.").into()),
            Some(_) => {},
            None => return Err(unavailable().into()),
        }
        Some(value)
    } else { None };
    let rows = sqlx::query(items_sql()).bind(input.company_id).bind(input.branch_id).bind(input.department_id).bind(input.order_id)
        .bind(input.quote_history).bind(input.sales_history).fetch_all(&mut *c).await?;
    let items = rows.into_iter().map(|row| safe_row(row.try_get::<Json<Value>, _>("value").map_err(|_| unavailable())?.0)).collect::<Result<Vec<_>, _>>()?;
    let recipients = if let Some(id) = input.order_id.filter(|id| !id.is_empty()) {
        let rows = sqlx::query(recipients_sql()).bind(input.company_id).bind(input.branch_id).bind(input.department_id).bind(id)
            .fetch_all(&mut *c).await?;
        rows.into_iter().map(|row| safe_row(row.try_get::<Json<Value>, _>("value").map_err(|_| unavailable())?.0)).collect::<Result<Vec<_>, _>>()?
    } else { Vec::new() };
    Ok(WorkspaceRows { order: selected, items, recipients })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rejects_only_unsafe_top_level_database_numbers() {
        assert!(safe_row(json!({"amount": 9_007_199_254_740_991_i64, "negative": -9_007_199_254_740_991_i64})).is_ok());
        assert_eq!(safe_row(json!({"amount": 9_007_199_254_740_992_i64})).unwrap_err().code, "DATABASE_NUMBER_OUT_OF_RANGE");
        assert_eq!(safe_row(json!({"amount": -9_007_199_254_740_992_i64})).unwrap_err().code, "DATABASE_NUMBER_OUT_OF_RANGE");
        assert!(safe_row(json!({"amount": 1.25, "text": "9007199254740992"})).is_ok());
    }
}
