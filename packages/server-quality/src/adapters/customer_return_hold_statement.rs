//! Native ownership of the customer-return Quality hold INSERT statement.
//!
//! Composition exposes this as a statement-descriptor port so a caller-owned
//! serializable transaction can execute the exact owner SQL without the legacy
//! TypeScript adapter carrying a second copy of the write. The active release
//! specification is re-read on the caller-supplied connection, so the native
//! port — not the caller — decides the spec witness bound into the statement.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const HOLD_SQL: &str = "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
 product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,\
 source_type,source_id,spec_id,spec_version,quantity_scope) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'partial')";

/// Exact wire shape of the legacy `ReceiptQualityHold` input.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerReturnHoldInput {
    pub id: String,
    pub key: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: Option<String>,
    pub quantity_milli: i64,
    pub reason: String,
    pub due_date: String,
    pub source_id: String,
    pub created_by: String,
    pub now: i64,
}

/// A single owner SQL statement with positional `?` bind values.
#[derive(Clone, Debug, Serialize)]
pub struct HoldStatement {
    pub sql: String,
    pub values: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerReturnHoldStatements {
    pub spec_guard: Option<HoldStatement>,
    pub hold: HoldStatement,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_RETURN_HOLD_INVALID",
        "Review the customer-return Quality hold request.",
    )
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("CUSTOMER_RETURN_HOLD_UNIT_REQUIRED").into());
    }
    Ok(())
}

/// Build the authoritative hold statements on the caller's serializable unit.
/// The active spec is re-read here; the returned `spec_guard` asserts that same
/// witness inside the caller's command assertion batch.
pub async fn customer_return_hold_statements(
    connection: &mut PgConnection,
    company_id: &CompanyId,
    input: &CustomerReturnHoldInput,
) -> Result<CustomerReturnHoldStatements, Error> {
    ready(connection).await?;
    if input.id.is_empty() || input.product_id.is_empty() || input.quantity_milli <= 0 {
        return Err(invalid().into());
    }
    let spec: Option<(String, i64)> = sqlx::query_as(
        "SELECT id::text,version::bigint FROM inventory_quality_specs \
         WHERE company_id=$1 AND product_id=$2 AND status='active' \
         ORDER BY version DESC LIMIT 1",
    )
    .bind(company_id.as_str())
    .bind(&input.product_id)
    .fetch_optional(&mut *connection)
    .await?;

    let spec_guard = spec.as_ref().map(|(id, version)| HoldStatement {
        sql: "INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,\
              CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_specs \
              WHERE id=? AND version=? AND status='active') THEN 1 ELSE 0 END)"
            .to_owned(),
        values: vec![
            serde_json::Value::String(input.key.clone()),
            serde_json::Value::String("quality-spec".to_owned()),
            serde_json::Value::String(id.clone()),
            serde_json::Value::Number((*version).into()),
        ],
    });

    let value = |v: &str| serde_json::Value::String(v.to_owned());
    let mut values: Vec<serde_json::Value> = vec![
        value(&input.id),
        value(company_id.as_str()),
        value(&input.branch_id),
        value(&input.warehouse_id),
        value(&input.product_id),
        value(&input.reason),
        value(&input.due_date),
        value(&input.created_by),
        serde_json::Value::Number(input.now.into()),
        serde_json::Value::Number(input.now.into()),
        input.lot_id.as_deref().map(value).unwrap_or(serde_json::Value::Null),
        serde_json::Value::Number(input.quantity_milli.into()),
        value("customer_return"),
        value(&input.source_id),
        spec.as_ref().map(|(id, _)| value(id)).unwrap_or(serde_json::Value::Null),
        spec.as_ref()
            .map(|(_, version)| serde_json::Value::Number((*version).into()))
            .unwrap_or(serde_json::Value::Null),
    ];

    Ok(CustomerReturnHoldStatements {
        spec_guard,
        hold: HoldStatement {
            sql: HOLD_SQL.to_owned(),
            values: {
                values.shrink_to_fit();
                values
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{CustomerReturnHoldInput, HOLD_SQL};

    #[test]
    fn hold_statement_shape_matches_legacy_writer() {
        let _input = CustomerReturnHoldInput {
            id: "h".into(), key: "k".into(), branch_id: "b".into(),
            warehouse_id: "w".into(), product_id: "p".into(), lot_id: None,
            quantity_milli: 1, reason: "r".into(), due_date: "d".into(),
            source_id: "s".into(), created_by: "u".into(), now: 1,
        };
        assert!(HOLD_SQL.contains("INSERT INTO inventory_quality_holds"));
        assert!(HOLD_SQL.contains("'partial'"));
        assert_eq!(HOLD_SQL.matches('?').count(), 16);
    }
}
