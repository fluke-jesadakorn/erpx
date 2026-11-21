//! Native PMS guest-account, charge, adjustment and transfer commands over real
//! migrated PostgreSQL. These are owner command receipts, not browser
//! acceptance or HTTP evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::guest_account::{
    PgPmsGuestAccountRepository, PmsAdjustmentInput, PmsChargeAddInput, PmsChargeImportInput,
    PmsGuestAccountActor, PmsGuestAccountCommand, PmsGuestAccountHttpBackend,
    PmsGuestAccountHttpCommandRequest, PmsGuestAccountHttpFuture, PmsGuestAccountHttpResponse,
    PmsGuestAccountHttpTarget, PmsGuestAccountInput, PmsGuestAccountOpenInput,
    PmsGuestAccountOperation, PmsGuestAccountResult, PmsGuestAccountUpdateInput, PmsTransferInput,
    guest_account_request_hash, lock_guest_account_sources, pms_guest_account,
    pms_guest_account_router,
};
use erpx_server_pms::public::room_board::PmsBoardActor;
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
        "country": "TH", "currency": "THB", "minorUnit": 2, "timezone": "Asia/Bangkok",
        "checkIn": "14:00", "checkOut": "12:00", "cancellationPolicy": "",
        "chartId": "chart",
        "accounts": {
            "cash": "cash", "bank": "bank", "edc": "edc", "receivable": "receivable",
            "advance": "advance", "deposit": "deposit", "revenue": "revenue",
            "service": "service", "tax": "tax", "prepaidTax": "prepaidTax",
            "inventory": "inventory", "consumption": "consumption",
            "commission": "commission", "cashDifference": "cashDifference"
        },
        "taxBps": 700, "serviceBps": 1000, "taxInclusive": false, "advanceTax": true,
        "preparedBy": "maker", "approvedBy": "reviewer", "revision": 1
    })
    .to_string()
}

async fn property(pool: &DatabasePool, id: &str, company: &str, branch: &str) {
    sqlx::query(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES($1,$2,$3,$4,'2026-11-04','open',$5,1,'maker',1)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(format!("Hotel {id}"))
    .bind(config())
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn record(pool: &DatabasePool, property_id: &str, id: &str, kind: &str, data: Value) {
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

fn account_data(name: &str, state: &str, due: i64, advance: i64, invoice: &str) -> Value {
    json!({
        "guestId": "guest-1", "reservationId": "reservation-1", "name": name,
        "partyId": "", "state": state, "due": due, "advance": advance,
        "advanceTax": 0, "deposit": 0, "invoiceId": invoice
    })
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    property(pool, "property-1", "company", "north").await;
    property(pool, "property-foreign", "other", "south").await;
    for (id, name) in [("guest-1", "Ada Guest"), ("guest-2", "Grace Guest")] {
        record(
            pool,
            "property-1",
            id,
            "guest",
            json!({
                "name": name, "email": "", "phone": "", "nationality": "",
                "documentRef": "", "notes": ""
            }),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "reservation-1",
        "reservation",
        json!({"number": "R-1", "guestId": "guest-1", "state": "confirmed"}),
    )
    .await;
    for (id, state, due, advance, invoice) in [
        ("account-1", "open", 0, 0, ""),
        ("account-2", "open", 0, 0, ""),
        ("account-transfer", "open", 5_000, 0, ""),
        ("account-closed", "closed", 0, 0, ""),
        ("account-advance", "open", 0, 5_000, ""),
        ("account-refund", "open", 0, 0, ""),
    ] {
        record(
            pool,
            "property-1",
            id,
            "guest_account",
            account_data(&format!("Bill {id}"), state, due, advance, invoice),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "entry-transfer",
        "entry",
        json!({
            "guestAccountId": "account-transfer", "stayId": "", "date": "2026-11-04",
            "kind": "charge", "description": "Room charge", "net": 5_000,
            "service": 0, "tax": 0, "total": 5_000, "method": "", "reference": "",
            "sourceKey": "room-charge:stay-1:2026-11-04", "originalId": "",
            "shiftId": "", "actorId": "maker", "at": 1
        }),
    )
    .await;
    sqlx::query(
        "INSERT INTO pms_sources(property_id,source_key,record_id) VALUES('property-1','room-charge:stay-1:2026-11-04','entry-transfer')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    record(
        pool,
        "property-1",
        "refund-1",
        "refund",
        json!({
            "guestAccountId": "account-refund", "amount": 100, "pocket": "advance",
            "reason": "Overpayment", "state": "requested", "requestedBy": "maker",
            "approvedBy": ""
        }),
    )
    .await;
    record(
        pool,
        "property-foreign",
        "account-foreign",
        "guest_account",
        account_data("Foreign bill", "open", 0, 0, ""),
    )
    .await;
}

fn actor_with(company: &str, branch: &str) -> PmsGuestAccountActor {
    PmsGuestAccountActor {
        board: PmsBoardActor {
            user_id: "user".into(),
            company_id: company.into(),
            branch_id: Some(branch.into()),
            department_id: None,
            permissions: vec!["pms.read".into(), "pms.cashier".into()],
            modules: vec!["pms".into()],
            expires_at: 9_007_199_254_740_991,
        },
        assignment_id: "assignment-1".into(),
        role: "cashier".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn actor() -> PmsGuestAccountActor {
    actor_with("company", "north")
}

fn create_command(reservation_id: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Create,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Create(PmsGuestAccountOpenInput {
            guest_id: "guest-1".into(),
            reservation_id: reservation_id.into(),
            name: "Ada Guest".into(),
        }),
    }
}

fn update_command(name: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Update,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Update(PmsGuestAccountUpdateInput {
            account_id: "account-1".into(),
            name: name.into(),
        }),
    }
}

fn charge_add(account_id: &str, price: i64, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::ChargeAdd,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::ChargeAdd(PmsChargeAddInput {
            account_id: account_id.into(),
            price,
            description: "Room service".into(),
            stay_id: "stay-1".into(),
            source_key: String::new(),
        }),
    }
}

fn charge_import(expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::ChargeImport,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::ChargeImport(PmsChargeImportInput {
            account_id: "account-1".into(),
            stay_id: "stay-1".into(),
            source_key: "pos:bill:1".into(),
            description: "Restaurant".into(),
            net: 5_000,
            service: 0,
            tax: 0,
            total: 5_000,
        }),
    }
}

fn reverse(original: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Adjustment,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Adjustment(PmsAdjustmentInput {
            account_id: "account-1".into(),
            original_entry_id: original.into(),
            description: "Correct the charge".into(),
            net: -10_000,
            service: -1_000,
            tax: -770,
            total: -11_770,
            source_key: String::new(),
        }),
    }
}

fn transfer(to: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Transfer,
        property_id: "property-1".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Transfer(PmsTransferInput {
            entry_id: "entry-transfer".into(),
            to_account_id: to.into(),
            reason: "Move the room charge".into(),
        }),
    }
}

async fn execute(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    command: &PmsGuestAccountCommand,
    now: i64,
) -> Result<PmsGuestAccountResult, PgTransactionError<BusinessError>> {
    let mut repository = PgPmsGuestAccountRepository::new(&mut **tx);
    pms_guest_account(actor, key, command, &mut repository, now).await
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

async fn count(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&mut **tx).await.unwrap()
}

#[tokio::test]
async fn create_update_charge_reverse_retain_evidence_and_replay() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let create = create_command("reservation-1", 1);
        let mut tx = serializable(&holding).await;
        lock_guest_account_sources(&mut tx).await.unwrap();
        let created = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-create-0001").unwrap(),
            &create,
            5_000,
        )
        .await
        .unwrap();
        assert!(created.id.starts_with("pms-guest-account-"));
        assert_eq!(created.account_id, created.id);
        assert_eq!(created.version, 2);
        assert_eq!(created.state, "open");
        assert_eq!(created.due, 0);
        let row = json_value(&mut tx, &created.id).await;
        assert_eq!(row["guestId"], "guest-1");
        assert_eq!(row["reservationId"], "reservation-1");
        assert_eq!(row["name"], "Ada Guest");
        assert_eq!(row["partyId"], "");
        assert_eq!(record_version(&mut tx, &created.id).await, 1);
        tx.commit().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_guest_account_sources(&mut tx).await.unwrap();
        let updated = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-update-0001").unwrap(),
            &update_command("  Renamed Bill  ", 2),
            5_001,
        )
        .await
        .unwrap();
        assert_eq!(updated.id, "account-1");
        assert_eq!(updated.version, 3);
        assert_eq!(json_value(&mut tx, "account-1").await["name"], "Renamed Bill");

        let charged = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-charge-0001").unwrap(),
            &charge_add("account-1", 10_000, 3),
            5_002,
        )
        .await
        .unwrap();
        assert_eq!(charged.account_id, "account-1");
        assert_eq!(charged.due, 11_770);
        assert_eq!(charged.version, 4);
        let entry = json_value(&mut tx, &charged.id).await;
        assert_eq!(entry["kind"], "charge");
        assert_eq!(entry["net"], 10_000);
        assert_eq!(entry["service"], 1_000);
        assert_eq!(entry["tax"], 770);
        assert_eq!(entry["total"], 11_770);
        assert_eq!(entry["date"], "2026-11-04");
        assert_eq!(entry["actorId"], "user");
        assert_eq!(json_value(&mut tx, "account-1").await["due"], 11_770);

        let imported = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-import-0001").unwrap(),
            &charge_import(4),
            5_003,
        )
        .await
        .unwrap();
        assert_eq!(imported.due, 16_770);
        let source: String = sqlx::query_scalar(
            "SELECT record_id FROM pms_sources WHERE property_id='property-1' AND source_key='pos:bill:1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(source, imported.id);

        let replayed_import = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-import-0002").unwrap(),
            &charge_import(5),
            5_004,
        )
        .await
        .unwrap();
        assert_eq!(replayed_import.id, imported.id);
        assert_eq!(replayed_import.due, 16_770);
        assert_eq!(
            count(
                &mut tx,
                "SELECT count(*) FROM pms_records WHERE kind='entry' AND data_json::jsonb->>'sourceKey'='pos:bill:1'",
            )
            .await,
            1
        );

        let reversed = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-reverse-0001").unwrap(),
            &reverse(&charged.id, 6),
            5_005,
        )
        .await
        .unwrap();
        assert_eq!(reversed.due, 5_000);
        let adjustment = json_value(&mut tx, &reversed.id).await;
        assert_eq!(adjustment["kind"], "adjustment");
        assert_eq!(adjustment["originalId"], charged.id);
        assert_eq!(adjustment["total"], -11_770);

        for (sql, expected) in [
            (
                "SELECT count(*) FROM command_receipts WHERE command_id='guest-charge-0001'",
                1,
            ),
            (
                "SELECT count(*) FROM audit_events WHERE command_id='guest-charge-0001'",
                1,
            ),
            (
                "SELECT count(*) FROM outbox_events WHERE command_id='guest-charge-0001'",
                1,
            ),
        ] {
            assert_eq!(count(&mut tx, sql).await, expected, "{sql}");
        }
        let receipt = sqlx::query(
            "SELECT operation,record_id,result_json FROM command_receipts WHERE command_id='guest-charge-0001'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "pms.charge.add");
        assert_eq!(receipt.get::<String, _>("record_id"), "property-1");
        let audit = sqlx::query(
            "SELECT action,record_type,record_version FROM audit_events WHERE command_id='guest-charge-0001'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "pms.charge.add");
        assert_eq!(audit.get::<String, _>("record_type"), "entry");
        assert_eq!(audit.get::<i64, _>("record_version"), 4);
        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,payload_json FROM outbox_events WHERE command_id='guest-charge-0001'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "pms.charge.add");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "entry");
        assert_eq!(outbox.get::<String, _>("aggregate_id"), charged.id);
        let payload: Value =
            serde_json::from_str(&outbox.get::<String, _>("payload_json")).unwrap();
        assert_eq!(payload["accountId"], "account-1");
        assert_eq!(payload["entryId"], charged.id);
        assert_eq!(payload["due"], 11_770);
        tx.commit().await.unwrap();

        let mut tx = serializable(&holding).await;
        lock_guest_account_sources(&mut tx).await.unwrap();
        let replay = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-charge-0001").unwrap(),
            &charge_add("account-1", 10_000, 3),
            9_000,
        )
        .await
        .unwrap();
        assert_eq!(replay, charged);
        let mut normalized = charge_add("account-1", 10_000, 3);
        normalized.normalize();
        assert_eq!(
            guest_account_request_hash(&actor(), &normalized).unwrap().len(),
            64
        );
        let conflict = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-charge-0001").unwrap(),
            &charge_add("account-1", 12_000, 3),
            9_001,
        )
        .await
        .unwrap_err();
        assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
        assert_eq!(
            count(
                &mut tx,
                "SELECT count(*) FROM command_receipts WHERE command_id='guest-charge-0001'",
            )
            .await,
            1
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn closed_foreign_guarded_and_stale_accounts_fail_closed() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_guest_account_sources(&mut tx).await.unwrap();

        for (account, expected) in [
            ("account-closed", "PMS_GUEST_ACCOUNT_CLOSED"),
            ("account-refund", "PMS_REFUND_PENDING"),
            ("account-advance", "PMS_GUEST_PAYMENT_PORT_REQUIRED"),
        ] {
            let error = execute(
                &mut tx,
                &actor(),
                &CommandKey::new("guest-guard-account-0001").unwrap(),
                &charge_add(account, 10_000, 1),
                5_000,
            )
            .await
            .unwrap_err();
            assert_eq!(business(error).code, expected);
        }
        assert_eq!(property_version(&mut tx, "property-1").await, 1);
        assert_eq!(
            count(
                &mut tx,
                "SELECT count(*) FROM pms_records WHERE kind='entry' AND property_id='property-1'",
            )
            .await,
            1
        );

        let stale = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-guard-stale-0001").unwrap(),
            &charge_add("account-1", 10_000, 2),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(stale).code, "PMS_VERSION_CONFLICT");

        let invalid = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-guard-price-0001").unwrap(),
            &charge_add("account-1", 0, 1),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(invalid).code, "PMS_GUEST_ACCOUNT_REQUEST_INVALID");

        let foreign = execute(
            &mut tx,
            &actor_with("company", "south"),
            &CommandKey::new("guest-guard-branch-0001").unwrap(),
            &charge_add("account-1", 10_000, 1),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(foreign).code, "PMS_NOT_FOUND");

        let other_property = execute(
            &mut tx,
            &actor_with("other", "south"),
            &CommandKey::new("guest-guard-property-0001").unwrap(),
            &create_command("", 1),
            5_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(other_property).code, "PMS_NOT_FOUND");
        assert!(persistence_empty(&mut tx, "property-foreign").await);
        tx.rollback().await.unwrap();
    })
    .await;
}

async fn persistence_empty(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    property_id: &str,
) -> bool {
    let version: i64 = sqlx::query_scalar("SELECT version FROM pms_properties WHERE id=$1")
        .bind(property_id)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE record_id=$1")
            .bind(property_id)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
    version == 1 && receipts == 0
}

#[tokio::test]
async fn transfer_moves_unpaid_item_and_refuses_a_second_move() {
    fixture::with_database(4, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_guest_account_sources(&mut tx).await.unwrap();
        let moved = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-transfer-0001").unwrap(),
            &transfer("account-2", 1),
            5_000,
        )
        .await
        .unwrap();
        assert_eq!(moved.account_id, "account-transfer");
        assert_eq!(moved.due, 0);
        assert_eq!(json_value(&mut tx, "account-transfer").await["due"], 0);
        assert_eq!(json_value(&mut tx, "account-2").await["due"], 5_000);
        let from_entry = json_value(&mut tx, &moved.id).await;
        assert_eq!(from_entry["kind"], "transfer");
        assert_eq!(from_entry["total"], -5_000);
        assert_eq!(from_entry["originalId"], "entry-transfer");
        let to_entry_id: String = sqlx::query_scalar(
            "SELECT id FROM pms_records WHERE kind='entry' AND data_json::jsonb->>'originalId'=$1",
        )
        .bind(&moved.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let to_entry = json_value(&mut tx, &to_entry_id).await;
        assert_eq!(to_entry["guestAccountId"], "account-2");
        assert_eq!(to_entry["total"], 5_000);

        let second = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-transfer-0002").unwrap(),
            &transfer("account-2", 2),
            5_001,
        )
        .await
        .unwrap_err();
        assert_eq!(business(second).code, "PMS_ENTRY_REFERENCED");

        let same = execute(
            &mut tx,
            &actor(),
            &CommandKey::new("guest-transfer-same-0001").unwrap(),
            &transfer("account-transfer", 2),
            5_002,
        )
        .await
        .unwrap_err();
        assert_eq!(business(same).code, "PMS_TRANSFER_INVALID");
        tx.rollback().await.unwrap();
    })
    .await;
}

const HTTP_HEADERS: &str = "Origin: https://acme.example.test\r\nIdempotency-Key: guest-account-0001\r\nContent-Type: application/json\r\nx-erpx-context-company: company\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment: assignment-1\r\nx-erpx-context-assignment-version: 1\r\n";

struct GuestHttpBackend {
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
impl GuestHttpBackend {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl PmsGuestAccountHttpBackend for GuestHttpBackend {
    fn command<'a>(
        &'a self,
        request: PmsGuestAccountHttpCommandRequest<'a>,
    ) -> PmsGuestAccountHttpFuture<'a, PmsGuestAccountHttpResponse> {
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
        let id = match &request.command.input {
            PmsGuestAccountInput::Create(input) => input.name.clone(),
            PmsGuestAccountInput::Update(input) => input.name.clone(),
            PmsGuestAccountInput::ChargeAdd(input) => input.account_id.clone(),
            PmsGuestAccountInput::ChargeImport(input) => input.account_id.clone(),
            PmsGuestAccountInput::Adjustment(input) => input.account_id.clone(),
            PmsGuestAccountInput::Transfer(input) => input.to_account_id.clone(),
        };
        let property_id = request.command.property_id.clone();
        let version = request.command.expected_version + 1;
        Box::pin(async move {
            Ok(PmsGuestAccountHttpResponse {
                data: json!({"id": id, "propertyId": property_id, "version": version}),
                context: json!({"userId": "user"}),
            })
        })
    }
}

fn http_target(backend: Arc<GuestHttpBackend>) -> PmsGuestAccountHttpTarget {
    PmsGuestAccountHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

async fn http_start(
    backend: Arc<GuestHttpBackend>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(
            listener,
            pms_guest_account_router(vec![http_target(backend)]).unwrap(),
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
async fn http_guest_account_retains_bounded_context_and_fails_closed() {
    let backend = Arc::new(GuestHttpBackend::new());
    let (address, task) = http_start(backend.clone()).await;
    let create_body = json!({
        "operation": "guest_account.create",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "guestId": "guest-1",
        "reservationId": "reservation-1",
        "name": "  Ada Guest  "
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account?branch=north",
        HTTP_HEADERS,
        &create_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "Ada Guest");
    assert_eq!(value["data"]["version"], 2);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.seen.lock().unwrap().as_slice(),
        ["acme.example.test:acme:guest-account-0001:guest_account.create:company:assignment-1"]
    );

    let charge_body = json!({
        "operation": "charge.add",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "guestAccountId": "account-1",
        "price": 10_000,
        "description": "Room service"
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        &charge_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "account-1");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 2);

    let transfer_body = json!({
        "operation": "guest_account.transfer",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "entryId": "entry-1",
        "toGuestAccountId": "account-2",
        "reason": "Move the charge"
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        &transfer_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "account-2");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 3);

    let adjustment_body = json!({
        "operation": "guest_account.adjustment",
        "propertyId": "property-1",
        "expectedVersion": 1,
        "guestAccountId": "account-1",
        "originalEntryId": "entry-1",
        "description": "Reverse the charge",
        "net": -100,
        "service": 0,
        "tax": 0,
        "total": -100
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        &adjustment_body,
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "account-1");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 4);

    let unknown_field = json!({
        "operation": "charge.add", "propertyId": "property-1",
        "expectedVersion": 1, "guestAccountId": "account-1", "price": 1,
        "description": "Charge", "unexpected": true
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        &unknown_field,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PMS_GUEST_ACCOUNT_REQUEST_INVALID");

    let mismatch_body = json!({
        "operation": "charge.import_pos", "propertyId": "property-1",
        "expectedVersion": 1, "guestAccountId": "account-1",
        "sourceKey": "pos:bill:1", "description": "Restaurant",
        "net": 100, "service": 0, "tax": 0, "total": 101
    })
    .to_string();
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        &mismatch_body,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "PMS_GUEST_ACCOUNT_REQUEST_INVALID");

    let unknown_operation = create_body.replace("guest_account.create", "guest_account.rebuild");
    let (status, _) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
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
        "POST /api/v1/holdings/acme/pms/guest-account",
        &wrong_origin,
        &create_body,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");

    let no_key = HTTP_HEADERS.replace("Idempotency-Key: guest-account-0001\r\n", "");
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        &no_key,
        &create_body,
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(value["error"]["code"], "COMMAND_KEY_REQUIRED");

    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/other/pms/guest-account",
        HTTP_HEADERS,
        &create_body,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    assert_eq!(backend.calls.load(Ordering::SeqCst), 4);
    task.abort();
}

#[tokio::test]
async fn http_guest_account_router_is_fail_closed_when_unconfigured_or_invalid() {
    let empty = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = empty.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(empty, pms_guest_account_router(Vec::new()).unwrap())
            .await
            .unwrap();
    });
    let (status, value) = http_call(
        address,
        "POST /api/v1/holdings/acme/pms/guest-account",
        HTTP_HEADERS,
        "{}",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(value["error"]["code"], "NOT_FOUND");
    task.abort();

    let backend = Arc::new(GuestHttpBackend::new());
    let target = || http_target(backend.clone());
    assert!(pms_guest_account_router(vec![target()]).is_ok());
    assert!(pms_guest_account_router(vec![target(), target()]).is_err());
    assert!(
        pms_guest_account_router(vec![PmsGuestAccountHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err()
    );
}
