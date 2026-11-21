//! Journal status set for the retained Service completion predicate. No Finance grant is minted.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct CurrentServiceCompletionJournals {
    ids: Vec<String>,
    digest: String,
    posted: bool,
    unit: Unit,
}
impl CurrentServiceCompletionJournals {
    pub async fn require_posted(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current(c).await?;
        if !self.posted {
            return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
        }
        Ok(())
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let (digest, posted) = read(c, &self.ids).await?;
        if digest != self.digest || posted != self.posted {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn inspect_service_completion_journals(
    c: &mut PgConnection,
    ids: &[String],
) -> Result<CurrentServiceCompletionJournals, Error> {
    let unit = unit(c).await?;
    let (digest, posted) = read(c, ids).await?;
    Ok(CurrentServiceCompletionJournals {
        ids: ids.to_vec(),
        digest,
        posted,
        unit,
    })
}
async fn read(c: &mut PgConnection, ids: &[String]) -> Result<(String, bool), Error> {
    let r=sqlx::query("SELECT encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(j) ORDER BY j.id),'[]'::jsonb)::text,'UTF8')),'hex') digest,coalesce(bool_and(j.status='posted'),true) posted FROM finance_journals j WHERE j.id=ANY($1)").bind(ids).fetch_one(c).await?;
    Ok((r.try_get("digest")?, r.try_get("posted")?))
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
    PersistenceError::new("FINANCE_SERVICE_COMPLETION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Service journal set changed.",
    )
    .into()
}
