//! Lane 35 Postgres adapter. This is the only place in the migrated path that
//! holds SQL text for the Records work-report family. It executes owner-local
//! statements on a pooled Holding connection and never trusts a caller string
//! as SQL: company scope and the id array are always bind parameters.

use crate::adapters::native_lane_35::application::{LaneFuture, WorkReportSource};
use crate::adapters::native_lane_35::domain::{
    CandidateRow, EventRow, SubmitTime, unavailable,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Byte-identical legacy candidate statement, kept as the before-oracle. The
/// kind predicate and the submit-time aggregation moved into Rust.
pub const LEGACY_CANDIDATES_SQL: &str = "SELECT approval_json,items_json,r.id,'request' kind,kind type,title,state status,requester_user_id requester,branch_id branch,department_id department,currency,NULL::bigint amount,NULL::numeric units,(SELECT MIN(created_at) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id AND e.action='submit') submitted_at,created_at,updated_at FROM employee_requests r WHERE company_id=? AND kind IN ('purchase','service')";

/// Byte-identical legacy event statement, kept as the before-oracle. The final
/// ordering moved into Rust; the extra `id` column preserves tie-break order.
pub const LEGACY_EVENTS_SQL: &str = "SELECT request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=? AND request_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at,id";

pub const CANDIDATES_SQL: &str = "SELECT approval_json,items_json,r.id,kind,title,state,requester_user_id,branch_id,department_id,currency,created_at,updated_at FROM employee_requests r WHERE company_id=$1";

pub const SUBMITS_SQL: &str = "SELECT request_id,MIN(created_at) submitted_at FROM employee_request_events WHERE company_id=$1 AND action='submit' AND request_id IN (SELECT jsonb_array_elements_text($2::jsonb)) GROUP BY request_id";

pub const EVENTS_SQL: &str = "SELECT id,request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=$1 AND request_id IN (SELECT jsonb_array_elements_text($2::jsonb))";

/// Postgres-backed owner reader for the lane family.
pub struct PgWorkReportSource {
    pool: DatabasePool,
}

impl PgWorkReportSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkReportSource for PgWorkReportSource {
    fn candidate_rows<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<CandidateRow>> {
        let company = company.to_owned();
        Box::pin(async move {
            let mut connection = self
                .pool
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let rows = sqlx::query(CANDIDATES_SQL)
                .bind(company.as_str())
                .fetch_all(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(CandidateRow {
                        approval_json: row.try_get("approval_json").map_err(|_| unavailable())?,
                        items_json: row.try_get("items_json").map_err(|_| unavailable())?,
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        kind: row.try_get("kind").map_err(|_| unavailable())?,
                        title: row.try_get("title").map_err(|_| unavailable())?,
                        state: row.try_get("state").map_err(|_| unavailable())?,
                        requester_user_id: row
                            .try_get("requester_user_id")
                            .map_err(|_| unavailable())?,
                        branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                        department_id: row.try_get("department_id").map_err(|_| unavailable())?,
                        currency: row.try_get("currency").map_err(|_| unavailable())?,
                        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
                        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }

    fn submit_times<'a>(
        &'a self,
        company: &'a str,
        ids: &'a [String],
    ) -> LaneFuture<'a, Vec<SubmitTime>> {
        let company = company.to_owned();
        let ids = ids.to_vec();
        Box::pin(async move {
            let ids = serde_json::to_string(&ids).map_err(|_| unavailable())?;
            let mut connection = self
                .pool
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let rows = sqlx::query(SUBMITS_SQL)
                .bind(company.as_str())
                .bind(ids.as_str())
                .fetch_all(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(SubmitTime {
                        request_id: row.try_get("request_id").map_err(|_| unavailable())?,
                        submitted_at: row.try_get("submitted_at").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }

    fn event_rows<'a>(&'a self, company: &'a str, ids: &'a str) -> LaneFuture<'a, Vec<EventRow>> {
        let company = company.to_owned();
        let ids = ids.to_owned();
        Box::pin(async move {
            let mut connection = self
                .pool
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let rows = sqlx::query(EVENTS_SQL)
                .bind(company.as_str())
                .bind(ids.as_str())
                .fetch_all(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(EventRow {
                        id: row.try_get("id").map_err(|_| unavailable())?,
                        record_id: row.try_get("record_id").map_err(|_| unavailable())?,
                        action: row.try_get("action").map_err(|_| unavailable())?,
                        actor: row.try_get("actor").map_err(|_| unavailable())?,
                        at: row.try_get("at").map_err(|_| unavailable())?,
                        reason: row.try_get("reason").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verbatim HEAD literals. The matching SHA-256 digests are recorded in
    /// `.scratch/migrate_rs.lane_35-inventory.json` and the TS contract test.
    const HEAD_CANDIDATES: &str = "SELECT approval_json,items_json,r.id,'request' kind,kind type,title,state status,requester_user_id requester,branch_id branch,department_id department,currency,NULL::bigint amount,NULL::numeric units,(SELECT MIN(created_at) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id AND e.action='submit') submitted_at,created_at,updated_at FROM employee_requests r WHERE company_id=? AND kind IN ('purchase','service')";
    const HEAD_EVENTS: &str = "SELECT request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=? AND request_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at,id";

    #[test]
    fn legacy_sql_oracles_are_byte_identical() {
        assert_eq!(LEGACY_CANDIDATES_SQL, HEAD_CANDIDATES);
        assert_eq!(LEGACY_EVENTS_SQL, HEAD_EVENTS);
        assert_eq!(LEGACY_CANDIDATES_SQL.matches("SELECT").count(), 2);
        assert_eq!(LEGACY_EVENTS_SQL.matches("SELECT").count(), 2);
    }

    #[test]
    fn executed_sql_is_owner_local_and_parameterised() {
        assert!(CANDIDATES_SQL.contains("company_id=$1"));
        assert!(SUBMITS_SQL.contains("company_id=$1"));
        assert!(EVENTS_SQL.contains("company_id=$1"));
        assert!(!CANDIDATES_SQL.contains("kind IN"));
        assert!(!EVENTS_SQL.contains("ORDER BY"));
    }
}
