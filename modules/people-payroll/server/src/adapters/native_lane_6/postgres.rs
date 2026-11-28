//! Postgres-backed owner reader for the lane-6 calendar-availability leave-day
//! read. This is the only place the retired TypeScript SQL now lives: the
//! statement, bind order and DISTINCT projection are executed with sqlx on a
//! Company-scoped unit of work. No TypeScript fallback exists.

use super::application::{
    CalendarAvailabilityRead, CalendarPeopleFuture, CalendarPersonScopeRead, LeaveDayFuture,
    PersonScopeFuture,
};
use super::domain::{
    CalendarPeopleQuery, CalendarPerson, LeaveDay, LeaveDayQuery, PersonScopeQuery, project,
    validate, validate_calendar_people, validate_person_scope,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::AssertSqlSafe;

/// Verbatim retired literal from
/// `modules/people-payroll/server/src/adapters/calendar-availability-facts.ts`
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

/// Verbatim retired literal from `calendar-index-facts.ts` (`read1`). Kept as a
/// byte-identity oracle for the second owner-local read in this lane.
#[cfg(test)]
pub(crate) const ORIGINAL_PERSON_SCOPE_SQL: &str = r#"SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} LIMIT 1"#;

#[cfg(test)]
const PERSON_SCOPE_IN_EXPRESSION: &str =
    r#"${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}"#;

/// Executable statement: the retired literal with `?` renamed to `$n` and the
/// in-list expression rendered inline. Bind order is person, company, scope.
const PERSON_SCOPE_SQL: &str = "SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=$1 AND p.company_id=$2 AND coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text($3::jsonb)) LIMIT 1";

pub struct PgCalendarPersonScopeRead {
    pool: DatabasePool,
}

impl PgCalendarPersonScopeRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CalendarPersonScopeRead for PgCalendarPersonScopeRead {
    fn present<'a>(
        &'a self,
        company: &'a str,
        person: &'a str,
        scope_ids: &'a str,
    ) -> PersonScopeFuture<'a, bool> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let person = person.to_owned();
        let scope_ids = scope_ids.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let person = person.clone();
                let scope_ids = scope_ids.clone();
                Box::pin(async move {
                    validate_person_scope(
                        &company,
                        &PersonScopeQuery {
                            person_id: person.clone(),
                            scope_ids: scope_ids.clone(),
                        },
                    )?;
                    let present = sqlx::query_scalar::<_, i32>(AssertSqlSafe(PERSON_SCOPE_SQL))
                        .bind(person.as_str())
                        .bind(company.as_str())
                        .bind(scope_ids.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .is_some();
                    Ok(present)
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

/// Verbatim retired literal from `calendar-access-facts.ts` (`people`).
#[cfg(test)]
pub(crate) const ORIGINAL_CALENDAR_PEOPLE_SQL: &str = r#"SELECT * FROM (SELECT DISTINCT ON(p.id) p.id,p.legal_name name,p.user_id "userId",e.department_id "departmentId",e.branch_id "branchId",e.manager_person_id "managerId",e.status
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  WHERE p.company_id=? AND p.merged_into_id IS NULL AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
  AND ((? AND (e.status!='ended' OR ${PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL})) OR (e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) AND (?='' OR p.legal_name ILIKE '%'||?||'%')
  ORDER BY p.id,CASE WHEN e.status='active' THEN 0 ELSE 1 END,e.start_date DESC,e.id) x ORDER BY name,id LIMIT ? OFFSET ?"#;

#[cfg(test)]
const CALENDAR_IN_EXPRESSION: &str =
    r#"${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}"#;
#[cfg(test)]
const CALENDAR_LIFECYCLE_EXPRESSION: &str = r#"${PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL}"#;

/// Executable statement: retired literal with `?` renamed to `$n` and both
/// owner-local fragments inlined. Bind order: company, scope JSON, lifecycle
/// flag, search, search, limit, offset.
const CALENDAR_PEOPLE_SQL: &str = r#"SELECT * FROM (SELECT DISTINCT ON(p.id) p.id,p.legal_name name,p.user_id "userId",e.department_id "departmentId",e.branch_id "branchId",e.manager_person_id "managerId",e.status
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text($2::jsonb))
  AND (($3 AND (e.status!='ended' OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id))) OR (e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) AND ($4='' OR p.legal_name ILIKE '%'||$5||'%')
  ORDER BY p.id,CASE WHEN e.status='active' THEN 0 ELSE 1 END,e.start_date DESC,e.id) x ORDER BY name,id LIMIT $6 OFFSET $7"#;

pub struct PgCalendarPeopleRead {
    pool: DatabasePool,
}

impl PgCalendarPeopleRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl super::application::CalendarPeopleRead for PgCalendarPeopleRead {
    fn people<'a>(
        &'a self,
        company: &'a str,
        query: &'a CalendarPeopleQuery,
    ) -> CalendarPeopleFuture<'a, Vec<CalendarPerson>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let people_ids = query.people_ids.clone();
        let scope_ids = serde_json::to_string(&people_ids).unwrap_or_default();
        let lifecycle_reader = query.lifecycle_reader;
        let search = query.search.clone();
        let take = query.take;
        let cursor = query.cursor;
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let people_ids = people_ids.clone();
                let scope_ids = scope_ids.clone();
                let search = search.clone();
                Box::pin(async move {
                    validate_calendar_people(
                        &company,
                        &CalendarPeopleQuery {
                            people_ids,
                            lifecycle_reader,
                            search: search.clone(),
                            take,
                            cursor,
                        },
                    )?;
                    if scope_ids.is_empty() {
                        return Err(unavailable().into());
                    }
                    let rows = sqlx::query_as::<
                        _,
                        (
                            String,
                            String,
                            Option<String>,
                            Option<String>,
                            Option<String>,
                            Option<String>,
                            String,
                        ),
                    >(AssertSqlSafe(CALENDAR_PEOPLE_SQL))
                    .bind(company.as_str())
                    .bind(scope_ids.as_str())
                    .bind(lifecycle_reader)
                    .bind(search.as_str())
                    .bind(search.as_str())
                    .bind(take)
                    .bind(cursor)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| unavailable())?;
                    Ok(rows
                        .into_iter()
                        .map(
                            |(id, name, user_id, department_id, branch_id, manager_id, status)| {
                                CalendarPerson {
                                    id,
                                    name,
                                    user_id,
                                    department_id,
                                    branch_id,
                                    manager_id,
                                    status,
                                }
                            },
                        )
                        .collect())
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
    fn person_scope_sql_is_byte_identical_to_retired_literal() {
        assert_eq!(
            hash(ORIGINAL_PERSON_SCOPE_SQL),
            "d60b2b715e9da1d1d2f1ff3222c6749c856e875f7f097161443667049c696702"
        );
        let rendered = ORIGINAL_PERSON_SCOPE_SQL.replace(
            PERSON_SCOPE_IN_EXPRESSION,
            "coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text(?::jsonb))",
        );
        assert_eq!(normalize(PERSON_SCOPE_SQL), rendered);
        assert_eq!(PERSON_SCOPE_SQL.matches("$").count(), 3);
    }

    #[test]
    fn calendar_people_sql_is_byte_identical_to_retired_literal() {
        assert_eq!(
            hash(ORIGINAL_CALENDAR_PEOPLE_SQL),
            "94666e17fa2d64329b3e7a2f94ddba1687c10d7f8ce5a1c53c11fc4e8d76abbb"
        );
        let rendered = ORIGINAL_CALENDAR_PEOPLE_SQL
            .replace(
                CALENDAR_IN_EXPRESSION,
                "coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text(?::jsonb))",
            )
            .replace(
                CALENDAR_LIFECYCLE_EXPRESSION,
                "EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)",
            );
        assert_eq!(normalize(CALENDAR_PEOPLE_SQL), rendered);
        assert_eq!(CALENDAR_PEOPLE_SQL.matches('$').count(), 7);
        assert!(CALENDAR_PEOPLE_SQL.contains("LIMIT $6 OFFSET $7"));
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
