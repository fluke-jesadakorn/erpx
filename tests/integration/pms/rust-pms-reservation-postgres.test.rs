//! Native PMS staff reservation commands over real migrated PostgreSQL.
//! These are owner command receipts, not browser acceptance or HTTP evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::reservation::{
    PgPmsReservationRepository, PmsReservationActor, PmsReservationCancelInput,
    PmsReservationChangeInput, PmsReservationCommand, PmsReservationCommandInput,
    PmsReservationCreateInput, PmsReservationGuestInput, PmsReservationHttpBackend,
    PmsReservationHttpCommandRequest, PmsReservationHttpFuture, PmsReservationHttpResponse,
    PmsReservationHttpTarget, PmsReservationLine, PmsReservationOperation,
    lock_reservation_sources, pms_reservation, pms_reservation_router, reservation_request_hash,
};
use erpx_server_pms::public::room_board::PmsBoardActor;
use erpx_server_runtime::public::http_support::serve;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

fn config() -> String {
    json!({
        "country": "TH",
        "currency": "THB",
        "minorUnit": 2,
        "timezone": "Asia/Bangkok",
        "checkIn": "14:00",
        "checkOut": "12:00",
        "cancellationPolicy": "",
        "chartId": "chart",
        "accounts": {"cash": "cash"},
        "taxBps": 700,
        "serviceBps": 1000,
        "taxInclusive": false,
        "advanceTax": true,
        "preparedBy": "maker",
        "approvedBy": "reviewer",
        "revision": 1
    })
    .to_string()
}

async fn property(pool: &DatabasePool, id: &str, company: &str, branch: &str, name: &str) {
    sqlx::query(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES($1,$2,$3,$4,'2026-11-04','open',$5,1,'maker',1)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(name)
    .bind(config())
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn record(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    kind: &str,
    data: serde_json::Value,
) {
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES($1,$2,$3,$4,1,1)",
    )
    .bind(id)
    .bind(property_id)
    .bind(kind)
    .bind(data.to_string())
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    property(pool, "property-1", "company", "north", "Riverside").await;
    property(pool, "property-2", "company", "south", "South Hotel").await;
    property(pool, "property-x", "other", "other", "Foreign Hotel").await;
    record(
        pool,
        "property-1",
        "type-1",
        "room_type",
        json!({"name":"Deluxe","capacity":2,"amenities":""}),
    )
    .await;
    record(
        pool,
        "property-1",
        "type-2",
        "room_type",
        json!({"name":"Suite","capacity":4,"amenities":""}),
    )
    .await;
    for (id, type_id, active) in [
        ("room-a", "type-1", true),
        ("room-b", "type-1", true),
        ("room-c", "type-1", false),
        ("room-d", "type-2", true),
    ] {
        record(
            pool,
            "property-1",
            id,
            "room",
            json!({
                "number": id, "typeId": type_id, "building": "Main", "floor": "1",
                "active": active, "cleanliness": "clean", "cleaningCycle": 0,
                "dnd": false, "observedOccupied": null
            }),
        )
        .await;
    }
    for (id, party_id) in [("rate-1", ""), ("rate-party", "party-1")] {
        record(
            pool,
            "property-1",
            id,
            "rate",
            json!({
                "name": id, "typeId": "type-1", "from": "2026-11-01", "to": "2026-11-30",
                "price": 100000, "extraAdult": 0, "childBands": [],
                "collection": null, "includedGuests": 2, "minNights": 1, "maxNights": 30,
                "stopSell": false, "closedArrival": false, "closedDeparture": false,
                "market": if party_id.is_empty() { "public" } else { "company" },
                "partyId": party_id, "package": "", "active": true, "calendar": []
            }),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "guest-1",
        "guest",
        json!({"name":"Ada Guest","email":"ada@example.test","phone":"0800000000","nationality":"","documentRef":"","notes":""}),
    )
    .await;
}

fn actor() -> PmsReservationActor {
    PmsReservationActor {
        board: PmsBoardActor {
            user_id: "user".into(),
            company_id: "company".into(),
            branch_id: Some("north".into()),
            department_id: None,
            permissions: vec!["pms.read".into(), "pms.reservations".into()],
            modules: vec!["pms".into()],
            expires_at: 9_007_199_254_740_991,
        },
        assignment_id: "assignment-1".into(),
        role: "pms_reservation".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn line(arrival: &str, departure: &str) -> PmsReservationLine {
    PmsReservationLine {
        type_id: "type-1".into(),
        rate_id: "rate-1".into(),
        arrival: arrival.into(),
        departure: departure.into(),
        adults: 2,
        child_ages: vec![],
        quantity: 1,
    }
}

fn create(expected_version: i64) -> PmsReservationCommand {
    PmsReservationCommand {
        operation: PmsReservationOperation::Create,
        property_id: "property-1".into(),
        expected_version,
        input: PmsReservationCommandInput::Create(PmsReservationCreateInput {
            guest_id: Some("guest-1".into()),
            guest: None,
            source: "direct".into(),
            campaign_code: String::new(),
            external_ref: String::new(),
            group_name: String::new(),
            notes: "Front desk".into(),
            block_id: String::new(),
            rooms: vec![line("2026-11-04", "2026-11-06")],
        }),
    }
}

fn create_with_guest(expected_version: i64) -> PmsReservationCommand {
    let mut command = create(expected_version);
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.guest_id = None;
        input.guest = Some(PmsReservationGuestInput {
            name: "New Guest".into(),
            email: "new@example.test".into(),
            phone: "0811111111".into(),
        });
    }
    command
}

fn change(
    reservation_id: &str,
    expected_version: i64,
    reservation_version: i64,
) -> PmsReservationCommand {
    PmsReservationCommand {
        operation: PmsReservationOperation::Change,
        property_id: "property-1".into(),
        expected_version,
        input: PmsReservationCommandInput::Change(PmsReservationChangeInput {
            reservation_id: reservation_id.into(),
            expected_reservation_version: Some(reservation_version),
            reason: "Guest changed dates".into(),
            notes: Some("Updated".into()),
            group_name: None,
            campaign_code: None,
            rooms: vec![line("2026-11-05", "2026-11-08")],
        }),
    }
}

fn cancel(
    reservation_id: &str,
    expected_version: i64,
    reservation_version: i64,
    fee: i64,
) -> PmsReservationCommand {
    PmsReservationCommand {
        operation: PmsReservationOperation::Cancel,
        property_id: "property-1".into(),
        expected_version,
        input: PmsReservationCommandInput::Cancel(PmsReservationCancelInput {
            reservation_id: reservation_id.into(),
            expected_reservation_version: Some(reservation_version),
            reason: "Guest cancelled".into(),
            fee,
        }),
    }
}

async fn execute(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key: &CommandKey,
    command: &PmsReservationCommand,
    now: i64,
) -> Result<
    erpx_server_pms::public::reservation::PmsReservationResult,
    PgTransactionError<BusinessError>,
> {
    let mut repository = PgPmsReservationRepository::new(&mut **tx);
    pms_reservation(&actor(), key, command, &mut repository, now).await
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence error: {error}"),
    }
}

fn persistence(error: PgTransactionError<BusinessError>) -> String {
    match error {
        PgTransactionError::Persistence(error) => error.code.to_owned(),
        PgTransactionError::Business(error) => panic!("unexpected business error: {error:?}"),
    }
}

async fn json_value(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    sql: &'static str,
    id: &str,
) -> Value {
    let raw: String = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[tokio::test]
async fn create_change_cancel_persist_receipt_audit_outbox_and_replay() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("reservation-create-0001").unwrap();
        let mut create_command = create_with_guest(1);
        if let PmsReservationCommandInput::Create(input) = &mut create_command.input {
            input.external_ref = "OTA-1".into();
        }
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &create_command, 5_000)
            .await
            .unwrap();
        assert_eq!(result.property_id, "property-1");
        assert_eq!(result.version, 2);
        let reservation_id = result.id.clone();
        assert!(reservation_id.starts_with("pms-reservation-"));

        let reservation = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            &reservation_id,
        )
        .await;
        assert_eq!(reservation["state"], "confirmed");
        assert_eq!(reservation["source"], "direct");
        assert_eq!(reservation["externalRef"], "OTA-1");
        assert_eq!(reservation["createdBy"], "user");
        assert_eq!(reservation["history"].as_array().unwrap().len(), 1);
        assert_eq!(reservation["history"][0]["action"], "Reservation confirmed");

        let stay_raw: String = sqlx::query_scalar(
            "SELECT data_json FROM pms_records WHERE property_id='property-1' AND kind='stay' AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let stay: Value = serde_json::from_str(&stay_raw).unwrap();
        assert_eq!(stay["state"], "reserved");
        assert_eq!(stay["roomId"], "");
        assert_eq!(stay["prices"].as_array().unwrap().len(), 2);
        assert_eq!(stay["prices"][0]["total"], 117700);
        let stay_id = stay["guestAccountId"].as_str().unwrap();
        let account = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            stay_id,
        )
        .await;
        assert_eq!(account["state"], "open");
        assert_eq!(account["due"], 0);
        assert_eq!(account["reservationId"], reservation_id);

        let source: String =
            sqlx::query_scalar("SELECT record_id FROM pms_sources WHERE property_id=$1 AND source_key=$2")
                .bind("property-1")
                .bind("reservation:direct:OTA-1")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(source, reservation_id);

        let property_version: i64 =
            sqlx::query_scalar("SELECT version FROM pms_properties WHERE id='property-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(property_version, 2);

        let receipt = sqlx::query(
            "SELECT operation,actor_id,request_hash,record_id,result_json,authority_json \
             FROM command_receipts WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "pms.reservation.create");
        assert_eq!(receipt.get::<String, _>("actor_id"), "user");
        assert_eq!(receipt.get::<String, _>("record_id"), "property-1");
        let mut normalized = create_command.clone();
        normalized.normalize();
        assert_eq!(
            receipt.get::<String, _>("request_hash"),
            reservation_request_hash(&actor(), &normalized).unwrap()
        );
        let audit = sqlx::query(
            "SELECT action,record_type,record_id,record_version,assignment_id \
             FROM audit_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "pms.reservation.create");
        assert_eq!(audit.get::<String, _>("record_type"), "reservation");
        assert_eq!(audit.get::<String, _>("record_id"), reservation_id);
        assert_eq!(audit.get::<i64, _>("record_version"), 2);
        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version,payload_json \
             FROM outbox_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "pms.reservation.create");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "reservation");
        assert_eq!(outbox.get::<String, _>("aggregate_id"), reservation_id);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["stayIds"].as_array().unwrap().len(), 1);
        tx.commit().await.unwrap();

        let mut replay = serializable(&holding).await;
        lock_reservation_sources(&mut replay).await.unwrap();
        let replayed = execute(&mut replay, &key, &create_command, 9_000)
            .await
            .unwrap();
        assert_eq!(replayed, result);
        let counts: (i64, i64, i64) = (
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay)
                .await
                .unwrap(),
            sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay)
                .await
                .unwrap(),
            sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay)
                .await
                .unwrap(),
        );
        assert_eq!(counts, (1, 1, 1));
        let mut conflicting = create_command.clone();
        if let PmsReservationCommandInput::Create(input) = &mut conflicting.input {
            input.rooms[0].departure = "2026-11-07".into();
        }
        let error = execute(&mut replay, &key, &conflicting, 9_001)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "IDEMPOTENCY_CONFLICT");
        replay.rollback().await.unwrap();

        let change_key = CommandKey::new("reservation-change-0001").unwrap();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let changed = execute(
            &mut tx,
            &change_key,
            &change(&reservation_id, 2, 1),
            6_000,
        )
        .await
        .unwrap();
        assert_eq!(changed.id, reservation_id);
        assert_eq!(changed.version, 3);
        let reservation = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            &reservation_id,
        )
        .await;
        assert_eq!(reservation["state"], "confirmed");
        assert_eq!(reservation["notes"], "Updated");
        assert_eq!(reservation["history"].as_array().unwrap().len(), 2);
        assert_eq!(reservation["history"][1]["action"], "Reservation changed");
        let stay_raw: String = sqlx::query_scalar(
            "SELECT data_json FROM pms_records WHERE property_id='property-1' AND kind='stay' AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let stay: Value = serde_json::from_str(&stay_raw).unwrap();
        assert_eq!(stay["arrival"], "2026-11-05");
        assert_eq!(stay["departure"], "2026-11-08");
        assert_eq!(stay["prices"].as_array().unwrap().len(), 3);
        let stay_version: i64 = sqlx::query_scalar(
            "SELECT version FROM pms_records WHERE property_id='property-1' AND kind='stay' AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stay_version, 2);
        tx.commit().await.unwrap();

        let cancel_key = CommandKey::new("reservation-cancel-0001").unwrap();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let cancelled = execute(&mut tx, &cancel_key, &cancel(&reservation_id, 3, 2, 0), 7_000)
            .await
            .unwrap();
        assert_eq!(cancelled.version, 4);
        let reservation = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            &reservation_id,
        )
        .await;
        assert_eq!(reservation["state"], "cancelled");
        assert_eq!(reservation["notes"], "Updated\nGuest cancelled");
        assert_eq!(reservation["history"].as_array().unwrap().len(), 3);
        assert_eq!(reservation["history"][2]["action"], "Reservation cancelled");
        let stay_raw: String = sqlx::query_scalar(
            "SELECT data_json FROM pms_records WHERE property_id='property-1' AND kind='stay' AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let stay: Value = serde_json::from_str(&stay_raw).unwrap();
        assert_eq!(stay["state"], "cancelled");
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn create_rejects_scope_versions_overlap_and_bounded_reads() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("reservation-guard-0001").unwrap();

        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &key, &create(2), 5_000).await.unwrap_err();
        assert_eq!(business(error).code, "PMS_VERSION_CONFLICT");
        tx.rollback().await.unwrap();

        let mut foreign = create(1);
        foreign.property_id = "property-x".into();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &key, &foreign, 5_000).await.unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        tx.rollback().await.unwrap();

        let mut other_branch = create(1);
        other_branch.property_id = "property-2".into();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &key, &other_branch, 5_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        tx.rollback().await.unwrap();

        // Two active type-1 rooms; the overlapping stays consume both nights.
        let mut tx = serializable(&holding).await;
        for index in 0..2 {
            sqlx::query(
                "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
                 VALUES($1,'property-1','stay',$2,1,1)",
            )
            .bind(format!("stay-overlap-{index}"))
            .bind(
                json!({
                    "reservationId": format!("reservation-overlap-{index}"),
                    "guestId": "guest-1", "typeId": "type-1", "rateId": "rate-1",
                    "roomId": "", "arrival": "2026-11-04", "departure": "2026-11-06",
                    "adults": 1, "state": "reserved", "guestAccountId": format!("account-{index}"),
                    "prices": [], "segments": []
                })
                .to_string(),
            )
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        let error = execute(&mut tx, &key, &create(1), 5_000).await.unwrap_err();
        assert_eq!(business(error).code, "PMS_SOLD_OUT");
        tx.rollback().await.unwrap();

        // A maintenance block removes one of the two rooms from availability.
        let mut tx = serializable(&holding).await;
        sqlx::query(
            "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
             VALUES('block-maintenance','property-1','block',$1,1,1)",
        )
        .bind(
            json!({
                "name": "Repair", "typeId": "type-1", "roomId": "room-a",
                "from": "2026-11-04", "to": "2026-11-10", "quantity": 1,
                "releaseDate": "", "state": "active", "kind": "maintenance",
                "pickedStayIds": []
            })
            .to_string(),
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
             VALUES('stay-overlap-one','property-1','stay',$1,1,1)",
        )
        .bind(
            json!({
                "reservationId": "reservation-overlap-one", "guestId": "guest-1",
                "typeId": "type-1", "rateId": "rate-1", "roomId": "",
                "arrival": "2026-11-04", "departure": "2026-11-06", "adults": 1,
                "state": "reserved", "guestAccountId": "account-one",
                "prices": [], "segments": []
            })
            .to_string(),
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let error = execute(&mut tx, &key, &create(1), 5_000).await.unwrap_err();
        assert_eq!(business(error).code, "PMS_SOLD_OUT");
        tx.rollback().await.unwrap();

        // Bounded rate read fails closed instead of loading an unbounded hotel.
        let mut tx = serializable(&holding).await;
        sqlx::query(
            "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
             SELECT 'rate-bound-'||g,'property-1','rate',jsonb_build_object(\
               'name','bounded','typeId','type-1','from','2026-11-01','to','2026-11-30',\
               'price',100000,'extraAdult',0,'includedGuests',2,'minNights',1,\
               'maxNights',30,'stopSell',false,'closedArrival',false,\
               'closedDeparture',false,'market','public','partyId','',\
               'package','','active',true,'childBands','[]'::jsonb,'calendar','[]'::jsonb\
             )::text,1,1 FROM generate_series(1,501) g",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let error = execute(&mut tx, &key, &create(1), 5_000).await.unwrap_err();
        let error = business(error);
        assert_eq!(error.kind, FailureKind::Unavailable);
        assert_eq!(error.code, "PMS_RESERVATION_READ_LIMIT");
        tx.rollback().await.unwrap();

        // Duplicate channel reference: the first create commits, the second fails.
        let mut first = create(1);
        if let PmsReservationCommandInput::Create(input) = &mut first.input {
            input.external_ref = "OTA-9".into();
        }
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        execute(&mut tx, &key, &first, 5_000).await.unwrap();
        tx.commit().await.unwrap();

        let duplicate_key = CommandKey::new("reservation-duplicate-0001").unwrap();
        let mut duplicate = create(2);
        if let PmsReservationCommandInput::Create(input) = &mut duplicate.input {
            input.external_ref = "OTA-9".into();
        }
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &duplicate_key, &duplicate, 5_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_EXTERNAL_REF_EXISTS");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn change_and_cancel_fail_closed_on_finance_and_provider_paths() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("reservation-create-0002").unwrap();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &create(1), 5_000).await.unwrap();
        tx.commit().await.unwrap();
        let reservation_id = result.id;

        let account_id: String = sqlx::query_scalar(
            "SELECT data_json::jsonb->>'guestAccountId' FROM pms_records \
             WHERE property_id='property-1' AND kind='stay' \
             AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
             VALUES('entry-1','property-1','entry',$1,1,1)",
        )
        .bind(
            json!({
                "kind": "charge", "guestAccountId": account_id, "total": 117700,
                "description": "Room charge", "sourceKey": "night:test"
            })
            .to_string(),
        )
        .execute(holding.raw())
        .await
        .unwrap();

        let change_key = CommandKey::new("reservation-change-0002").unwrap();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(
            &mut tx,
            &change_key,
            &change(&reservation_id, 2, 1),
            6_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_CONTACT_HOTEL");
        tx.rollback().await.unwrap();

        let fee_key = CommandKey::new("reservation-cancel-fee-0001").unwrap();
        let mut tx = serializable(&holding).await;
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &fee_key, &cancel(&reservation_id, 2, 1, 5000), 7_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_FEE_ACCOUNT_REQUIRED");
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        sqlx::query(
            "UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"payment_pending\"')::text \
             WHERE id=$1",
        )
        .bind(&reservation_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        let cancel_key = CommandKey::new("reservation-cancel-pending-0001").unwrap();
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &cancel_key, &cancel(&reservation_id, 2, 1, 0), 7_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_RESERVATION_CLOSED");
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        sqlx::query(
            "UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"in_house\"') \
             WHERE property_id='property-1' AND kind='stay' \
             AND data_json::jsonb->>'reservationId'=$1",
        )
        .bind(&reservation_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        let checked_in_key = CommandKey::new("reservation-cancel-house-0001").unwrap();
        lock_reservation_sources(&mut tx).await.unwrap();
        let error = execute(
            &mut tx,
            &checked_in_key,
            &cancel(&reservation_id, 2, 1, 0),
            7_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ALREADY_CHECKED_IN");
        tx.rollback().await.unwrap();

        // Requiring a serializable unit is enforced by the adapter itself.
        let mut connection = holding.raw().acquire().await.unwrap();
        let error = lock_reservation_sources(&mut *connection)
            .await
            .unwrap_err();
        assert_eq!(persistence(error), "PMS_RESERVATION_TRANSACTION_REQUIRED");
    })
    .await;
}

const HTTP_HEADERS: &str = "Origin: https://acme.example.test\r\nIdempotency-Key: reservation-create-0009\r\nContent-Type: application/json\r\nx-erpx-context-company: company\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment: assignment-1\r\nx-erpx-context-assignment-version: 1\r\n";

struct ReservationHttpBackend {
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
impl ReservationHttpBackend {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl PmsReservationHttpBackend for ReservationHttpBackend {
    fn command<'a>(
        &'a self,
        request: PmsReservationHttpCommandRequest<'a>,
    ) -> PmsReservationHttpFuture<'a, PmsReservationHttpResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}:{}:{}:{}:{}:{}",
            request.host,
            request.holding_slug,
            request.key.as_str(),
            request.command.operation().as_str(),
            request.command.property_id,
            request.command.input.reservation_id().unwrap_or("-"),
            request.context.company_id,
            request.context.branch_slug.as_deref().unwrap_or("-"),
        ));
        let property_id = request.command.property_id.clone();
        let version = request.command.expected_version + 1;
        Box::pin(async move {
            Ok(PmsReservationHttpResponse {
                data: json!({"id":"pms-reservation-http","propertyId":property_id,"version":version}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn http_target(backend: Arc<ReservationHttpBackend>) -> PmsReservationHttpTarget {
    PmsReservationHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

async fn http_start(
    backend: Arc<ReservationHttpBackend>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(
            listener,
            pms_reservation_router(vec![http_target(backend)]).unwrap(),
        )
        .await
        .unwrap();
    });
    (address, task)
}

async fn http_call(
    address: std::net::SocketAddr,
    method_path: &str,
    headers: &str,
    body: &str,
) -> (u16, Value, String) {
    let request = format!(
        "{method_path} HTTP/1.1\r\nHost: acme.example.test\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8_lossy(&bytes);
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap();
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        head.to_owned(),
    )
}

#[tokio::test]
async fn http_command_retains_bounded_context_and_fails_closed() {
    let backend = Arc::new(ReservationHttpBackend::new());
    let (address, task) = http_start(backend.clone()).await;
    let body = json!({
        "operation": "reservation.create",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "source": "direct",
        "guestId": "guest-1",
        "rooms": [{
            "typeId": "type-1",
            "rateId": "rate-1",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "adults": 2
        }]
    })
    .to_string();
    let (status, value, head) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation?branch=north",
        HTTP_HEADERS,
        &body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "pms-reservation-http");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(value["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        [
            "acme.example.test:acme:reservation-create-0009:reservation.create:property-1:-:company:north"
        ]
    );

    let missing_assignment =
        HTTP_HEADERS.replace("x-erpx-context-assignment: assignment-1\r\n", "");
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        &missing_assignment,
        &body,
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "WORK_CONTEXT_CHANGED");

    let unknown_field = json!({
        "operation": "reservation.create", "propertyId": "property-1",
        "expectedVersion": 1, "source": "direct", "guestId": "guest-1",
        "rooms": [{"typeId":"type-1","rateId":"rate-1","arrival":"2026-11-04",
                   "departure":"2026-11-06","adults":2}],
        "unexpected": true
    })
    .to_string();
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        HTTP_HEADERS,
        &unknown_field,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PMS_RESERVATION_REQUEST_INVALID");

    let cancel_with_rooms = json!({
        "operation": "reservation.cancel",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "reservationId": "reservation-1",
        "reason": "Guest cancelled",
        "rooms": [{
            "typeId": "type-1", "rateId": "rate-1", "arrival": "2026-11-04",
            "departure": "2026-11-06", "adults": 2
        }]
    })
    .to_string();
    let (status, _, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        HTTP_HEADERS,
        &cancel_with_rooms,
    )
    .await;
    assert_eq!(status, 422, "operation-specific fields are bounded");

    let missing_source = json!({
        "operation": "reservation.create",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "guestId": "guest-1",
        "rooms": [{
            "typeId": "type-1",
            "rateId": "rate-1",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "adults": 2
        }]
    })
    .to_string();
    let (status, _, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        HTTP_HEADERS,
        &missing_source,
    )
    .await;
    assert_eq!(status, 422);

    let wrong_origin = HTTP_HEADERS.replace(
        "Origin: https://acme.example.test",
        "Origin: https://evil.example.test",
    );
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        &wrong_origin,
        &body,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let no_key = HTTP_HEADERS.replace("Idempotency-Key: reservation-create-0009\r\n", "");
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        &no_key,
        &body,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, _, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation?unknown=1",
        HTTP_HEADERS,
        &body,
    )
    .await;
    assert_eq!(status, 422);
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/other/pms/reservation",
        HTTP_HEADERS,
        &body,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn http_command_router_is_fail_closed_when_unconfigured_or_invalid() {
    let empty = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = empty.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(empty, pms_reservation_router(Vec::new()).unwrap())
            .await
            .unwrap();
    });
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/reservation",
        HTTP_HEADERS,
        "{}",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    task.abort();

    let backend = Arc::new(ReservationHttpBackend::new());
    let target = || http_target(backend.clone());
    assert!(pms_reservation_router(vec![target()]).is_ok());
    assert!(pms_reservation_router(vec![target(), target()]).is_err());
    assert!(
        pms_reservation_router(vec![PmsReservationHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        pms_reservation_router(vec![PmsReservationHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
}
