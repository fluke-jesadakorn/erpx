//! Identity-owned same-person facts for a retained Quality transaction.
//! This proves a predicate, not the caller's authority or source scope.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
type MemberImage = Vec<(String, Option<String>)>;

pub struct QualitySamePersonLockPreparation<'a> { connection: &'a mut PgConnection }
pub struct QualitySamePersonSourceLocks { unit: Unit }

/// Acquire before the first Holding snapshot and in the composition's owner
/// order. NOWAIT permits the complete outer unit to retry on conflicting IAM
/// writers rather than wait while holding earlier source locks.
pub async fn lock_quality_same_person_sources(c: &mut PgConnection) -> Result<QualitySamePersonLockPreparation<'_>, Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("IAM_SAME_PERSON_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE iam_members IN SHARE MODE NOWAIT").execute(&mut *c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(QualitySamePersonLockPreparation { connection: c })
}

impl QualitySamePersonLockPreparation<'_> {
    pub fn connection(&mut self) -> &mut PgConnection { self.connection }
    /// Bind only after all remaining owner utility locks have been acquired.
    pub async fn bind(self) -> Result<QualitySamePersonSourceLocks, Error> {
        require_lock(self.connection).await?;
        Ok(QualitySamePersonSourceLocks { unit: unit(self.connection).await? })
    }
}

pub struct CurrentQualitySamePerson {
    unit: Unit,
    company: CompanyId,
    left: String,
    right: String,
    image: MemberImage,
    same: bool,
}

impl CurrentQualitySamePerson {
    pub fn same_person(&self) -> bool { self.same }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_lock(c).await?;
        if unit(c).await? != self.unit { return Err(changed()); }
        let (image, same) = read(c, &self.company, &self.left, &self.right).await?;
        if image != self.image || same != self.same { return Err(changed()); }
        Ok(())
    }
}

pub async fn inspect_quality_same_person(c: &mut PgConnection, locks: &QualitySamePersonSourceLocks, company: &CompanyId, left: &str, right: &str) -> Result<CurrentQualitySamePerson, Error> {
    require_lock(c).await?;
    let current = unit(c).await?;
    if current != locks.unit { return Err(changed()); }
    let (image, same) = read(c, company, left, right).await?;
    Ok(CurrentQualitySamePerson { unit: current, company: company.clone(), left: left.to_owned(), right: right.to_owned(), image, same })
}

fn changed() -> Error { PersistenceError::new("IAM_SAME_PERSON_SOURCE_CHANGED").into() }

async fn require_lock(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("IAM_SAME_PERSON_TRANSACTION_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='iam_members'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;
    if !held { return Err(PersistenceError::new("IAM_SAME_PERSON_LOCK_REQUIRED").into()); }
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

async fn read(c: &mut PgConnection, company: &CompanyId, left: &str, right: &str) -> Result<(MemberImage, bool), Error> {
    // Match iam_same_person exactly: status/login policy are intentionally not
    // predicates. Primary key (company_id,user_id) bounds this image to two.
    let rows = sqlx::query("SELECT user_id,person_id FROM iam_members WHERE company_id=$1 AND user_id IN ($2,$3) ORDER BY user_id COLLATE \"C\"")
        .bind(company.as_str()).bind(left).bind(right).fetch_all(&mut *c).await?;
    let image = rows.into_iter().map(|row| Ok((row.try_get("user_id")?, row.try_get("person_id")?))).collect::<Result<MemberImage, sqlx::Error>>()?;
    let same = sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)").bind(left).bind(right).bind(company.as_str()).fetch_one(c).await?;
    Ok((image, same))
}
