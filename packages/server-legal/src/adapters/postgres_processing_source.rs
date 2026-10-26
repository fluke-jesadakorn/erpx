use super::postgres_processing::parse_processing_job;
use crate::domain::processing::{
    LegalProcessingJob, LegalProcessingLease, PROCESSING_ACTOR_LIMIT, PROCESSING_RESULT_LIMIT,
    lease_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

/// Data selected by the trusted worker/tenant coordinator, never authorization.
pub struct LegalProcessingSourceRequest {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub job_id: RecordId,
    pub lease_token: RecordId,
}

/// An opaque current owner source, minted only after reading/locking the actual
/// running row. No public constructor, Deserialize, or cached IAM grant exists.
/// Identity composition must strictly decode the historical origin snapshot and
/// read current IAM/People/Org facts; this proof alone grants no permission.
pub struct CurrentLegalProcessingSource {
    lease: LegalProcessingLease,
    transaction: (String, i32, String),
    completed: std::sync::Mutex<Option<CompletedInspection>>,
}
#[derive(Clone)]
struct CompletedInspection {
    inspection_id: RecordId,
    result: serde_json::Value,
}
impl CurrentLegalProcessingSource {
    pub fn holding_id(&self) -> &HoldingId {
        &self.lease.job.holding_id
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.lease.job.company_id
    }
    pub fn user_id(&self) -> &UserId {
        &self.lease.job.created_by
    }
    pub fn job(&self) -> &LegalProcessingJob {
        &self.lease.job
    }
    pub fn lease(&self) -> &LegalProcessingLease {
        &self.lease
    }
    pub fn actor_snapshot_json(&self) -> &str {
        &self.lease.job.actor_snapshot_json
    }
    pub fn deadline(&self) -> i64 {
        self.lease.until
    }

    pub async fn require_current(
        &self,
        connection: &mut PgConnection,
        now: i64,
    ) -> Result<(), Error> {
        require_transaction(connection).await?;
        if self.transaction != transaction(connection).await? {
            return Err(lease_changed().into());
        }
        let current = load_current_legal_processing_source(
            connection,
            LegalProcessingSourceRequest {
                holding_id: self.holding_id().clone(),
                company_id: self.company_id().clone(),
                job_id: self.job().id.clone(),
                lease_token: self.lease.token.clone(),
            },
            now,
        )
        .await?;
        if current.lease != self.lease {
            return Err(lease_changed().into());
        }
        Ok(())
    }
    /// Explicit final-hook policy. A completed row is accepted only when this
    /// exact source received a private stamp from actual Legal completion.
    pub async fn require_current_or_completed(
        &self,
        connection: &mut PgConnection,
        now: i64,
    ) -> Result<(), Error> {
        let completed = self
            .completed
            .lock()
            .map_err(|_| PersistenceError::new("LEGAL_PROCESSING_WITNESS_UNAVAILABLE"))?
            .clone();
        match completed {
            None => self.require_current(connection, now).await,
            Some(completed) => self.require_completed(connection, now, &completed).await,
        }
    }
    pub(super) async fn record_completed(
        &self,
        connection: &mut PgConnection,
        now: i64,
        result: &serde_json::Value,
    ) -> Result<(), Error> {
        let inspection_id =
            RecordId::new(result["inspectionId"].as_str().ok_or_else(lease_changed)?)?;
        let completed = CompletedInspection {
            inspection_id,
            result: result.clone(),
        };
        self.require_completed(connection, now, &completed).await?;
        let mut guard = self
            .completed
            .lock()
            .map_err(|_| PersistenceError::new("LEGAL_PROCESSING_WITNESS_UNAVAILABLE"))?;
        if guard.is_some() {
            return Err(lease_changed().into());
        }
        *guard = Some(completed);
        Ok(())
    }
    async fn require_completed(
        &self,
        connection: &mut PgConnection,
        now: i64,
        completed: &CompletedInspection,
    ) -> Result<(), Error> {
        require_transaction(connection).await?;
        self.lease
            .require_live(self.holding_id(), self.company_id(), now)?;
        if transaction(connection).await? != self.transaction {
            return Err(lease_changed().into());
        }
        let row=sqlx::query("SELECT j.id,j.item_id,j.document_id,j.checksum,j.kind,j.attempts::bigint AS attempts,j.created_by,j.created_at,CASE WHEN octet_length(j.actor_json)<=$4 THEN j.actor_json END AS actor_json,i.version::bigint AS item_version,CASE WHEN octet_length(j.result_json)<=$5 THEN j.result_json END AS result_json,CASE WHEN octet_length(v.result_json)<=$5 THEN v.result_json END AS inspection_json FROM legal_processing_jobs j JOIN legal_items i ON i.id=j.item_id AND i.company_id=j.company_id JOIN legal_documents d ON d.id=j.document_id AND d.item_id=i.id JOIN legal_document_inspections v ON v.id=$3 AND v.document_id=d.id AND v.kind=j.kind AND v.created_by=j.created_by WHERE j.id=$1 AND j.company_id=$2 AND j.state='completed' AND j.error IS NULL AND j.lease_token IS NULL AND j.lease_until IS NULL AND d.checksum=j.checksum AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) FOR UPDATE OF j FOR SHARE OF i,d,v")
            .bind(self.job().id.as_str()).bind(self.company_id().as_str()).bind(completed.inspection_id.as_str()).bind(PROCESSING_ACTOR_LIMIT as i64).bind(PROCESSING_RESULT_LIMIT as i64).fetch_optional(&mut *connection).await?.ok_or_else(lease_changed)?;
        let job = parse_processing_job(&row, self.holding_id(), self.company_id())?;
        let parse = |name: &str| -> Result<serde_json::Value, Error> {
            let value: Option<String> = row.try_get(name)?;
            serde_json::from_str(&value.ok_or_else(lease_changed)?)
                .map_err(|_| lease_changed().into())
        };
        if job != self.lease.job
            || row.try_get::<i64, _>("attempts")? != i64::from(self.lease.attempts)
            || parse("result_json")? != completed.result
            || parse("inspection_json")? != completed.result
            || completed.result["processingJobId"] != self.job().id.as_str()
            || completed.result["checksum"] != self.job().document.checksum_sha256
            || completed.result["inspectionId"] != completed.inspection_id.as_str()
        {
            return Err(lease_changed().into());
        }
        Ok(())
    }
}

/// Call only after the coordinator has retained its current Control authority
/// and established the fresh Holding policy snapshot. No transaction is opened,
/// committed or retried here. The owner source never reads foreign-owner tables.
pub async fn load_current_legal_processing_source(
    connection: &mut PgConnection,
    request: LegalProcessingSourceRequest,
    now: i64,
) -> Result<CurrentLegalProcessingSource, Error> {
    require_transaction(connection).await?;
    // Utility locks precede every SELECT: a SERIALIZABLE advisory-lock SELECT
    // may establish its snapshot *before* waiting. Inserts into denial tables
    // do not necessarily update the selected item/document/job tuple.
    sqlx::query("LOCK TABLE legal_disposals,legal_destructions IN SHARE MODE")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("legal:{}", request.company_id.as_str()))
        .execute(&mut *connection)
        .await?;
    let row=sqlx::query("SELECT j.id,j.item_id,j.document_id,j.checksum,j.kind,j.attempts::bigint AS attempts,j.created_by,j.created_at,j.lease_until,CASE WHEN octet_length(j.actor_json)<=$5 THEN j.actor_json END AS actor_json,i.version::bigint AS item_version FROM legal_processing_jobs j JOIN legal_items i ON i.id=j.item_id AND i.company_id=j.company_id JOIN legal_documents d ON d.id=j.document_id AND d.item_id=i.id WHERE j.id=$1 AND j.company_id=$2 AND j.state='running' AND j.lease_token=$3 AND j.lease_until>=$4 AND d.checksum=j.checksum AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN ('executing','completed')) FOR UPDATE OF j FOR SHARE OF i,d")
        .bind(request.job_id.as_str()).bind(request.company_id.as_str()).bind(request.lease_token.as_str()).bind(now).bind(PROCESSING_ACTOR_LIMIT as i64).fetch_optional(&mut *connection).await?.ok_or_else(lease_changed)?;
    let job = parse_processing_job(&row, &request.holding_id, &request.company_id)?;
    let attempts = u32::try_from(row.try_get::<i64, _>("attempts")?)
        .map_err(|_| PersistenceError::new("LEGAL_PROCESSING_CORRUPT_RECORD"))?;
    let lease = LegalProcessingLease {
        job,
        token: request.lease_token,
        until: row.try_get("lease_until")?,
        attempts,
    };
    lease.require_live(&request.holding_id, &request.company_id, now)?;
    Ok(CurrentLegalProcessingSource {
        lease,
        transaction: transaction(connection).await?,
        completed: std::sync::Mutex::new(None),
    })
}
async fn require_transaction(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(connection: &mut PgConnection) -> Result<(String, i32, String), Error> {
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text")
            .fetch_one(connection)
            .await?,
    )
}
