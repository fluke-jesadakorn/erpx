//! PostgreSQL implementation of the lane 29 owner reads.
//!
//! The statement heads below are the byte-identical projections, joins and
//! predicates retired from `calendar-availability-facts.ts`; only the `?`
//! bind markers are rewritten to PostgreSQL's `$n`. Bind order is unchanged:
//! Company first, then the read's own identities, then the recipient list.
//! No predicate or aggregation is added in TypeScript.

use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::application::{Lane29Future, Lane29Read};
use super::domain::{
    AvailabilitySource, ConflictProbe, ConflictScope, MovedOccurrence, OccurrenceException, SeriesRow,
};

/// `occurrenceExceptionPointSql('start_local,end_local,cancelled')`.
pub const SQL_OCCURRENCE_EXCEPTION: &str = "SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions \
     WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3";

/// `occurrenceExceptionsSql('occurrence_key,start_local,end_local', ...)`.
pub const SQL_MOVED_OCCURRENCES: &str = "SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions \
     WHERE appointment_id=$1 AND company_id=$2 AND cancelled=false \
     AND start_local IS NOT NULL AND end_local IS NOT NULL";

/// `appointmentAttendeeSeriesSql` head with the `?` markers rewritten to `$1`
/// (Company) and `$2` (window end). The recipient `IN` list is appended in
/// `sql_availability_sources` in the original bind order.
pub const SQL_AVAILABILITY_HEAD: &str = "SELECT a.id,a.starts_at,a.ends_at,a.timezone,a.status,a.all_day,s.frequency,\
     s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,t.user_id,t.response \
     FROM calendar_appointments a JOIN calendar_attendees t \
     ON t.appointment_id=a.id AND t.company_id=a.company_id \
     LEFT JOIN calendar_series s ON s.appointment_id=a.id \
     WHERE a.company_id=$1 AND a.status='scheduled' AND a.starts_at<$2 \
     AND t.response!='declined' AND t.user_id IN (";

/// Render the exact recipient list the TypeScript caller used to render with
/// `facts.value3.map(() => "?").join(",")`.
pub fn sql_availability_sources(user_count: usize) -> String {
    let placeholders = (0..user_count)
        .map(|index| format!("${}", index + 3))
        .collect::<Vec<_>>()
        .join(",");
    format!("{SQL_AVAILABILITY_HEAD}{placeholders})")
}

/// `calendar-appointments-facts.ts` `byAppointmentIds('calendar_series')`.
pub const SQL_SERIES_BATCH: &str = "SELECT * FROM calendar_series WHERE company_id=$1 \
     AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))";

/// `overlapProbe('calendar_appointment_participants', 'p.person_id')`.
pub const SQL_CONFLICT_PARTICIPANT: &str =
    "SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p \
     ON p.appointment_id=a.id WHERE a.company_id=$1 AND a.status='scheduled' AND a.id!=$2 \
     AND a.starts_at<$3 AND a.ends_at>$4 \
     AND p.person_id IN (SELECT jsonb_array_elements_text($5::jsonb)) LIMIT 1";

/// `overlapProbe('calendar_attendees', 'p.user_id', " AND p.response!='declined'")`.
pub const SQL_CONFLICT_ATTENDEE: &str =
    "SELECT 1 FROM calendar_appointments a JOIN calendar_attendees p \
     ON p.appointment_id=a.id WHERE a.company_id=$1 AND a.status='scheduled' AND a.id!=$2 \
     AND a.starts_at<$3 AND a.ends_at>$4 AND p.response!='declined' \
     AND p.user_id IN (SELECT jsonb_array_elements_text($5::jsonb)) LIMIT 1";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE29_READ_UNAVAILABLE",
        "The native Work availability read is unavailable.",
    )
}

fn transaction(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Pg-backed owner reader. It uses the shared transaction unit so the read
/// observes the same serializable snapshot as the other Work owner adapters.
pub struct PgLane29Read {
    pool: DatabasePool,
}

impl PgLane29Read {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl Lane29Read for PgLane29Read {
    fn occurrence_exception<'a>(
        &'a self,
        company: &'a str,
        appointment: &'a str,
        occurrence_key: &'a str,
    ) -> Lane29Future<'a, Option<OccurrenceException>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let appointment = appointment.to_owned();
        let occurrence_key = occurrence_key.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let appointment = appointment.clone();
                let occurrence_key = occurrence_key.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_OCCURRENCE_EXCEPTION)
                        .bind(appointment.as_str())
                        .bind(company.as_str())
                        .bind(occurrence_key.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        Ok(OccurrenceException {
                            start_local: row
                                .try_get::<Option<String>, _>("start_local")
                                .map_err(|_| unavailable())?,
                            end_local: row
                                .try_get::<Option<String>, _>("end_local")
                                .map_err(|_| unavailable())?,
                            cancelled: row
                                .try_get::<bool, _>("cancelled")
                                .map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn moved_occurrences<'a>(
        &'a self,
        company: &'a str,
        appointment: &'a str,
    ) -> Lane29Future<'a, Vec<MovedOccurrence>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let appointment = appointment.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let appointment = appointment.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SQL_MOVED_OCCURRENCES)
                        .bind(appointment.as_str())
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(MovedOccurrence {
                                occurrence_key: row
                                    .try_get::<String, _>("occurrence_key")
                                    .map_err(|_| unavailable())?,
                                start_local: row
                                    .try_get::<String, _>("start_local")
                                    .map_err(|_| unavailable())?,
                                end_local: row
                                    .try_get::<String, _>("end_local")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn availability_sources<'a>(
        &'a self,
        company: &'a str,
        before: i64,
        user_ids: &'a [String],
    ) -> Lane29Future<'a, Vec<AvailabilitySource>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let users = user_ids.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let users = users.clone();
                Box::pin(async move {
                    // QueryBuilder keeps the byte-identical head and appends
                    // only counted `$n` placeholders; no caller text is SQL.
                    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(SQL_AVAILABILITY_HEAD);
                    builder.push_bind(company.as_str());
                    builder.push_bind(before);
                    for user in &users {
                        builder.push_bind(user.as_str());
                    }
                    builder.push(")");
                    let rows = builder
                        .build()
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(AvailabilitySource {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                starts_at: row
                                    .try_get::<i64, _>("starts_at")
                                    .map_err(|_| unavailable())?,
                                ends_at: row
                                    .try_get::<i64, _>("ends_at")
                                    .map_err(|_| unavailable())?,
                                timezone: row
                                    .try_get::<String, _>("timezone")
                                    .map_err(|_| unavailable())?,
                                status: row
                                    .try_get::<String, _>("status")
                                    .map_err(|_| unavailable())?,
                                all_day: row
                                    .try_get::<bool, _>("all_day")
                                    .map_err(|_| unavailable())?,
                                frequency: row
                                    .try_get::<Option<String>, _>("frequency")
                                    .map_err(|_| unavailable())?,
                                interval_count: row
                                    .try_get::<Option<i32>, _>("interval_count")
                                    .map_err(|_| unavailable())?,
                                weekdays: row
                                    .try_get::<Option<String>, _>("weekdays")
                                    .map_err(|_| unavailable())?,
                                end_kind: row
                                    .try_get::<Option<String>, _>("end_kind")
                                    .map_err(|_| unavailable())?,
                                occurrence_count: row
                                    .try_get::<Option<i32>, _>("occurrence_count")
                                    .map_err(|_| unavailable())?,
                                until_date: row
                                    .try_get::<Option<String>, _>("until_date")
                                    .map_err(|_| unavailable())?,
                                user_id: row
                                    .try_get::<String, _>("user_id")
                                    .map_err(|_| unavailable())?,
                                response: row
                                    .try_get::<String, _>("response")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn series_batch<'a>(
        &'a self,
        company: &'a str,
        appointment_ids: &'a [String],
    ) -> Lane29Future<'a, Vec<SeriesRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let ids = serde_json::to_string(appointment_ids).unwrap_or_else(|_| "[]".to_owned());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SQL_SERIES_BATCH)
                        .bind(company.as_str())
                        .bind(ids.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(SeriesRow {
                                appointment_id: row
                                    .try_get::<String, _>("appointment_id")
                                    .map_err(|_| unavailable())?,
                                company_id: row
                                    .try_get::<String, _>("company_id")
                                    .map_err(|_| unavailable())?,
                                frequency: row
                                    .try_get::<String, _>("frequency")
                                    .map_err(|_| unavailable())?,
                                interval_count: row
                                    .try_get::<i32, _>("interval_count")
                                    .map_err(|_| unavailable())?,
                                weekdays: row
                                    .try_get::<String, _>("weekdays")
                                    .map_err(|_| unavailable())?,
                                end_kind: row
                                    .try_get::<String, _>("end_kind")
                                    .map_err(|_| unavailable())?,
                                occurrence_count: row
                                    .try_get::<Option<i32>, _>("occurrence_count")
                                    .map_err(|_| unavailable())?,
                                until_date: row
                                    .try_get::<Option<String>, _>("until_date")
                                    .map_err(|_| unavailable())?,
                                version: row
                                    .try_get::<i32, _>("version")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect()
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn conflict_probe<'a>(
        &'a self,
        company: &'a str,
        scope: ConflictScope,
        exclude: &'a str,
        before: i64,
        after: i64,
        ids: &'a [String],
    ) -> Lane29Future<'a, ConflictProbe> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let exclude = exclude.to_owned();
        let ids = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_owned());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let exclude = exclude.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let sql = match scope {
                        ConflictScope::Participant => SQL_CONFLICT_PARTICIPANT,
                        ConflictScope::Attendee => SQL_CONFLICT_ATTENDEE,
                    };
                    let row = sqlx::query(sql)
                        .bind(company.as_str())
                        .bind(exclude.as_str())
                        .bind(before)
                        .bind(after)
                        .bind(ids.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ConflictProbe {
                        conflict: row.is_some(),
                    })
                })
            })
            .await
            .map_err(transaction)
        })
    }
}

#[cfg(test)]
fn hash(sql: &str) -> String {
    hex(Sha256::digest(sql.as_bytes()))
}

#[cfg(test)]
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occurrence_sql_is_byte_identical_to_retired_ts_literal() {
        assert_eq!(
            SQL_OCCURRENCE_EXCEPTION,
            "SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions \
             WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3"
        );
        assert_eq!(
            hash(SQL_OCCURRENCE_EXCEPTION),
            "29cc9700b4a8a2f0e6a2015581c51141b0d4787b9a550386e83f609ad21c413d"
        );
    }

    #[test]
    fn moved_sql_is_byte_identical_to_retired_ts_literal() {
        assert_eq!(
            hash(SQL_MOVED_OCCURRENCES),
            "a57f2c260b73dd27eb8ab93a5bb4d5ecc13e6b1aee1a0327354dc2137addddd8"
        );
    }

    #[test]
    fn conflict_probe_sql_is_byte_identical() {
        assert_eq!(
            hash(SQL_CONFLICT_PARTICIPANT),
            "613db35276b9d29e8ab019c00ad5b4b72381621d7543fe8266eebebfa6e487c1"
        );
        assert_eq!(
            hash(SQL_CONFLICT_ATTENDEE),
            "fa8e749772b2dc348cf49b54bbb5d4f06a4802ef132e8e49fe7aafc41ae52a31"
        );
    }

    #[test]
    fn series_sql_is_byte_identical() {
        assert_eq!(
            hash(SQL_SERIES_BATCH),
            "46ae301749a20a402fc7dfc7560f0007a805bace47c5df793e1280aafec2d336"
        );
    }

    #[test]
    fn availability_bind_order_is_stable() {
        assert_eq!(
            hash(SQL_AVAILABILITY_HEAD),
            "81c3a94f1900fcb5797c80272608478b7ce6f65588372ea85bcb2699dcc8b763"
        );
        assert_eq!(
            sql_availability_sources(2),
            format!("{SQL_AVAILABILITY_HEAD}$3,$4)")
        );
        assert_eq!(
            hash(&sql_availability_sources(2)),
            "c38d5eb96b3b1031ae05d5699693a688bc9bc236fa380e0fc356b9fb1eb5e8c0"
        );
        assert_eq!(sql_availability_sources(1).matches('$').count(), 3);
    }
}
