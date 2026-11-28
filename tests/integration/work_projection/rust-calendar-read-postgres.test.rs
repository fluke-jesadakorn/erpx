//! PG parity proof for the twelve native Work calendar reads (F1-F6).
//!
//! Each allow-listed kind executes on a disposable Holding database seeded with
//! known calendar rows. The asserted values are exactly what the retired
//! TypeScript SELECT facts returned for the same seed; the local-interval fact
//! additionally compares against the original PostgreSQL expression verbatim.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey};
use erpx_work_projection_server::public::calendar_read::{
    CalendarReadCommand, READ_60, READ_61, READ_62, READ_63, READ_64, READ_65, READ_66, READ_67,
    execute_calendar_read,
};
use erpx_work_projection_server::public::owner_write::{
    OwnerWriteCommand, PgOwnerWrite, WP_CAL_APPOINTMENT_STATUS, execute_owner_write,
};
use erpx_work_projection_server::public::calendar_read_tail::CalendarReadTail;
use serde_json::{Value, json};
use sqlx::Row;

const TIME: &str = "2026-01-01T09:00";
const ZONE: &str = "Asia/Bangkok";
const END: &str = "2026-01-01T10:00";

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
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

/// Seeds the three read-tail families on top of the original calendar fixture.
/// Every row is also the input to a verbatim TS-authority SQL oracle below.
async fn seed_tail(pool: &DatabasePool) {
    sqlx::query(
        "INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,start_local,end_local,cancelled,version) VALUES \
         ('co-1','appt-1','k1','2026-01-08T09:00','2026-01-08T10:00',false,1), \
         ('co-1','appt-1','k2',NULL,NULL,true,1), \
         ('co-1','appt-1','k3',NULL,NULL,false,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO calendar_occurrence_responses(company_id,appointment_id,occurrence_key,user_id,response,responded_at) VALUES \
         ('co-1','appt-1','k1','user-1','accepted',2000), \
         ('co-1','appt-1','k2','user-1','declined',2100), \
         ('co-1','appt-1','k1','user-2','tentative',2200)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES \
         ('cmd-1','calendar.appointment.save','user-1','req-1','appt-1','{\"id\":\"appt-1\",\"version\":7}',1000,'{}')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    let hash = "a".repeat(64);
    let authority = r#"{"userId":"user-1","assignmentId":"asg-1","companyId":"co-1"}"#;
    sqlx::query(
        "INSERT INTO calendar_reminder_execution_authorizations(command_id,appointment_id,recipient_id,company_id,actor_id,assignment_id,action,authority_json,source_hash,source_version,created_at) VALUES \
         ($1,'appt-1','user-1','co-1','user-1','asg-1','appointment.save',$2,$3,7,1000)",
    )
    .bind("cmd-1")
    .bind(authority)
    .bind(hash)
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn calendar_read_tail_matches_retained_ts_authority() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_tail(&pool).await;

        // --- occurrence exception/response family -------------------------
        // calendar-availability-facts.ts::readOccurrenceException (verbatim).
        let oracle = sqlx::query(
            "SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3",
        )
        .bind("appt-1")
        .bind("co-1")
        .bind("k1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let expected = json!({
            "start_local": oracle.try_get::<Option<String>, _>("start_local").unwrap(),
            "end_local": oracle.try_get::<Option<String>, _>("end_local").unwrap(),
            "cancelled": oracle.try_get::<bool, _>("cancelled").unwrap(),
        });
        assert_eq!(
            read(&pool, READ_60, vec![json!("appt-1"), json!("co-1"), json!("k1")], "co-1").await,
            expected
        );

        // calendar-availability-facts.ts::readOccurrenceResponse (verbatim).
        let oracle = sqlx::query(
            "SELECT response FROM calendar_occurrence_responses WHERE appointment_id=$1 AND company_id=$2 AND occurrence_key=$3 AND user_id=$4",
        )
        .bind("appt-1")
        .bind("co-1")
        .bind("k1")
        .bind("user-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let expected = json!({"response": oracle.try_get::<String, _>("response").unwrap()});
        assert_eq!(
            read(
                &pool,
                READ_61,
                vec![json!("appt-1"), json!("co-1"), json!("k1"), json!("user-1")],
                "co-1"
            )
            .await,
            expected
        );

        // calendar-availability-facts.ts::readMovedOccurrences (verbatim).
        let oracle = sqlx::query(
            "SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=$1 AND company_id=$2 AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL",
        )
        .bind("appt-1")
        .bind("co-1")
        .fetch_all(pool.raw())
        .await
        .unwrap();
        let expected: Vec<Value> = oracle
            .iter()
            .map(|row| {
                json!({
                    "occurrence_key": row.try_get::<String, _>("occurrence_key").unwrap(),
                    "start_local": row.try_get::<String, _>("start_local").unwrap(),
                    "end_local": row.try_get::<String, _>("end_local").unwrap(),
                })
            })
            .collect();
        assert_eq!(
            read(&pool, READ_62, vec![json!("appt-1"), json!("co-1")], "co-1").await,
            Value::Array(expected)
        );

        // calendar-appointments-facts.ts::read4/read5 (verbatim list facts).
        let ids = json!(["appt-1"]);
        let oracle = sqlx::query(
            "SELECT * FROM calendar_occurrence_exceptions WHERE company_id=$1 AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))",
        )
        .bind("co-1")
        .bind(ids.to_string())
        .fetch_all(pool.raw())
        .await
        .unwrap();
        let expected: Vec<Value> = oracle
            .iter()
            .map(|row| {
                json!({
                    "appointment_id": row.try_get::<String, _>("appointment_id").unwrap(),
                    "occurrence_key": row.try_get::<String, _>("occurrence_key").unwrap(),
                    "start_local": row.try_get::<Option<String>, _>("start_local").unwrap(),
                    "end_local": row.try_get::<Option<String>, _>("end_local").unwrap(),
                    "cancelled": row.try_get::<bool, _>("cancelled").unwrap(),
                })
            })
            .collect();
        assert_eq!(
            read(&pool, READ_63, vec![json!("co-1"), ids.clone()], "co-1").await,
            Value::Array(expected)
        );

        let oracle = sqlx::query(
            "SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=$1 AND appointment_id IN (SELECT jsonb_array_elements_text($2::jsonb))",
        )
        .bind("co-1")
        .bind(ids.to_string())
        .fetch_all(pool.raw())
        .await
        .unwrap();
        let expected: Vec<Value> = oracle
            .iter()
            .map(|row| {
                json!({
                    "appointment_id": row.try_get::<String, _>("appointment_id").unwrap(),
                    "occurrence_key": row.try_get::<String, _>("occurrence_key").unwrap(),
                    "user_id": row.try_get::<String, _>("user_id").unwrap(),
                    "response": row.try_get::<String, _>("response").unwrap(),
                })
            })
            .collect();
        assert_eq!(
            read(&pool, READ_64, vec![json!("co-1"), ids.clone()], "co-1").await,
            Value::Array(expected)
        );

        // --- reminder authority family ------------------------------------
        // reminder-authority.ts::appointmentVersionSql (verbatim).
        let oracle = sqlx::query("SELECT version FROM calendar_appointments WHERE id=$1 AND company_id=$2")
            .bind("appt-1")
            .bind("co-1")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let expected = json!({"version": oracle.try_get::<i32, _>("version").unwrap()});
        assert_eq!(
            read(&pool, READ_65, vec![json!("appt-1"), json!("co-1")], "co-1").await,
            expected
        );

        // reminder-purchases.ts newest-authorization SELECT (verbatim).
        let oracle = sqlx::query(
            "SELECT * FROM calendar_reminder_execution_authorizations WHERE appointment_id=$1 AND company_id=$2 AND recipient_id=$3 ORDER BY source_version DESC,created_at DESC,command_id DESC LIMIT 1",
        )
        .bind("appt-1")
        .bind("co-1")
        .bind("user-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let expected = json!({
            "command_id": oracle.try_get::<String, _>("command_id").unwrap(),
            "appointment_id": oracle.try_get::<String, _>("appointment_id").unwrap(),
            "recipient_id": oracle.try_get::<String, _>("recipient_id").unwrap(),
            "company_id": oracle.try_get::<String, _>("company_id").unwrap(),
            "actor_id": oracle.try_get::<String, _>("actor_id").unwrap(),
            "assignment_id": oracle.try_get::<String, _>("assignment_id").unwrap(),
            "action": oracle.try_get::<String, _>("action").unwrap(),
            "authority_json": oracle.try_get::<String, _>("authority_json").unwrap(),
            "source_hash": oracle.try_get::<String, _>("source_hash").unwrap(),
            "source_version": oracle.try_get::<i64, _>("source_version").unwrap(),
            "created_at": oracle.try_get::<i64, _>("created_at").unwrap(),
        });
        assert_eq!(
            read(&pool, READ_67, vec![json!("appt-1"), json!("co-1"), json!("user-1")], "co-1").await,
            expected
        );

        // The configuration image is the exact jsonb_build_object of
        // reminder-authority.ts. Compare the native projection to the verbatim
        // expression and then to the documented record/attendee/series/
        // exceptions shape (version fields excluded, exceptions ordered).
        let image = read(
            &pool,
            READ_66,
            vec![json!("appt-1"), json!("co-1"), json!("user-1")],
            "co-1",
        )
        .await;
        let oracle = sqlx::query(
            "SELECT jsonb_build_object('record',to_jsonb(a)-ARRAY['version','updated_at'],'attendee',to_jsonb(t)-ARRAY['response','response_at'],'series',(SELECT to_jsonb(s)-'version' FROM calendar_series s WHERE s.appointment_id=a.id AND s.company_id=a.company_id),'exceptions',(SELECT jsonb_agg(to_jsonb(e)-'version' ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=a.id AND e.company_id=a.company_id))::text image FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=$1 AND a.company_id=$2 AND t.user_id=$3",
        )
        .bind("appt-1")
        .bind("co-1")
        .bind("user-1")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let oracle_image = oracle.try_get::<Option<String>, _>("image").unwrap().unwrap();
        assert_eq!(image, json!({"image": oracle_image.clone()}));
        let shape: Value = serde_json::from_str(&oracle_image).unwrap();
        assert_eq!(shape["record"]["id"], json!("appt-1"));
        assert!(shape["record"].get("version").is_none());
        assert_eq!(shape["attendee"]["user_id"], json!("user-1"));
        assert!(shape["attendee"].get("response").is_none());
        assert_eq!(shape["series"]["frequency"], json!("weekly"));
        assert!(shape["series"].get("version").is_none());
        assert_eq!(shape["exceptions"][0]["occurrence_key"], json!("k1"));
        assert!(shape["exceptions"][0].get("version").is_none());

        // Absence parity: first-shaped facts return null, list facts [].
        assert_eq!(
            read(&pool, READ_60, vec![json!("appt-1"), json!("co-1"), json!("missing")], "co-1").await,
            Value::Null
        );
        assert_eq!(
            read(&pool, READ_63, vec![json!("co-1"), json!(["missing"])], "co-1").await,
            json!([])
        );

        // --- typed application port over the same transaction ------------
        PgUnitOfWork::new(pool.clone())
            .run_once::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = "co-1".to_owned();
                Box::pin(async move {
                    let mut read = CalendarReadTail::new(connection, &company);
                    assert_eq!(read.appointment_row("appt-1").await.unwrap().unwrap().title, "Planning");
                    let change = read.occurrence_exception("appt-1", "k1").await.unwrap().unwrap();
                    assert_eq!(change.start_local.as_deref(), Some("2026-01-08T09:00"));
                    assert!(!change.cancelled);
                    assert_eq!(
                        read.occurrence_response("appt-1", "k1", "user-1")
                            .await.unwrap().unwrap().response,
                        "accepted"
                    );
                    assert_eq!(read.moved_occurrences("appt-1").await.unwrap().len(), 1);
                    assert_eq!(read.occurrence_exceptions(&["appt-1"]).await.unwrap().len(), 3);
                    assert_eq!(read.occurrence_responses(&["appt-1"]).await.unwrap().len(), 3);
                    assert_eq!(read.appointment_version("appt-1").await.unwrap().unwrap().version, 7);
                    assert_eq!(
                        read.reminder_authorization("appt-1", "user-1")
                            .await.unwrap().unwrap().source_version,
                        7
                    );
                    assert!(read.reminder_configuration("appt-1", "user-1")
                        .await.unwrap().unwrap().image.is_some());
                    Ok(())
                })
            })
            .await
            .unwrap();
    }).await;
}

/// The native command path executes real calendar SQL in the Holding write
/// pool: the result is visible to the native calendar read port and the receipt
/// is committed in the same transaction. This is the owner-side proof for the
/// mounted `/api/v1/holdings/{holding}/work/owner-write` route.
#[tokio::test]
async fn calendar_command_executes_in_the_holding_write_pool() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let key = CommandKey::new("wp-cal-52:appt-1:7").unwrap();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let key = key.as_str().to_owned();
                Box::pin(async move {
                    let mut write = PgOwnerWrite::new(&mut *connection);
                    let command = OwnerWriteCommand {
                        kind: WP_CAL_APPOINTMENT_STATUS,
                        values: &[
                            json!("completed"),
                            json!(2000),
                            json!("appt-1"),
                            json!("co-1"),
                        ],
                        company_id: "co-1",
                        actor_id: "user-1",
                        now: 2000,
                    };
                    let reply = execute_owner_write(&mut write, &command, &key)
                        .await
                        .unwrap();
                    assert_eq!(reply["ok"], json!(true));
                    assert_eq!(reply["kind"], json!(WP_CAL_APPOINTMENT_STATUS));
                    Ok(())
                })
            })
            .await
            .unwrap();

        let appointment = read(
            &pool,
            "wp_cal_read_12",
            vec![json!("appt-1"), json!("co-1")],
            "co-1",
        )
        .await;
        assert_eq!(appointment["status"], json!("completed"));
        assert_eq!(appointment["version"], json!(8));

        let receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1")
                .bind("work-owner-write:wp_cal_appointment_status:wp-cal-52:appt-1:7")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(receipts, 1);
    })
    .await;
}
