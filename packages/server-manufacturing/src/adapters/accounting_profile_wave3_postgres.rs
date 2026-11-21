//! PostgreSQL adapter for the native Manufacturing accounting-profile writer.
//!
//! The caller owns one serializable Holding transaction. This adapter writes
//! only the Manufacturing-owned profile row plus the Workflow receipt, audit
//! and outbox events in the same unit. It never opens a transaction and never
//! writes another owner's rows. All SQL for the capability lives here.
use crate::application::accounting_profile_wave3::{
    ProfileFuture, ProfileInsert, ProfileLedgerEntry, ProfileRepository, ProfileReview,
    StoredProfileReceipt,
};
use crate::domain::accounting_profile_wave3::{ProfileRow, review_conflict, unavailable};
use erpx_server_runtime::public::http_support::serde_json::{self, Value};
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
        review_conflict().into()
    } else {
        error.into()
    }
}

pub struct PgProfileCommand<'c> {
    connection: &'c mut PgConnection,
    company: String,
}

impl<'c> PgProfileCommand<'c> {
    pub fn new(connection: &'c mut PgConnection, company: &str) -> Self {
        Self {
            connection,
            company: company.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PROFILE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PROFILE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &ProfileLedgerEntry<'_>) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&entry.actor.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(entry.result.to_string())
        .bind(entry.now)
        .bind(&entry.actor.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This Idempotency-Key was already used for a different profile command.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(entry.key)
        .bind(&entry.actor.user_id)
        .bind(&entry.actor.assignment_id)
        .bind(entry.operation)
        .bind("manufacturing_accounting_profile")
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&entry.actor.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value =
            serde_json::from_str(&entry.actor.authority_json).map_err(|_| unavailable())?;
        let payload = serde_json::json!({
            "authority": authority,
            "id": entry.record_id,
            "companyId": &self.company,
            "version": entry.version,
            "profile": entry.result,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind("manufacturing_accounting_profile")
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl ProfileRepository for PgProfileCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> ProfileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("manufacturing-accounting-profile-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(&'a mut self, identity: &'a str) -> ProfileFuture<'a, String, Self::Error> {
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
        _company_id: &'a str,
    ) -> ProfileFuture<'a, Option<StoredProfileReceipt>, Self::Error> {
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
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                Some(text) => Some(serde_json::from_str(&text).unwrap_or(Value::Null)),
                None => None,
            };
            Ok(Some(StoredProfileReceipt {
                request_hash: row.try_get("request_hash")?,
                result,
            }))
        })
    }

    fn current<'a>(
        &'a mut self,
        id: &'a str,
        company_id: &'a str,
    ) -> ProfileFuture<'a, Option<ProfileRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT status,version,created_by FROM manufacturing_accounting_profiles \
                 WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| ProfileRow {
                status: row.try_get("status").unwrap_or_default(),
                version: row.try_get::<i32, _>("version").unwrap_or_default() as i64,
                created_by: row.try_get("created_by").unwrap_or_default(),
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a ProfileLedgerEntry<'a>,
    ) -> ProfileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn insert_profile<'a>(
        &'a mut self,
        command: &'a ProfileInsert<'a>,
    ) -> ProfileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO manufacturing_accounting_profiles\
                 (id,company_id,chart_id,currency,inventory_account_id,wip_account_id,\
                 labor_account_id,overhead_account_id,scrap_account_id,status,version,\
                 created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,'THB',$4,$5,$6,$7,$8,'draft',$9,$10,$11,$12) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(command.id)
            .bind(&command.actor.company_id)
            .bind(&command.draft.chart_id)
            .bind(&command.draft.inventory_account_id)
            .bind(&command.draft.wip_account_id)
            .bind(&command.draft.labor_account_id)
            .bind(&command.draft.overhead_account_id)
            .bind(&command.draft.scrap_account_id)
            .bind(command.version as i32)
            .bind(&command.actor.user_id)
            .bind(command.now)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(review_conflict().into());
            }
            Ok(())
        })
    }

    fn review_profile<'a>(
        &'a mut self,
        command: &'a ProfileReview<'a>,
    ) -> ProfileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE manufacturing_accounting_profiles SET status=$3,reviewed_by=$4,\
                 version=$5,updated_at=$6 WHERE id=$1 AND company_id=$2 AND version=$7",
            )
            .bind(command.id)
            .bind(&command.actor.company_id)
            .bind(command.plan.action.status())
            .bind(&command.actor.user_id)
            .bind(command.plan.next_version as i32)
            .bind(command.now)
            .bind(command.plan.expected_version as i32)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(review_conflict().into());
            }
            Ok(())
        })
    }
}
