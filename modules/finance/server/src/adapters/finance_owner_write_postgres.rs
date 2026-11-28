//! Owner Postgres adapter for native Finance owner-write commands.
//!
//! This is the only place in the migrated path that holds executable SQL text.
//! It runs an admitted statement and its positional values on the caller-owned
//! serializable connection and writes the receipt/audit/outbox ledger in that
//! same unit. It never opens a transaction, never mints authority and never
//! accepts caller SQL: values are rendered as escaped literals in fixed
//! owner-authored statements.
use crate::adapters::finance_owner_write::statement_sql;
use crate::application::finance_owner_write::{OwnerWriteFuture, OwnerWriteRepository};
use crate::domain::finance_owner_write::{unavailable, validate};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection};

/// Adapter bound to one caller-owned connection for the lifetime of a command.
pub struct PgFinanceOwnerWriteRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgFinanceOwnerWriteRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

/// Render one positional value as a SQL literal. The statement text itself is
/// owner-authored; only this position is caller data. Single-quote doubling is
/// the standard escape, so a value can never close the literal or add syntax.
fn quote(value: &str) -> Result<String, BusinessError> {
    if value.contains('\0') {
        return Err(unavailable());
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

fn literal(value: Value) -> Result<String, BusinessError> {
    Ok(match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(flag) => if flag { "TRUE" } else { "FALSE" }.to_owned(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => quote(&text)?,
        Value::Array(items) => quote(&Value::Array(items).to_string())?,
        Value::Object(object) => quote(&Value::Object(object).to_string())?,
    })
}

/// Replace the admitted statement's positional placeholders. PostgreSQL then
/// infers each literal's target type from the statement, which is the same
/// coercion the retired D1-style binder relied on for date/numeric columns.
fn interpolate(sql: &str, values: Vec<Value>) -> Result<String, BusinessError> {
    let mut rendered = String::with_capacity(sql.len() + values.len() * 8);
    let mut values = values.into_iter();
    for ch in sql.chars() {
        if ch == '?' {
            rendered.push_str(&literal(values.next().ok_or_else(unavailable)?)?);
        } else {
            rendered.push(ch);
        }
    }
    if values.next().is_some() {
        return Err(unavailable());
    }
    Ok(rendered)
}

impl OwnerWriteRepository for PgFinanceOwnerWriteRepository<'_> {
    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn execute<'a>(
        &'a mut self,
        kind: &'a str,
        values: Vec<Value>,
        key: &'a str,
        now: i64,
    ) -> OwnerWriteFuture<'a, u64> {
        Box::pin(async move {
            if !self.connection.is_in_transaction() {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "FINANCE_OWNER_TRANSACTION_REQUIRED",
                    "A serializable Finance unit is required.",
                ));
            }
            validate(kind, &values)?;
            let sql = statement_sql(kind, values.len()).ok_or_else(|| {
                BusinessError::new(
                    erpx_platform_shared_server::public::FailureKind::InvalidInput,
                    "FINANCE_OWNER_WRITE_KIND_UNSUPPORTED",
                    "The finance owner write kind is not supported.",
                )
            })?;
            if sql.matches('?').count() != values.len() {
                return Err(unavailable());
            }
            let statement = interpolate(&sql, values)?;
            let result = sqlx::query(AssertSqlSafe(statement))
                .execute(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            let rows = result.rows_affected();

            // Receipt/audit/outbox share the caller's unit with the DML. The
            // owner command key is the transport Idempotency-Key; the retained
            // Finance command receipt remains the business key written by the
            // caller's saveCommand/executeCommand in the same transaction.
            // JSON columns are typed explicitly and every value is escaped, so
            // the ledger executes through the same literal path as the DML and
            // never sends a text bind where PostgreSQL expects jsonb.
            let operation = quote(&format!("finance.owner.{kind}"))?;
            let request_hash = quote(&format!("{kind}:{key}:{now}"))?;
            let key_literal = quote(key)?;
            let attribution = serde_json::json!({"ok":true,"kind":kind,"rows":rows}).to_string();
            let attribution = format!("{}::jsonb", quote(&attribution)?);
            let empty = "'{}'::jsonb";
            sqlx::query(AssertSqlSafe(format!(
                "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
                 VALUES({key_literal},{operation},'system',{request_hash},{key_literal},{attribution},{now},{empty}) \
                 ON CONFLICT(command_id) DO NOTHING"
            )))
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            sqlx::query(AssertSqlSafe(format!(
                "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,{key_literal},'system','system',{operation},'finance_owner_write',{key_literal},1,{operation},{now},{empty})"
            )))
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            sqlx::query(AssertSqlSafe(format!(
                "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
                 VALUES(gen_random_uuid()::text,{key_literal},{operation},'finance_owner_write',{key_literal},1,{attribution},{now})"
            )))
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            Ok(rows)
        })
    }
}
