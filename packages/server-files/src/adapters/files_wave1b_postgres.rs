//! Native Files wave-1b Postgres adapter. Executes owner SQL in the caller's
//! serializable unit and writes receipt/audit/outbox rows in that same unit.
use crate::application::files_wave1b::{
    CommandResult, FilesCommandFuture, FilesCommandPort, unit_error,
};
use crate::domain::files_wave1b::{invalid, sql};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

pub struct PostgresFilesCommandPort;

impl PostgresFilesCommandPort {
    pub async fn execute(
        connection: &mut PgConnection,
        company: &CompanyId,
        key: &str,
        operation: &str,
        kind: &str,
        values: &[Value],
        now: i64,
    ) -> Result<CommandResult, Error> {
        if !connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *connection)
                .await?
                != "serializable"
        {
            return Err(erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_COMMAND_UNIT_REQUIRED",
            )
            .into());
        }
        let statement = sql(kind).ok_or_else(invalid)?;
        let mut query = sqlx::query(statement);
        for value in values {
            query = bind_value(query, value);
        }
        let result = query.execute(&mut *connection).await?;
        let rows = result.rows_affected();
        let returning = statement.contains("RETURNING");
        let row = if returning {
            sqlx::query_scalar::<_, String>(
                "SELECT company_id FROM attachments WHERE id=$1",
            )
            .bind(values.first().and_then(|v| v.as_str()).unwrap_or_default())
            .fetch_optional(&mut *connection)
            .await?
            .map(|company_id| serde_json::json!({"company_id": company_id}))
        } else {
            None
        };
        // Receipt, audit and outbox are part of the same caller-owned unit.
        let id = format!("files-{}", key);
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&id)
        .bind(operation)
        .bind("system:files")
        .bind(key)
        .bind(company.as_str())
        .bind(serde_json::json!({"kind":kind,"rows":rows}).to_string())
        .bind(now)
        .bind("{}")
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,(SELECT authority_json FROM command_receipts WHERE command_id=$11)) \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("files-audit-{}", key))
        .bind(&id)
        .bind("system:files")
        .bind("")
        .bind(operation)
        .bind("files")
        .bind(company.as_str())
        .bind(1_i64)
        .bind("files-command")
        .bind(now)
        .bind(&id)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("files-outbox-{}", key))
        .bind(&id)
        .bind(operation)
        .bind("files")
        .bind(company.as_str())
        .bind(1_i64)
        .bind(serde_json::json!({"kind":kind,"rows":rows}).to_string())
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(CommandResult {
            rows,
            row,
            rows_affected: Some(rows),
        })
    }
}

fn bind_value<'a>(
    query: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'a Value,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(value) => query.bind(*value),
        Value::Number(value) if value.is_i64() => query.bind(value.as_i64().unwrap_or_default()),
        Value::Number(value) => query.bind(value.as_f64().unwrap_or_default()),
        Value::String(value) => query.bind(value.clone()),
        Value::Array(_) | Value::Object(_) => query.bind(value.to_string()),
    }
}

impl FilesCommandPort for PostgresFilesCommandPort {
    fn execute<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company: &'a CompanyId,
        key: &'a str,
        operation: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
    ) -> FilesCommandFuture<'a, CommandResult> {
        Box::pin(async move {
            let Some(connection) = unit else {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "FILES_COMMAND_UNAVAILABLE",
                    "The native Files command unit is unavailable.",
                ));
            };
            Self::execute(connection, company, key, operation, kind, values, now)
                .await
                .map_err(unit_error)
        })
    }
}
