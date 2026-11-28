//! sqlx execution for lane 37. The four statements are byte-identical to the
//! retired TypeScript literals; only the bind placeholder is translated from
//! `?` to `$1`. No advisory lock, no write and no cross-owner projection.

use sqlx::Row;

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use crate::adapters::native_lane_37::application::{SeatHistoryFuture, SeatHistoryRead};
use crate::adapters::native_lane_37::domain::{
    AssignmentEvent, CurrentSnapshotFact, MemberEvent, MeterRowFact, ProjectionFacts, RoleEvent,
    SeatEvent, SeatHistoryRows, SnapshotHeaderFact, SQL_ASSIGNMENTS_PG, SQL_COVERAGE_PG,
    SQL_MEMBERS_PG, SQL_PROJECTION_CURRENT_PG, SQL_PROJECTION_METERS_PG,
    SQL_PROJECTION_SNAPSHOTS_PG, SQL_ROLES_PG,
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

/// Wave 2 owner reader: the control-DB usage projection facts. The three
/// statements are byte-identical to the retired TypeScript literals.
pub struct PgProjectionFactsRead {
    pool: DatabasePool,
}

impl PgProjectionFactsRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn text(value: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    value
        .try_get::<String, _>(column)
        .map_err(|_| unavailable())
}
fn optional_text(
    value: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>, BusinessError> {
    value
        .try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}
fn integer(value: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    value.try_get::<i64, _>(column).map_err(|_| unavailable())
}
fn optional_integer(
    value: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<i64>, BusinessError> {
    value
        .try_get::<Option<i64>, _>(column)
        .map_err(|_| unavailable())
}

impl crate::adapters::native_lane_37::application::ProjectionFactsRead for PgProjectionFactsRead {
    fn facts<'a>(
        &'a self,
        month: &'a str,
    ) -> crate::adapters::native_lane_37::application::ProjectionFactsFuture<'a, ProjectionFacts>
    {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let month = month.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let month = month.clone();
                Box::pin(async move {
                    let snapshots = sqlx::query(SQL_PROJECTION_SNAPSHOTS_PG)
                        .bind(&month)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(SnapshotHeaderFact {
                                holding_id: text(&row, "holding_id")?,
                                ai_coverage: text(&row, "ai_coverage")?,
                                storage_coverage: text(&row, "storage_coverage")?,
                                seat_coverage: text(&row, "seat_coverage")?,
                                seat_source_count: integer(&row, "seat_source_count")?,
                                unreconciled_originals: integer(&row, "unreconciled_originals")?,
                                active_seat_count: optional_integer(&row, "active_seat_count")?,
                                active_seat_observed_at: optional_integer(
                                    &row,
                                    "active_seat_observed_at",
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let source_rows = sqlx::query(SQL_PROJECTION_METERS_PG)
                        .bind(&month)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(MeterRowFact {
                                holding_id: text(&row, "holding_id")?,
                                month: text(&row, "month")?,
                                source_key: text(&row, "source_key")?,
                                company_id: text(&row, "company_id")?,
                                company_name: text(&row, "company_name")?,
                                branch_id: optional_text(&row, "branch_id")?,
                                branch_name: optional_text(&row, "branch_name")?,
                                meter_kind: text(&row, "meter_kind")?,
                                funding_source: text(&row, "funding_source")?,
                                pricing_status: text(&row, "pricing_status")?,
                                quantity: integer(&row, "quantity")?,
                                input_tokens: integer(&row, "input_tokens")?,
                                output_tokens: integer(&row, "output_tokens")?,
                                average_daily_bytes: integer(&row, "average_daily_bytes")?,
                                active_at_period_end: row
                                    .try_get::<bool, _>("active_at_period_end")
                                    .map_err(|_| unavailable())?,
                                request_status: optional_text(&row, "request_status")?,
                                gateway_status: optional_text(&row, "gateway_status")?,
                                provider: optional_text(&row, "provider")?,
                                model: optional_text(&row, "model")?,
                                units_json: optional_text(&row, "units_json")?,
                                cost_status: text(&row, "cost_status")?,
                                verified_cost_satang: optional_integer(
                                    &row,
                                    "verified_cost_satang",
                                )?,
                                cost_evidence_key: optional_text(&row, "cost_evidence_key")?,
                                cost_reconciled_at: optional_integer(&row, "cost_reconciled_at")?,
                                active_from: optional_integer(&row, "active_from")?,
                                active_until: optional_integer(&row, "active_until")?,
                                byte_milliseconds: optional_text(&row, "byte_milliseconds")?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let current_snapshots = sqlx::query(SQL_PROJECTION_CURRENT_PG)
                        .bind(&month)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(CurrentSnapshotFact {
                                holding_id: text(&row, "holding_id")?,
                                active_seat_count: optional_integer(&row, "active_seat_count")?,
                                active_seat_observed_at: optional_integer(
                                    &row,
                                    "active_seat_observed_at",
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ProjectionFacts {
                        snapshots,
                        source_rows,
                        current_snapshots,
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
    fn projection_statements_bind_one_month_and_stay_read_only() {
        for sql in [
            SQL_PROJECTION_SNAPSHOTS_PG,
            SQL_PROJECTION_METERS_PG,
            SQL_PROJECTION_CURRENT_PG,
        ] {
            assert!(sql.starts_with("SELECT "), "{sql}");
            assert_eq!(sql.matches("$1").count(), 1, "{sql}");
            assert!(!sql.contains('?'), "{sql}");
            assert!(!sql.contains("INSERT"), "{sql}");
        }
    }

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
