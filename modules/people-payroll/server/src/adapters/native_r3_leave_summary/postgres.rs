//! sqlx execution of the migrated `GET .../leave/summary` read. This is the only
//! place the retired TypeScript statements now live: every statement is
//! executed on a Company-scoped `PgUnitOfWork`, with no TypeScript prepare/batch
//! fallback. Statements are inlined from `adapters/leave-self-service.ts`
//! (`readPolicyForMatches`, `readBalanceEffect`, `readLeaveSummaryTypes`,
//! `readLeaveSummaryRequests`, `readLeaveSummaryHolidays`). Only placeholder
//! style (`?` -> `$1..`) and explicit `NUMERIC/INTEGER -> float8/int8`
//! projection casts are applied; columns, predicates, binds and ordering match
//! the retired statements.

use super::application::{
    LeaveSummaryQuery, LeaveSummaryRead, SummaryFuture, assert_unambiguous,
};
use super::domain::{
    Balance, HolidayRow, LeaveRequestRow, LeaveSummaryFacts, LeaveType, PolicyFact, unavailable,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Resolve the one active engagement for the account in this Company. The
/// retired `activeStaff` gates the row on an active engagement and requires
/// exactly one result; a non-single result yields the retired setup fallback.
pub const SQL_ACTIVE_STAFF: &str = r#"SELECT e.id AS engagement_id FROM hr_people p
JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id=$2
AND e.status='active'
AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
LIMIT 2"#;

/// Five binds: `[company, engagement, today, today, today, today]`.
pub const SQL_POLICY_MATCH: &str = r#"SELECT a.id assignment_id,p.id policy_version_id,p.name policy_name,p.version::int8 policy_version,p.timezone,a.holiday_calendar_id
    FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id
    WHERE a.company_id=$1 AND a.engagement_id=$2 AND p.status='active' AND a.effective_from<=$3 AND (a.effective_to IS NULL OR a.effective_to>=$4)
    AND p.effective_from<=$5 AND (p.effective_to IS NULL OR p.effective_to>=$6) ORDER BY a.effective_from DESC,p.version DESC LIMIT 2"#;

/// One bind: `[policy_version_id]`.
pub const SQL_SUMMARY_TYPES: &str = r#"SELECT t.id,t.code,t.name,t.description,t.tracked,t.evidence_after_units::float8,t.evidence_note,
      r.evidence_min_units::float8,r.evidence_purpose,r.evidence_min_count::int8,r.evidence_due_stage,r.evidence_grace_days::int8,
      r.evidence_note rule_evidence_note,r.evidence_privacy,
      r.grant_kind,r.grant_units::float8,r.accrual_units::float8,r.accrual_period,r.carryover_limit::float8,r.employee_eligible,r.contractor_eligible
    FROM leave_policy_rules r JOIN leave_types t ON t.id=r.leave_type_id WHERE r.policy_version_id=$1 AND t.active=true ORDER BY t.name"#;

/// Four binds: `[company, engagement, leave_type_id, entitlement_year]`.
pub const SQL_SUMMARY_BALANCE: &str = r#"SELECT
    coalesce(sum(units),0)::float8 available,
    coalesce(-sum(units) FILTER (WHERE kind IN ('reservation','reservation_release')),0)::float8 reserved,
    coalesce(-sum(units) FILTER (WHERE kind IN ('usage','usage_reversal')),0)::float8 used,
    coalesce(sum(units) FILTER (WHERE kind='accrual'),0)::float8 accrued
    FROM leave_balance_entries WHERE company_id=$1 AND engagement_id=$2 AND leave_type_id=$3 AND entitlement_year=$4"#;

/// Two binds: `[company, engagement]`.
pub const SQL_SUMMARY_REQUESTS: &str = r#"SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units::float8,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
    FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id WHERE r.company_id=$1 AND r.engagement_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT 100"#;

/// One bind: `[holiday_calendar_id]`; `current_date` keeps the retired
/// future-only selection in the database, as in the original statement.
pub const SQL_SUMMARY_HOLIDAYS: &str = r#"SELECT h.holiday_date,h.name FROM leave_holidays h WHERE h.calendar_id=$1 AND h.holiday_date>=to_char(current_date,'YYYY-MM-DD') ORDER BY h.holiday_date LIMIT 8"#;

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn opt_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<i64, _>(column).map_err(|_| unavailable())
}

fn float(row: &sqlx::postgres::PgRow, column: &str) -> Result<f64, BusinessError> {
    row.try_get::<f64, _>(column).map_err(|_| unavailable())
}

fn opt_float(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<f64>, BusinessError> {
    row.try_get::<Option<f64>, _>(column)
        .map_err(|_| unavailable())
}

fn boolean(row: &sqlx::postgres::PgRow, column: &str) -> Result<bool, BusinessError> {
    row.try_get::<bool, _>(column).map_err(|_| unavailable())
}

fn pack_policy(row: &sqlx::postgres::PgRow) -> Result<PolicyFact, BusinessError> {
    Ok(PolicyFact {
        assignment_id: text(row, "assignment_id")?,
        policy_version_id: text(row, "policy_version_id")?,
        policy_name: text(row, "policy_name")?,
        policy_version: integer(row, "policy_version")?,
        timezone: text(row, "timezone")?,
        holiday_calendar_id: opt_text(row, "holiday_calendar_id")?,
    })
}

fn pack_type(row: &sqlx::postgres::PgRow) -> Result<LeaveType, BusinessError> {
    Ok(LeaveType {
        id: text(row, "id")?,
        code: text(row, "code")?,
        name: text(row, "name")?,
        description: text(row, "description")?,
        tracked: boolean(row, "tracked")?,
        evidence_after_units: opt_float(row, "evidence_after_units")?,
        evidence_note: text(row, "evidence_note")?,
        evidence_min_units: opt_float(row, "evidence_min_units")?,
        evidence_purpose: opt_text(row, "evidence_purpose")?,
        evidence_min_count: integer(row, "evidence_min_count")?,
        evidence_due_stage: text(row, "evidence_due_stage")?,
        evidence_grace_days: integer(row, "evidence_grace_days")?,
        rule_evidence_note: text(row, "rule_evidence_note")?,
        evidence_privacy: text(row, "evidence_privacy")?,
        grant_kind: text(row, "grant_kind")?,
        grant_units: float(row, "grant_units")?,
        accrual_units: float(row, "accrual_units")?,
        accrual_period: opt_text(row, "accrual_period")?,
        carryover_limit: opt_float(row, "carryover_limit")?,
        employee_eligible: boolean(row, "employee_eligible")?,
        contractor_eligible: boolean(row, "contractor_eligible")?,
    })
}

fn pack_balance(
    row: &sqlx::postgres::PgRow,
    leave_type_id: &str,
) -> Result<Balance, BusinessError> {
    Ok(Balance {
        leave_type_id: leave_type_id.to_owned(),
        available: float(row, "available")?,
        reserved: float(row, "reserved")?,
        used: float(row, "used")?,
        accrued: float(row, "accrued")?,
    })
}

fn pack_request(row: &sqlx::postgres::PgRow) -> Result<LeaveRequestRow, BusinessError> {
    Ok(LeaveRequestRow {
        id: text(row, "id")?,
        number: text(row, "number")?,
        start_date: text(row, "start_date")?,
        end_date: opt_text(row, "end_date")?,
        start_segment: opt_text(row, "start_segment")?,
        end_segment: opt_text(row, "end_segment")?,
        calculated_units: opt_float(row, "calculated_units")?,
        days: integer(row, "days")?,
        state: text(row, "state")?,
        cancellation_state: opt_text(row, "cancellation_state")?,
        version: integer(row, "version")?,
        created_at: integer(row, "created_at")?,
        updated_at: integer(row, "updated_at")?,
        decision_note: opt_text(row, "decision_note")?,
        leave_type_name: opt_text(row, "leave_type_name")?,
    })
}

fn pack_holiday(row: &sqlx::postgres::PgRow) -> Result<HolidayRow, BusinessError> {
    Ok(HolidayRow {
        holiday_date: text(row, "holiday_date")?,
        name: text(row, "name")?,
    })
}

pub struct PgLeaveSummaryRead {
    pool: DatabasePool,
}

impl PgLeaveSummaryRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl LeaveSummaryRead for PgLeaveSummaryRead {
    fn read<'a>(
        &'a self,
        query: &'a LeaveSummaryQuery,
    ) -> SummaryFuture<'a, LeaveSummaryFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let query = query.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let query = query.clone();
                Box::pin(async move {
                    let staff = sqlx::query(SQL_ACTIVE_STAFF)
                        .bind(query.company_id.as_str())
                        .bind(query.user_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if staff.len() != 1 {
                        return Ok(LeaveSummaryFacts::default());
                    }
                    let engagement: String = staff[0]
                        .try_get("engagement_id")
                        .map_err(|_| unavailable())?;

                    let matches = sqlx::query(SQL_POLICY_MATCH)
                        .bind(query.company_id.as_str())
                        .bind(engagement.as_str())
                        .bind(query.today.as_str())
                        .bind(query.today.as_str())
                        .bind(query.today.as_str())
                        .bind(query.today.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let first = matches.first().map(pack_policy).transpose()?;
                    assert_unambiguous(first.as_ref(), matches.get(1).map(pack_policy).transpose()?.as_ref())?;

                    let (types, balances, requests, holidays) = if let Some(policy) = &first {
                        let types = sqlx::query(SQL_SUMMARY_TYPES)
                            .bind(policy.policy_version_id.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        let mut balances = Vec::with_capacity(types.len());
                        for row in &types {
                            let leave_type_id: String = row
                                .try_get("id")
                                .map_err(|_| unavailable())?;
                            let row = sqlx::query(SQL_SUMMARY_BALANCE)
                                .bind(query.company_id.as_str())
                                .bind(engagement.as_str())
                                .bind(leave_type_id.as_str())
                                .bind(query.year)
                                .fetch_one(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            balances.push(pack_balance(&row, &leave_type_id)?);
                        }
                        let requests = sqlx::query(SQL_SUMMARY_REQUESTS)
                            .bind(query.company_id.as_str())
                            .bind(engagement.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        let holidays = if let Some(calendar_id) = &policy.holiday_calendar_id {
                            sqlx::query(SQL_SUMMARY_HOLIDAYS)
                                .bind(calendar_id.as_str())
                                .fetch_all(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?
                        } else {
                            vec![]
                        };
                        (
                            types.iter().map(pack_type).collect::<Result<Vec<_>, _>>()?,
                            balances,
                            requests.iter().map(pack_request).collect::<Result<Vec<_>, _>>()?,
                            holidays.iter().map(pack_holiday).collect::<Result<Vec<_>, _>>()?,
                        )
                    } else {
                        let requests = sqlx::query(SQL_SUMMARY_REQUESTS)
                            .bind(query.company_id.as_str())
                            .bind(engagement.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        (vec![], vec![], requests.iter().map(pack_request).collect::<Result<Vec<_>, _>>()?, vec![])
                    };

                    Ok(LeaveSummaryFacts {
                        staff_linked: true,
                        policy: first,
                        types,
                        balances,
                        requests,
                        holidays,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_keep_owner_scope_predicates_and_bind_order() {
        for fragment in [
            "p.company_id=$1",
            "p.user_id=$2",
            "e.status='active'",
            "LIMIT 2",
        ] {
            assert!(SQL_ACTIVE_STAFF.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "a.company_id=$1",
            "a.engagement_id=$2",
            "p.status='active'",
            "a.effective_from<=$3",
            "a.effective_to>=$4",
            "p.effective_from<=$5",
            "p.effective_to>=$6",
            "LIMIT 2",
        ] {
            assert!(SQL_POLICY_MATCH.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "FROM leave_policy_rules r JOIN leave_types t ON t.id=r.leave_type_id",
            "r.policy_version_id=$1",
            "t.active=true",
            "ORDER BY t.name",
            "evidence_min_count::int8",
            "grant_units::float8",
        ] {
            assert!(SQL_SUMMARY_TYPES.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "FROM leave_balance_entries",
            "company_id=$1",
            "engagement_id=$2",
            "leave_type_id=$3",
            "entitlement_year=$4",
            "kind IN ('reservation','reservation_release')",
        ] {
            assert!(SQL_SUMMARY_BALANCE.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id",
            "r.company_id=$1",
            "r.engagement_id=$2",
            "ORDER BY r.created_at DESC,r.id DESC LIMIT 100",
        ] {
            assert!(SQL_SUMMARY_REQUESTS.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "FROM leave_holidays h",
            "h.calendar_id=$1",
            "h.holiday_date>=to_char(current_date,'YYYY-MM-DD')",
            "ORDER BY h.holiday_date LIMIT 8",
        ] {
            assert!(SQL_SUMMARY_HOLIDAYS.contains(fragment), "missing {fragment}");
        }
        assert_eq!(SQL_POLICY_MATCH.matches('$').count(), 6);
        assert_eq!(SQL_SUMMARY_BALANCE.matches('$').count(), 4);
        for sql in [
            SQL_ACTIVE_STAFF,
            SQL_POLICY_MATCH,
            SQL_SUMMARY_TYPES,
            SQL_SUMMARY_BALANCE,
            SQL_SUMMARY_REQUESTS,
            SQL_SUMMARY_HOLIDAYS,
        ] {
            assert!(!sql.contains('?'), "placeholder must be $n");
        }
    }
}
