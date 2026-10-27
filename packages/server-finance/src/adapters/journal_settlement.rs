//! Factual status snapshots for source-owner settlement. This never grants
//! Finance read/write, posting, purchase or source-owner close authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct JournalSettlementScan {
    unit: Unit,
    digest: Sha256,
    count: u64,
    blocked: bool,
}
pub struct JournalSettlementImage {
    unit: Unit,
    digest: [u8; 32],
    count: u64,
    blocked: bool,
}
pub async fn lock_journal_settlement_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE finance_journals IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
impl JournalSettlementScan {
    pub async fn begin(c: &mut PgConnection) -> Result<Self, Error> {
        let mut digest = Sha256::new();
        digest.update(b"erpx-journal-settlement-v1\0");
        Ok(Self {
            unit: unit(c).await?,
            digest,
            count: 0,
            blocked: false,
        })
    }
    /// The Service feeds actual owner references. Memory is constant regardless
    /// of history length; private journal bodies are hashed inside PostgreSQL.
    pub async fn observe(&mut self, c: &mut PgConnection, id: &str) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let row:Option<(bool,Vec<u8>)>=sqlx::query_as("SELECT COALESCE(status<>'posted',false),sha256(convert_to(to_jsonb(j)::text,'UTF8')) FROM finance_journals j WHERE id=$1 FOR SHARE").bind(id).fetch_optional(c).await?;
        self.count = self.count.checked_add(1).ok_or_else(invalid)?;
        self.digest.update((id.len() as u64).to_be_bytes());
        self.digest.update(id.as_bytes());
        // Exact B0 inner-join/SQL-null semantics: a missing or NULL-status journal
        // does not satisfy `status <> posted`. No new Company/type/cost admission.
        match row {
            Some((blocked, hash)) => {
                self.digest.update([1]);
                self.digest.update(hash);
                self.blocked |= blocked;
            }
            None => self.digest.update([0]),
        }
        Ok(())
    }
    pub fn finish(self) -> JournalSettlementImage {
        JournalSettlementImage {
            unit: self.unit,
            digest: self.digest.finalize().into(),
            count: self.count,
            blocked: self.blocked,
        }
    }
}
impl JournalSettlementImage {
    pub fn blocks_source_closure(&self) -> bool {
        self.blocked
    }
    pub async fn require_same(&self, c: &mut PgConnection, fresh: &Self) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || fresh.unit != self.unit
            || fresh.count != self.count
            || fresh.blocked != self.blocked
            || fresh.digest != self.digest
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("JOURNAL_SETTLEMENT_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "JOURNAL_SETTLEMENT_SOURCE_CHANGED",
        "The original journal settlement image changed.",
    )
    .into()
}
