//! PG parity proof for the twelve native Work calendar reads (F1-F6).
//!
//! Each allow-listed kind executes on a disposable Holding database seeded with
//! known calendar rows. The asserted values are exactly what the retired
//! TypeScript SELECT facts returned for the same seed; the local-interval fact
//! additionally compares against the original PostgreSQL expression verbatim.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_work_projection::public::calendar_read::{CalendarReadCommand, execute_calendar_read};
use serde_json::{Value, json};
use sqlx::Row;

const TIME: &str = "2026-01-01T09:00";
const ZONE: &str = "Asia/Bangkok";
const END: &str = "2026-01-01T10:00";

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co-1','Ada','TH','active','sys',1000,1000)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_appointments(id,company_id,organizer_person_id,title,starts_at,ends_at,timezone,status,version,created_by,created_at,updated_at) VALUES \
         ('appt-1','co-1','person-1','Planning',2000,3000,'Asia/Bangkok','scheduled',7,'user-1',1000,1000)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_announcements(id,company_id,title,content,start_date,end_date,status,version,created_by,created_at,updated_at) VALUES \
         ('notice-1','co-1','Title','Body','2026-01-01','2026-01-31','published',2,'user-1',1000,1000)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes,response_at) VALUES \
         ('co-1','appt-1','user-1','person-1','organizer','accepted',15,2000), \
         ('co-1','appt-1','user-2',NULL,'required','pending',NULL,NULL)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id) VALUES \
         ('co-1','appt-1','person-1')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date) VALUES \
         ('appt-1','co-1','weekly',1,'1,3','count',4,NULL)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read(pool: &DatabasePool, kind: &'static str, values: Vec<Value>, company: &str) -> Value {
    let company = company.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run_once::<Value, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let kind = kind.to_owned();
            let values = values.clone();
            let company = company.clone();
            Box::pin(async move {
                let command = CalendarReadCommand {
                    kind: &kind,
                    values: &values,
                    company_id: &company,
                };
                execute_calendar_read(connection, &command).await
            })
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn calendar_reads_match_retired_sql() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;

    // F1 read1: original time expression, executed verbatim as the oracle.
    let time_binds = [TIME, ZONE, END, ZONE, TIME, ZONE, ZONE, END, ZONE, ZONE];
    let oracle = sqlx::query(
        "SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE $2))*1000)::bigint starts_at, \
                (extract(epoch FROM ($3::timestamp AT TIME ZONE $4))*1000)::bigint ends_at, \
                to_char(($5::timestamp AT TIME ZONE $6) AT TIME ZONE $7,'YYYY-MM-DD\"T\"HH24:MI') start_local, \
                to_char(($8::timestamp AT TIME ZONE $9) AT TIME ZONE $10,'YYYY-MM-DD\"T\"HH24:MI') end_local",
    )
    .bind(time_binds[0])
    .bind(time_binds[1])
    .bind(time_binds[2])
    .bind(time_binds[3])
    .bind(time_binds[4])
    .bind(time_binds[5])
    .bind(time_binds[6])
    .bind(time_binds[7])
    .bind(time_binds[8])
    .bind(time_binds[9])
    .fetch_one(pool.raw())
    .await
    .unwrap();
    let expected = json!({
        "starts_at": oracle.try_get::<i64, _>("starts_at").unwrap(),
        "ends_at": oracle.try_get::<i64, _>("ends_at").unwrap(),
        "start_local": oracle.try_get::<String, _>("start_local").unwrap(),
        "end_local": oracle.try_get::<String, _>("end_local").unwrap(),
    });
    assert_eq!(
        read(&pool, "wp_cal_read_01", time_binds.iter().map(|v| json!(v)).collect(), "").await,
        expected
    );

    // F2 read3: calendar_series row.
    assert_eq!(
        read(&pool, "wp_cal_read_03", vec![json!("appt-1")], "").await,
        json!({"frequency":"weekly","interval_count":1,"weekdays":"1,3","end_kind":"count","occurrence_count":4,"until_date":null})
    );

    // F3 read4 / read30 / read38 / read42: calendar_attendees projections.
    assert_eq!(
        read(&pool, "wp_cal_read_04", vec![json!("appt-1"), json!("co-1")], "co-1").await,
        json!([{"user_id":"user-1"},{"user_id":"user-2"}])
    );
    assert_eq!(
        read(&pool, "wp_cal_read_30", vec![json!("appt-1"), json!("co-1")], "co-1").await,
        json!([
            {"user_id":"user-1","response":"accepted","reminder_minutes":15,"response_at":2000},
            {"user_id":"user-2","response":"pending","reminder_minutes":null,"response_at":null}
        ])
    );
    assert_eq!(
        read(&pool, "wp_cal_read_38", vec![json!("co-1"), json!("appt-1"), json!("user-1")], "co-1").await,
        json!({"attendance":"organizer"})
    );
    assert_eq!(
        read(&pool, "wp_cal_read_42", vec![json!("appt-1"), json!("co-1")], "co-1").await,
        json!({"user_id":"user-1"})
    );

    // F4 read12 / read26 / read37: calendar_appointments row, all three facts.
    let appointment = json!({
        "id":"appt-1","company_id":"co-1","branch_id":null,"department_id":null,
        "organizer_person_id":"person-1","title":"Planning","description":"",
        "starts_at":2000,"ends_at":3000,"timezone":"Asia/Bangkok","location":"",
        "meeting_url":"","private":true,"guests":[],"status":"scheduled","version":7,
        "created_by":"user-1","created_at":1000,"updated_at":1000,
        "all_day":false,"start_date":null,"end_date":null,
    });
    for kind in ["wp_cal_read_12", "wp_cal_read_26", "wp_cal_read_37"] {
        assert_eq!(
            read(&pool, kind, vec![json!("appt-1"), json!("co-1")], "co-1").await,
            appointment
        );
    }

    // F5 read23 / read53: calendar_announcements row.
    let announcement = json!({
        "id":"notice-1","company_id":"co-1","branch_id":null,"department_id":null,
        "title":"Title","content":"Body","start_date":"2026-01-01","end_date":"2026-01-31",
        "status":"published","version":2,"created_by":"user-1","created_at":1000,"updated_at":1000,
    });
    for kind in ["wp_cal_read_23", "wp_cal_read_53"] {
        assert_eq!(
            read(&pool, kind, vec![json!("notice-1"), json!("co-1")], "co-1").await,
            announcement
        );
    }

    // F6 read13: calendar_appointment_participants projection.
    assert_eq!(
        read(&pool, "wp_cal_read_13", vec![json!("appt-1"), json!("co-1")], "co-1").await,
        json!([{"person_id":"person-1"}])
    );

    // Absence parity: first-shaped facts return JSON null, all-shaped facts [].
    for kind in ["wp_cal_read_12", "wp_cal_read_23"] {
        assert_eq!(
            read(&pool, kind, vec![json!("missing"), json!("co-1")], "co-1").await,
            Value::Null
        );
    }
    assert_eq!(
        read(&pool, "wp_cal_read_13", vec![json!("missing"), json!("co-1")], "co-1").await,
        json!([])
    );
    }).await;
}
