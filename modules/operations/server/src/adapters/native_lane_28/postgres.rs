//! sqlx execution of the owner-local staff-level read family. The statements
//! are byte-identical successors of the retired TypeScript SQL; only bind
//! markers and a deterministic ORDER BY differ. No TypeScript fallback exists.

use super::application::{StaffLevelRead, StaffLevelReadFuture};
use super::domain::{
    ACCEPTANCE_COUNT_SQL, ACTIVE_RANKS_SQL, LEVEL_HISTORY_SQL, LevelHistoryRow, StaffLevel,
    StaffLevelFacts, acceptance_count,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

fn unavailable_business() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_LANE28_READ_UNAVAILABLE",
        "The native Operations staff-level read service is unavailable.",
    )
}

fn unavailable() -> Error {
    unavailable_business().into()
}

/// Postgres-backed owner reader. `staff_levels` is an Organization-owned
/// relation read here strictly as a bounded fact; no Organization authority,
/// acceptance decision or writer is inferred from this read.
pub struct PgStaffLevelRead {
    pool: DatabasePool,
}

impl PgStaffLevelRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl StaffLevelRead for PgStaffLevelRead {
    fn facts<'a>(&'a self, company: &'a str) -> StaffLevelReadFuture<'a, StaffLevelFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(ACTIVE_RANKS_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let levels = rows
                        .into_iter()
                        .map(|row| {
                            Ok(StaffLevel {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                rank: row.try_get::<i64, _>("rank").map_err(|_| unavailable())?,
                                level_number: i64::from(
                                    row.try_get::<Option<i32>, _>("level_number")
                                        .map_err(|_| unavailable())?
                                        .unwrap_or(0),
                                ),
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    // The count is owned by the Rust projection, not by a
                    // second SQL statement in the retired TypeScript family.
                    let derived = acceptance_count(&levels);
                    let counted: i64 = sqlx::query_scalar(ACCEPTANCE_COUNT_SQL)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if derived != counted {
                        return Err(unavailable());
                    }
                    Ok(StaffLevelFacts { levels, active_count: derived })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_business(),
            })
        })
    }

    fn history<'a>(&'a self, engagement: &'a str) -> StaffLevelReadFuture<'a, Vec<LevelHistoryRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let engagement = engagement.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let engagement = engagement.clone();
                Box::pin(async move {
                    let rows = sqlx::query(LEVEL_HISTORY_SQL)
                        .bind(engagement.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(LevelHistoryRow {
                                created_at: row.try_get::<i64, _>("created_at").map_err(|_| unavailable())?,
                                reason: row.try_get::<String, _>("reason").map_err(|_| unavailable())?,
                                level_name: row.try_get::<Option<String>, _>("level_name").map_err(|_| unavailable())?,
                                previous_level_name: row
                                    .try_get::<Option<String>, _>("previous_level_name")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable_business(),
            })
        })
    }
}
