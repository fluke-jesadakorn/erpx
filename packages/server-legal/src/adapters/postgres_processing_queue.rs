//! An owner-minted queue descriptor can cross no transaction boundary. Workflow
//! resolves receipt/replay before its apply callback invokes the canonical Legal
//! application; there is one job INSERT implementation.
use crate::domain::processing::*;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use sqlx::{PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
pub struct StagedLegalProcessingQueue {
    actor: LegalProcessingActor,
    key: CommandKey,
    kind: LegalProcessingKind,
    prepared: PreparedLegalProcessingQueue,
    transaction: (String, i32, String),
}
impl StagedLegalProcessingQueue {
    pub(super) async fn mint(
        c: &mut PgConnection,
        actor: LegalProcessingActor,
        key: CommandKey,
        kind: LegalProcessingKind,
        prepared: PreparedLegalProcessingQueue,
    ) -> Result<Self, Error> {
        Ok(Self {
            actor,
            key,
            kind,
            prepared,
            transaction: transaction(c).await?,
        })
    }
    pub fn actor(&self) -> &LegalProcessingActor {
        &self.actor
    }
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub fn kind(&self) -> LegalProcessingKind {
        self.kind
    }
    pub fn document(&self) -> &LegalProcessingDocument {
        &self.prepared.document
    }
    pub fn receipt(&self) -> Result<LegalProcessingReceipt, BusinessError> {
        Ok(LegalProcessingReceipt {
            id: self.prepared.document.item_id.clone(),
            job_id: RecordId::new(self.key.as_str())?,
            state: "queued".into(),
        })
    }
    pub async fn require_queued(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current(c).await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM legal_processing_jobs WHERE id=$1 AND company_id=$2 AND item_id=$3 AND document_id=$4 AND checksum=$5 AND kind=$6 AND state='queued'AND attempts=0 AND lease_token IS NULL AND lease_until IS NULL AND result_json IS NULL AND error IS NULL AND actor_json=$7 AND created_by=$8)")
            .bind(self.key.as_str()).bind(self.actor.company_id.as_str()).bind(self.prepared.document.item_id.as_str()).bind(self.prepared.document.document_id.as_str()).bind(&self.prepared.document.checksum_sha256).bind(self.kind.as_str()).bind(&self.prepared.authority.actor_snapshot_json).bind(self.actor.user_id.as_str()).fetch_one(c).await?;
        if !valid {
            return Err(scope_error().into());
        }
        Ok(())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(scope_error().into());
        }
        let row=sqlx::query("SELECT i.version::bigint version,d.item_id,d.checksum FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=$1 AND i.company_id=$2 AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id)AND NOT EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=i.id AND x.state IN('executing','completed')) FOR SHARE OF i,d").bind(self.prepared.document.document_id.as_str()).bind(self.actor.company_id.as_str()).fetch_optional(c).await?.ok_or_else(scope_error)?;
        if row.try_get::<String, _>("item_id")? != self.prepared.document.item_id.as_str()
            || row.try_get::<i64, _>("version")? != self.prepared.document.item_version as i64
            || row.try_get::<String, _>("checksum")? != self.prepared.document.checksum_sha256
        {
            return Err(scope_error().into());
        }
        Ok(())
    }
}
async fn transaction(c: &mut PgConnection) -> Result<(String, i32, String), Error> {
    use sqlx::Connection;
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text")
            .fetch_one(c)
            .await?,
    )
}
