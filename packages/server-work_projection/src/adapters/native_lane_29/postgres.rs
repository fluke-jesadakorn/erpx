//! PostgreSQL implementation of the lane 29 owner reads.
//!
//! The statement heads below are the byte-identical projections, joins and
//! predicates retired from `calendar-availability-facts.ts`; only the `?`
//! bind markers are rewritten to PostgreSQL's `$n`. Bind order is unchanged:
//! Company first, then the read's own identities, then the recipient list.
//! No predicate or aggregation is added in TypeScript.

use erpx_server_runtime::public::http_support::serde_json::{self, Value};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::application::{Lane29Future, Lane29Read};
use super::domain::{
    AnnouncementCandidate, AppointmentCandidate, AttendeeRow, AvailabilitySource, ConflictProbe,
    ConflictScope, MovedOccurrence, OccurrenceException, SeriesRow,
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

/// Byte-identical rendered candidate-list SQL; 20 binds in original order.
pub const SQL_APPOINTMENT_CANDIDATES: &str = r#"WITH candidates AS (
 SELECT x.*,p.legal_name organizer_name,
 (($1 AND coalesce(x.department_id,x.branch_id,x.company_id) IN (SELECT jsonb_array_elements_text($2::jsonb))) OR x.organizer_person_id=$3 OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$4)) full_detail
 FROM calendar_appointments x LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ON p.id=x.organizer_person_id AND p.company_id=x.company_id
 WHERE x.company_id=$5 AND x.starts_at<$6 AND (x.ends_at>$7 OR EXISTS(SELECT 1 FROM calendar_series s WHERE s.appointment_id=x.id))
 AND (coalesce(x.department_id,x.branch_id,x.company_id) IN (SELECT jsonb_array_elements_text($8::jsonb)) OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$9) OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN (SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements) e ON e.person_id=ap.person_id AND e.company_id=ap.company_id WHERE ap.appointment_id=x.id AND e.status='active' AND coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text($10::jsonb)))))
 SELECT x.* FROM candidates x WHERE (full_detail OR ($11 AND x.status='scheduled'))
 AND (($12::text IS NOT NULL AND EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=$13))
   OR ($14::text IS NULL AND ($15='team' OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=$16) OR x.organizer_person_id=$17)))
 AND ($18='' OR (CASE WHEN full_detail THEN x.title ELSE 'Busy' END) ILIKE '%'||$19||'%') ORDER BY x.id LIMIT $20"#;

/// Byte-identical rendered candidate-list SQL; 2 binds in original order.
pub const SQL_ATTENDEE_ROWS: &str = r#"SELECT ap.appointment_id,ap.user_id,ap.person_id,p.legal_name name,EXISTS(SELECT 1 FROM (SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND coalesce(e.department_id,e.branch_id,e.company_id) IN (SELECT jsonb_array_elements_text($1::jsonb)))) in_scope,ap.attendance,ap.response
          FROM calendar_attendees ap LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ON p.id=ap.person_id AND p.company_id=ap.company_id WHERE ap.appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb)) ORDER BY p.legal_name,ap.user_id"#;

/// Byte-identical rendered candidate-list SQL; 10 binds in original order.
pub const SQL_ANNOUNCEMENT_CANDIDATES: &str = r#"SELECT n.*,coalesce(p.legal_name,'Publisher') owner_name FROM calendar_announcements n LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ON p.user_id=n.created_by AND p.company_id=n.company_id AND p.merged_into_id IS NULL
 WHERE n.company_id=$1 AND coalesce(n.department_id,n.branch_id,n.company_id) IN (SELECT jsonb_array_elements_text($2::jsonb)) AND (n.status='published' OR ($3 AND coalesce(n.department_id,n.branch_id,n.company_id) IN (SELECT jsonb_array_elements_text($4::jsonb))))
 AND n.start_date<$5 AND n.end_date>=$6 AND ($7='' OR n.title ILIKE '%'||$8||'%' OR n.content ILIKE '%'||$9||'%') ORDER BY n.id LIMIT $10"#;

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

    fn appointment_candidates<'a>(
        &'a self,
        values: &'a [Value],
    ) -> Lane29Future<'a, Vec<AppointmentCandidate>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let values = values.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let values = values.clone();
                Box::pin(async move {
                    let scope = text_value(&values[1])?;
                    let organizer = nullable_text(&values[2])?;
                    let person = nullable_text(&values[8])?;
                    let rows = sqlx::query(SQL_APPOINTMENT_CANDIDATES)
                        .bind(flag_value(&values[0])?)
                        .bind(scope.as_str())
                        .bind(organizer.as_deref())
                        .bind(text_value(&values[3])?)
                        .bind(text_value(&values[4])?)
                        .bind(number_value(&values[5])?)
                        .bind(number_value(&values[6])?)
                        .bind(scope.as_str())
                        .bind(text_value(&values[3])?)
                        .bind(scope.as_str())
                        .bind(flag_value(&values[7])?)
                        .bind(person.as_deref())
                        .bind(person.as_deref())
                        .bind(person.as_deref())
                        .bind(text_value(&values[9])?)
                        .bind(text_value(&values[3])?)
                        .bind(text_value(&values[10])?)
                        .bind(text_value(&values[11])?)
                        .bind(text_value(&values[11])?)
                        .bind(number_value(&values[12])?)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(rows.iter().map(appointment_row).collect::<Result<Vec<_>, BusinessError>>()?)
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn attendee_rows<'a>(&'a self, values: &'a [Value]) -> Lane29Future<'a, Vec<AttendeeRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let values = values.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let values = values.clone();
                Box::pin(async move {
                    let scope = text_value(&values[0])?;
                    let ids = text_value(&values[1])?;
                    let rows = sqlx::query(SQL_ATTENDEE_ROWS)
                        .bind(scope.as_str())
                        .bind(ids.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(rows.iter().map(attendee_row).collect::<Result<Vec<_>, BusinessError>>()?)
                })
            })
            .await
            .map_err(transaction)
        })
    }

    fn announcement_candidates<'a>(
        &'a self,
        values: &'a [Value],
    ) -> Lane29Future<'a, Vec<AnnouncementCandidate>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let values = values.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let values = values.clone();
                Box::pin(async move {
                    let scope = text_value(&values[1])?;
                    let scope2 = text_value(&values[3])?;
                    let query = text_value(&values[7])?;
                    let rows = sqlx::query(SQL_ANNOUNCEMENT_CANDIDATES)
                        .bind(text_value(&values[0])?)
                        .bind(scope.as_str())
                        .bind(flag_value(&values[2])?)
                        .bind(scope2.as_str())
                        .bind(text_value(&values[5])?)
                        .bind(text_value(&values[6])?)
                        .bind(query.as_str())
                        .bind(query.as_str())
                        .bind(query.as_str())
                        .bind(number_value(&values[8])?)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(rows.iter().map(announcement_row).collect::<Result<Vec<_>, BusinessError>>()?)
                })
            })
            .await
            .map_err(transaction)
        })
    }
}

fn text_value(value: &serde_json::Value) -> Result<String, BusinessError> {
    value.as_str().map(str::to_owned).ok_or_else(unavailable)
}

fn nullable_text(value: &serde_json::Value) -> Result<Option<String>, BusinessError> {
    if value.is_null() { Ok(None) } else { text_value(value).map(Some) }
}

fn number_value(value: &serde_json::Value) -> Result<i64, BusinessError> {
    value.as_i64().ok_or_else(unavailable)
}

fn flag_value(value: &serde_json::Value) -> Result<bool, BusinessError> {
    value.as_bool().ok_or_else(unavailable)
}

fn appointment_row(row: &sqlx::postgres::PgRow) -> Result<AppointmentCandidate, BusinessError> {
    Ok(AppointmentCandidate {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
        department_id: row.try_get("department_id").map_err(|_| unavailable())?,
        organizer_person_id: row.try_get("organizer_person_id").map_err(|_| unavailable())?,
        title: row.try_get("title").map_err(|_| unavailable())?,
        description: row.try_get("description").map_err(|_| unavailable())?,
        starts_at: row.try_get("starts_at").map_err(|_| unavailable())?,
        ends_at: row.try_get("ends_at").map_err(|_| unavailable())?,
        timezone: row.try_get("timezone").map_err(|_| unavailable())?,
        location: row.try_get("location").map_err(|_| unavailable())?,
        meeting_url: row.try_get("meeting_url").map_err(|_| unavailable())?,
        private: row.try_get("private").map_err(|_| unavailable())?,
        guests: row.try_get("guests").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        all_day: row.try_get("all_day").map_err(|_| unavailable())?,
        start_date: row.try_get("start_date").map_err(|_| unavailable())?,
        end_date: row.try_get("end_date").map_err(|_| unavailable())?,
        full_detail: row.try_get("full_detail").map_err(|_| unavailable())?,
        organizer_name: row.try_get("organizer_name").map_err(|_| unavailable())?,
    })
}

fn attendee_row(row: &sqlx::postgres::PgRow) -> Result<AttendeeRow, BusinessError> {
    Ok(AttendeeRow {
        appointment_id: row.try_get("appointment_id").map_err(|_| unavailable())?,
        user_id: row.try_get("user_id").map_err(|_| unavailable())?,
        person_id: row.try_get("person_id").map_err(|_| unavailable())?,
        name: row.try_get("name").map_err(|_| unavailable())?,
        in_scope: row.try_get("in_scope").map_err(|_| unavailable())?,
        attendance: row.try_get("attendance").map_err(|_| unavailable())?,
        response: row.try_get("response").map_err(|_| unavailable())?,
    })
}

fn announcement_row(row: &sqlx::postgres::PgRow) -> Result<AnnouncementCandidate, BusinessError> {
    Ok(AnnouncementCandidate {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
        department_id: row.try_get("department_id").map_err(|_| unavailable())?,
        title: row.try_get("title").map_err(|_| unavailable())?,
        content: row.try_get("content").map_err(|_| unavailable())?,
        start_date: row.try_get("start_date").map_err(|_| unavailable())?,
        end_date: row.try_get("end_date").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
        owner_name: row.try_get("owner_name").map_err(|_| unavailable())?,
    })
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
    fn candidate_list_sql_is_byte_identical() {
        assert_eq!(
            hash(SQL_APPOINTMENT_CANDIDATES),
            "d6c5f523d29aefbf566e6d80bf3827d5de0e2d4e9b1e395a4fccfaedf5788265"
        );
        assert_eq!(
            hash(SQL_ATTENDEE_ROWS),
            "2291bea94a779bee303217b13d61a76f42d9c5ec27f81edca1b5b9c7a87e467d"
        );
        assert_eq!(
            hash(SQL_ANNOUNCEMENT_CANDIDATES),
            "dd8aac8bb80605a577e0e3bfcaae1d076b7c163d5f83fdb30926c1db19d4b8a7"
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
