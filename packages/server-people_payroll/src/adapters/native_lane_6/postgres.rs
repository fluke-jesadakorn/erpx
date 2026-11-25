//! Postgres-backed owner reader for the lane-6 calendar-availability leave-day
//! read. This is the only place the retired TypeScript SQL now lives: the
//! statement, bind order and DISTINCT projection are executed with sqlx on a
//! Company-scoped unit of work. No TypeScript fallback exists.

use super::application::{CalendarAvailabilityRead, LeaveDayFuture};
use super::domain::{LeaveDay, LeaveDayQuery, project, validate};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::AssertSqlSafe;

/// Verbatim retired literal from
/// `packages/server-people_payroll/src/adapters/calendar-availability-facts.ts`
/// (the `db.prepare` template at the `read7` reader). Kept only as a byte-identity
/// oracle; the executable statement below is the sqlx `? -> $N` translation of
/// this exact text, with the IN list generated from the bounded user count.
#[cfg(test)]
pub(crate) const ORIGINAL_TS_SQL: &str = r#"SELECT DISTINCT p.user_id,d.leave_date::text leave_date
    FROM leave_requests l JOIN hr_people p ON p.id=l.person_id AND p.company_id=l.company_id
    CROSS JOIN LATERAL (
      SELECT s.leave_date::date leave_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
      UNION ALL
      SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date
      WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id)
    ) d
    WHERE l.company_id=? AND l.state='approved' AND p.user_id IN (${facts.value2.map(() => "?").join(",")})
    AND d.leave_date BETWEEN ?::date AND ?::date"#;

#[cfg(test)]
const IN_EXPRESSION: &str = r#"${facts.value2.map(() => "?").join(",")}"#;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE",
        "The native People Payroll availability service is unavailable.",
    )
}

/// Render the byte-identical statement for a bounded number of users. Bind
/// order matches the retired reader exactly: company, users..., from, to.
pub(crate) fn render_sql(user_count: usize) -> String {
    let users = (2..=user_count + 1)
        .map(|index| format!("${index}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "SELECT DISTINCT p.user_id,d.leave_date::text leave_date\n    \
         FROM leave_requests l JOIN hr_people p ON p.id=l.person_id AND p.company_id=l.company_id\n    \
         CROSS JOIN LATERAL (\n      \
           SELECT s.leave_date::date leave_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id\n      \
           UNION ALL\n      \
           SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date\n      \
           WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id)\n    \
         ) d\n    \
         WHERE l.company_id=$1 AND l.state='approved' AND p.user_id IN ({users})\n    \
         AND d.leave_date BETWEEN ${from}::date AND ${to}::date",
        from = user_count + 2,
        to = user_count + 3,
    )
}

pub struct PgCalendarAvailabilityRead {
    pool: DatabasePool,
}

impl PgCalendarAvailabilityRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CalendarAvailabilityRead for PgCalendarAvailabilityRead {
    fn leave_days<'a>(
        &'a self,
        company: &'a str,
        query: &'a LeaveDayQuery,
    ) -> LeaveDayFuture<'a, Vec<LeaveDay>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let users = query.user_ids.clone();
        let from = query.from.clone();
        let to = query.to.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let users = users.clone();
                let from = from.clone();
                let to = to.clone();
                Box::pin(async move {
                    validate(
                        &company,
                        &LeaveDayQuery {
                            user_ids: users.clone(),
                            from: from.clone(),
                            to: to.clone(),
                        },
                    )?;
                    let sql = render_sql(users.len());
                    let mut statement = sqlx::query_as::<_, (String, String)>(AssertSqlSafe(sql))
                        .bind(company.as_str());
                    for user in &users {
                        statement = statement.bind(user.as_str());
                    }
                    statement = statement.bind(from.as_str()).bind(to.as_str());
                    let rows = statement
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(project(
                        rows.into_iter()
                            .map(|(user_id, leave_date)| LeaveDay {
                                user_id,
                                leave_date,
                            })
                            .collect(),
                    ))
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
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        Sha256::digest(value.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn normalize(value: &str) -> String {
        let mut out = String::new();
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '$' && chars.peek().is_some_and(|next| next.is_ascii_digit()) {
                out.push('?');
                while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
                    chars.next();
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn sql_is_byte_identical_to_retired_literal() {
        assert_eq!(
            hash(ORIGINAL_TS_SQL),
            "7abbf7e047b1e1bc94f62a116ad13b3922208aec4e6bd7324fbb04dbd83797b7"
        );
        let expected = ORIGINAL_TS_SQL.replace(IN_EXPRESSION, "?");
        assert_eq!(normalize(&render_sql(1)), expected);
        let expected2 = ORIGINAL_TS_SQL.replace(IN_EXPRESSION, "?,?");
        assert_eq!(normalize(&render_sql(2)), expected2);
        assert_eq!(
            hash(&expected),
            "1f1fedc2b02495da161f210bca3473f4cb8dec0630f12cfc2f32d49db5e69070"
        );
    }

    #[test]
    fn bind_order_and_arity_follow_company_users_dates() {
        let sql = render_sql(3);
        assert!(sql.contains("l.company_id=$1"));
        assert!(sql.contains("p.user_id IN ($2,$3,$4)"));
        assert!(sql.contains("d.leave_date BETWEEN $5::date AND $6::date"));
        assert_eq!(sql.matches('$').count(), 6);
        assert_eq!(sql.matches("SELECT").count(), 4);
    }
}
