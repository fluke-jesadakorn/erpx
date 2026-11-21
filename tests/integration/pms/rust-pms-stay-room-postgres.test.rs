//! Native PMS `stay.assign`/`stay.move` commands over real migrated PostgreSQL.
//! These are owner command receipts, not browser acceptance or HTTP evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::room_board::PmsBoardActor;
use erpx_server_pms::public::stay_room::{
    MAX_TASKS, PgPmsStayRoomRepository, PmsBoardFuture, PmsStayRoomActor, PmsStayRoomCommand,
    PmsStayRoomEntitlement, PmsStayRoomEntitlementPort, PmsStayRoomHttpBackend,
    PmsStayRoomHttpCommandRequest, PmsStayRoomHttpFuture, PmsStayRoomHttpResponse,
    PmsStayRoomHttpTarget, PmsStayRoomOperation, lock_stay_room_sources, pms_stay_room,
    pms_stay_room_router, stay_room_request_hash,
};
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
        "accounts": {
            "cash": "cash", "bank": "bank", "edc": "edc", "receivable": "receivable",
            "advance": "advance", "deposit": "deposit", "revenue": "revenue",
            "service": "service", "tax": "tax", "prepaidTax": "prepaidTax",
            "inventory": "inventory", "consumption": "consumption",
            "commission": "commission", "cashDifference": "cashDifference"
        },
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
    for (id, number, cleanliness, cycle, active, dnd) in [
        ("room-old", "101", "clean", 3, true, true),
        ("room-new", "102", "inspected", 0, true, false),
        ("room-clean", "103", "clean", 0, true, false),
        ("room-busy", "104", "clean", 0, true, false),
        ("room-block", "105", "clean", 0, true, false),
        ("room-inactive", "106", "inspected", 0, false, false),
        ("room-wrongtype", "107", "inspected", 0, true, false),
    ] {
        record(
            pool,
            "property-1",
            id,
            "room",
            json!({
                "number": number,
                "typeId": if id == "room-wrongtype" { "type-2" } else { "type-1" },
                "building": "Main",
                "floor": "1",
                "active": active,
                "cleanliness": cleanliness,
                "cleaningCycle": cycle,
                "dnd": dnd,
                "observedOccupied": null
            }),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "stay-reserved",
        "stay",
        json!({
            "reservationId": "reservation-1", "guestId": "guest-1", "typeId": "type-1",
            "rateId": "rate-1", "roomId": "", "arrival": "2026-11-04",
            "departure": "2026-11-06", "adults": 2, "state": "reserved",
            "guestAccountId": "account-1", "prices": [], "segments": []
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-inhouse",
        "stay",
        json!({
            "reservationId": "reservation-2", "guestId": "guest-2", "typeId": "type-1",
            "rateId": "rate-1", "roomId": "room-old", "arrival": "2026-11-03",
            "departure": "2026-11-06", "adults": 2, "state": "in_house",
            "guestAccountId": "account-2", "prices": [],
            "segments": [{"roomId":"room-old","from":"2026-11-03","to":"2026-11-06"}]
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-busy",
        "stay",
        json!({
            "reservationId": "reservation-3", "guestId": "guest-3", "typeId": "type-1",
            "rateId": "rate-1", "roomId": "room-busy", "arrival": "2026-11-04",
            "departure": "2026-11-06", "adults": 1, "state": "reserved",
            "guestAccountId": "account-3", "prices": [],
            "segments": [{"roomId":"room-busy","from":"2026-11-04","to":"2026-11-06"}]
        }),
    )
    .await;
    for id in ["reservation-1", "reservation-2", "reservation-3"] {
        record(
            pool,
            "property-1",
            id,
            "reservation",
            json!({"number":id,"guestId":"guest-1","source":"direct","externalRef":"","groupName":"","notes":"","state":"confirmed","createdBy":"maker","history":[]}),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "block-1",
        "block",
        json!({
            "name": "Maintenance", "typeId": "type-1", "roomId": "room-block",
            "from": "2026-11-04", "to": "2026-11-05", "quantity": 1,
            "releaseDate": "", "state": "active", "kind": "maintenance",
            "pickedStayIds": []
        }),
    )
    .await;
    for (id, room, task_type, state) in [
        ("task-old-open", "room-old", "stayover", "pending"),
        ("task-old-done", "room-old", "stayover", "done"),
        ("task-old-maintenance", "room-old", "maintenance", "pending"),
        ("task-new-open", "room-new", "stayover", "pending"),
    ] {
        record(
            pool,
            "property-1",
            id,
            "task",
            json!({
                "roomCycle": if room == "room-old" { 3 } else { 0 },
                "roomId": room, "type": task_type, "stayId": "stay-inhouse",
                "date": "2026-11-04", "assigneeId": "", "priority": "normal",
                "state": state, "checklist": [], "checked": [], "notes": "",
                "completedBy": "", "inspectedBy": ""
            }),
        )
        .await;
    }
}

fn actor() -> PmsStayRoomActor {
    PmsStayRoomActor {
        board: PmsBoardActor {
            user_id: "user".into(),
            company_id: "company".into(),
            branch_id: Some("north".into()),
            department_id: None,
            permissions: vec!["pms.read".into(), "pms.frontdesk".into()],
            modules: vec!["pms".into()],
            expires_at: 9_007_199_254_740_991,
        },
        assignment_id: "assignment-1".into(),
        role: "pms_frontdesk".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn command(
    operation: PmsStayRoomOperation,
    stay_id: &str,
    room_id: &str,
    expected_version: i64,
) -> PmsStayRoomCommand {
    PmsStayRoomCommand {
        operation,
        property_id: "property-1".into(),
        stay_id: stay_id.into(),
        room_id: room_id.into(),
        reason: "synthetic".into(),
        expected_version,
        expected_stay_version: Some(1),
    }
}

struct Packages;
impl PmsStayRoomEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsStayRoomEntitlement, Self::Error> {
        Box::pin(async {
            Ok(PmsStayRoomEntitlement {
                enforced: true,
                settle_existing_allowed: true,
            })
        })
    }
}

async fn execute(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key: &CommandKey,
    command: &PmsStayRoomCommand,
    now: i64,
) -> Result<erpx_server_pms::public::stay_room::PmsStayRoomResult, PgTransactionError<BusinessError>>
{
    let mut repository = PgPmsStayRoomRepository::new(&mut **tx);
    let mut packages = Packages;
    pms_stay_room(&actor(), key, command, &mut repository, &mut packages, now).await
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
async fn assign_reserved_stay_persists_plan_receipt_audit_outbox_and_replays() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("stay-assign-0001").unwrap();
        let assign = command(
            PmsStayRoomOperation::Assign,
            "stay-reserved",
            "room-clean",
            1,
        );
        let mut tx = serializable(&holding).await;
        lock_stay_room_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &assign, 5_000).await.unwrap();
        assert_eq!(
            result,
            erpx_server_pms::public::stay_room::PmsStayRoomResult {
                id: "stay-reserved".into(),
                property_id: "property-1".into(),
                version: 2,
            }
        );
        let stay = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "stay-reserved",
        )
        .await;
        assert_eq!(stay["roomId"], "room-clean");
        assert_eq!(stay["segments"][0]["roomId"], "room-clean");
        assert_eq!(stay["segments"][0]["from"], "2026-11-04");
        assert_eq!(stay["segments"][0]["to"], "2026-11-06");
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM pms_records WHERE id='stay-reserved'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(version, 2);
        let updated_at: i64 =
            sqlx::query_scalar("SELECT updated_at FROM pms_records WHERE id='stay-reserved'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(updated_at, 5_000);
        let reservation = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "reservation-1",
        )
        .await;
        assert_eq!(reservation["history"].as_array().unwrap().len(), 1);
        assert_eq!(reservation["history"][0]["action"], "stay assign");
        assert_eq!(reservation["history"][0]["actor"], "user");
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
        assert_eq!(receipt.get::<String, _>("operation"), "stay.assign");
        assert_eq!(receipt.get::<String, _>("actor_id"), "user");
        assert_eq!(receipt.get::<String, _>("record_id"), "property-1");
        let mut normalized = assign.clone();
        normalized.normalize();
        assert_eq!(
            receipt.get::<String, _>("request_hash"),
            stay_room_request_hash(&actor(), &normalized).unwrap()
        );
        let retained: Value =
            serde_json::from_str(&receipt.get::<String, _>("result_json")).unwrap();
        assert_eq!(retained["version"], 2);
        let authority: Value =
            serde_json::from_str(&receipt.get::<String, _>("authority_json")).unwrap();
        assert_eq!(authority["companyId"], "company");
        assert_eq!(authority["assignmentId"], "assignment-1");

        let audit = sqlx::query(
            "SELECT action,record_type,record_id,record_version,assignment_id \
             FROM audit_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "stay.assign");
        assert_eq!(audit.get::<String, _>("record_type"), "stay");
        assert_eq!(audit.get::<String, _>("record_id"), "stay-reserved");
        assert_eq!(audit.get::<i64, _>("record_version"), 2);
        assert_eq!(audit.get::<String, _>("assignment_id"), "assignment-1");

        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version,payload_json \
             FROM outbox_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "stay.assign");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "stay");
        assert_eq!(outbox.get::<String, _>("aggregate_id"), "stay-reserved");
        assert_eq!(outbox.get::<i64, _>("aggregate_version"), 2);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["roomId"], "room-clean");
        tx.commit().await.unwrap();

        let mut replay_tx = serializable(&holding).await;
        lock_stay_room_sources(&mut replay_tx).await.unwrap();
        let replayed = execute(&mut replay_tx, &key, &assign, 9_000).await.unwrap();
        assert_eq!(replayed, result);
        let receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay_tx)
                .await
                .unwrap();
        let audits: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay_tx)
                .await
                .unwrap();
        let outbox: i64 =
            sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *replay_tx)
                .await
                .unwrap();
        assert_eq!((receipts, audits, outbox), (1, 1, 1));

        let mut conflicting = assign.clone();
        conflicting.room_id = "room-busy".into();
        let error = execute(&mut replay_tx, &key, &conflicting, 9_001)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "IDEMPOTENCY_CONFLICT");
        replay_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn move_in_house_stay_supersedes_old_tasks_and_creates_departure_task() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let stale = command(PmsStayRoomOperation::Move, "stay-inhouse", "room-new", 2);
        let key = CommandKey::new("stay-move-0001").unwrap();
        let mut tx = serializable(&holding).await;
        lock_stay_room_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &key, &stale, 5_000).await.unwrap_err();
        assert_eq!(business(error).code, "PMS_VERSION_CONFLICT");
        let untouched: i64 =
            sqlx::query_scalar("SELECT version FROM pms_properties WHERE id='property-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(untouched, 1);
        tx.rollback().await.unwrap();

        let move_command = command(PmsStayRoomOperation::Move, "stay-inhouse", "room-new", 1);
        let mut tx = serializable(&holding).await;
        lock_stay_room_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &move_command, 5_000).await.unwrap();
        assert_eq!(result.version, 2);
        let stay = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "stay-inhouse",
        )
        .await;
        assert_eq!(stay["roomId"], "room-new");
        let segments = stay["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0]["roomId"], "room-old");
        assert_eq!(segments[0]["to"], "2026-11-04");
        assert_eq!(segments[1]["roomId"], "room-new");
        let old_room = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "room-old",
        )
        .await;
        assert_eq!(old_room["cleanliness"], "dirty");
        assert_eq!(old_room["cleaningCycle"], 4);
        assert_eq!(old_room["dnd"], false);
        let old_open = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "task-old-open",
        )
        .await;
        assert_eq!(old_open["state"], "superseded");
        let old_done = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "task-old-done",
        )
        .await;
        assert_eq!(old_done["state"], "done");
        let old_maintenance = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "task-old-maintenance",
        )
        .await;
        assert_eq!(old_maintenance["state"], "pending");
        let new_open = json_value(
            &mut tx,
            "SELECT data_json FROM pms_records WHERE id=$1",
            "task-new-open",
        )
        .await;
        assert_eq!(new_open["state"], "pending");
        let departure: String = sqlx::query_scalar(
            "SELECT data_json FROM pms_records WHERE kind='task' AND data_json::jsonb->>'type'='departure'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let departure: Value = serde_json::from_str(&departure).unwrap();
        assert_eq!(departure["roomId"], "room-old");
        assert_eq!(departure["roomCycle"], 4);
        assert_eq!(departure["stayId"], "stay-inhouse");
        assert_eq!(departure["date"], "2026-11-04");
        assert_eq!(departure["state"], "pending");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn scope_transaction_lock_and_missing_facts_fail_closed() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let assign_command = command(
            PmsStayRoomOperation::Assign,
            "stay-reserved",
            "room-clean",
            1,
        );
        let key = CommandKey::new("stay-assign-scope").unwrap();

        let mut connection = holding.raw().acquire().await.unwrap();
        let mut repository = PgPmsStayRoomRepository::new(&mut connection);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &assign_command,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PMS_STAY_ROOM_TRANSACTION_REQUIRED");
        drop(repository);
        drop(connection);

        let mut read_committed = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *read_committed)
            .await
            .unwrap();
        let mut repository = PgPmsStayRoomRepository::new(&mut read_committed);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &assign_command,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PMS_STAY_ROOM_SERIALIZABLE_REQUIRED");
        drop(repository);
        read_committed.rollback().await.unwrap();

        let mut blocking = serializable(&holding).await;
        sqlx::query("UPDATE pms_properties SET updated_at=updated_at WHERE id='property-1'")
            .execute(&mut *blocking)
            .await
            .unwrap();
        let mut contended = serializable(&holding).await;
        let mut repository = PgPmsStayRoomRepository::new(&mut contended);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &assign_command,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PRE_SNAPSHOT_LOCK_CONTENDED");
        drop(repository);
        contended.rollback().await.unwrap();
        blocking.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_stay_room_sources(&mut tx).await.unwrap();
        let mut south = actor();
        south.board.branch_id = Some("south".into());
        let mut repository = PgPmsStayRoomRepository::new(&mut tx);
        let mut packages = Packages;
        let error = pms_stay_room(
            &south,
            &key,
            &assign_command,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        drop(repository);

        let missing_room = command(PmsStayRoomOperation::Assign, "stay-reserved", "missing", 1);
        let mut repository = PgPmsStayRoomRepository::new(&mut tx);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &missing_room,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_RECORD_NOT_FOUND");
        drop(repository);

        let wrong_type = command(
            PmsStayRoomOperation::Assign,
            "stay-reserved",
            "room-wrongtype",
            1,
        );
        let mut repository = PgPmsStayRoomRepository::new(&mut tx);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &wrong_type,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ROOM_TYPE");
        drop(repository);

        let overlap = command(
            PmsStayRoomOperation::Assign,
            "stay-reserved",
            "room-busy",
            1,
        );
        let mut repository = PgPmsStayRoomRepository::new(&mut tx);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &overlap,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ROOM_OVERLAP");
        drop(repository);

        let blocked = command(
            PmsStayRoomOperation::Assign,
            "stay-reserved",
            "room-block",
            1,
        );
        let mut repository = PgPmsStayRoomRepository::new(&mut tx);
        let mut packages = Packages;
        let error = pms_stay_room(
            &actor(),
            &key,
            &blocked,
            &mut repository,
            &mut packages,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ROOM_BLOCKED");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn bounded_task_reads_reject_more_than_the_command_can_hold() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        for index in 0..=MAX_TASKS {
            record(
                &holding,
                "property-1",
                &format!("task-bound-{index:04}"),
                "task",
                json!({
                    "roomCycle": 3, "roomId": "room-old", "type": "stayover",
                    "stayId": "stay-inhouse", "date": "2026-11-04", "assigneeId": "",
                    "priority": "normal", "state": "pending", "checklist": [],
                    "checked": [], "notes": "", "completedBy": "", "inspectedBy": ""
                }),
            )
            .await;
        }
        let move_command = command(PmsStayRoomOperation::Move, "stay-inhouse", "room-new", 1);
        let key = CommandKey::new("stay-move-bounded").unwrap();
        let mut tx = serializable(&holding).await;
        lock_stay_room_sources(&mut tx).await.unwrap();
        let error = execute(&mut tx, &key, &move_command, 5_000)
            .await
            .unwrap_err();
        let error = business(error);
        assert_eq!(error.kind, FailureKind::Unavailable);
        assert_eq!(error.code, "PMS_STAY_ROOM_READ_LIMIT");
        tx.rollback().await.unwrap();
    })
    .await;
}

const HTTP_HEADERS: &str = "Origin: https://acme.example.test\r\nIdempotency-Key: stay-assign-0001\r\nContent-Type: application/json\r\nx-erpx-context-company: company\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment: assignment-1\r\nx-erpx-context-assignment-version: 1\r\n";

struct StayRoomHttpBackend {
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
impl StayRoomHttpBackend {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl PmsStayRoomHttpBackend for StayRoomHttpBackend {
    fn command<'a>(
        &'a self,
        request: PmsStayRoomHttpCommandRequest<'a>,
    ) -> PmsStayRoomHttpFuture<'a, PmsStayRoomHttpResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            request.host,
            request.holding_slug,
            request.key.as_str(),
            request.command.operation().as_str(),
            request.command.property_id,
            request.command.stay_id,
            request.command.room_id,
            request.context.company_id,
            request.context.assignment_id,
            request.context.branch_slug.as_deref().unwrap_or("-"),
            request.context.department_slug.as_deref().unwrap_or("-"),
        ));
        let id = request.command.stay_id.clone();
        let property_id = request.command.property_id.clone();
        let version = request.command.expected_version + 1;
        Box::pin(async move {
            Ok(PmsStayRoomHttpResponse {
                data: json!({"id": id, "propertyId": property_id, "version": version}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn http_target(backend: Arc<StayRoomHttpBackend>) -> PmsStayRoomHttpTarget {
    PmsStayRoomHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

async fn http_start(
    backend: Arc<StayRoomHttpBackend>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(
            listener,
            pms_stay_room_router(vec![http_target(backend)]).unwrap(),
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
async fn http_command_target_retains_bounded_context_and_fails_closed() {
    let backend = Arc::new(StayRoomHttpBackend::new());
    let (address, task) = http_start(backend.clone()).await;
    let body = json!({
        "operation": "stay.assign",
        "propertyId": "property-1",
        "stayId": "stay-reserved",
        "roomId": "room-clean",
        "reason": "assign",
        "expectedVersion": 1,
        "expectedStayVersion": 1
    })
    .to_string();
    let (status, value, head) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room?branch=north",
        HTTP_HEADERS,
        &body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "stay-reserved");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(value["context"]["userId"], "user");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));
    assert!(head.contains("x-content-type-options: nosniff"));
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        [
            "acme.example.test:acme:stay-assign-0001:stay.assign:property-1:stay-reserved:room-clean:company:assignment-1:north:-"
        ]
    );

    let missing_assignment =
        HTTP_HEADERS.replace("x-erpx-context-assignment: assignment-1\r\n", "");
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        &missing_assignment,
        &body,
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(value["error"]["code"], "WORK_CONTEXT_CHANGED");

    let unknown_field = json!({
        "operation": "stay.assign", "propertyId": "property-1",
        "stayId": "stay-reserved", "roomId": "room-clean",
        "expectedVersion": 1, "unexpected": true
    })
    .to_string();
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        HTTP_HEADERS,
        &unknown_field,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PMS_STAY_ROOM_REQUEST_INVALID");

    let unknown_operation = body.replace("stay.assign", "stay.reassign");
    let (status, _, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        HTTP_HEADERS,
        &unknown_operation,
    )
    .await;
    assert_eq!(status, 422);

    let wrong_origin = HTTP_HEADERS.replace(
        "Origin: https://acme.example.test",
        "Origin: https://evil.example.test",
    );
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        &wrong_origin,
        &body,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let no_key = HTTP_HEADERS.replace("Idempotency-Key: stay-assign-0001\r\n", "");
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        &no_key,
        &body,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, _, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room?unknown=1",
        HTTP_HEADERS,
        &body,
    )
    .await;
    assert_eq!(status, 422);
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/other/pms/stay-room",
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
        serve(empty, pms_stay_room_router(Vec::new()).unwrap())
            .await
            .unwrap();
    });
    let (status, value, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/stay-room",
        HTTP_HEADERS,
        "{}",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    task.abort();

    let backend = Arc::new(StayRoomHttpBackend::new());
    let target = || http_target(backend.clone());
    assert!(pms_stay_room_router(vec![target()]).is_ok());
    assert!(pms_stay_room_router(vec![target(), target()]).is_err());
    assert!(
        pms_stay_room_router(vec![PmsStayRoomHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
    assert!(
        pms_stay_room_router(vec![PmsStayRoomHttpTarget {
            origin: "https://acme.example.test/path".into(),
            ..target()
        }])
        .is_err()
    );
}
