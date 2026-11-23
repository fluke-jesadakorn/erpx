//! Native Work Projection calendar read port.
//!
//! The twelve SELECT facts consumed by the calendar command unit execute here
//! on the caller's PostgreSQL connection. The TypeScript facts adapter carries
//! no calendar SQL: the transport bridge posts an allow-listed kind and its
//! positional values to the private `/work/owner-read` route, composition
//! opens the Holding transaction, and this module returns the exact row shape
//! the calendar command application already consumes. No SQL mirror and no
//! TypeScript fallback exist behind this port.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
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

const ALL: &[&str] = &[
    READ_01, READ_03, READ_04, READ_12, READ_13, READ_23, READ_26, READ_30, READ_37, READ_38,
    READ_42, READ_53,
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

pub fn supported(kind: &str) -> bool {
    ALL.contains(&kind)
}

/// Exact positional arity; `read1` takes the ten time binds, `read38` three.
pub fn arity(kind: &str) -> Option<usize> {
    match kind {
        READ_01 => Some(10),
        READ_38 => Some(3),
        READ_03 => Some(1),
        READ_04
        | READ_12
        | READ_13
        | READ_23
        | READ_26
        | READ_30
        | READ_37
        | READ_42
        | READ_53 => Some(2),
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
        _ => return Err(BusinessError::invalid(
            "WORK_CALENDAR_READ_INVALID",
            "Unsupported calendar read kind.",
        )
        .into()),
    };
    Ok(result)
}
