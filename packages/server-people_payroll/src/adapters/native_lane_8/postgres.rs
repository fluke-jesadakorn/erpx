//! Owner PostgreSQL execution for the lane 8 read family. The statements are
//! the byte-identical owner SQL from the retired TypeScript call sites with the
//! only execution-time adaptation being D1 `?` binds rewritten to sqlx/Postgres
//! `$n` binds. Bind order, predicates, projection and row order are unchanged;
//! the Company scope is always the first bind.

use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::application::{AppEntrypointRead, LaneReadFuture};
use super::domain::{
    ApproverName, PayGroup, PersonOption, ReadKind, RosterShift, unavailable,
};

use erpx_server_runtime::public::postgres::DatabasePool;

const PAY_GROUPS_SQL: &str =
    "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND status='active' ORDER BY name,id";
const PEOPLE_SQL: &str =
    "SELECT id,legal_name,email,version FROM hr_people WHERE company_id=$1 AND status='active' AND merged_into_id IS NULL ORDER BY legal_name,id";
const APPROVER_NAME_SQL: &str =
    "SELECT legal_name FROM hr_people WHERE company_id=$1 AND iam_same_person(user_id,$2,company_id) AND merged_into_id IS NULL LIMIT 1";
const ROSTER_SHIFT_SQL: &str = "SELECT s.id,s.local_date,s.start_time,s.end_time,w.timezone,to_char(to_timestamp(s.ends_at/1000.0) AT TIME ZONE w.timezone,'YYYY-MM-DD\"T\"HH24:MI') end_local FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=$1 AND w.company_id=$2 AND w.state='published' AND s.state='active' AND a.engagement_id=$3 AND a.state='assigned'";

/// The exact statement owned by each read kind; exposed for the SQL oracle.
pub fn rendered_sql(kind: ReadKind) -> &'static str {
    match kind {
        ReadKind::PayGroups => PAY_GROUPS_SQL,
        ReadKind::People => PEOPLE_SQL,
        ReadKind::ApproverName => APPROVER_NAME_SQL,
        ReadKind::RosterShift => ROSTER_SHIFT_SQL,
    }
}

fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Stable SQL/result oracle over a read kind, its exact statement and the
/// canonical projected result. Used by the Rust and TypeScript parity tests.
pub fn oracle(kind: ReadKind, result: &str) -> String {
    sha256(&format!("{}|{}|{}", kind.as_str(), rendered_sql(kind), result))
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    if let Ok(Some(value)) = row.try_get::<Option<String>, _>(column) {
        return Ok(value);
    }
    if let Ok(Some(value)) = row.try_get::<Option<i64>, _>(column) {
        return Ok(value.to_string());
    }
    row.try_get::<Option<i32>, _>(column)
        .ok()
        .flatten()
        .map(|value| value.to_string())
        .ok_or_else(unavailable)
}

fn option_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    if let Ok(value) = row.try_get::<Option<String>, _>(column) {
        return Ok(value);
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(column) {
        return Ok(value.map(|item| item.to_string()));
    }
    row.try_get::<Option<i32>, _>(column)
        .map(|value| value.map(|item| item.to_string()))
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<i64, _>(column)
        .or_else(|_| row.try_get::<i32, _>(column).map(i64::from))
        .map_err(|_| unavailable())
}

/// Pg-backed owner reader. Only People Payroll-owned tables are touched; no
/// permission, stock or command authority is inferred from these reads.
pub struct PgAppEntrypointRead {
    pool: DatabasePool,
}

impl PgAppEntrypointRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AppEntrypointRead for PgAppEntrypointRead {
    fn pay_groups<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PayGroup>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(PAY_GROUPS_SQL)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PayGroup {
                        id: text(row, "id")?,
                        name: text(row, "name")?,
                        currency: text(row, "currency")?,
                        frequency: text(row, "frequency")?,
                    })
                })
                .collect()
        })
    }

    fn people<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PersonOption>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(PEOPLE_SQL)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PersonOption {
                        id: text(row, "id")?,
                        legal_name: text(row, "legal_name")?,
                        email: option_text(row, "email")?,
                        version: integer(row, "version")?,
                    })
                })
                .collect()
        })
    }

    fn approver_name<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<ApproverName>> {
        let pool = self.pool.clone();
        let (company, user) = (company.to_owned(), user.to_owned());
        Box::pin(async move {
            let row = sqlx::query(APPROVER_NAME_SQL)
                .bind(company.as_str())
                .bind(user.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| Ok(ApproverName { legal_name: text(&row, "legal_name")? }))
                .transpose()
        })
    }

    fn roster_shift<'a>(
        &'a self,
        company: &'a str,
        shift: &'a str,
        engagement: &'a str,
    ) -> LaneReadFuture<'a, Option<RosterShift>> {
        let pool = self.pool.clone();
        let (company, shift, engagement) =
            (company.to_owned(), shift.to_owned(), engagement.to_owned());
        Box::pin(async move {
            let row = sqlx::query(ROSTER_SHIFT_SQL)
                .bind(shift.as_str())
                .bind(company.as_str())
                .bind(engagement.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(RosterShift {
                    id: text(&row, "id")?,
                    local_date: text(&row, "local_date")?,
                    start_time: text(&row, "start_time")?,
                    end_time: option_text(&row, "end_time")?,
                    end_local: text(&row, "end_local")?,
                    timezone: text(&row, "timezone")?,
                })
            })
            .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_oracles_are_pinned_and_distinct() {
        let results = [
            (ReadKind::PayGroups, r#"[{"id":"g1","name":"Monthly","currency":"THB","frequency":"monthly"}]"#),
            (ReadKind::People, r#"[{"id":"p1","legal_name":"Alice","email":null,"version":1}]"#),
            (ReadKind::ApproverName, r#"{"legal_name":"Alice"}"#),
            (ReadKind::RosterShift, r#"{"id":"s1","local_date":"2026-11-25","start_time":"09:00","end_time":"17:00","end_local":"2026-11-25T17:00","timezone":"Asia/Bangkok"}"#),
        ];
        let expected = [
            "c18b7d1ed74f6ff7dd6fd47d0ba34d11d6bbbb61fa17a29accaadcbec099de50",
            "8eacbb2f511143f0b6c808f8cd5401965277cc25ea587f7fd171f56e688665a7",
            "05faf8c3c0901abc4d238efaffa4d2472c8e79b6f87779db2bbf02bcf147ce16",
            "3c8b33376a170467ed04174fcf966714eaedd1da84414fecdbedf66c64017170",
        ];
        for ((kind, result), digest) in results.into_iter().zip(expected) {
            let sql = rendered_sql(kind);
            assert!(sql.starts_with("SELECT "), "owner SQL must be a read");
            assert!(!sql.contains('?'), "sqlx binds use $n");
            assert_eq!(oracle(kind, result), digest);
        }
    }
}
