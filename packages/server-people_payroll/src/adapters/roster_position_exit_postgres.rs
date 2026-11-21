//! Native People roster Position-exit PostgreSQL effects.
//!
//! Each function runs on the caller-owned serializable connection and opens no
//! transaction of its own. Composition stages the retained order and interleaves
//! the two Collaboration notices:
//!
//! 1. [`stage_roster_position_exit_lock_and_history`] (locks, weeks, release history)
//! 2. Collaboration `roster_week` notice
//! 3. [`stage_roster_position_exit_release_and_history`] (assignments, request history)
//! 4. Collaboration `roster_request` notice
//! 5. [`stage_roster_position_exit_invalidate_requests`]
//!
//! Every predicate, parameter and history evidence value is the retained SQL.
//! Authorization and the current Position decision stay with the caller.
use crate::domain::roster_position_exit::{
    ROSTER_POSITION_EXIT_INVALIDATED_ACTION, ROSTER_POSITION_EXIT_RELEASED_ACTION,
    RosterPositionExitPlan, affected_assignments, pending_position_requests,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// One affected future active assignment for the exiting engagements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterPositionExitAffected {
    pub assignment_id: String,
    pub person_id: String,
    pub engagement_id: String,
    pub week_id: String,
    pub department_id: String,
}

/// Row counts of the lock/week version phase and its release history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterPositionExitLocked {
    pub locks: u64,
    pub weeks: u64,
    pub history: u64,
}

/// Row counts of the assignment release phase and its request history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterPositionExitReleased {
    pub assignments: u64,
    pub history: u64,
}

/// Row counts of the request invalidation phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterPositionExitInvalidated {
    pub requests: u64,
}

/// The phase functions require the caller's serializable transaction; these
/// effects are never staged from an autocommit connection or a weaker snapshot.
async fn ready(c: &mut PgConnection, plan: &RosterPositionExitPlan) -> Result<(), Error> {
    plan.validate()?;
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ROSTER_POSITION_EXIT_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("ROSTER_POSITION_EXIT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Shared affected-assignment read. Composition and tests can inspect exactly
/// which future assignments the People phases and the Collaboration `roster_week`
/// notice will select without staging a write.
pub async fn roster_position_exit_affected(
    c: &mut PgConnection,
    plan: &RosterPositionExitPlan,
) -> Result<Vec<RosterPositionExitAffected>, Error> {
    ready(c, plan).await?;
    let affected = affected_assignments("$1", "$2", "$3");
    let sql = format!(
        "SELECT affected.id,affected.person_id,affected.engagement_id,affected.week_id,affected.department_id
     FROM ({affected}) affected ORDER BY affected.id"
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(plan.company_id.as_str())
        .bind(plan.engagement_ids.as_slice())
        .bind(plan.now.get())
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(RosterPositionExitAffected {
                assignment_id: row.try_get("id")?,
                person_id: row.try_get("person_id")?,
                engagement_id: row.try_get("engagement_id")?,
                week_id: row.try_get("week_id")?,
                department_id: row.try_get("department_id")?,
            })
        })
        .collect()
}

/// Phase (a+b): bump the Company roster lock and every affected future active
/// week, then append the retained `assignment.released.position` history rows.
/// The week bump runs before the history insert, exactly as before, so the
/// Collaboration `roster_week` notice reads the bumped week version.
pub async fn stage_roster_position_exit_lock_and_history(
    c: &mut PgConnection,
    plan: &RosterPositionExitPlan,
) -> Result<RosterPositionExitLocked, Error> {
    ready(c, plan).await?;
    let company = plan.company_id.as_str();
    let ids = plan.engagement_ids.as_slice();
    let now = plan.now.get();
    let locks = sqlx::query("UPDATE people_roster_locks SET version=version+1 WHERE company_id=$1")
        .bind(company)
        .execute(&mut *c)
        .await?
        .rows_affected();
    // The statement parameters are company=$1, now=$2, ids=$3; the nested
    // affected predicate reuses those exact tokens.
    let week_sql = format!(
        "UPDATE people_roster_weeks SET version=version+1,updated_at=$2 WHERE id IN (SELECT week_id FROM ({affected}) affected)",
        affected = affected_assignments("$1", "$3", "$2")
    );
    let weeks = sqlx::query(AssertSqlSafe(week_sql))
        .bind(company)
        .bind(now)
        .bind(ids)
        .execute(&mut *c)
        .await?
        .rows_affected();
    // History parameters are command=$1, company=$2, actor=$3, reason=$4,
    // now=$5, ids=$6. The retained text concatenation casts the bound key.
    let history_sql = format!(
        "INSERT INTO people_roster_history(id,company_id,department_id,week_id,record_id,action,actor_id,reason,evidence_json,created_at)
      SELECT $1::text || '-release-' || a.id,$2,a.department_id,a.week_id,a.id,'{action}',$3,$4,json_build_object('engagementId',a.engagement_id)::text,$5
      FROM ({affected}) a ON CONFLICT DO NOTHING",
        action = ROSTER_POSITION_EXIT_RELEASED_ACTION,
        affected = affected_assignments("$2", "$6", "$5")
    );
    let history = sqlx::query(AssertSqlSafe(history_sql))
        .bind(&plan.command_id)
        .bind(company)
        .bind(&plan.actor_user_id)
        .bind(&plan.reason)
        .bind(now)
        .bind(ids)
        .execute(&mut *c)
        .await?
        .rows_affected();
    Ok(RosterPositionExitLocked {
        locks,
        weeks,
        history,
    })
}

/// Phase (d+e): release the affected future active assignments, then append the
/// retained `request.invalidated.position` history rows. The request predicate
/// is re-evaluated after the release, exactly as the retained statement order
/// did; the assignment subqueries carry no state filter.
pub async fn stage_roster_position_exit_release_and_history(
    c: &mut PgConnection,
    plan: &RosterPositionExitPlan,
) -> Result<RosterPositionExitReleased, Error> {
    ready(c, plan).await?;
    let company = plan.company_id.as_str();
    let ids = plan.engagement_ids.as_slice();
    let now = plan.now.get();
    let assignments_sql = format!(
        "UPDATE people_roster_assignments SET state='released',version=version+1 WHERE id IN (SELECT id FROM ({affected}) a)",
        affected = affected_assignments("$1", "$2", "$3")
    );
    let assignments = sqlx::query(AssertSqlSafe(assignments_sql))
        .bind(company)
        .bind(ids)
        .bind(now)
        .execute(&mut *c)
        .await?
        .rows_affected();
    // History parameters are command=$1, company=$2, actor=$3, reason=$4,
    // now=$5, ids=$6; the requests predicate reuses company=$2 and ids=$6.
    let history_sql = format!(
        "INSERT INTO people_roster_history(id,company_id,department_id,request_id,record_id,action,actor_id,reason,evidence_json,created_at)
      SELECT $1::text || '-request-' || r.id,$2,r.department_id,r.id,r.id,'{action}',$3,$4,json_build_object('positionChange',true)::text,$5
      FROM ({requests}) r ON CONFLICT DO NOTHING",
        action = ROSTER_POSITION_EXIT_INVALIDATED_ACTION,
        requests = pending_position_requests("$2", "$6")
    );
    let history = sqlx::query(AssertSqlSafe(history_sql))
        .bind(&plan.command_id)
        .bind(company)
        .bind(&plan.actor_user_id)
        .bind(&plan.reason)
        .bind(now)
        .bind(ids)
        .execute(&mut *c)
        .await?
        .rows_affected();
    Ok(RosterPositionExitReleased {
        assignments,
        history,
    })
}

/// Phase (g): invalidate the affected pending roster requests with the retained
/// Position reason as their decision note.
pub async fn stage_roster_position_exit_invalidate_requests(
    c: &mut PgConnection,
    plan: &RosterPositionExitPlan,
) -> Result<RosterPositionExitInvalidated, Error> {
    ready(c, plan).await?;
    let company = plan.company_id.as_str();
    let ids = plan.engagement_ids.as_slice();
    // Parameters are reason=$1, now=$2, company=$3, ids=$4.
    let sql = format!(
        "UPDATE people_roster_requests SET state='invalidated',decision_note=$1,version=version+1,updated_at=$2 WHERE id IN (SELECT id FROM ({requests}) r)",
        requests = pending_position_requests("$3", "$4")
    );
    let requests = sqlx::query(AssertSqlSafe(sql))
        .bind(&plan.reason)
        .bind(plan.now.get())
        .bind(company)
        .bind(ids)
        .execute(&mut *c)
        .await?
        .rows_affected();
    Ok(RosterPositionExitInvalidated { requests })
}
