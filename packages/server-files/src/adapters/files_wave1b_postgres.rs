//! Native Files wave-1b Postgres adapter. Executes owner SQL in the caller's
//! serializable unit and writes receipt/audit/outbox rows in that same unit.
use crate::application::files_wave1b::{
    CommandResult, FilesCommandFuture, FilesCommandInput, FilesCommandPort, unit_error, validate,
};
use crate::domain::files_wave1b::{invalid, sql};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// One caller-unit statement: fixed owner SQL plus positional values.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStatement {
    pub sql: String,
    pub values: Vec<Value>,
}

fn lit(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Convert owner `$n` placeholders to the caller's `?` dialect, expanding a
/// repeated parameter into one value per occurrence.
fn question_placeholders(
    statement: &str,
    values: &[Value],
) -> Result<(String, Vec<Value>), BusinessError> {
    let mut sql = String::with_capacity(statement.len());
    let mut expanded = Vec::new();
    let mut chars = statement.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            sql.push(ch);
            continue;
        }
        let mut digits = String::new();
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_digit() {
                digits.push(next);
                chars.next();
            } else {
                break;
            }
        }
        let index: usize = digits.parse().map_err(|_| invalid())?;
        if index == 0 || index > values.len() {
            return Err(invalid());
        }
        let value = values[index - 1].clone();
        sql.push('?');
        expanded.push(value);
    }
    Ok((sql, expanded))
}

/// Plan the full command for execution in the caller's unit. The ledger shape
/// matches the executed path statement-for-statement: the same tables, columns,
/// conflict targets and jsonb_build_object payloads, including the audit row's
/// authority read-back from the receipt written earlier in the same unit. The
/// receipt/outbox result carries the kind and a zero row count because the
/// count is only known after the caller executes the owner statement; the
/// caller's batch result carries the true count. This function never opens a
/// pool, commits, retries or mints authority: validation and SQL ownership
/// stay here, while source/authority rechecks stay in the owner's read and the
/// transport headers only scope rows, never substitute for authority.
pub fn plan(
    company: &CompanyId,
    key: &str,
    input: &FilesCommandInput,
) -> Result<Vec<PlannedStatement>, BusinessError> {
    validate(input)?;
    let statement = sql(&input.kind).ok_or_else(invalid)?;
    let (statement, statement_values) = question_placeholders(statement, &input.values)?;
    // Read kinds carry no write ledger: the statement is executed on the
    // caller's already-open unit, whose read scope is the authority. Returning
    // receipt/audit/outbox here would turn a pure read into a write.
    if crate::domain::files_wave1b::is_read(&input.kind) {
        return Ok(vec![PlannedStatement { sql: statement, values: statement_values }]);
    }
    let id = format!("files-{}", key);
    let operation = format!("files.command.{}", input.kind);
    let receipt = format!(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES({}, {}, 'system:files', {}, {}, jsonb_build_object('kind', {}, 'rows', 0), {}, '{{}}'::jsonb) \
         ON CONFLICT(command_id) DO NOTHING",
        lit(&id),
        lit(&operation),
        lit(key),
        lit(company.as_str()),
        lit(&input.kind),
        input.now,
    );
    let audit = format!(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES({}, {}, 'system:files', '', {}, 'files', {}, 1, 'files-command', {}, (SELECT authority_json FROM command_receipts WHERE command_id = {})) \
         ON CONFLICT(id) DO NOTHING",
        lit(&format!("files-audit-{}", key)),
        lit(&id),
        lit(&operation),
        lit(company.as_str()),
        input.now,
        lit(&id),
    );
    let outbox = format!(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES({}, {}, {}, 'files', {}, 1, jsonb_build_object('kind', {}, 'rows', 0), {}) \
         ON CONFLICT(id) DO NOTHING",
        lit(&format!("files-outbox-{}", key)),
        lit(&id),
        lit(&operation),
        lit(company.as_str()),
        lit(&input.kind),
        input.now,
    );
    Ok(vec![
        PlannedStatement { sql: statement, values: statement_values },
        PlannedStatement { sql: receipt, values: vec![] },
        PlannedStatement { sql: audit, values: vec![] },
        PlannedStatement { sql: outbox, values: vec![] },
    ])
}

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
        // Batch kinds are data-modifying CTEs that return one json text column
        // with every affected row, so multi-row RETURNING stays lossless.
        let (rows, row, batch) = if statement.starts_with("WITH ") {
            let fetched = query.fetch_all(&mut *connection).await?;
            let text = fetched
                .first()
                .and_then(|row| row.try_get::<String, _>(0).ok())
                .unwrap_or_else(|| "[]".to_owned());
            let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| Value::Array(Vec::new()));
            let count = parsed.as_array().map(|items| items.len() as u64).unwrap_or(0);
            (count, None, Some(parsed))
        } else {
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
            (rows, row, None)
        };
        // Receipt, audit and outbox are part of the same caller-owned unit.
        let id = format!("files-{}", key);
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8::jsonb) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&id)
        .bind(operation)
        .bind("system:files")
        .bind(key)
        .bind(company.as_str())
        .bind(sqlx::types::Json(serde_json::json!({"kind":kind,"rows":rows})))
        .bind(now)
        .bind(sqlx::types::Json(serde_json::json!({})))
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
             VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("files-outbox-{}", key))
        .bind(&id)
        .bind(operation)
        .bind("files")
        .bind(company.as_str())
        .bind(1_i64)
        .bind(sqlx::types::Json(serde_json::json!({"kind":kind,"rows":rows})))
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(CommandResult {
            rows,
            row,
            rows_affected: Some(rows),
            batch,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn value(number: i64) -> Value {
        serde_json::json!(number)
    }

    #[test]
    fn placeholders_reject_zero_and_bare_markers() {
        // `$0` has no positional meaning and a bare `$` is never emitted by
        // the owner SQL: both fail closed instead of binding the wrong value.
        assert!(question_placeholders("SELECT $0", &[value(1)]).is_err());
        assert!(question_placeholders("SELECT $", &[value(1)]).is_err());
        assert!(question_placeholders("SELECT $1", &[]).is_err());
    }

    #[test]
    fn placeholders_support_multi_digit_indexes_and_casts() {
        let values: Vec<Value> = (1..=12).map(value).collect();
        let (sql, expanded) =
            question_placeholders("SELECT $10::bigint, $3::text, $10", &values).unwrap();
        assert_eq!(sql, "SELECT ?::bigint, ?::text, ?");
        assert_eq!(expanded, vec![value(10), value(3), value(10)]);
    }
}
