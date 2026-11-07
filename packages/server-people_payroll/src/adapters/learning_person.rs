//! Person-only predicate used by Learning certificate consumption. This grants
//! neither employment nor authority, and deliberately does not inspect engagements.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

/// Call before the first transaction snapshot and before binding any owner facts.
/// The coordinator must establish a compatible global lock order. This helper
/// cannot prove that a caller did not take a snapshot earlier.
pub async fn lock_learning_person_source(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_LEARNING_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE hr_people IN SHARE MODE").execute(c).await?;
    Ok(())
}

pub struct CurrentLearningPerson {
    company: String,
    person: String,
    active_unmerged: bool,
    unit: Unit,
}
impl CurrentLearningPerson {
    pub fn is_active_unmerged(&self) -> bool { self.active_unmerged }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || !holds_source_lock(c).await?
            || read(c, &self.company, &self.person).await? != self.active_unmerged {
            return Err(BusinessError::new(FailureKind::Conflict, "PEOPLE_LEARNING_PERSON_CHANGED", "The Learning Person source changed.").into());
        }
        Ok(())
    }
}

/// Requires the utility lock in this same serializable unit; the caller must
/// acquire it before any source or authority snapshot.
pub async fn inspect_learning_person(c: &mut PgConnection, company: &str, person: &str) -> Result<CurrentLearningPerson, Error> {
    let unit = unit(c).await?;
    if !holds_source_lock(c).await? {
        return Err(PersistenceError::new("PEOPLE_LEARNING_SOURCE_LOCK_REQUIRED").into());
    }
    let active_unmerged = read(c, company, person).await?;
    Ok(CurrentLearningPerson { company: company.to_owned(), person: person.to_owned(), active_unmerged, unit })
}
async fn read(c: &mut PgConnection, company: &str, person: &str) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM hr_people WHERE company_id=$1 AND id=$2 AND status='active' AND merged_into_id IS NULL)")
        .bind(company).bind(person).fetch_one(c).await?)
}
async fn holds_source_lock(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='hr_people'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("PEOPLE_LEARNING_TRANSACTION_REQUIRED").into()); }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("PEOPLE_LEARNING_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
