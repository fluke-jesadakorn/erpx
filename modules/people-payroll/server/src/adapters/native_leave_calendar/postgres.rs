//! sqlx execution of the migrated `GET .../leave/calendar` read. This is the
//! only place the retired TypeScript statements now live: both are executed on
//! a Company-scoped `PgUnitOfWork`, with no TypeScript prepare/batch fallback.
//!
//! The two read statements are inlined from `adapters/leave-self-service.ts`
//! (`readLeaveCalendarMonthRequestsholidays` and
//! `readLeaveCalendarMonthRequestsholidays2`). Only placeholder style changes
//! (`?` -> `$1..$8`) and one `NUMERIC -> float8` projection cast are applied;
//! columns, predicates, binds and ordering match the retired statements.
//! Active-staff engagement resolution is the owner's Company-scoped projection
//! query; composition's authorized bridge supplies the Company and account
//! identities as bounded bind parameters.

use super::application::{CalendarFuture, LeaveCalendarQuery, LeaveCalendarRead};
use super::domain::{
    HolidayRow, LeaveCalendarFacts, LeaveCalendarMonth, LeaveRequestRow, unavailable,
};
use super::http::LeaveCalendarAuthorityContext;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

/// Performs current IAM and purchased-capability checks using the same
/// connection and transaction as the calendar queries. Implementations must
/// validate the Holding, selected assignment, session revision and version.
pub trait PgLeaveCalendarAuthority: Send + Sync {
    fn authorize<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        context: &'a LeaveCalendarAuthorityContext,
    ) -> CalendarFuture<'a, ()>;
}

struct UnconfiguredPgAuthority;
impl PgLeaveCalendarAuthority for UnconfiguredPgAuthority {
    fn authorize<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a LeaveCalendarAuthorityContext,
    ) -> CalendarFuture<'a, ()> {
        Box::pin(async { Err(unavailable()) })
    }
}

/// Resolve the one active engagement for the account in this Company. The
/// retired `activeStaff` also gates the row on an active IAM assignment and
/// requires exactly one result; here a non-single result yields the retired
/// `{ requests: [], holidays: [] }` staff-link fallback.
pub const SQL_ACTIVE_STAFF: &str = r#"SELECT e.id AS engagement_id FROM hr_people p
JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id=$2
AND e.status='active'
AND EXISTS (SELECT 1 FROM assignments a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
  AND a.revoked_at IS NULL
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
  AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
LIMIT 2"#;

/// Five binds in retired order: `[company, engagement, user, end, start]`.
pub const SQL_CALENDAR_REQUESTS: &str = r#"SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units::float8,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
        FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id
        WHERE r.company_id=$1 AND r.engagement_id=$2 AND r.created_by=$3
          AND r.state IN ('draft','pending_approval','approved')
          AND r.start_date::date<=$4::date
          AND coalesce(r.end_date::date,r.start_date::date+greatest(r.days,1)::int-1)>=$5::date
        ORDER BY r.start_date,r.id"#;

/// Eight binds in retired order:
/// `[company, engagement, end, start, end, start, start, end]`.
pub const SQL_CALENDAR_HOLIDAYS: &str = r#"SELECT DISTINCT h.holiday_date,h.name
        FROM leave_policy_assignments a
        JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id
        JOIN leave_holidays h ON h.calendar_id=a.holiday_calendar_id
        WHERE a.company_id=$1 AND a.engagement_id=$2 AND a.holiday_calendar_id IS NOT NULL
          AND a.effective_from<=$3 AND (a.effective_to IS NULL OR a.effective_to>=$4)
          AND p.effective_from<=$5 AND (p.effective_to IS NULL OR p.effective_to>=$6)
          AND h.holiday_date BETWEEN $7 AND $8
        ORDER BY h.holiday_date,h.name"#;

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

fn pack_request(row: &sqlx::postgres::PgRow) -> Result<LeaveRequestRow, BusinessError> {
    Ok(LeaveRequestRow {
        id: text(row, "id")?,
        number: text(row, "number")?,
        start_date: text(row, "start_date")?,
        end_date: opt_text(row, "end_date")?,
        start_segment: opt_text(row, "start_segment")?,
        end_segment: opt_text(row, "end_segment")?,
        calculated_units: row
            .try_get::<Option<f64>, _>("calculated_units")
            .map_err(|_| unavailable())?,
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

pub struct PgLeaveCalendarRead {
    pool: DatabasePool,
    authority: Arc<dyn PgLeaveCalendarAuthority>,
}

impl PgLeaveCalendarRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            authority: Arc::new(UnconfiguredPgAuthority),
        }
    }
}

impl PgLeaveCalendarRead {
    pub fn with_authority(
        pool: DatabasePool,
        authority: Arc<dyn PgLeaveCalendarAuthority>,
    ) -> Self {
        Self { pool, authority }
    }
}

/// Read the calendar on the caller's already-authorized Holding transaction.
/// The authority implementation must validate the actual identity-owned work
/// context, the supplied scope and purchased access on this same connection.
/// This owner cannot depend on identity-access because identity depends on it.
/// No connection, transaction, session or authority is created by this API.
pub async fn read_authorized_on_connection(
    connection: &mut PgConnection,
    context: &LeaveCalendarAuthorityContext,
    month: &str,
    authority: &dyn PgLeaveCalendarAuthority,
) -> Result<LeaveCalendarMonth, BusinessError> {
    let query = LeaveCalendarQuery::try_new(&context.company_id, &context.user_id, month)?;
    if !super::domain::valid_id(&context.assignment_id)
        || context.host.is_empty()
        || context.holding_slug.is_empty()
        || !(1..=9_007_199_254_740_991).contains(&context.revision)
        || !(1..=9_007_199_254_740_991).contains(&context.assignment_version)
    {
        return Err(super::domain::invalid());
    }
    authority.authorize(connection, context).await?;
    let facts = read_facts_on_connection(connection, &query).await?;
    Ok(LeaveCalendarMonth {
        month: query.month,
        requests: facts.requests,
        holidays: facts.holidays,
    })
}

async fn read_facts_on_connection(
    connection: &mut PgConnection,
    query: &LeaveCalendarQuery,
) -> Result<LeaveCalendarFacts, BusinessError> {
    let staff = sqlx::query(SQL_ACTIVE_STAFF)
        .bind(query.company_id.as_str())
        .bind(query.user_id.as_str())
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    if staff.len() != 1 {
        return Ok(LeaveCalendarFacts::default());
    }
    let engagement: String = staff[0]
        .try_get("engagement_id")
        .map_err(|_| unavailable())?;

    let requests = sqlx::query(SQL_CALENDAR_REQUESTS)
        .bind(query.company_id.as_str())
        .bind(engagement.as_str())
        .bind(query.user_id.as_str())
        .bind(query.end.as_str())
        .bind(query.start.as_str())
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
    let holidays = sqlx::query(SQL_CALENDAR_HOLIDAYS)
        .bind(query.company_id.as_str())
        .bind(engagement.as_str())
        .bind(query.end.as_str())
        .bind(query.start.as_str())
        .bind(query.end.as_str())
        .bind(query.start.as_str())
        .bind(query.start.as_str())
        .bind(query.end.as_str())
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?;

    Ok(LeaveCalendarFacts {
        requests: requests
            .iter()
            .map(pack_request)
            .collect::<Result<Vec<_>, BusinessError>>()?,
        holidays: holidays
            .iter()
            .map(pack_holiday)
            .collect::<Result<Vec<_>, BusinessError>>()?,
    })
}

impl LeaveCalendarRead for PgLeaveCalendarRead {
    fn read<'a>(&'a self, query: &'a LeaveCalendarQuery) -> CalendarFuture<'a, LeaveCalendarFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let query = query.clone();
        let authority = self.authority.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let query = query.clone();
                let authority = authority.clone();
                Box::pin(async move {
                    let context = query.authority.as_ref().ok_or_else(unavailable)?;
                    if context.company_id != query.company_id || context.user_id != query.user_id {
                        return Err(unavailable().into());
                    }
                    authority.authorize(&mut *connection, context).await?;
                    read_facts_on_connection(connection, &query)
                        .await
                        .map_err(Into::into)
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
            "a.revoked_at IS NULL",
            "iam_assignment_active(a.id",
            "iam_principal_active(a.user_id,a.company_id",
            "a.valid_from<=",
            "a.valid_until>",
            "LIMIT 2",
        ] {
            assert!(SQL_ACTIVE_STAFF.contains(fragment), "missing {fragment}");
        }
        for fragment in [
            "FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id",
            "r.company_id=$1",
            "r.engagement_id=$2",
            "r.created_by=$3",
            "r.state IN ('draft','pending_approval','approved')",
            "r.start_date::date<=$4::date",
            ">=$5::date",
            "ORDER BY r.start_date,r.id",
        ] {
            assert!(
                SQL_CALENDAR_REQUESTS.contains(fragment),
                "missing {fragment}"
            );
        }
        for fragment in [
            "FROM leave_policy_assignments a",
            "JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id",
            "JOIN leave_holidays h ON h.calendar_id=a.holiday_calendar_id",
            "a.company_id=$1",
            "a.engagement_id=$2",
            "a.holiday_calendar_id IS NOT NULL",
            "a.effective_from<=$3",
            "a.effective_to>=$4",
            "p.effective_from<=$5",
            "p.effective_to>=$6",
            "h.holiday_date BETWEEN $7 AND $8",
            "ORDER BY h.holiday_date,h.name",
        ] {
            assert!(
                SQL_CALENDAR_HOLIDAYS.contains(fragment),
                "missing {fragment}"
            );
        }
        assert_eq!(SQL_CALENDAR_REQUESTS.matches('$').count(), 5);
        assert_eq!(SQL_CALENDAR_HOLIDAYS.matches('$').count(), 8);
        assert!(!SQL_CALENDAR_REQUESTS.contains('?'));
        assert!(!SQL_CALENDAR_HOLIDAYS.contains('?'));
    }
}
