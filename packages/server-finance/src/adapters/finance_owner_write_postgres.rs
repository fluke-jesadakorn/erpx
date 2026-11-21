//! Owner Postgres adapter for native Finance owner-write commands.
//!
//! This is the only place in the migrated path that holds executable SQL text.
//! It runs an admitted statement and its positional values on the caller-owned
//! serializable connection and writes the receipt/audit/outbox ledger in that
//! same unit. It never opens a transaction, never mints authority and never
//! interpolates caller text into SQL: values are bound by their JSON type.
use crate::adapters::finance_owner_write::statement_sql;
use crate::application::finance_owner_write::{OwnerWriteFuture, OwnerWriteRepository};
use crate::domain::finance_owner_write::{unavailable, validate};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
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

type BoundQuery<'q> = Query<'q, sqlx::Postgres, PgArguments>;

fn bind(query: BoundQuery<'_>, value: Value) -> BoundQuery<'_> {
    match value {
        Value::Null => query.bind(Option::<&str>::None),
        Value::Bool(flag) => query.bind(flag),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => query.bind(integer),
            None => query.bind(number.as_f64().unwrap_or(0.0)),
        },
        Value::String(text) => query.bind(text),
        Value::Array(items) => query.bind(Value::Array(items).to_string()),
        Value::Object(object) => query.bind(Value::Object(object).to_string()),
    }
}

fn bind_batch(mut query: BoundQuery<'_>, values: Vec<Value>) -> BoundQuery<'_> {
    for value in values {
        query = bind(query, value);
    }
    query
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
                    erpx_server_shared::public::FailureKind::InvalidInput,
                    "FINANCE_OWNER_WRITE_KIND_UNSUPPORTED",
                    "The finance owner write kind is not supported.",
                )
            })?;
            if sql.matches('?').count() != values.len() {
                return Err(unavailable());
            }
            // `?` is the retained owner payload placeholder; sqlx uses `$n`.
            // Convert only the placeholder tokens, never caller text.
            let mut index = 0usize;
            let mut pg = String::with_capacity(sql.len() + values.len() * 2);
            let mut chars = sql.chars();
            while let Some(ch) = chars.next() {
                if ch == '?' {
                    index += 1;
                    pg.push('$');
                    pg.push_str(&index.to_string());
                } else {
                    pg.push(ch);
                }
            }
            let query = bind_batch(sqlx::query(AssertSqlSafe(pg)), values);
            let result = query
                .execute(&mut *self.connection)
                .await
                .map_err(|_| unavailable())?;
            let rows = result.rows_affected();

            // Receipt/audit/outbox share the caller's unit with the DML. The
            // owner command key is the transport Idempotency-Key; the retained
            // Finance command receipt remains the business key written by the
            // caller's saveCommand/executeCommand in the same transaction.
            let operation = format!("finance.owner.{kind}");
            let request_hash = format!("{kind}:{key}:{now}");
            sqlx::query(
                "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(key)
            .bind(&operation)
            .bind("system")
            .bind(&request_hash)
            .bind(key)
            .bind(serde_json::json!({"ok":true,"kind":kind,"rows":rows}).to_string())
            .bind(now)
            .bind("{}")
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            let record_type = "finance_owner_write";
            sqlx::query(
                "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
            )
            .bind(key)
            .bind("system")
            .bind("system")
            .bind(&operation)
            .bind(record_type)
            .bind(key)
            .bind(1i64)
            .bind(now)
            .bind("{}")
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            sqlx::query(
                "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(key)
            .bind(&operation)
            .bind(record_type)
            .bind(key)
            .bind(1i64)
            .bind(serde_json::json!({"ok":true,"kind":kind,"rows":rows}).to_string())
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(|_| unavailable())?;

            Ok(rows)
        })
    }
}
