//! sqlx execution of the People-owned next-shift fact behind
//! `GET .../self-service/summary`. This is the only layer that touches the
//! roster tables. `TRUE` is the retired `workforcePurchasedHistorySql` branch
//! for the non-enforced package mode; the enforced-mode commercial predicate
//! is a composed port and is reported as a parity gap, not duplicated here.

use super::application::{NextShiftRead, OwnerFuture};
use super::domain::{NextShift, invalid, unavailable, valid_id};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

pub const SQL_NEXT_SHIFT: &str = "SELECT s.id,s.name,s.local_date,s.start_time,s.end_time,s.starts_at,s.ends_at,w.timezone FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=$1 AND TRUE AND a.engagement_id=$2 AND w.state='published' AND a.state='assigned' AND s.state='active' AND s.ends_at>$3 ORDER BY s.starts_at LIMIT 1";

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    row.try_get::<String, _>(column).map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

fn pack(row: &sqlx::postgres::PgRow) -> Result<NextShift, BusinessError> {
    Ok(NextShift {
        id: text(row, "id")?,
        name: text(row, "name")?,
        local_date: text(row, "local_date")?,
        start_time: text(row, "start_time")?,
        end_time: text(row, "end_time")?,
        starts_at: integer(row, "starts_at")?,
        ends_at: integer(row, "ends_at")?,
        timezone: text(row, "timezone")?,
    })
}

pub struct PgNextShiftRead {
    pool: DatabasePool,
}

impl PgNextShiftRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl NextShiftRead for PgNextShiftRead {
    fn next_shift<'a>(
        &'a self,
        company: &'a str,
        engagement: &'a str,
        now: i64,
    ) -> OwnerFuture<'a, Option<NextShift>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let invalid_request = !valid_id(company) || !valid_id(engagement);
        let company = company.to_owned();
        let engagement = engagement.to_owned();
        Box::pin(async move {
            if invalid_request {
                return Err(invalid());
            }
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let engagement = engagement.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_NEXT_SHIFT)
                        .bind(company.as_str())
                        .bind(engagement.as_str())
                        .bind(now)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.as_ref()
                        .map(pack)
                        .transpose()
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
    fn statement_keeps_the_retired_roster_shape() {
        assert!(SQL_NEXT_SHIFT.contains("w.company_id=$1"));
        assert!(SQL_NEXT_SHIFT.contains("a.engagement_id=$2"));
        assert!(SQL_NEXT_SHIFT.contains("s.ends_at>$3"));
        assert!(SQL_NEXT_SHIFT.contains("w.state='published'"));
        assert!(SQL_NEXT_SHIFT.contains("a.state='assigned'"));
        assert!(SQL_NEXT_SHIFT.contains("s.state='active'"));
        assert!(SQL_NEXT_SHIFT.contains("ORDER BY s.starts_at LIMIT 1"));
        assert!(!SQL_NEXT_SHIFT.contains('?'));
    }
}
