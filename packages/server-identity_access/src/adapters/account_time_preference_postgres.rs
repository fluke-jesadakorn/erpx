//! PostgreSQL adapter for native Account time-preference commands.
//!
//! The caller owns one serializable Control transaction. This adapter writes
//! only the Account-owned preference row plus the Control-side command receipt
//! and append-only audit/outbox events (`iam_receipts`/`iam_audit`, the Control
//! ledger idiom). It never opens a transaction and never writes another
//! owner's rows. All SQL lives here, inside the owner crate.
use crate::application::account_time_preference::{
    StoredTimePreferenceReceipt, TimePreferenceFuture, TimePreferenceInsert,
    TimePreferenceLedgerEntry, TimePreferenceRepository, TimePreferenceUpdate,
};
use crate::domain::account_time_preference::{TimePreference, changed};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

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
            "TIME_PREFERENCE_CHANGED",
            "Reload your time settings and try again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgTimePreferenceCommand<'c> {
    connection: &'c mut PgConnection,
    company: String,
}

impl<'c> PgTimePreferenceCommand<'c> {
    /// `scope` is the Control-side company id carried on the ledger rows.
    pub fn new(connection: &'c mut PgConnection, scope: &str) -> Self {
        Self {
            connection,
            company: scope.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TIME_PREFERENCE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("TIME_PREFERENCE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &TimePreferenceLedgerEntry<'_>) -> Result<(), Error> {
        // Exact command receipt: the key may only be used once, for this hash.
        let changed = sqlx::query(
            "INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) \
             VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("time-preference-{}", entry.key))
        .bind(&self.company)
        .bind(entry.record_id)
        .bind(entry.request_hash)
        .bind(entry.result)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different preference.",
            )
            .into());
        }
        // Audit event, carrying the committed result as evidence.
        let audit = serde_json::json!({
            "version": entry.version,
            "state": entry.result,
        });
        sqlx::query(
            "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,\
             evidence,created_at) VALUES($1,$2,$3,$3,$4,$5,$6,$7) \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("time-preference-audit-{}", entry.key))
        .bind(&self.company)
        .bind(entry.record_id)
        .bind(entry.operation)
        .bind(entry.record_id)
        .bind(&audit)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        // Append-only outbox event inside the same unit. Control has no generic
        // outbox_events table; an `iam_audit` row with a distinguished action is
        // the established durable event record for this side.
        let payload = serde_json::json!({
            "companyId": &self.company,
            "userId": entry.record_id,
            "version": entry.version,
            "preference": entry.result,
        });
        sqlx::query(
            "INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,\
             evidence,created_at) VALUES($1,$2,$3,$3,$4,$5,$6,$7) \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("time-preference-outbox-{}", entry.key))
        .bind(&self.company)
        .bind(entry.record_id)
        .bind("account.time_preference.changed")
        .bind(entry.record_id)
        .bind(&payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

fn map_preference(row: sqlx::postgres::PgRow) -> Result<TimePreference, sqlx::Error> {
    Ok(TimePreference {
        mode: row.try_get("mode")?,
        time_zone: row.try_get("time_zone")?,
        hour_cycle: row.try_get("hour_cycle")?,
        version: row.try_get::<i32, _>("version")? as i64,
    })
}

impl TimePreferenceRepository for PgTimePreferenceCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> TimePreferenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("account-time-preference-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> TimePreferenceFuture<'a, String, Self::Error> {
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
    ) -> TimePreferenceFuture<'a, Option<StoredTimePreferenceReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result FROM iam_receipts WHERE id=$1 AND company_id=$2",
            )
            .bind(format!("time-preference-{key}"))
            .bind(&self.company)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            // `iam_receipts.result` is JSONB and decodes straight into Value.
            let result = row.try_get::<serde_json::Value, _>("result")?;
            Ok(Some(StoredTimePreferenceReceipt {
                request_hash,
                result: Some(result),
            }))
        })
    }

    fn current<'a>(
        &'a mut self,
        user_id: &'a str,
    ) -> TimePreferenceFuture<'a, Option<TimePreference>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT mode,time_zone,hour_cycle,version FROM account_time_preferences \
                 WHERE user_id=$1 FOR UPDATE",
            )
            .bind(user_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_preference).transpose().map_err(Error::from)
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a TimePreferenceLedgerEntry<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn insert_preference<'a>(
        &'a mut self,
        command: &'a TimePreferenceInsert<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "INSERT INTO account_time_preferences(user_id,mode,time_zone,hour_cycle,\
                 version,updated_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(user_id) DO NOTHING",
            )
            .bind(command.user_id)
            .bind(command.mode)
            .bind(command.time_zone)
            .bind(command.hour_cycle)
            .bind(command.version as i32)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed().into());
            }
            Ok(())
        })
    }

    fn update_preference<'a>(
        &'a mut self,
        command: &'a TimePreferenceUpdate<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed_rows = sqlx::query(
                "UPDATE account_time_preferences SET mode=$3,time_zone=$4,hour_cycle=$5,\
                 version=$6,updated_at=$7 WHERE user_id=$1 AND version=$2",
            )
            .bind(command.user_id)
            .bind(command.expected_version as i32)
            .bind(command.mode)
            .bind(command.time_zone)
            .bind(command.hour_cycle)
            .bind(command.version as i32)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed_rows != 1 {
                return Err(changed().into());
            }
            Ok(())
        })
    }
}

/// Fail-closed reader for the retained read side. Requires only an open
/// caller-owned transaction, never a serializable one.
pub struct PgTimePreferenceReader<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTimePreferenceReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn current(&mut self, user_id: &str) -> Result<Option<TimePreference>, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TIME_PREFERENCE_TRANSACTION_REQUIRED").into());
        }
        let row = sqlx::query(
            "SELECT mode,time_zone,hour_cycle,version FROM account_time_preferences WHERE user_id=$1",
        )
        .bind(user_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(map_preference).transpose().map_err(Error::from)
    }
}
