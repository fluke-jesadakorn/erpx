//! PostgreSQL executor for native PMS owner writes (wf1a2 slice).
//!
//! The caller owns one serializable Holding transaction and fresh authority.
//! This adapter converts the owner-selected `?` placeholders to positional
//! binds, binds only caller-supplied values, executes the statement on that
//! unit and writes the command receipt/audit/outbox in the same transaction.
use crate::application::pms_owner_write::{
    OwnerWriteLedgerEntry, PmsOwnerWriteActor, PmsOwnerWriteFuture, PmsOwnerWriteRepository,
    StoredOwnerReceipt,
};
use crate::domain::pms_owner_write::{PmsOwnerStatement, invalid};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Map, Value, json};
use sqlx::{Column, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        conflict(
            "PMS_OWNER_WRITE_CONFLICT",
            "The hotel record changed or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgPmsOwnerWrite<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgPmsOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &PmsOwnerWriteActor) -> Self {
        Self {
            connection,
            company: CompanyId::new(&actor.company_id).expect("validated company"),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json.clone(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *self.connection)
                .await?
                != "serializable"
        {
            return Err(PersistenceError::new("PMS_OWNER_WRITE_UNIT_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &OwnerWriteLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| invalid())?;
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.kind)
        .bind(&self.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(result)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(crate::domain::pms_owner_write::invalid().into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("pms-owner-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(entry.kind)
        .bind(entry.kind)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = json!({
            "authority": authority,
            "id": entry.record_id,
            "state": entry.state,
            "companyId": self.company.as_str(),
            "kind": entry.kind,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("pms-owner-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.kind)
        .bind(entry.kind)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

/// Convert an owner statement's `?` placeholders into positional `$n` binds.
fn positional(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

fn row_json(row: &sqlx::postgres::PgRow) -> Value {
    let mut object = Map::new();
    for column in row.columns() {
        let name = column.name();
        let value = if let Ok(value) = row.try_get::<Option<String>, _>(name) {
            match value {
                Some(text) => Value::String(text),
                None => Value::Null,
            }
        } else if let Ok(value) = row.try_get::<Option<i64>, _>(name) {
            value.map(|number| Value::Number(number.into())).unwrap_or(Value::Null)
        } else if let Ok(value) = row.try_get::<Option<bool>, _>(name) {
            value.map(Value::Bool).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        object.insert(name.to_owned(), value);
    }
    Value::Object(object)
}

impl PmsOwnerWriteRepository for PgPmsOwnerWrite<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PmsOwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("pms-owner-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> PmsOwnerWriteFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PmsOwnerWriteFuture<'a, Option<StoredOwnerReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                None => None,
                Some(value) => Some(serde_json::from_str(&value).map_err(|_| {
                    Error::Business(BusinessError::new(
                        FailureKind::Unavailable,
                        "PMS_OWNER_RECEIPT_INVALID",
                        "The PMS command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredOwnerReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a OwnerWriteLedgerEntry<'a>,
    ) -> PmsOwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn execute_statement<'a>(
        &'a mut self,
        command: &'a PmsOwnerStatement,
    ) -> PmsOwnerWriteFuture<'a, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = positional(&command.sql);
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for value in &command.values {
                query = match value {
                    Value::Null => query.bind(Option::<String>::None),
                    Value::Bool(flag) => query.bind(*flag),
                    Value::Number(number) => match number.as_i64() {
                        Some(integer) => query.bind(integer),
                        None => query.bind(number.as_f64().unwrap_or_default()),
                    },
                    Value::String(text) => query.bind(text.clone()),
                    other => query.bind(other.to_string()),
                };
            }
            let row = query.fetch_optional(&mut *self.connection).await.map_err(write_error)?;
            Ok(row.as_ref().map(row_json))
        })
    }
}
