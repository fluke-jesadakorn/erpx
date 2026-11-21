//! POS-owned day-close persistence on the caller's serializable transaction.
//! Authorization, same-person identity and current cross-owner report facts must
//! be checked by the application in this very transaction before these writes.
use crate::domain::day_close::PreparedDayClose;
use crate::domain::day_receipt::PosDayReceiptIdentity;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
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
    pub async fn replay_receipt(
        &mut self,
        identity: &PosDayReceiptIdentity,
    ) -> Result<Option<CommandJson>, Error> {
        self.ready().await?;
        let row = sqlx::query("SELECT request_hash,result_json FROM pos_receipts WHERE id=$1")
            .bind(identity.key.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            let hash: String = row.try_get("request_hash")?;
            let result: String = row.try_get("result_json")?;
            identity.replay_sha256(&hash, &result).map_err(Error::from)
        })
        .transpose()
    }
    /// Insert the owner receipt on the same transaction as its effects.
    pub async fn insert_receipt(
        &mut self,
        identity: &PosDayReceiptIdentity,
        result: &CommandJson,
        now: i64,
    ) -> Result<(), Error> {
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
    /// Whether a prepared close already exists for the exact company, branch
    /// and business date. Checked on the locked transaction before a prepare.
    pub async fn prepared_exists(
        &mut self,
        company_id: &str,
        branch_id: &str,
        business_date: &str,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let found = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pos_day_closes WHERE company_id=$1 AND branch_id=$2 AND business_date=$3)",
        )
        .bind(company_id)
        .bind(branch_id)
        .bind(business_date)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(found)
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

impl crate::application::day_close::DayCloseStore for PgPosDayClose<'_> {
    type Error = Error;

    fn replay_receipt<'a>(
        &'a mut self,
        identity: &'a PosDayReceiptIdentity,
    ) -> crate::application::day_close::DayCloseFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            Ok(PgPosDayClose::replay_receipt(self, identity)
                .await?
                .map(|value| value.as_str().to_owned()))
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        identity: &'a PosDayReceiptIdentity,
        result_json: &'a str,
        now: i64,
    ) -> crate::application::day_close::DayCloseFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let result = CommandJson::parse(result_json).map_err(Error::from)?;
            PgPosDayClose::insert_receipt(self, identity, &result, now).await
        })
    }

    fn lock_company<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> crate::application::day_close::DayCloseFuture<'a, (), Self::Error> {
        Box::pin(async move { PgPosDayClose::lock_company(self, company_id).await })
    }

    fn prepared_exists<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> crate::application::day_close::DayCloseFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgPosDayClose::prepared_exists(self, company_id, branch_id, business_date).await
        })
    }

    fn lock_close<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> crate::application::day_close::DayCloseFuture<'a, Option<PreparedDayClose>, Self::Error>
    {
        Box::pin(async move { PgPosDayClose::lock_close(self, company_id, id).await })
    }

    fn prepare<'a>(
        &'a mut self,
        close: &'a NewDayClose<'a>,
    ) -> crate::application::day_close::DayCloseFuture<'a, bool, Self::Error> {
        Box::pin(async move { PgPosDayClose::prepare(self, close).await })
    }

    fn confirm<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        expected_version: i64,
        reviewer: &'a str,
        now: i64,
    ) -> crate::application::day_close::DayCloseFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            PgPosDayClose::confirm(self, company_id, id, expected_version, reviewer, now).await
        })
    }
}
