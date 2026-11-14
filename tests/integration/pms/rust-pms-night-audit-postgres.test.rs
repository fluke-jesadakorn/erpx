//! Native PMS day-close commands over real migrated PostgreSQL. These are
//! owner command receipts, not browser acceptance or HTTP evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::night_audit::{
    MAX_STAYS, PgPmsNightAuditRepository, PmsBoardFuture, PmsNightAuditActor, PmsNightAuditCommand,
    PmsNightAuditGuardPort, PmsNightAuditOperation, PmsNightAuditRepository, PmsNightAuditResult,
    PmsNightAuditSettlement, PmsNightControlScope, PmsNightDayControlFacts, PmsNightSetupFacts,
    PmsNightSetupScope, lock_night_audit_sources, night_audit_request_hash, pms_night_audit,
    pms_night_audit_step, read_night_audit_jobs, run_night_audit_job,
};
use erpx_server_pms::public::room_board::PmsBoardActor;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::{Value, json};
use sqlx::Row;

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
    let mut accounts = serde_json::Map::new();
    for kind in [
        "cash",
        "bank",
        "edc",
        "receivable",
        "advance",
        "deposit",
        "revenue",
        "service",
        "tax",
        "prepaidTax",
        "inventory",
        "consumption",
        "commission",
        "cashDifference",
    ] {
        accounts.insert(kind.to_owned(), json!(format!("account-{kind}")));
    }
    json!({
        "country": "TH",
        "currency": "THB",
        "minorUnit": 2,
        "timezone": "Asia/Bangkok",
        "checkIn": "14:00",
        "checkOut": "12:00",
        "cancellationPolicy": "",
        "chartId": "chart",
        "accounts": accounts,
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

async fn seed_ready(pool: &DatabasePool, property_id: &str) {
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
        &format!("{property_id}-room-a"),
        "room",
        json!({
            "number": "101", "typeId": format!("{property_id}-type-1"), "building": "Main",
            "floor": "1", "active": true, "cleanliness": "clean", "cleaningCycle": 0,
            "dnd": false
        }),
    )
    .await;
    record(
        pool,
        property_id,
        &format!("{property_id}-rate-1"),
        "rate",
        json!({
            "name": "Flexible", "typeId": format!("{property_id}-type-1"),
            "from": "2026-01-01", "to": "2027-12-31", "price": 100, "extraAdult": 0,
            "includedGuests": 2, "minNights": 1, "maxNights": 30, "stopSell": false,
            "closedArrival": false, "closedDeparture": false, "market": "public",
            "partyId": "", "active": true
        }),
    )
    .await;
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    property(pool, "property-1", "company", "north", "open").await;
    property(
        pool,
        "property-closing",
        "company",
        "closingplace",
        "closing",
    )
    .await;
    property(pool, "property-foreign", "other", "north", "open").await;
    property(pool, "property-setup", "company", "setup", "setup").await;
    seed_ready(pool, "property-1").await;
    seed_ready(pool, "property-closing").await;
    seed_ready(pool, "property-setup").await;
}

fn actor_with(company: &str, branch: &str, permissions: &[&str]) -> PmsNightAuditActor {
    PmsNightAuditActor {
        board: PmsBoardActor {
            user_id: "night-auditor".into(),
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
        role: "pms_night_auditor".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn actor() -> PmsNightAuditActor {
    actor_with(
        "company",
        "north",
        &["pms.read", "pms.audit", "pms.reports"],
    )
}

/// A company-wide Night Auditor. The retained scope allows a selected Company
/// without a branch area, which is how one auditor covers several hotels.
fn company_actor() -> PmsNightAuditActor {
    let mut actor = actor();
    actor.board.branch_id = None;
    actor
}

struct Guard {
    controls: PmsNightDayControlFacts,
    setup: PmsNightSetupFacts,
}

impl Guard {
    fn clear() -> Self {
        Self {
            controls: PmsNightDayControlFacts::default(),
            setup: PmsNightSetupFacts::default(),
        }
    }
}

impl PmsNightAuditGuardPort for Guard {
    type Error = BusinessError;
    fn day_controls<'a>(
        &'a mut self,
        _scope: &'a PmsNightControlScope,
        _now: i64,
    ) -> PmsBoardFuture<'a, PmsNightDayControlFacts, Self::Error> {
        let controls = self.controls;
        Box::pin(async move { Ok(controls) })
    }
    fn setup<'a>(
        &'a mut self,
        _scope: &'a PmsNightSetupScope,
        _now: i64,
    ) -> PmsBoardFuture<'a, PmsNightSetupFacts, Self::Error> {
        let setup = self.setup.clone();
        Box::pin(async move { Ok(setup) })
    }
}

fn start(property_id: &str, expected: i64) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Start,
        property_id: property_id.into(),
        expected_version: Some(expected),
        night_id: None,
        worker: false,
    }
}

fn resume(property_id: &str, night_id: &str, expected: i64) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Resume,
        property_id: property_id.into(),
        expected_version: Some(expected),
        night_id: Some(night_id.into()),
        worker: false,
    }
}

fn step(property_id: &str, night_id: &str) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Step,
        property_id: property_id.into(),
        expected_version: None,
        night_id: Some(night_id.into()),
        worker: true,
    }
}

async fn execute(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    now: i64,
    guard: &mut Guard,
) -> Result<PmsNightAuditResult, PgTransactionError<BusinessError>> {
    let actor = actor();
    let mut repository = PgPmsNightAuditRepository::new(&mut **tx);
    pms_night_audit(&actor, key, command, &mut repository, guard, now).await
}

async fn execute_company(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    now: i64,
    guard: &mut Guard,
) -> Result<PmsNightAuditResult, PgTransactionError<BusinessError>> {
    execute_as(tx, &company_actor(), key, command, now, guard).await
}

async fn execute_as(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    now: i64,
    guard: &mut Guard,
) -> Result<PmsNightAuditResult, PgTransactionError<BusinessError>> {
    let mut repository = PgPmsNightAuditRepository::new(&mut **tx);
    pms_night_audit(actor, key, command, &mut repository, guard, now).await
}

async fn execute_step(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    command: &PmsNightAuditCommand,
    now: i64,
) -> Result<PmsNightAuditResult, PgTransactionError<BusinessError>> {
    let mut repository = PgPmsNightAuditRepository::new(&mut **tx);
    pms_night_audit_step(actor, key, command, &mut repository, now).await
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

async fn property_row(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    id: &str,
) -> (String, String, i64) {
    let row = sqlx::query("SELECT state,business_date,version FROM pms_properties WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    (
        row.get("state"),
        row.get("business_date"),
        row.get("version"),
    )
}

async fn job_row(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    id: &str,
) -> Option<(String, String, String)> {
    let row =
        sqlx::query("SELECT state,business_date,actor_json,error_code FROM pms_jobs WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .unwrap()?;
    Some((
        row.get("state"),
        row.get("business_date"),
        row.get::<Option<String>, _>("error_code")
            .unwrap_or_default(),
    ))
}

async fn count(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    sql: &'static str,
    id: &str,
) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

#[tokio::test]
async fn start_persists_the_night_closing_property_and_job_then_replays() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let key = CommandKey::new("night-start-0001").unwrap();
        let command = start("property-1", 1);
        let mut guard = Guard::clear();
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();
        let result = execute(&mut tx, &key, &command, 5_000, &mut guard)
            .await
            .unwrap();
        assert_eq!(result.property_id, "property-1");
        assert_eq!(result.version, 2);
        assert!(result.id.starts_with("pms-night-"));
        let night = json_value(&mut tx, &result.id).await;
        assert_eq!(night["date"], "2026-11-04");
        assert_eq!(night["state"], "pending");
        assert_eq!(night["initiatedBy"], "night-auditor");
        assert_eq!(night["message"], "");
        assert_eq!(
            property_row(&mut tx, "property-1").await,
            ("closing".to_owned(), "2026-11-04".to_owned(), 2)
        );
        let (state, business_date, error) = job_row(&mut tx, &result.id).await.unwrap();
        assert_eq!(state, "pending");
        assert_eq!(business_date, "2026-11-04");
        assert_eq!(error, "");
        let actor_json: String = sqlx::query_scalar("SELECT actor_json FROM pms_jobs WHERE id=$1")
            .bind(&result.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let authority: Value = serde_json::from_str(&actor_json).unwrap();
        assert_eq!(authority["userId"], "night-auditor");
        assert_eq!(authority["companyId"], "company");
        assert_eq!(authority["assignmentId"], "assignment-1");

        let receipt = sqlx::query(
            "SELECT operation,actor_id,request_hash,record_id \
             FROM command_receipts WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "pms.night.start");
        assert_eq!(receipt.get::<String, _>("actor_id"), "night-auditor");
        assert_eq!(receipt.get::<String, _>("record_id"), "property-1");
        assert_eq!(
            receipt.get::<String, _>("request_hash"),
            night_audit_request_hash(&actor(), &command).unwrap()
        );

        let audit = sqlx::query(
            "SELECT action,record_type,record_id,record_version \
             FROM audit_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "pms.night.start");
        assert_eq!(audit.get::<String, _>("record_type"), "night");
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
        assert_eq!(outbox.get::<String, _>("event_type"), "pms.night.start");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "night");
        assert_eq!(outbox.get::<String, _>("aggregate_id"), result.id);
        assert_eq!(outbox.get::<i64, _>("aggregate_version"), 2);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["businessDate"], "2026-11-04");
        assert_eq!(payload["state"], "pending");
        tx.commit().await.unwrap();

        let mut replay_tx = serializable(&holding).await;
        lock_night_audit_sources(&mut replay_tx).await.unwrap();
        let replayed = execute(&mut replay_tx, &key, &command, 9_000, &mut guard)
            .await
            .unwrap();
        assert_eq!(replayed, result);
        for sql in [
            "SELECT count(*) FROM command_receipts WHERE command_id=$1",
            "SELECT count(*) FROM audit_events WHERE command_id=$1",
            "SELECT count(*) FROM outbox_events WHERE command_id=$1",
        ] {
            assert_eq!(count(&mut replay_tx, sql, key.as_str()).await, 1, "{sql}");
        }
        let conflicting = start("property-1", 2);
        assert_eq!(
            business(
                execute(&mut replay_tx, &key, &conflicting, 9_001, &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "IDEMPOTENCY_CONFLICT"
        );
        let other_operation = resume("property-1", &result.id, 2);
        assert_eq!(
            business(
                execute(&mut replay_tx, &key, &other_operation, 9_002, &mut guard)
                    .await
                    .unwrap_err()
            )
            .code,
            "IDEMPOTENCY_CONFLICT"
        );
        replay_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn start_reports_guards_and_writes_nothing_when_blocked() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();

        let mut blocked = Guard::clear();
        blocked.controls = PmsNightDayControlFacts {
            open_shifts: 2,
            ..PmsNightDayControlFacts::default()
        };
        assert_eq!(
            business(
                execute(
                    &mut tx,
                    &CommandKey::new("night-start-controls").unwrap(),
                    &start("property-1", 1),
                    5_000,
                    &mut blocked
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_DAY_RECONCILIATION_BLOCKED"
        );

        let mut setup = Guard::clear();
        setup.setup = PmsNightSetupFacts {
            chart_valid: false,
            ..PmsNightSetupFacts::default()
        };
        assert_eq!(
            business(
                execute(
                    &mut tx,
                    &CommandKey::new("night-start-setup").unwrap(),
                    &start("property-1", 1),
                    5_000,
                    &mut setup
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_CHART_INVALID"
        );

        // A setup-state hotel never starts a day close.
        assert_eq!(
            business(
                execute_company(
                    &mut tx,
                    &CommandKey::new("night-start-setup-state").unwrap(),
                    &start("property-setup", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_SETUP_REQUIRED"
        );

        // A hotel that is already closing cannot start a second close.
        assert_eq!(
            business(
                execute_company(
                    &mut tx,
                    &CommandKey::new("night-start-closing").unwrap(),
                    &start("property-closing", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_DAY_CLOSING"
        );

        // A stale property version is a conflict, never a second write.
        assert_eq!(
            business(
                execute(
                    &mut tx,
                    &CommandKey::new("night-start-stale").unwrap(),
                    &start("property-1", 7),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_VERSION_CONFLICT"
        );

        // A foreign company cannot read the hotel at all.
        assert_eq!(
            business(
                execute_as(
                    &mut tx,
                    &actor_with("other", "north", &["pms.read", "pms.audit"]),
                    &CommandKey::new("night-start-foreign").unwrap(),
                    &start("property-1", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NOT_FOUND"
        );

        // Missing audit permission follows the retained permission gate.
        assert_eq!(
            business(
                execute_as(
                    &mut tx,
                    &actor_with("company", "north", &["pms.read"]),
                    &CommandKey::new("night-start-permission").unwrap(),
                    &start("property-1", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PERMISSION_REQUIRED"
        );

        // The user entry point refuses the worker-only step.
        assert_eq!(
            business(
                execute(
                    &mut tx,
                    &CommandKey::new("night-step-user").unwrap(),
                    &PmsNightAuditCommand {
                        worker: false,
                        expected_version: Some(1),
                        ..step("property-1", "night-missing")
                    },
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_WORKER_ONLY"
        );

        // Every refused command rolled back: no record, job, receipt or audit.
        let records: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pms_records WHERE kind='night'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(records, 0);
        let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM pms_jobs")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(jobs, 0);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(receipts, 0);
        assert_eq!(
            property_row(&mut tx, "property-1").await,
            ("open".to_owned(), "2026-11-04".to_owned(), 1)
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn resume_requeues_a_blocked_job_and_refuses_a_closed_day() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        record(
            &holding,
            "property-closing",
            "night-blocked",
            "night",
            json!({
                "date": "2026-11-04", "state": "blocked", "initiatedBy": "night-auditor",
                "message": "PMS_JOB_FAILED"
            }),
        )
        .await;
        sqlx::query(
            "INSERT INTO pms_jobs(id,property_id,business_date,state,actor_json,error_code,updated_at) \
             VALUES('night-blocked','property-closing','2026-11-04','blocked','{}','PMS_JOB_FAILED',1)",
        )
        .execute(holding.raw())
        .await
        .unwrap();

        let key = CommandKey::new("night-resume-0001").unwrap();
        let command = resume("property-closing", "night-blocked", 1);
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();
        let result = execute_company(&mut tx, &key, &command, 5_000, &mut Guard::clear())
            .await
            .unwrap();
        assert_eq!(result.id, "night-blocked");
        assert_eq!(result.version, 2);
        let night = json_value(&mut tx, "night-blocked").await;
        assert_eq!(night["state"], "pending");
        assert_eq!(night["message"], "");
        assert_eq!(night["initiatedBy"], "night-auditor");
        assert_eq!(
            property_row(&mut tx, "property-closing").await,
            ("closing".to_owned(), "2026-11-04".to_owned(), 2)
        );
        let (state, _, error) = job_row(&mut tx, "night-blocked").await.unwrap();
        assert_eq!(state, "pending");
        assert_eq!(error, "");
        let audit: String = sqlx::query_scalar(
            "SELECT action FROM audit_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit, "pms.night.resume");

        // A completed job cannot be resumed.
        assert_eq!(
            business(
                execute_company(
                    &mut tx,
                    &CommandKey::new("night-resume-closed").unwrap(),
                    &resume("property-closing", "night-closed", 2),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_RECORD_NOT_FOUND"
        );
        tx.rollback().await.unwrap();

        record(
            &holding,
            "property-closing",
            "night-closed",
            "night",
            json!({
                "date": "2026-11-04", "state": "completed", "initiatedBy": "night-auditor",
                "message": "", "completedAt": 1
            }),
        )
        .await;
        let mut closed_tx = serializable(&holding).await;
        lock_night_audit_sources(&mut closed_tx).await.unwrap();
        assert_eq!(
            business(
                execute_company(
                    &mut closed_tx,
                    &CommandKey::new("night-resume-completed").unwrap(),
                    &resume("property-closing", "night-closed", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NIGHT_CLOSED"
        );
        closed_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn step_fails_closed_without_the_posting_port_and_blocks_the_job() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        record(
            &holding,
            "property-closing",
            "night-pending",
            "night",
            json!({
                "date": "2026-11-04", "state": "pending", "initiatedBy": "night-auditor",
                "message": ""
            }),
        )
        .await;
        record(
            &holding,
            "property-closing",
            "stay-inhouse",
            "stay",
            json!({
                "reservationId": "reservation-1", "guestId": "guest-1",
                "typeId": "property-closing-type-1", "rateId": "property-closing-rate-1",
                "roomId": "property-closing-room-a", "arrival": "2026-11-03",
                "departure": "2026-11-06", "adults": 2, "state": "in_house",
                "guestAccountId": "account-1", "prices": [], "segments": []
            }),
        )
        .await;
        sqlx::query(
            "INSERT INTO pms_jobs(id,property_id,business_date,state,actor_json,updated_at) \
             VALUES('night-pending','property-closing','2026-11-04','pending',$1,1)",
        )
        .bind(json!({"companyId": "company", "userId": "night-auditor"}).to_string())
        .execute(holding.raw())
        .await
        .unwrap();

        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();
        let error = execute_step(
            &mut tx,
            &company_actor(),
            &CommandKey::new("night-step-unposted").unwrap(),
            &step("property-closing", "night-pending"),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NIGHT_POSTING_PORT_REQUIRED");
        assert_eq!(
            property_row(&mut tx, "property-closing").await,
            ("closing".to_owned(), "2026-11-04".to_owned(), 1)
        );
        tx.rollback().await.unwrap();

        // The durable worker records the failure exactly like the retained
        // `drainPms`: the job and its night record are marked blocked.
        let unit = PgUnitOfWork::new(holding.clone());
        let mut read = holding.raw().acquire().await.unwrap();
        let jobs = read_night_audit_jobs(&mut read, Some("property-closing"), None)
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "night-pending");
        drop(read);
        let settlement = run_night_audit_job(
            &unit,
            &jobs[0],
            &company_actor(),
            &CommandKey::new("night-step-worker-0001").unwrap(),
            5_000,
        )
        .await
        .unwrap();
        match settlement {
            PmsNightAuditSettlement::Blocked { code } => {
                assert_eq!(code, "PMS_NIGHT_POSTING_PORT_REQUIRED");
            }
            other => panic!("unexpected settlement: {other:?}"),
        }
        let mut tx = serializable(&holding).await;
        let (state, _, error) = job_row(&mut tx, "night-pending").await.unwrap();
        assert_eq!(state, "blocked");
        assert_eq!(error, "PMS_NIGHT_POSTING_PORT_REQUIRED");
        let night = json_value(&mut tx, "night-pending").await;
        assert_eq!(night["state"], "blocked");
        assert_eq!(night["message"], "PMS_NIGHT_POSTING_PORT_REQUIRED");
        assert_eq!(
            property_row(&mut tx, "property-closing").await,
            ("closing".to_owned(), "2026-11-04".to_owned(), 1)
        );

        // The blocked job is no longer selected, and a resume clears it.
        let mut read = holding.raw().acquire().await.unwrap();
        let jobs = read_night_audit_jobs(&mut read, Some("property-closing"), None)
            .await
            .unwrap();
        assert!(jobs.is_empty());
        drop(read);
        tx.rollback().await.unwrap();

        let mut resume_tx = serializable(&holding).await;
        lock_night_audit_sources(&mut resume_tx).await.unwrap();
        execute_company(
            &mut resume_tx,
            &CommandKey::new("night-resume-after-block").unwrap(),
            &resume("property-closing", "night-pending", 1),
            6_000,
            &mut Guard::clear(),
        )
        .await
        .unwrap();
        let (state, _, error) = job_row(&mut resume_tx, "night-pending").await.unwrap();
        assert_eq!(state, "pending");
        assert_eq!(error, "");
        resume_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn step_completes_the_day_and_advances_the_business_date() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        record(
            &holding,
            "property-closing",
            "night-pending",
            "night",
            json!({
                "date": "2026-11-04", "state": "pending", "initiatedBy": "night-auditor",
                "message": ""
            }),
        )
        .await;
        record(
            &holding,
            "property-closing",
            "stay-inhouse",
            "stay",
            json!({
                "reservationId": "reservation-1", "guestId": "guest-1",
                "typeId": "property-closing-type-1", "rateId": "property-closing-rate-1",
                "roomId": "property-closing-room-a", "arrival": "2026-11-03",
                "departure": "2026-11-06", "adults": 2, "state": "in_house",
                "guestAccountId": "account-1", "prices": [], "segments": []
            }),
        )
        .await;
        record(
            &holding,
            "property-closing",
            "entry-night",
            "entry",
            json!({
                "guestAccountId": "account-1", "stayId": "stay-inhouse",
                "date": "2026-11-04", "kind": "charge", "description": "Room charge",
                "net": 100, "service": 0, "tax": 0, "total": 100, "method": "",
                "reference": "", "sourceKey": "night:stay-inhouse:2026-11-04",
                "originalId": "", "shiftId": "", "actorId": "night-auditor", "at": 1
            }),
        )
        .await;
        record(
            &holding,
            "property-closing",
            "block-group-due",
            "block",
            json!({
                "name": "Allotment", "typeId": "property-closing-type-1", "roomId": "",
                "from": "2026-11-20", "to": "2026-11-22", "quantity": 1,
                "releaseDate": "2026-11-05", "state": "active", "kind": "group",
                "pickedStayIds": []
            }),
        )
        .await;
        sqlx::query(
            "INSERT INTO pms_jobs(id,property_id,business_date,state,actor_json,updated_at) \
             VALUES('night-pending','property-closing','2026-11-04','pending',$1,1)",
        )
        .bind(json!({"companyId": "company", "userId": "night-auditor"}).to_string())
        .execute(holding.raw())
        .await
        .unwrap();

        let key = CommandKey::new("night-step-0001").unwrap();
        let command = step("property-closing", "night-pending");
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();
        let result = execute_step(&mut tx, &company_actor(), &key, &command, 5_000)
            .await
            .unwrap();
        assert_eq!(result.id, "night-pending");
        assert_eq!(result.version, 2);
        let night = json_value(&mut tx, "night-pending").await;
        assert_eq!(night["state"], "completed");
        assert_eq!(night["completedAt"], 5_000);
        assert_eq!(night["roomMetrics"]["capacity"], 1);
        assert_eq!(night["roomMetrics"]["sold"], 1);
        assert_eq!(
            property_row(&mut tx, "property-closing").await,
            ("open".to_owned(), "2026-11-05".to_owned(), 2)
        );
        let (state, _, _) = job_row(&mut tx, "night-pending").await.unwrap();
        assert_eq!(state, "completed");
        let tasks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pms_records WHERE kind='task' AND property_id='property-closing'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(tasks, 1);
        let task = sqlx::query(
            "SELECT id,data_json FROM pms_records WHERE kind='task' AND property_id='property-closing'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let task_data: Value = serde_json::from_str(&task.get::<String, _>("data_json")).unwrap();
        assert_eq!(task_data["type"], "stayover");
        assert_eq!(task_data["date"], "2026-11-05");
        assert_eq!(task_data["stayId"], "stay-inhouse");
        assert_eq!(task_data["roomId"], "property-closing-room-a");
        let block = json_value(&mut tx, "block-group-due").await;
        assert_eq!(block["state"], "released");
        let audit: String =
            sqlx::query_scalar("SELECT action FROM audit_events WHERE command_id=$1")
                .bind(key.as_str())
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(audit, "pms.night.step");
        let outbox: String = sqlx::query_scalar(
            "SELECT payload_json FROM outbox_events WHERE command_id=$1",
        )
        .bind(key.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let payload: Value = serde_json::from_str(&outbox).unwrap();
        assert_eq!(payload["businessDate"], "2026-11-05");
        assert_eq!(payload["state"], "completed");
        assert_eq!(payload["roomMetrics"]["capacity"], 1);
        tx.commit().await.unwrap();

        let mut replay_tx = serializable(&holding).await;
        lock_night_audit_sources(&mut replay_tx).await.unwrap();
        let replayed = execute_step(&mut replay_tx, &company_actor(), &key, &command, 9_000)
            .await
            .unwrap();
        assert_eq!(replayed, result);
        let nights: i64 = sqlx::query_scalar("SELECT count(*) FROM pms_records WHERE kind='night'")
            .fetch_one(&mut *replay_tx)
            .await
            .unwrap();
        assert_eq!(nights, 1);
        replay_tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn step_is_worker_only_and_scoped() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        record(
            &holding,
            "property-closing",
            "night-pending",
            "night",
            json!({
                "date": "2026-11-04", "state": "pending", "initiatedBy": "night-auditor",
                "message": ""
            }),
        )
        .await;
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();

        // The step entry point accepts only the retained worker command.
        assert_eq!(
            business(
                execute_step(
                    &mut tx,
                    &actor(),
                    &CommandKey::new("night-step-user-entry").unwrap(),
                    &PmsNightAuditCommand {
                        worker: false,
                        ..step("property-closing", "night-pending")
                    },
                    5_000
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NIGHT_AUDIT_REQUEST_INVALID"
        );

        // A worker without the audit permission is refused.
        assert_eq!(
            business(
                execute_step(
                    &mut tx,
                    &actor_with("company", "closingplace", &["pms.read"]),
                    &CommandKey::new("night-step-permission").unwrap(),
                    &step("property-closing", "night-pending"),
                    5_000
                )
                .await
                .unwrap_err()
            )
            .code,
            "PERMISSION_REQUIRED"
        );

        // The worker is scoped to its Company and hotel.
        assert_eq!(
            business(
                execute_step(
                    &mut tx,
                    &actor_with("other", "north", &["pms.read", "pms.audit"]),
                    &CommandKey::new("night-step-foreign").unwrap(),
                    &step("property-closing", "night-pending"),
                    5_000
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NOT_FOUND"
        );

        // A branch-scoped worker cannot close another branch's hotel.
        assert_eq!(
            business(
                execute_step(
                    &mut tx,
                    &actor_with("company", "north", &["pms.read", "pms.audit"]),
                    &CommandKey::new("night-step-branch").unwrap(),
                    &step("property-closing", "night-pending"),
                    5_000
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NOT_FOUND"
        );

        // A missing night record fails closed before any write.
        assert_eq!(
            business(
                execute_step(
                    &mut tx,
                    &actor_with("company", "closingplace", &["pms.read", "pms.audit"]),
                    &CommandKey::new("night-step-missing").unwrap(),
                    &step("property-closing", "night-missing"),
                    5_000
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_RECORD_NOT_FOUND"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn bounded_reads_reject_more_stays_than_the_command_can_hold() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        property(&holding, "property-wide", "company", "wide", "open").await;
        seed_ready(&holding, "property-wide").await;
        sqlx::query(
            "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
             SELECT 'wide-stay-'||n,'property-wide','stay',jsonb_build_object( \
               'reservationId','r-'||n,'guestId','g-'||n,'typeId','property-wide-type-1', \
               'rateId','property-wide-rate-1','roomId','','arrival','2026-11-04', \
               'departure','2026-11-06','adults',1,'state','reserved', \
               'guestAccountId','a-'||n,'prices','[]'::jsonb,'segments','[]'::jsonb)::text,1,1 \
             FROM generate_series(1,$1::int) AS n",
        )
        .bind(MAX_STAYS as i32 + 1)
        .execute(holding.raw())
        .await
        .unwrap();
        let mut tx = serializable(&holding).await;
        lock_night_audit_sources(&mut tx).await.unwrap();
        assert_eq!(
            business(
                execute_company(
                    &mut tx,
                    &CommandKey::new("night-start-wide").unwrap(),
                    &start("property-wide", 1),
                    5_000,
                    &mut Guard::clear()
                )
                .await
                .unwrap_err()
            )
            .code,
            "PMS_NIGHT_AUDIT_READ_LIMIT"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn non_serializable_units_and_unlocked_replays_fail_closed() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        // A plain read transaction is not the reviewed serializable unit.
        let mut tx = holding.raw().begin().await.unwrap();
        let mut repository = PgPmsNightAuditRepository::new(&mut tx);
        let error = repository
            .replay(
                &CommandKey::new("night-start-readonly").unwrap(),
                "hash",
                &start("property-1", 1),
            )
            .await
            .unwrap_err();
        assert_eq!(persistence(error), "PMS_NIGHT_AUDIT_SERIALIZABLE_REQUIRED");
        tx.rollback().await.unwrap();

        // An autocommit connection cannot own the PMS write unit.
        let mut connection = holding.raw().acquire().await.unwrap();
        assert_eq!(
            persistence(lock_night_audit_sources(&mut connection).await.unwrap_err()),
            "PMS_NIGHT_AUDIT_TRANSACTION_REQUIRED"
        );
        drop(connection);

        let _ = TransactionOptions::default();
    })
    .await;
}
