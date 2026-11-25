//! sqlx execution for lane 37. The four statements are byte-identical to the
//! retired TypeScript literals; only the bind placeholder is translated from
//! `?` to `$1`. No advisory lock, no write and no cross-owner projection.

use sqlx::Row;

use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

use crate::adapters::native_lane_37::application::{SeatHistoryFuture, SeatHistoryRead};
use crate::adapters::native_lane_37::domain::{
    AssignmentEvent, MemberEvent, RoleEvent, SeatEvent, SeatHistoryRows, SQL_ASSIGNMENTS_PG,
    SQL_COVERAGE_PG, SQL_MEMBERS_PG, SQL_ROLES_PG,
};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE37_READ_UNAVAILABLE",
        "The native Commercial seat-history read is unavailable.",
    )
}

/// The owner reads `billing_seat_*` tables directly from its injected pool.
pub struct PgSeatHistoryRead {
    pool: DatabasePool,
}

impl PgSeatHistoryRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl SeatHistoryRead for PgSeatHistoryRead {
    fn rows<'a>(&'a self, end: i64) -> SeatHistoryFuture<'a, SeatHistoryRows> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let history_started_at = sqlx::query_scalar::<_, i64>(SQL_COVERAGE_PG)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let assignments = sqlx::query(SQL_ASSIGNMENTS_PG)
                        .bind(end)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(AssignmentEvent {
                                event: SeatEvent {
                                    event_id: row
                                        .try_get::<i64, _>("event_id")
                                        .map_err(|_| unavailable())?,
                                    captured_at: row
                                        .try_get::<i64, _>("captured_at")
                                        .map_err(|_| unavailable())?,
                                    present: row
                                        .try_get::<bool, _>("present")
                                        .map_err(|_| unavailable())?,
                                    state_json: row
                                        .try_get::<Option<String>, _>("state_json")
                                        .map_err(|_| unavailable())?,
                                },
                                assignment_id: row
                                    .try_get::<String, _>("assignment_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let members = sqlx::query(SQL_MEMBERS_PG)
                        .bind(end)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(MemberEvent {
                                event: SeatEvent {
                                    event_id: row
                                        .try_get::<i64, _>("event_id")
                                        .map_err(|_| unavailable())?,
                                    captured_at: row
                                        .try_get::<i64, _>("captured_at")
                                        .map_err(|_| unavailable())?,
                                    present: row
                                        .try_get::<bool, _>("present")
                                        .map_err(|_| unavailable())?,
                                    state_json: row
                                        .try_get::<Option<String>, _>("state_json")
                                        .map_err(|_| unavailable())?,
                                },
                                company_id: row
                                    .try_get::<String, _>("company_id")
                                    .map_err(|_| unavailable())?,
                                user_id: row
                                    .try_get::<String, _>("user_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let roles = sqlx::query(SQL_ROLES_PG)
                        .bind(end)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(RoleEvent {
                                event: SeatEvent {
                                    event_id: row
                                        .try_get::<i64, _>("event_id")
                                        .map_err(|_| unavailable())?,
                                    captured_at: row
                                        .try_get::<i64, _>("captured_at")
                                        .map_err(|_| unavailable())?,
                                    present: row
                                        .try_get::<bool, _>("present")
                                        .map_err(|_| unavailable())?,
                                    state_json: row
                                        .try_get::<Option<String>, _>("state_json")
                                        .map_err(|_| unavailable())?,
                                },
                                company_id: row
                                    .try_get::<String, _>("company_id")
                                    .map_err(|_| unavailable())?,
                                role_id: row
                                    .try_get::<String, _>("role_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(SeatHistoryRows {
                        history_started_at,
                        assignments,
                        members,
                        roles,
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
    fn executable_statements_keep_the_registered_projection_and_order() {
        assert_eq!(SQL_COVERAGE_PG, SQL_COVERAGE_PG.replace('?', "$1"));
        for sql in [SQL_ASSIGNMENTS_PG, SQL_MEMBERS_PG, SQL_ROLES_PG] {
            assert!(sql.starts_with("SELECT "), "{sql}");
            assert_eq!(sql.matches("$1").count(), 1, "{sql}");
            assert!(!sql.contains('?'), "{sql}");
            assert!(sql.contains(" ORDER BY "), "{sql}");
            assert!(!sql.contains("pg_advisory"), "{sql}");
            assert!(!sql.contains("FOR UPDATE"), "{sql}");
        }
    }
}
