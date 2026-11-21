//! Collaboration-owned roster Position-change staff notices.
//!
//! The retained Organization position save wrote two `staff_notifications`
//! inserts: one per affected future shift week (`roster_week`) and one per
//! pending roster request closed by the change (`roster_request`). Selection is
//! People-owned, so these statements repeat the People predicates verbatim and
//! write only the Collaboration inbox rows. Composition calls the week notice
//! after the week version bump and the request notice before the request
//! invalidation, preserving the retained read of `w.version` / `r.version+1`.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{AssertSqlSafe, Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

pub const ROSTER_POSITION_WEEK_MESSAGE: &str =
    "A future shift was removed after a Position change.";
pub const ROSTER_POSITION_REQUEST_MESSAGE: &str =
    "A pending roster request closed after a Position change.";
pub const ROSTER_POSITION_NOTICE_MAX_ENGAGEMENTS: usize = 500;

const MAX_ENGAGEMENT_ID: usize = 200;
const MAX_TIMESTAMP_MILLIS: i64 = 9_007_199_254_740_991;

/// Row counts of the two retained notice inserts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterPositionExitNotices {
    pub week: u64,
    pub request: u64,
}

pub fn roster_position_notice_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ROSTER_POSITION_NOTICE_INVALID",
        "The roster Position change notice input is invalid.",
    )
}

/// The retained `affected` selection, with SQL parameter tokens supplied by the
/// notice statement (`company`, `ids`, `now`).
fn affected_assignments(company: &str, ids: &str, now: &str) -> String {
    format!(
        "SELECT a.id,a.person_id,a.engagement_id,s.week_id,w.department_id
    FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id
    JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id={company} AND a.engagement_id=ANY({ids}::text[]) AND a.state='assigned' AND s.state='active' AND s.starts_at>{now}"
    )
}

/// The retained pending-request selection for the same exiting engagements.
fn pending_position_requests(company: &str, ids: &str) -> String {
    format!(
        "SELECT r.* FROM people_roster_requests r WHERE r.company_id={company} AND r.state IN ('pending_coworker','pending_manager')
    AND (r.requester_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY({ids}::text[]))
      OR r.coworker_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY({ids}::text[]))
      OR r.source_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY({ids}::text[]))
      OR r.target_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY({ids}::text[])))"
    )
}

/// Retained `roster_week` notice SQL. The row identity stays `gen_random_uuid()`
/// and duplicate suppression stays `ON CONFLICT DO NOTHING` over the retained
/// `(user_id,record_type,record_id,record_version,message)` unique tuple.
pub fn roster_position_week_notice_sql(company: &str, ids: &str, now: &str) -> String {
    format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,{company},p.user_id,'roster_week',a.week_id,w.version,'{message}',{now}
      FROM ({affected}) a JOIN hr_people p ON p.id=a.person_id AND p.company_id={company} JOIN people_roster_weeks w ON w.id=a.week_id
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING",
        message = ROSTER_POSITION_WEEK_MESSAGE,
        affected = affected_assignments(company, ids, now)
    )
}

/// Retained `roster_request` notice SQL. `r.version+1` is the version the
/// request invalidation is about to write, exactly as the retained builder read
/// it before its final update.
pub fn roster_position_request_notice_sql(company: &str, ids: &str, now: &str) -> String {
    format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,{company},p.user_id,'roster_request',r.id,r.version+1,'{message}',{now}
      FROM ({requests}) r JOIN hr_people p ON p.id IN (r.requester_person_id,r.coworker_person_id) AND p.company_id={company}
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING",
        message = ROSTER_POSITION_REQUEST_MESSAGE,
        requests = pending_position_requests(company, ids)
    )
}

/// Bounded notice input. Company scope is required, at least one engagement id
/// and a non-negative millisecond clock are required; every id must be the
/// untrimmed identifier the People plan received.
pub fn validate_roster_position_notice_input(
    company_id: &CompanyId,
    engagement_ids: &[String],
    now: i64,
) -> Result<(), BusinessError> {
    if company_id.as_str().is_empty()
        || engagement_ids.is_empty()
        || engagement_ids.len() > ROSTER_POSITION_NOTICE_MAX_ENGAGEMENTS
        || engagement_ids.iter().any(|id| {
            id.trim().is_empty() || id.chars().count() > MAX_ENGAGEMENT_ID || id != id.trim()
        })
        || !(0..=MAX_TIMESTAMP_MILLIS).contains(&now)
    {
        return Err(roster_position_notice_invalid());
    }
    Ok(())
}

/// The notice inserts require the caller's serializable transaction; a notice
/// staged outside it could never commit with the People effects it describes.
async fn ready(
    c: &mut PgConnection,
    company_id: &CompanyId,
    engagement_ids: &[String],
    now: i64,
) -> Result<(), Error> {
    validate_roster_position_notice_input(company_id, engagement_ids, now)?;
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ROSTER_POSITION_NOTICE_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("ROSTER_POSITION_NOTICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Write the retained `roster_week` notices for affected future shift weeks.
/// Call after the People week version bump so `w.version` is the bumped value.
pub async fn write_roster_position_exit_week_notices(
    c: &mut PgConnection,
    company_id: &CompanyId,
    engagement_ids: &[String],
    now: i64,
) -> Result<u64, Error> {
    ready(c, company_id, engagement_ids, now).await?;
    let sql = roster_position_week_notice_sql("$1", "$3", "$2");
    Ok(sqlx::query(AssertSqlSafe(sql))
        .bind(company_id.as_str())
        .bind(now)
        .bind(engagement_ids)
        .execute(&mut *c)
        .await?
        .rows_affected())
}

/// Write the retained `roster_request` notices for pending roster requests.
/// Call after the request history row and before the request invalidation so
/// `r.version+1` stays the notified version.
pub async fn write_roster_position_exit_request_notices(
    c: &mut PgConnection,
    company_id: &CompanyId,
    engagement_ids: &[String],
    now: i64,
) -> Result<u64, Error> {
    ready(c, company_id, engagement_ids, now).await?;
    let sql = roster_position_request_notice_sql("$1", "$3", "$2");
    Ok(sqlx::query(AssertSqlSafe(sql))
        .bind(company_id.as_str())
        .bind(now)
        .bind(engagement_ids)
        .execute(&mut *c)
        .await?
        .rows_affected())
}

/// Write both retained notices in the retained order. Composition that must
/// interleave the People phases calls the two functions separately; this helper
/// is for callers that stage a contiguous block.
pub async fn write_roster_position_exit_notices(
    c: &mut PgConnection,
    company_id: &CompanyId,
    engagement_ids: &[String],
    now: i64,
) -> Result<RosterPositionExitNotices, Error> {
    let week = write_roster_position_exit_week_notices(c, company_id, engagement_ids, now).await?;
    let request =
        write_roster_position_exit_request_notices(c, company_id, engagement_ids, now).await?;
    Ok(RosterPositionExitNotices { week, request })
}
