//! POS-owned day-close persistence on the caller's serializable transaction.
//! Authorization, same-person identity and current cross-owner report facts must
//! be checked by the application in this very transaction before these writes.
use crate::domain::day_close::PreparedDayClose;
use crate::domain::day_receipt::PosDayReceiptIdentity;
use erpx_server_workflow::public::commands::CommandJson;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPosDayClose<'c> {
    connection: &'c mut PgConnection,
}

pub struct NewDayClose<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub branch_id: &'a str,
    pub business_date: &'a str,
    pub currency: &'a str,
    pub snapshot: &'a serde_json::Value,
    pub digest: &'a str,
    pub prepared_by: &'a str,
    pub prepared_at: i64,
}

impl<'c> PgPosDayClose<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *self.connection)
                .await?
                != "serializable"
        {
            return Err(PersistenceError::new("POS_DAY_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
    /// Caller must first recheck current authority and take the POS Company
    /// lock. Keys are globally scoped as in the retained POS writer.
    pub async fn replay_receipt(&mut self, identity: &PosDayReceiptIdentity) -> Result<Option<CommandJson>, Error> {
        self.ready().await?;
        let row = sqlx::query("SELECT request_hash,result_json FROM pos_receipts WHERE id=$1")
            .bind(identity.key.as_str()).fetch_optional(&mut *self.connection).await?;
        row.map(|row| {
            let hash: String = row.try_get("request_hash")?;
            let result: String = row.try_get("result_json")?;
            identity.replay_sha256(&hash, &result).map_err(Error::from)
        }).transpose()
    }
    /// Insert the owner receipt on the same transaction as its effects.
    pub async fn insert_receipt(&mut self, identity: &PosDayReceiptIdentity, result: &CommandJson, now: i64) -> Result<(), Error> {
        self.ready().await?;
        sqlx::query("INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) VALUES($1,$2,$3,$4,$5)")
            .bind(identity.key.as_str()).bind(identity.company_id()).bind(identity.request_hash()).bind(result.as_str()).bind(now)
            .execute(&mut *self.connection).await?;
        Ok(())
    }
    /// Acquire the same transaction-scoped Company lock as the retained POS
    /// writer. The application must first recheck live authority, then acquire
    /// this lock before receipt replay and before reading the report's sources.
    pub async fn lock_company(&mut self, company_id: &str) -> Result<(), Error> {
        self.ready().await?;
        if company_id.is_empty() {
            return Err(PersistenceError::new("POS_DAY_COMPANY_REQUIRED").into());
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:' || $1,0))")
            .bind(company_id)
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
    /// Read and lock the exact company-scoped record used for confirmation.
    pub async fn lock_close(
        &mut self,
        company_id: &str,
        id: &str,
    ) -> Result<Option<PreparedDayClose>, Error> {
        self.ready().await?;
        let row = sqlx::query("SELECT company_id,branch_id,business_date,state,version,source_digest,prepared_by FROM pos_day_closes WHERE company_id=$1 AND id=$2 FOR UPDATE")
            .bind(company_id).bind(id).fetch_optional(&mut *self.connection).await?;
        row.map(|r| {
            Ok(PreparedDayClose {
                company_id: r.try_get("company_id")?,
                branch_id: r.try_get("branch_id")?,
                business_date: r.try_get("business_date")?,
                state: r.try_get("state")?,
                version: r.try_get("version")?,
                source_digest: r.try_get("source_digest")?,
                prepared_by: r.try_get("prepared_by")?,
            })
        })
        .transpose()
    }
    /// The selected day is unique; a concurrent preparer must not be replaced.
    pub async fn prepare(&mut self, input: &NewDayClose<'_>) -> Result<bool, Error> {
        self.ready().await?;
        let result = sqlx::query("INSERT INTO pos_day_closes(id,company_id,branch_id,business_date,currency,state,snapshot_json,source_digest,prepared_by,prepared_at) VALUES($1,$2,$3,$4,$5,'prepared',$6,$7,$8,$9) ON CONFLICT (company_id,branch_id,business_date) DO NOTHING")
            .bind(input.id).bind(input.company_id).bind(input.branch_id).bind(input.business_date).bind(input.currency).bind(input.snapshot).bind(input.digest).bind(input.prepared_by).bind(input.prepared_at).execute(&mut *self.connection).await?;
        Ok(result.rows_affected() == 1)
    }
    /// Version and state remain in the predicate even after FOR UPDATE so the
    /// write cannot silently confirm a stale source if the call order changes.
    pub async fn confirm(
        &mut self,
        company_id: &str,
        id: &str,
        expected_version: i64,
        reviewer: &str,
        now: i64,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let result = sqlx::query("UPDATE pos_day_closes SET state='confirmed',confirmed_by=$1,confirmed_at=$2,version=version+1 WHERE company_id=$3 AND id=$4 AND version=$5 AND state='prepared'")
            .bind(reviewer).bind(now).bind(company_id).bind(id).bind(expected_version).execute(&mut *self.connection).await?;
        Ok(result.rows_affected() == 1)
    }
}
