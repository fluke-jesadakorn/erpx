//! Native PMS room/housekeeping commands over real migrated PostgreSQL. These
//! are owner command receipts, not browser acceptance or HTTP evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::room_board::PmsBoardActor;
use erpx_server_pms::public::room_command::{
    MAX_ROOMS, PgPmsRoomCommandRepository, PmsBoardFuture, PmsRoomAssigneeScopePort,
    PmsRoomCommand, PmsRoomCommandActor, PmsRoomCommandHttpBackend,
    PmsRoomCommandHttpCommandRequest, PmsRoomCommandHttpFuture, PmsRoomCommandHttpResponse,
    PmsRoomCommandHttpTarget, PmsRoomCommandInput, PmsRoomCommandOperation, PmsRoomSaveInput,
    PmsTaskPriority, lock_room_command_sources, pms_room_command, pms_room_command_router,
    room_command_request_hash,
};
use erpx_server_runtime::public::http_support::serve;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
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

async fn property(pool: &DatabasePool, id: &str, company: &str, branch: &str, state: &str) {
    sqlx::query(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES($1,$2,$3,$4,'2026-11-04',$5,$6,1,'maker',1)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(format!("Hotel {id}"))
    .bind(state)
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

async fn seed_rooms(pool: &DatabasePool, property_id: &str) {
    record(
        pool,
        property_id,
        &format!("{property_id}-type-1"),
        "room_type",
        json!({"name":"Deluxe","capacity":2,"amenities":""}),
    )
    .await;
    record(
        pool,
        property_id,
        &format!("{property_id}-type-2"),
        "room_type",
        json!({"name":"Suite","capacity":4,"amenities":""}),
    )
    .await;
    for (id, number, cleanliness, dnd) in [
        ("room-a", "101", "clean", false),
        ("room-b", "102", "inspected", false),
        ("room-busy", "103", "clean", false),
        ("room-idle", "104", "clean", false),
        ("room-dnd", "105", "clean", false),
        ("room-task", "106", "dirty", false),
    ] {
        record(
            pool,
            property_id,
            id,
            "room",
            json!({
                "number": number,
                "typeId": format!("{property_id}-type-1"),
                "building": "Main",
                "floor": "1",
                "active": true,
                "cleanliness": cleanliness,
                "cleaningCycle": 0,
                "dnd": dnd,
                "observedOccupied": null
            }),
        )
        .await;
    }
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    property(pool, "property-1", "company", "north", "open").await;
    property(pool, "property-south", "company", "south", "open").await;
    property(pool, "property-foreign", "other", "other", "open").await;
    property(pool, "property-setup", "company", "setupplace", "setup").await;
    property(pool, "property-sold", "sold", "main", "open").await;
    seed_rooms(pool, "property-1").await;
    record(
        pool,
        "property-1",
        "stay-inhouse",
        "stay",
        json!({
            "reservationId": "reservation-1", "guestId": "guest-1",
            "typeId": "property-1-type-1", "rateId": "rate-1", "roomId": "room-busy",
            "arrival": "2026-11-03", "departure": "2026-11-06", "adults": 2,
            "state": "in_house", "guestAccountId": "account-1", "prices": [],
            "segments": [{"roomId":"room-busy","from":"2026-11-03","to":"2026-11-06"}]
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-reserved",
        "stay",
        json!({
            "reservationId": "reservation-2", "guestId": "guest-2",
            "typeId": "property-1-type-1", "rateId": "rate-1", "roomId": "",
            "arrival": "2026-11-04", "departure": "2026-11-06", "adults": 2,
            "state": "reserved", "guestAccountId": "account-2", "prices": [],
            "segments": []
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-assigned",
        "stay",
        json!({
            "reservationId": "reservation-3", "guestId": "guest-3",
            "typeId": "property-1-type-1", "rateId": "rate-1", "roomId": "room-a",
            "arrival": "2026-11-04", "departure": "2026-11-05", "adults": 1,
            "state": "reserved", "guestAccountId": "account-3", "prices": [],
            "segments": [{"roomId":"room-a","from":"2026-11-04","to":"2026-11-05"}]
        }),
    )
    .await;
    for (id, room, state, completed_by) in [
        ("task-open", "room-task", "pending", ""),
        ("task-clean", "room-b", "clean", "other-supervisor"),
    ] {
        record(
            pool,
            "property-1",
            id,
            "task",
            json!({
                "roomCycle": 0, "roomId": room, "type": "stayover", "stayId": "",
                "date": "2026-11-04", "assigneeId": "", "priority": "normal",
                "state": state,
                "checklist": ["Bathroom", "Bed and linen"],
                "checked": [], "notes": "", "completedBy": completed_by,
                "inspectedBy": ""
            }),
        )
        .await;
    }
    // A functionally separate property for the retained capacity guard: two
    // active rooms, two reserved stays on the same type.
    record(
        pool,
        "property-sold",
        "sold-type-1",
        "room_type",
        json!({"name":"Sold","capacity":2,"amenities":""}),
    )
    .await;
    for (id, number) in [("sold-room-a", "9"), ("sold-room-b", "10")] {
        record(
            pool,
            "property-sold",
            id,
            "room",
            json!({
                "number": number, "typeId": "sold-type-1", "building": "", "floor": "",
                "active": true, "cleanliness": "dirty", "cleaningCycle": 0,
                "dnd": false, "observedOccupied": null
            }),
        )
        .await;
    }
    for (id, reservation, guest) in [("stay-sold-1", "r1", "g1"), ("stay-sold-2", "r2", "g2")] {
        record(
            pool,
            "property-sold",
            id,
            "stay",
            json!({
                "reservationId": reservation, "guestId": guest, "typeId": "sold-type-1",
                "rateId": "rate-1", "roomId": "", "arrival": "2026-11-04",
                "departure": "2026-11-06", "adults": 1, "state": "reserved",
                "guestAccountId": guest, "prices": [], "segments": []
            }),
        )
        .await;
    }
    // A setup-state property for the retained `PMS_SETUP_REQUIRED` gate.
    record(
        pool,
        "property-setup",
        "setup-type-1",
        "room_type",
        json!({"name":"Setup","capacity":2,"amenities":""}),
    )
    .await;
    record(
        pool,
        "property-setup",
        "setup-room-a",
        "room",
        json!({
            "number": "1", "typeId": "setup-type-1", "building": "", "floor": "",
            "active": true, "cleanliness": "clean", "cleaningCycle": 0,
            "dnd": false, "observedOccupied": null
        }),
    )
    .await;
}

fn actor_with(company: &str, branch: &str, permissions: &[&str]) -> PmsRoomCommandActor {
    PmsRoomCommandActor {
        board: PmsBoardActor {
            user_id: "user".into(),
            company_id: company.into(),
            branch_id: Some(branch.into()),
            department_id: None,
            permissions: permissions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            modules: vec!["pms".into()],
            expires_at: 9_007_199_254_740_991,
        },
        assignment_id: "assignment-1".into(),
        role: "housekeeping".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn actor() -> PmsRoomCommandActor {
    actor_with(
        "company",
        "north",
        &[
            "pms.read",
            "pms.configure",
            "pms.housekeeping",
            "pms.inspect",
        ],
    )
}

fn save(
    room_id: Option<&str>,
    number: &str,
    type_id: &str,
    active: bool,
    expected: i64,
) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Save,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsRoomCommandInput::Save(PmsRoomSaveInput {
            room_id: room_id.map(str::to_owned),
            number: number.into(),
            type_id: type_id.into(),
            building: "Main".into(),
            floor: "2".into(),
            active,
        }),
    }
}

fn dnd(room_id: &str, expected: i64) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Dnd,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsRoomCommandInput::Dnd {
            room_id: room_id.into(),
            dnd: true,
        },
    }
}

fn observe(room_id: &str, occupied: bool, expected: i64) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Observe,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsRoomCommandInput::Observe {
            room_id: room_id.into(),
            occupied,
        },
    }
}

fn task(
    operation: PmsRoomCommandOperation,
    task_id: &str,
    checked: &[&str],
    expected: i64,
) -> PmsRoomCommand {
    let input = match operation {
        PmsRoomCommandOperation::TaskAssign => PmsRoomCommandInput::TaskAssign {
            task_id: task_id.into(),
            assignee_id: "assignee-1".into(),
            priority: PmsTaskPriority::Urgent,
        },
        PmsRoomCommandOperation::TaskStart => PmsRoomCommandInput::TaskStart {
            task_id: task_id.into(),
            checked: Vec::new(),
            notes: "starting".into(),
        },
        PmsRoomCommandOperation::TaskComplete => PmsRoomCommandInput::TaskComplete {
            task_id: task_id.into(),
            checked: checked.iter().map(|value| (*value).to_owned()).collect(),
            notes: "done".into(),
        },
        PmsRoomCommandOperation::TaskInspect => PmsRoomCommandInput::TaskInspect {
            task_id: task_id.into(),
        },
        PmsRoomCommandOperation::Save
        | PmsRoomCommandOperation::Dnd
        | PmsRoomCommandOperation::Observe => unreachable!(),
    };
    PmsRoomCommand {
        operation,
        property_id: "property-1".into(),
        expected_version: expected,
        input,
    }
}

struct Assignees(bool);
impl PmsRoomAssigneeScopePort for Assignees {
    type Error = BusinessError;
    fn assignee_in_scope<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: &'a str,
        _assignee_id: &'a str,
        _now: i64,
    ) -> PmsBoardFuture<'a, bool, Self::Error> {
        let allowed = self.0;
        Box::pin(async move { Ok(allowed) })
    }
}

async fn execute(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key: &CommandKey,
    command: &PmsRoomCommand,
    now: i64,
) -> Result<
    erpx_server_pms::public::room_command::PmsRoomCommandResult,
    PgTransactionError<BusinessError>,
> {
    execute_as(tx, &actor(), key, command, now, true).await
}

async fn execute_as(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    command: &PmsRoomCommand,
    now: i64,
    assignee_allowed: bool,
) -> Result<
    erpx_server_pms::public::room_command::PmsRoomCommandResult,
    PgTransactionError<BusinessError>,
> {
    let mut repository = PgPmsRoomCommandRepository::new(&mut **tx);
    let mut assignees = Assignees(assignee_allowed);
    pms_room_command(actor, key, command, &mut repository, &mut assignees, now).await
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

async fn json_value(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, id: &str) -> Value {
    let raw: String = sqlx::query_scalar("SELECT data_json FROM pms_records WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

async fn record_version(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, id: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM pms_records WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

async fn property_version(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, id: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM pms_properties WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

#[tokio::test]
async fn save_creates_room_and_retains_receipt_audit_outbox_then_replays() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("room-save-create-0001").unwrap();
        let create = save(None, "107", "property-1-type-1", true, 1);
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &create, 5_000).await.unwrap();
        assert_eq!(result.property_id, "property-1");
        assert_eq!(result.version, 2);
        assert!(result.id.starts_with("pms-room-"));
        let room = json_value(&mut tx, &result.id).await;
        assert_eq!(room["number"], "107");
        assert_eq!(room["typeId"], "property-1-type-1");
        assert_eq!(room["cleanliness"], "dirty");
        assert_eq!(room["dnd"], false);
        assert!(room["observedOccupied"].is_null());
        assert_eq!(record_version(&mut tx, &result.id).await, 1);
        assert_eq!(property_version(&mut tx, "property-1").await, 2);

        let receipt = sqlx::query(
            "SELECT operation,actor_id,request_hash,record_id,result_json \
             FROM command_receipts WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "pms.room.save");
        assert_eq!(receipt.get::<String, _>("actor_id"), "user");
        assert_eq!(receipt.get::<String, _>("record_id"), "property-1");
        let mut normalized = create.clone();
        normalized.normalize();
        assert_eq!(
            receipt.get::<String, _>("request_hash"),
            room_command_request_hash(&actor(), &normalized).unwrap()
        );

        let audit = sqlx::query(
            "SELECT action,record_type,record_id,record_version \
             FROM audit_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "pms.room.save");
        assert_eq!(audit.get::<String, _>("record_type"), "room");
        assert_eq!(audit.get::<String, _>("record_id"), result.id);
        assert_eq!(audit.get::<i64, _>("record_version"), 2);

        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version,payload_json \
             FROM outbox_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "pms.room.save");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "room");
        assert_eq!(outbox.get::<String, _>("aggregate_id"), result.id);
        assert_eq!(outbox.get::<i64, _>("aggregate_version"), 2);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["roomId"], result.id);
        assert_eq!(payload["state"], "dirty");
        tx.commit().await.unwrap();

        let mut replay_tx = serializable(&holding).await;
        lock_room_command_sources(&mut replay_tx).await.unwrap();
        let replayed = execute(&mut replay_tx, &key, &create, 9_000).await.unwrap();
        assert_eq!(replayed, result);
        for sql in [
            "SELECT count(*) FROM command_receipts WHERE command_id=$1",
            "SELECT count(*) FROM audit_events WHERE command_id=$1",
            "SELECT count(*) FROM outbox_events WHERE command_id=$1",
        ] {
            let count: i64 = sqlx::query_scalar(sql)
                .bind(key.as_str())
                .fetch_one(&mut *replay_tx)
                .await
                .unwrap();
            assert_eq!(count, 1, "{sql}");
        }
        let conflicting = save(None, "108", "property-1-type-1", true, 1);
        let error = execute(&mut replay_tx, &key, &conflicting, 9_001)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "IDEMPOTENCY_CONFLICT");
        replay_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn save_update_guards_assigned_rooms_and_duplicate_numbers() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();

        let duplicate = save(None, "101", "property-1-type-1", true, 1);
        let error = execute(
            &mut tx,
            &CommandKey::new("room-save-dup-0001").unwrap(),
            &duplicate,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ROOM_NUMBER_EXISTS");

        let deactivate = save(Some("room-a"), "101", "property-1-type-1", false, 1);
        let error = execute(
            &mut tx,
            &CommandKey::new("room-save-inuse-0001").unwrap(),
            &deactivate,
            5_000,
        )
        .await
        .unwrap_err();
        let error = business(error);
        assert_eq!(error.code, "PMS_ROOM_IN_USE");
        assert_eq!(error.message, "Room is assigned to a stay");
        assert_eq!(record_version(&mut tx, "room-a").await, 1);

        let update = save(Some("room-a"), "101", "property-1-type-1", true, 1);
        let result = execute(
            &mut tx,
            &CommandKey::new("room-save-update-0001").unwrap(),
            &update,
            5_000,
        )
        .await
        .unwrap();
        assert_eq!(result.id, "room-a");
        assert_eq!(result.version, 2);
        let room = json_value(&mut tx, "room-a").await;
        assert_eq!(room["floor"], "2");
        assert_eq!(room["id"], "room-a");
        assert_eq!(room["cleaningCycle"], 0);
        assert_eq!(record_version(&mut tx, "room-a").await, 2);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn dnd_and_observe_persist_room_state_and_one_discrepancy_task() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();

        let result = execute(
            &mut tx,
            &CommandKey::new("room-dnd-0001").unwrap(),
            &dnd("room-dnd", 1),
            5_000,
        )
        .await
        .unwrap();
        assert_eq!(result.id, "room-dnd");
        assert_eq!(json_value(&mut tx, "room-dnd").await["dnd"], true);

        let result = execute(
            &mut tx,
            &CommandKey::new("room-observe-0001").unwrap(),
            &observe("room-idle", true, 2),
            5_000,
        )
        .await
        .unwrap();
        assert_eq!(result.version, 3);
        let room = json_value(&mut tx, "room-idle").await;
        assert_eq!(room["observedOccupied"], true);
        let tasks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pms_records WHERE kind='task' AND data_json::jsonb->>'roomId'='room-idle'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(tasks, 1);

        execute(
            &mut tx,
            &CommandKey::new("room-observe-0002").unwrap(),
            &observe("room-idle", false, 3),
            5_000,
        )
        .await
        .unwrap();
        let room = json_value(&mut tx, "room-idle").await;
        assert_eq!(room["observedOccupied"], false);
        let tasks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pms_records WHERE kind='task' AND data_json::jsonb->>'roomId'='room-idle'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(tasks, 1, "the retained task() dedupe reuses the cycle task");
        let discrepancy: Value = serde_json::from_str(
            &sqlx::query_scalar::<_, String>(
                "SELECT data_json FROM pms_records WHERE kind='task' AND data_json::jsonb->>'roomId'='room-idle'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(discrepancy["type"], "discrepancy");
        assert_eq!(discrepancy["priority"], "normal");
        assert_eq!(discrepancy["state"], "pending");
        assert_eq!(discrepancy["date"], "2026-11-04");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn task_commands_run_the_retained_state_machine_and_assignee_scope() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();

        let assign = task(PmsRoomCommandOperation::TaskAssign, "task-open", &[], 1);
        let error = execute_as(
            &mut tx,
            &actor(),
            &CommandKey::new("task-assign-denied-0001").unwrap(),
            &assign,
            5_000,
            false,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_ASSIGNEE_SCOPE");

        execute_as(
            &mut tx,
            &actor(),
            &CommandKey::new("task-assign-0001").unwrap(),
            &assign,
            5_000,
            true,
        )
        .await
        .unwrap();
        let assigned = json_value(&mut tx, "task-open").await;
        assert_eq!(assigned["assigneeId"], "assignee-1");
        assert_eq!(assigned["priority"], "urgent");

        let error = execute(
            &mut tx,
            &CommandKey::new("task-start-0001").unwrap(),
            &task(PmsRoomCommandOperation::TaskStart, "task-open", &[], 2),
            5_000,
        )
        .await
        .unwrap_err();
        // The retained start requires the task to be unassigned or owned by the
        // caller; a claimed task belongs to the assignee.
        assert_eq!(business(error).code, "PMS_TASK_ASSIGNMENT");

        sqlx::query(
            "UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{assigneeId}','\"\"')::text WHERE id='task-open'",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        execute(
            &mut tx,
            &CommandKey::new("task-start-0002").unwrap(),
            &task(PmsRoomCommandOperation::TaskStart, "task-open", &[], 2),
            5_000,
        )
        .await
        .unwrap();
        let started = json_value(&mut tx, "task-open").await;
        assert_eq!(started["state"], "working");
        assert_eq!(started["assigneeId"], "user");

        let error = execute(
            &mut tx,
            &CommandKey::new("task-complete-0001").unwrap(),
            &task(
                PmsRoomCommandOperation::TaskComplete,
                "task-open",
                &["Bathroom"],
                3,
            ),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_CHECKLIST_REQUIRED");

        execute(
            &mut tx,
            &CommandKey::new("task-complete-0002").unwrap(),
            &task(
                PmsRoomCommandOperation::TaskComplete,
                "task-open",
                &["Bathroom", "Bed and linen"],
                3,
            ),
            5_000,
        )
        .await
        .unwrap();
        let completed = json_value(&mut tx, "task-open").await;
        assert_eq!(completed["state"], "clean");
        assert_eq!(completed["completedBy"], "user");
        assert_eq!(
            json_value(&mut tx, "room-task").await["cleanliness"],
            "clean"
        );

        let error = execute(
            &mut tx,
            &CommandKey::new("task-inspect-same-0001").unwrap(),
            &task(PmsRoomCommandOperation::TaskInspect, "task-open", &[], 4),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_INDEPENDENT_INSPECTOR");

        let inspected = execute(
            &mut tx,
            &CommandKey::new("task-inspect-0001").unwrap(),
            &task(PmsRoomCommandOperation::TaskInspect, "task-clean", &[], 4),
            5_000,
        )
        .await
        .unwrap();
        assert_eq!(inspected.id, "task-clean");
        let inspected_task = json_value(&mut tx, "task-clean").await;
        assert_eq!(inspected_task["state"], "done");
        assert_eq!(inspected_task["inspectedBy"], "user");
        assert_eq!(
            json_value(&mut tx, "room-b").await["cleanliness"],
            "inspected"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn save_capacity_reconciliation_refuses_an_oversold_type() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let sold = actor_with("sold", "main", &["pms.read", "pms.configure"]);
        let deactivate = PmsRoomCommand {
            operation: PmsRoomCommandOperation::Save,
            property_id: "property-sold".into(),
            expected_version: 1,
            input: PmsRoomCommandInput::Save(PmsRoomSaveInput {
                room_id: Some("sold-room-b".into()),
                number: "10".into(),
                type_id: "sold-type-1".into(),
                building: String::new(),
                floor: String::new(),
                active: false,
            }),
        };
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();
        let error = execute_as(
            &mut tx,
            &sold,
            &CommandKey::new("room-save-sold-0001").unwrap(),
            &deactivate,
            5_000,
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_SOLD_OUT");
        assert_eq!(record_version(&mut tx, "sold-room-b").await, 1);
        assert_eq!(property_version(&mut tx, "property-sold").await, 1);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn scope_transaction_lock_state_and_missing_facts_fail_closed() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let create = save(None, "107", "property-1-type-1", true, 1);
        let key = CommandKey::new("room-save-scope-0001").unwrap();

        let mut connection = holding.raw().acquire().await.unwrap();
        let mut repository = PgPmsRoomCommandRepository::new(&mut connection);
        let mut assignees = Assignees(true);
        let error = pms_room_command(
            &actor(),
            &key,
            &create,
            &mut repository,
            &mut assignees,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PMS_ROOM_COMMAND_TRANSACTION_REQUIRED");
        drop(repository);
        drop(connection);

        let mut read_committed = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *read_committed)
            .await
            .unwrap();
        let mut repository = PgPmsRoomCommandRepository::new(&mut read_committed);
        let mut assignees = Assignees(true);
        let error = pms_room_command(
            &actor(),
            &key,
            &create,
            &mut repository,
            &mut assignees,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PMS_ROOM_COMMAND_SERIALIZABLE_REQUIRED");
        drop(repository);
        read_committed.rollback().await.unwrap();

        let mut blocking = serializable(&holding).await;
        sqlx::query("UPDATE pms_properties SET updated_at=updated_at WHERE id='property-1'")
            .execute(&mut *blocking)
            .await
            .unwrap();
        let mut contended = serializable(&holding).await;
        let mut repository = PgPmsRoomCommandRepository::new(&mut contended);
        let mut assignees = Assignees(true);
        let error = pms_room_command(
            &actor(),
            &key,
            &create,
            &mut repository,
            &mut assignees,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(persistence(error), "PRE_SNAPSHOT_LOCK_CONTENDED");
        drop(repository);
        contended.rollback().await.unwrap();
        blocking.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();

        let south = actor_with("company", "south", &["pms.read", "pms.configure"]);
        let error = execute_as(&mut tx, &south, &key, &create, 5_000, true)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");

        let missing_type = save(None, "107", "missing-type", true, 1);
        let error = execute(&mut tx, &key, &missing_type, 5_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_RECORD_NOT_FOUND");

        let missing_room = dnd("missing-room", 1);
        let error = execute(&mut tx, &key, &missing_room, 5_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_RECORD_NOT_FOUND");
        tx.rollback().await.unwrap();

        let mut closing = serializable(&holding).await;
        sqlx::query("UPDATE pms_properties SET state='closing' WHERE id='property-1'")
            .execute(&mut *closing)
            .await
            .unwrap();
        let mut repository = PgPmsRoomCommandRepository::new(&mut closing);
        let mut assignees = Assignees(true);
        let error = pms_room_command(
            &actor(),
            &key,
            &create,
            &mut repository,
            &mut assignees,
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_DAY_CLOSING");
        drop(repository);
        closing.rollback().await.unwrap();

        let mut setup = serializable(&holding).await;
        lock_room_command_sources(&mut setup).await.unwrap();
        let setup_actor = actor_with(
            "company",
            "setupplace",
            &["pms.read", "pms.configure", "pms.housekeeping"],
        );
        let mut setup_dnd = dnd("setup-room-a", 1);
        setup_dnd.property_id = "property-setup".into();
        let error = execute_as(&mut setup, &setup_actor, &key, &setup_dnd, 5_000, true)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_SETUP_REQUIRED");
        setup.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn bounded_room_reads_reject_more_than_the_command_can_hold() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        for index in 0..=MAX_ROOMS {
            record(
                &holding,
                "property-1",
                &format!("room-bound-{index:04}"),
                "room",
                json!({
                    "number": format!("9{index:04}"),
                    "typeId": "property-1-type-1",
                    "building": "", "floor": "", "active": false,
                    "cleanliness": "dirty", "cleaningCycle": 0, "dnd": false,
                    "observedOccupied": null
                }),
            )
            .await;
        }
        let mut tx = serializable(&holding).await;
        lock_room_command_sources(&mut tx).await.unwrap();
        let error = execute(
            &mut tx,
            &CommandKey::new("room-save-bounded-0001").unwrap(),
            &save(None, "999999", "property-1-type-1", true, 1),
            5_000,
        )
        .await
        .unwrap_err();
        let error = business(error);
        assert_eq!(error.code, "PMS_ROOM_COMMAND_READ_LIMIT");
        tx.rollback().await.unwrap();
    })
    .await;
}

const HTTP_HEADERS: &str = "Origin: https://acme.example.test\r\nIdempotency-Key: room-command-0001\r\nContent-Type: application/json\r\nx-erpx-context-company: company\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment: assignment-1\r\nx-erpx-context-assignment-version: 1\r\n";

struct RoomHttpBackend {
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
impl RoomHttpBackend {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl PmsRoomCommandHttpBackend for RoomHttpBackend {
    fn command<'a>(
        &'a self,
        request: PmsRoomCommandHttpCommandRequest<'a>,
    ) -> PmsRoomCommandHttpFuture<'a, PmsRoomCommandHttpResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(format!(
            "{}:{}:{}:{}:{}:{}",
            request.host,
            request.holding_slug,
            request.key.as_str(),
            request.command.operation().as_str(),
            request.context.company_id,
            request.context.assignment_id,
        ));
        let id = request
            .command
            .input
            .target_id()
            .unwrap_or("property-1")
            .to_owned();
        let property_id = request.command.property_id.clone();
        let version = request.command.expected_version + 1;
        Box::pin(async move {
            Ok(PmsRoomCommandHttpResponse {
                data: json!({"id": id, "propertyId": property_id, "version": version}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn http_target(backend: Arc<RoomHttpBackend>) -> PmsRoomCommandHttpTarget {
    PmsRoomCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

async fn http_start(
    backend: Arc<RoomHttpBackend>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(
            listener,
            pms_room_command_router(vec![http_target(backend)]).unwrap(),
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
) -> (u16, Value) {
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
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn http_room_command_retains_bounded_context_and_fails_closed() {
    let backend = Arc::new(RoomHttpBackend::new());
    let (address, task) = http_start(backend.clone()).await;
    let save_body = json!({
        "operation": "room.save",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "roomId": "room-a",
        "number": " 101 ",
        "typeId": "property-1-type-1",
        "building": "Main",
        "floor": "1",
        "active": true
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command?branch=north",
        HTTP_HEADERS,
        &save_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "room-a");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        ["acme.example.test:acme:room-command-0001:room.save:company:assignment-1"]
    );

    let dnd_body = json!({
        "operation": "room.dnd",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "roomId": "room-a",
        "dnd": true
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        HTTP_HEADERS,
        &dnd_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 2);

    let unknown_field = json!({
        "operation": "room.dnd", "propertyId": "property-1", "expectedVersion": 1,
        "roomId": "room-a", "dnd": true, "unexpected": true
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        HTTP_HEADERS,
        &unknown_field,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PMS_ROOM_COMMAND_REQUEST_INVALID");

    let missing_task = json!({
        "operation": "task.inspect", "propertyId": "property-1", "expectedVersion": 1
    })
    .to_string();
    let (status, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        HTTP_HEADERS,
        &missing_task,
    )
    .await;
    assert_eq!(status, 422);

    let unknown_operation = save_body.replace("room.save", "room.rebuild");
    let (status, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        HTTP_HEADERS,
        &unknown_operation,
    )
    .await;
    assert_eq!(status, 422);

    let wrong_origin = HTTP_HEADERS.replace(
        "Origin: https://acme.example.test",
        "Origin: https://evil.example.test",
    );
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        &wrong_origin,
        &save_body,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let no_key = HTTP_HEADERS.replace("Idempotency-Key: room-command-0001\r\n", "");
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        &no_key,
        &save_body,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/other/pms/room-command",
        HTTP_HEADERS,
        &save_body,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn http_room_command_router_is_fail_closed_when_unconfigured_or_invalid() {
    let empty = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = empty.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(empty, pms_room_command_router(Vec::new()).unwrap())
            .await
            .unwrap();
    });
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/room-command",
        HTTP_HEADERS,
        "{}",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    task.abort();

    let backend = Arc::new(RoomHttpBackend::new());
    let target = || http_target(backend.clone());
    assert!(pms_room_command_router(vec![target()]).is_ok());
    assert!(pms_room_command_router(vec![target(), target()]).is_err());
    assert!(
        pms_room_command_router(vec![PmsRoomCommandHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
}
