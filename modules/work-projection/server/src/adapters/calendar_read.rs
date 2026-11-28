//! Native Work Projection calendar read port.
//!
//! The twelve SELECT facts consumed by the calendar command unit and the eight
//! occurrence/reminder read-tail facts execute here on the caller's PostgreSQL
//! connection. The TypeScript facts adapter carries no calendar SQL: the
//! transport bridge posts an allow-listed kind and its positional values to the
//! private `/work/owner-read` route, composition opens the Holding transaction,
//! and this module returns the exact row shape the calendar application already
//! consumes. No SQL mirror and no TypeScript fallback exist behind this port.
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Database, PgConnection, Postgres, Row, query::Query, postgres::PgRow};

pub type CalendarReadError = PgTransactionError<BusinessError>;

/// One allow-listed kind per clock/command read fact (F1-F6).
pub const READ_01: &str = "wp_cal_read_01"; // local interval time (F1)
pub const READ_03: &str = "wp_cal_read_03"; // series (F2)
pub const READ_04: &str = "wp_cal_read_04"; // attendee users (F3)
pub const READ_12: &str = "wp_cal_read_12"; // appointment (F4)
pub const READ_13: &str = "wp_cal_read_13"; // participants (F6)
pub const READ_23: &str = "wp_cal_read_23"; // announcement (F5)
pub const READ_26: &str = "wp_cal_read_26"; // appointment (F4)
pub const READ_30: &str = "wp_cal_read_30"; // attendee state (F3)
pub const READ_37: &str = "wp_cal_read_37"; // appointment (F4)
pub const READ_38: &str = "wp_cal_read_38"; // attendance (F3)
pub const READ_42: &str = "wp_cal_read_42"; // organizer (F3)
pub const READ_53: &str = "wp_cal_read_53"; // announcement (F5)

// Occurrence exception/response read tail (calendar-appointments-facts.ts
// read4/read5 and calendar-availability-facts.ts readOccurrence* helpers).
pub const READ_60: &str = "wp_cal_read_60"; // occurrence exception by key
pub const READ_61: &str = "wp_cal_read_61"; // occurrence response by key/user
pub const READ_62: &str = "wp_cal_read_62"; // moved occurrences
pub const READ_63: &str = "wp_cal_read_63"; // exceptions by appointment list
pub const READ_64: &str = "wp_cal_read_64"; // responses by appointment list

// Reminder authority read tail (calendar/reminder-authority.ts,
// calendar/reminder-purchases.ts and calendar-reminders-facts.ts).
pub const READ_65: &str = "wp_cal_read_65"; // appointment version
pub const READ_66: &str = "wp_cal_read_66"; // reminder configuration image
pub const READ_67: &str = "wp_cal_read_67"; // latest execution authorization

// Work index read tail (calendar-index-facts.ts read2/read5): byte-identical
// projections retired from TypeScript. The read5 people arm is the same fixed
// `hr_people` projection the owner already inlines for its native candidate
// reads; no cross-owner predicate remains.
pub const READ_68: &str = "wp_cal_read_68"; // interval from_ms/to_ms bounds
pub const READ_69: &str = "wp_cal_read_69"; // authorized calendar history

const ALL: &[&str] = &[
    READ_01, READ_03, READ_04, READ_12, READ_13, READ_23, READ_26, READ_30, READ_37, READ_38,
    READ_42, READ_53, READ_60, READ_61, READ_62, READ_63, READ_64, READ_65, READ_66, READ_67,
    READ_68, READ_69,
];

const TIME_SQL: &str = "SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE $2::text))*1000)::bigint starts_at,(extract(epoch FROM ($3::timestamp AT TIME ZONE $4::text))*1000)::bigint ends_at,to_char(($5::timestamp AT TIME ZONE $6::text) AT TIME ZONE $7::text,'YYYY-MM-DD\"T\"HH24:MI') start_local,to_char(($8::timestamp AT TIME ZONE $9::text) AT TIME ZONE $10::text,'YYYY-MM-DD\"T\"HH24:MI') end_local";
const SERIES_SQL: &str = "SELECT frequency,interval_count,weekdays,end_kind,occurrence_count,until_date FROM calendar_series WHERE appointment_id=$1";
const ATTENDEE_USERS_SQL: &str = "SELECT user_id FROM calendar_attendees WHERE appointment_id=$1 AND company_id=$2";
const APPOINTMENT_SQL: &str = "SELECT * FROM calendar_appointments WHERE id=$1 AND company_id=$2";
const PARTICIPANTS_SQL: &str = "SELECT person_id FROM calendar_appointment_participants WHERE appointment_id=$1 AND company_id=$2";
const ANNOUNCEMENT_SQL: &str = "SELECT * FROM calendar_announcements WHERE id=$1 AND company_id=$2";
const ATTENDEE_STATE_SQL: &str = "SELECT user_id,response,reminder_minutes,response_at FROM calendar_attendees WHERE appointment_id=$1 AND company_id=$2";
const ATTENDANCE_SQL: &str = "SELECT attendance FROM calendar_attendees WHERE company_id=$1 AND appointment_id=$2 AND user_id=$3";
const ORGANIZER_SQL: &str = "SELECT user_id FROM calendar_attendees WHERE appointment_id=$1 AND company_id=$2 AND attendance='organizer'";

// Verbatim PostgreSQL authority of the retained TypeScript facts. `$n` is the
// only translation from their `?` placeholders; predicates and projections are
// unchanged so a row-for-row parity oracle is executable in the PG test.
const OCCURRENCE_EXCEPTION_SQL: &str = "SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3";
const OCCURRENCE_RESPONSE_SQL: &str = "SELECT response FROM calendar_occurrence_responses WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3 AND user_id=$4";
const MOVED_OCCURRENCES_SQL: &str = "SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL";
const OCCURRENCE_EXCEPTIONS_SQL: &str = "SELECT * FROM calendar_occurrence_exceptions WHERE company_id=$1 AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))";
const OCCURRENCE_RESPONSES_SQL: &str = "SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=$1 AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))";
const APPOINTMENT_VERSION_SQL: &str = "SELECT version FROM calendar_appointments WHERE id=$1 AND company_id=$2";
const REMINDER_CONFIGURATION_SQL: &str = "SELECT jsonb_build_object('record',to_jsonb(a)-ARRAY['version','updated_at'],'attendee',to_jsonb(t)-ARRAY['response','response_at'],'series',(SELECT to_jsonb(s)-'version' FROM calendar_series s WHERE s.appointment_id=a.id AND s.company_id=a.company_id),'exceptions',(SELECT jsonb_agg(to_jsonb(e)-'version' ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=a.id AND e.company_id=a.company_id))::text image FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=$1 AND a.company_id=$2 AND t.user_id=$3";
const REMINDER_AUTHORIZATION_SQL: &str = "SELECT * FROM calendar_reminder_execution_authorizations WHERE appointment_id=$1 AND company_id=$2 AND recipient_id=$3 ORDER BY source_version DESC,created_at DESC,command_id DESC LIMIT 1";
/// Byte-identical to the retired `calendar-index-facts.ts` read2 literal; `$n`
/// is the only translation from its `?` placeholders.
pub const INTERVAL_BOUNDS_SQL: &str = "SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE $2))*1000)::bigint from_ms,(extract(epoch FROM ($3::timestamp AT TIME ZONE $4))*1000)::bigint to_ms";
/// Byte-identical to the retired `calendar-index-facts.ts` read5 literal after
/// inlining its two fixed TS fragments (`inList(record_id)` and the r45 people
/// projection). `$n` is the only translation from its `?` placeholders.
pub const CALENDAR_HISTORY_SQL: &str = "SELECT h.record_id,h.action,coalesce(p.legal_name,'Authorized colleague') actor,h.created_at at FROM (SELECT *,row_number() OVER(PARTITION BY record_id ORDER BY created_at DESC,id DESC) ordinal FROM calendar_history WHERE company_id=$1 AND record_id IN (SELECT jsonb_array_elements_text($2::jsonb))) h LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL WHERE ordinal<=20 ORDER BY h.created_at DESC";

pub fn supported(kind: &str) -> bool {
    ALL.contains(&kind)
}

/// Exact positional arity; `read1` takes the ten time binds, `read38` three.
pub fn arity(kind: &str) -> Option<usize> {
    match kind {
        READ_01 => Some(10),
        READ_38 | READ_60 | READ_66 | READ_67 => Some(3),
        READ_61 | READ_68 => Some(4),
        READ_69 => Some(2),
        READ_03 => Some(1),
        READ_04
        | READ_12
        | READ_13
        | READ_23
        | READ_26
        | READ_30
        | READ_37
        | READ_42
        | READ_53
        | READ_62
        | READ_63
        | READ_64
        | READ_65 => Some(2),
        _ => None,
    }
}

pub fn validate(kind: &str, count: usize) -> Result<(), BusinessError> {
    if arity(kind) == Some(count) {
        Ok(())
    } else {
        Err(BusinessError::invalid(
            "WORK_CALENDAR_READ_INVALID",
            "Unsupported calendar read kind or value count.",
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CalendarReadCommand<'a> {
    pub kind: &'a str,
    pub values: &'a [Value],
    pub company_id: &'a str,
}

type BoundQuery<'q> = Query<'q, Postgres, <Postgres as Database>::Arguments>;

fn bind_one<'q>(query: BoundQuery<'q>, value: &'q Value) -> BoundQuery<'q> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(value) => query.bind(*value),
        Value::Number(number) if number.is_i64() => query.bind(number.as_i64().unwrap_or(0)),
        Value::Number(number) if number.is_u64() => {
            query.bind(number.as_u64().unwrap_or(0) as i64)
        }
        Value::Number(number) => query.bind(number.as_f64().unwrap_or(0.0)),
        Value::String(value) => query.bind(value.clone()),
        other => query.bind(other.to_string()),
    }
}

fn bind_all<'q>(mut query: BoundQuery<'q>, values: &'q [Value]) -> BoundQuery<'q> {
    for value in values {
        query = bind_one(query, value);
    }
    query
}

fn time(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "starts_at": row.try_get::<i64, _>("starts_at")?,
        "ends_at": row.try_get::<i64, _>("ends_at")?,
        "start_local": row.try_get::<String, _>("start_local")?,
        "end_local": row.try_get::<String, _>("end_local")?,
    }))
}

fn series(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "frequency": row.try_get::<String, _>("frequency")?,
        "interval_count": row.try_get::<i32, _>("interval_count")?,
        "weekdays": row.try_get::<String, _>("weekdays")?,
        "end_kind": row.try_get::<String, _>("end_kind")?,
        "occurrence_count": row.try_get::<Option<i32>, _>("occurrence_count")?,
        "until_date": row.try_get::<Option<String>, _>("until_date")?,
    }))
}

fn attendee_user(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"user_id": row.try_get::<String, _>("user_id")?}))
}

fn attendee_state(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "user_id": row.try_get::<String, _>("user_id")?,
        "response": row.try_get::<String, _>("response")?,
        "reminder_minutes": row.try_get::<Option<i32>, _>("reminder_minutes")?,
        "response_at": row.try_get::<Option<i64>, _>("response_at")?,
    }))
}

fn attendance(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"attendance": row.try_get::<String, _>("attendance")?}))
}

fn participant(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"person_id": row.try_get::<String, _>("person_id")?}))
}

fn appointment(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "company_id": row.try_get::<String, _>("company_id")?,
        "branch_id": row.try_get::<Option<String>, _>("branch_id")?,
        "department_id": row.try_get::<Option<String>, _>("department_id")?,
        "organizer_person_id": row.try_get::<String, _>("organizer_person_id")?,
        "title": row.try_get::<String, _>("title")?,
        "description": row.try_get::<String, _>("description")?,
        "starts_at": row.try_get::<i64, _>("starts_at")?,
        "ends_at": row.try_get::<i64, _>("ends_at")?,
        "timezone": row.try_get::<String, _>("timezone")?,
        "location": row.try_get::<String, _>("location")?,
        "meeting_url": row.try_get::<String, _>("meeting_url")?,
        "private": row.try_get::<bool, _>("private")?,
        "guests": row.try_get::<Value, _>("guests")?,
        "status": row.try_get::<String, _>("status")?,
        "version": row.try_get::<i32, _>("version")?,
        "created_by": row.try_get::<String, _>("created_by")?,
        "created_at": row.try_get::<i64, _>("created_at")?,
        "updated_at": row.try_get::<i64, _>("updated_at")?,
        "all_day": row.try_get::<bool, _>("all_day")?,
        "start_date": row.try_get::<Option<String>, _>("start_date")?,
        "end_date": row.try_get::<Option<String>, _>("end_date")?,
    }))
}

fn announcement(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "company_id": row.try_get::<String, _>("company_id")?,
        "branch_id": row.try_get::<Option<String>, _>("branch_id")?,
        "department_id": row.try_get::<Option<String>, _>("department_id")?,
        "title": row.try_get::<String, _>("title")?,
        "content": row.try_get::<String, _>("content")?,
        "start_date": row.try_get::<String, _>("start_date")?,
        "end_date": row.try_get::<String, _>("end_date")?,
        "status": row.try_get::<String, _>("status")?,
        "version": row.try_get::<i32, _>("version")?,
        "created_by": row.try_get::<String, _>("created_by")?,
        "created_at": row.try_get::<i64, _>("created_at")?,
        "updated_at": row.try_get::<i64, _>("updated_at")?,
    }))
}

fn occurrence_exception_change(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "start_local": row.try_get::<Option<String>, _>("start_local")?,
        "end_local": row.try_get::<Option<String>, _>("end_local")?,
        "cancelled": row.try_get::<bool, _>("cancelled")?,
    }))
}

fn occurrence_response_value(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"response": row.try_get::<String, _>("response")?}))
}

fn moved_occurrence(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "occurrence_key": row.try_get::<String, _>("occurrence_key")?,
        "start_local": row.try_get::<String, _>("start_local")?,
        "end_local": row.try_get::<String, _>("end_local")?,
    }))
}

fn occurrence_exception(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "appointment_id": row.try_get::<String, _>("appointment_id")?,
        "occurrence_key": row.try_get::<String, _>("occurrence_key")?,
        "start_local": row.try_get::<Option<String>, _>("start_local")?,
        "end_local": row.try_get::<Option<String>, _>("end_local")?,
        "cancelled": row.try_get::<bool, _>("cancelled")?,
    }))
}

fn occurrence_response(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "appointment_id": row.try_get::<String, _>("appointment_id")?,
        "occurrence_key": row.try_get::<String, _>("occurrence_key")?,
        "user_id": row.try_get::<String, _>("user_id")?,
        "response": row.try_get::<String, _>("response")?,
    }))
}

fn version(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"version": row.try_get::<i32, _>("version")?}))
}

fn image(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({"image": row.try_get::<Option<String>, _>("image")?}))
}

fn interval_bounds(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "from_ms": row.try_get::<i64, _>("from_ms")?,
        "to_ms": row.try_get::<i64, _>("to_ms")?,
    }))
}

fn calendar_history(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "record_id": row.try_get::<String, _>("record_id")?,
        "action": row.try_get::<String, _>("action")?,
        "actor": row.try_get::<String, _>("actor")?,
        "at": row.try_get::<i64, _>("at")?,
    }))
}

fn reminder_authorization(row: &PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "command_id": row.try_get::<String, _>("command_id")?,
        "appointment_id": row.try_get::<String, _>("appointment_id")?,
        "recipient_id": row.try_get::<String, _>("recipient_id")?,
        "company_id": row.try_get::<String, _>("company_id")?,
        "actor_id": row.try_get::<String, _>("actor_id")?,
        "assignment_id": row.try_get::<String, _>("assignment_id")?,
        "action": row.try_get::<String, _>("action")?,
        "authority_json": row.try_get::<String, _>("authority_json")?,
        "source_hash": row.try_get::<String, _>("source_hash")?,
        "source_version": row.try_get::<i64, _>("source_version")?,
        "created_at": row.try_get::<i64, _>("created_at")?,
    }))
}

async fn first<F>(
    sql: &'static str,
    values: &[Value],
    map: F,
    connection: &mut PgConnection,
) -> Result<Value, sqlx::Error>
where
    F: Fn(&PgRow) -> Result<Value, sqlx::Error>,
{
    let row = bind_all(sqlx::query(sql), values)
        .fetch_optional(&mut *connection)
        .await?;
    row.as_ref().map(&map).transpose().map(|v| v.unwrap_or(Value::Null))
}

async fn all<F>(
    sql: &'static str,
    values: &[Value],
    map: F,
    connection: &mut PgConnection,
) -> Result<Value, sqlx::Error>
where
    F: Fn(&PgRow) -> Result<Value, sqlx::Error>,
{
    let rows = bind_all(sqlx::query(sql), values)
        .fetch_all(&mut *connection)
        .await?;
    let mut values = Vec::with_capacity(rows.len());
    for row in &rows {
        values.push(map(row)?);
    }
    Ok(Value::Array(values))
}

/// Execute one allow-listed calendar read on the caller's connection.
pub async fn execute_calendar_read(
    connection: &mut PgConnection,
    command: &CalendarReadCommand<'_>,
) -> Result<Value, CalendarReadError> {
    validate(command.kind, command.values.len()).map_err(CalendarReadError::from)?;
    let result = match command.kind {
        READ_01 => first(TIME_SQL, command.values, time, connection).await?,
        READ_03 => first(SERIES_SQL, command.values, series, connection).await?,
        READ_04 => all(ATTENDEE_USERS_SQL, command.values, attendee_user, connection).await?,
        READ_12 | READ_26 | READ_37 => {
            first(APPOINTMENT_SQL, command.values, appointment, connection).await?
        }
        READ_13 => all(PARTICIPANTS_SQL, command.values, participant, connection).await?,
        READ_23 | READ_53 => {
            first(ANNOUNCEMENT_SQL, command.values, announcement, connection).await?
        }
        READ_30 => all(ATTENDEE_STATE_SQL, command.values, attendee_state, connection).await?,
        READ_38 => first(ATTENDANCE_SQL, command.values, attendance, connection).await?,
        READ_42 => first(ORGANIZER_SQL, command.values, attendee_user, connection).await?,
        READ_60 => {
            first(OCCURRENCE_EXCEPTION_SQL, command.values, occurrence_exception_change, connection)
                .await?
        }
        READ_61 => {
            first(OCCURRENCE_RESPONSE_SQL, command.values, occurrence_response_value, connection)
                .await?
        }
        READ_62 => all(MOVED_OCCURRENCES_SQL, command.values, moved_occurrence, connection).await?,
        READ_63 => all(OCCURRENCE_EXCEPTIONS_SQL, command.values, occurrence_exception, connection).await?,
        READ_64 => all(OCCURRENCE_RESPONSES_SQL, command.values, occurrence_response, connection).await?,
        READ_65 => first(APPOINTMENT_VERSION_SQL, command.values, version, connection).await?,
        READ_66 => first(REMINDER_CONFIGURATION_SQL, command.values, image, connection).await?,
        READ_67 => {
            first(REMINDER_AUTHORIZATION_SQL, command.values, reminder_authorization, connection)
                .await?
        }
        READ_68 => first(INTERVAL_BOUNDS_SQL, command.values, interval_bounds, connection).await?,
        READ_69 => all(CALENDAR_HISTORY_SQL, command.values, calendar_history, connection).await?,
        _ => return Err(BusinessError::invalid(
            "WORK_CALENDAR_READ_INVALID",
            "Unsupported calendar read kind.",
        )
        .into()),
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(sql: &str) -> String {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&Sha256::digest(sql.as_bytes()));
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn interval_bounds_sql_is_byte_identical_to_retired_ts_literal() {
        assert_eq!(
            hash(INTERVAL_BOUNDS_SQL),
            "84cb1cbf466f34450b94fa78287bdf16b0463efeb77cc61667e2329207e67cb7"
        );
    }

    #[test]
    fn interval_bounds_kind_is_allow_listed_with_four_binds() {
        assert!(supported(READ_68));
        assert_eq!(arity(READ_68), Some(4));
        assert!(validate(READ_68, 4).is_ok());
        assert!(validate(READ_68, 3).is_err());
    }

    #[test]
    fn calendar_history_sql_is_byte_identical_to_retired_ts_literal() {
        assert_eq!(
            hash(CALENDAR_HISTORY_SQL),
            "541bb31b09d37dd5208fbee258d95432c615d588051cb398505c245f1c17b104"
        );
        assert!(CALENDAR_HISTORY_SQL.contains("hr_people"));
        assert!(CALENDAR_HISTORY_SQL.contains("jsonb_array_elements_text($2::jsonb)"));
    }

    #[test]
    fn calendar_history_kind_is_allow_listed_with_two_binds() {
        assert!(supported(READ_69));
        assert_eq!(arity(READ_69), Some(2));
        assert!(validate(READ_69, 2).is_ok());
        assert!(validate(READ_69, 1).is_err());
    }
}
