//! Native PMS guest-account, charge, adjustment and transfer domain/application
//! facts. No database, browser, adapter, persisted receipt or HTTP acceptance is
//! inferred.
#![allow(dead_code)]
mod domain {
    #[path = "../../../../packages/server-pms/src/domain/guest_account.rs"]
    pub mod guest_account;
    #[path = "../../../../packages/server-pms/src/domain/room_board.rs"]
    pub mod room_board;
}
mod application {
    #[path = "../../../../packages/server-pms/src/application/guest_account.rs"]
    pub mod guest_account;
    #[path = "../../../../packages/server-pms/src/application/room_board.rs"]
    pub mod room_board;
}

use application::guest_account::{
    PmsGuestAccountRepository, guest_account_request_hash, pms_guest_account,
};
use application::room_board::PmsBoardFuture;
use domain::guest_account::{
    PmsAdjustmentInput, PmsChargeAddInput, PmsChargeImportInput, PmsGuestAccountActor,
    PmsGuestAccountCommand, PmsGuestAccountFacts, PmsGuestAccountInput, PmsGuestAccountOpenInput,
    PmsGuestAccountOperation, PmsGuestAccountPlan, PmsGuestAccountResult,
    PmsGuestAccountUpdateInput, PmsTransferInput, plan_guest_account, price_amounts,
};
use domain::room_board::{PmsBoardActor, PmsPropertyRow, PmsRecord};
use erpx_server_shared::public::{BusinessError, CommandKey};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const NOW: i64 = 5_000;

fn board() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("north".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.cashier".into()],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn actor() -> PmsGuestAccountActor {
    PmsGuestAccountActor {
        board: board(),
        assignment_id: "assignment".into(),
        role: "cashier".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn property() -> PmsPropertyRow {
    PmsPropertyRow {
        id: "property".into(),
        company_id: "company".into(),
        branch_id: "north".into(),
        name: "Hotel".into(),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        config_json: json!({
            "taxBps": 700,
            "serviceBps": 1000,
            "taxInclusive": false,
            "advanceTax": true,
            "approvedBy": "reviewer"
        })
        .to_string(),
        version: 1,
    }
}

fn record(id: &str, kind: &str, data: serde_json::Value) -> PmsRecord {
    PmsRecord {
        id: id.into(),
        kind: kind.into(),
        version: 1,
        updated_at: 1,
        data,
    }
}

fn guest(id: &str) -> PmsRecord {
    record(
        id,
        "guest",
        json!({
            "name": "Ada Guest", "email": "", "phone": "", "nationality": "",
            "documentRef": "", "notes": ""
        }),
    )
}

fn reservation(id: &str, guest_id: &str) -> PmsRecord {
    record(
        id,
        "reservation",
        json!({"number": "R-1", "guestId": guest_id, "state": "confirmed"}),
    )
}

fn account(id: &str, state: &str, due: i64, advance: i64, invoice_id: &str) -> PmsRecord {
    record(
        id,
        "guest_account",
        json!({
            "guestId": "guest-1",
            "reservationId": "reservation-1",
            "name": "Ada Guest",
            "partyId": "",
            "state": state,
            "due": due,
            "advance": advance,
            "advanceTax": 0,
            "deposit": 0,
            "invoiceId": invoice_id,
        }),
    )
}

fn entry(
    id: &str,
    account_id: &str,
    kind: &str,
    method: &str,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
) -> PmsRecord {
    record(
        id,
        "entry",
        json!({
            "guestAccountId": account_id, "stayId": "", "date": "2026-11-04",
            "kind": kind, "description": "source", "net": net, "service": service,
            "tax": tax, "total": total, "method": method, "reference": "",
            "sourceKey": "", "originalId": "", "shiftId": "",
            "actorId": "user", "at": 1
        }),
    )
}

fn facts(
    account_id: &str,
    state: &str,
    due: i64,
    advance: i64,
    invoice_id: &str,
) -> PmsGuestAccountFacts {
    let mut facts = PmsGuestAccountFacts::empty(property());
    facts.account = Some(account(account_id, state, due, advance, invoice_id));
    facts.guest = Some(guest("guest-1"));
    facts.reservation = Some(reservation("reservation-1", "guest-1"));
    facts
}

fn create(reservation_id: &str, name: &str) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Create,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsGuestAccountInput::Create(PmsGuestAccountOpenInput {
            guest_id: "guest-1".into(),
            reservation_id: reservation_id.into(),
            name: name.into(),
        }),
    }
}

fn update(name: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Update,
        property_id: "property".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Update(PmsGuestAccountUpdateInput {
            account_id: "account-1".into(),
            name: name.into(),
        }),
    }
}

fn charge_add(
    price: i64,
    description: &str,
    source_key: &str,
    expected: i64,
) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::ChargeAdd,
        property_id: "property".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::ChargeAdd(PmsChargeAddInput {
            account_id: "account-1".into(),
            price,
            description: description.into(),
            stay_id: "stay-1".into(),
            source_key: source_key.into(),
        }),
    }
}

fn charge_import(
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
    expected: i64,
) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::ChargeImport,
        property_id: "property".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::ChargeImport(PmsChargeImportInput {
            account_id: "account-1".into(),
            stay_id: "stay-1".into(),
            source_key: "pos:bill:1".into(),
            description: "Restaurant".into(),
            net,
            service,
            tax,
            total,
        }),
    }
}

fn adjustment(
    original: &str,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
    expected: i64,
) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Adjustment,
        property_id: "property".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Adjustment(PmsAdjustmentInput {
            account_id: "account-1".into(),
            original_entry_id: original.into(),
            description: "Correct the charge".into(),
            net,
            service,
            tax,
            total,
            source_key: String::new(),
        }),
    }
}

fn transfer(to: &str, expected: i64) -> PmsGuestAccountCommand {
    PmsGuestAccountCommand {
        operation: PmsGuestAccountOperation::Transfer,
        property_id: "property".into(),
        expected_version: expected,
        input: PmsGuestAccountInput::Transfer(PmsTransferInput {
            entry_id: "entry-source".into(),
            to_account_id: to.into(),
            reason: "Move the room charge".into(),
        }),
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn error(plan: Result<PmsGuestAccountPlan, BusinessError>) -> BusinessError {
    plan.expect_err("expected a refusal")
}

fn plan(facts: &PmsGuestAccountFacts, command: &PmsGuestAccountCommand) -> PmsGuestAccountPlan {
    plan_guest_account(&actor(), &key("guest-account-0001"), facts, command, NOW).unwrap()
}

#[test]
fn create_opens_linked_account_with_retained_fields() {
    let plan = plan(
        &facts("account-1", "open", 0, 0, ""),
        &create("reservation-1", "  Ada Guest  "),
    );
    assert_eq!(plan.result.version, 2);
    assert!(plan.result.id.starts_with("pms-guest-account-"));
    assert_eq!(plan.result.id, plan.result.account_id);
    assert_eq!(plan.result.state, "open");
    assert_eq!(plan.result.due, 0);
    let created = plan.new_account.as_ref().unwrap();
    assert_eq!(created.version, 1);
    assert_eq!(created.data["guestId"], "guest-1");
    assert_eq!(created.data["reservationId"], "reservation-1");
    assert_eq!(created.data["name"], "Ada Guest");
    assert_eq!(created.data["partyId"], "");
    assert_eq!(created.data["state"], "open");
    assert_eq!(created.data["due"], 0);
    assert_eq!(created.data["advance"], 0);
    assert_eq!(created.data["advanceTax"], 0);
    assert_eq!(created.data["deposit"], 0);
    assert!(plan.account.is_none());

    let mut mismatched = facts("account-1", "open", 0, 0, "");
    mismatched.reservation = Some(reservation("reservation-1", "guest-2"));
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-link-0001"),
        &mismatched,
        &create("reservation-1", "Ada Guest"),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_LINKAGE");

    let mut missing = PmsGuestAccountFacts::empty(property());
    missing.guest = None;
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-missing-0001"),
        &missing,
        &create("", "Ada Guest"),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_RECORD_NOT_FOUND");
}

#[test]
fn update_renames_open_account_and_refuses_closed() {
    let plan = plan(
        &facts("account-1", "open", 4_200, 0, ""),
        &update("  New Bill  ", 1),
    );
    assert_eq!(plan.result.id, "account-1");
    assert_eq!(plan.result.due, 4_200);
    let updated = plan.account.as_ref().unwrap();
    assert_eq!(updated.version, 2);
    assert_eq!(updated.data["name"], "New Bill");
    assert_eq!(updated.data["due"], 4_200);

    let error = plan_guest_account(
        &actor(),
        &key("guest-account-closed-0001"),
        &facts("account-1", "closed", 0, 0, ""),
        &update("New Bill", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_ACCOUNT_CLOSED");
}

#[test]
fn charge_add_derives_retained_amounts_and_records_source_key() {
    let plan = plan(
        &facts("account-1", "open", 0, 0, ""),
        &charge_add(10_000, " Room service ", "charge-key-0001", 1),
    );
    assert_eq!(plan.result.due, 11_770);
    assert_eq!(plan.result.account_id, "account-1");
    let created = plan.new_entry.as_ref().unwrap();
    assert_eq!(created.data["kind"], "charge");
    assert_eq!(created.data["net"], 10_000);
    assert_eq!(created.data["service"], 1_000);
    assert_eq!(created.data["tax"], 770);
    assert_eq!(created.data["total"], 11_770);
    assert_eq!(created.data["description"], "Room service");
    assert_eq!(created.data["date"], "2026-11-04");
    assert_eq!(created.data["stayId"], "stay-1");
    assert_eq!(created.data["actorId"], "user");
    assert_eq!(created.data["sourceKey"], "charge-key-0001");
    assert_eq!(created.data["originalId"], "");
    assert_eq!(plan.account.as_ref().unwrap().data["due"], 11_770);
    assert_eq!(plan.source_key.as_deref(), Some("charge-key-0001"));

    let mut inclusive = facts("account-1", "open", 0, 0, "");
    inclusive.property.config_json = json!({
        "taxBps": 700,
        "serviceBps": 1000,
        "taxInclusive": true,
        "approvedBy": "reviewer"
    })
    .to_string();
    let inclusive_plan = plan_guest_account(
        &actor(),
        &key("guest-account-inclusive-0001"),
        &inclusive,
        &charge_add(11_770, "Inclusive", "", 1),
        NOW,
    )
    .unwrap();
    let created = inclusive_plan.new_entry.as_ref().unwrap();
    assert_eq!(created.data["net"], 10_000);
    assert_eq!(created.data["service"], 1_000);
    assert_eq!(created.data["tax"], 770);
    assert_eq!(created.data["total"], 11_770);
}

#[test]
fn charge_refuses_closed_invoiced_advanced_and_invalid_input() {
    for (account_state, invoice, advance, expected) in [
        ("closed", "", 0, "PMS_GUEST_ACCOUNT_CLOSED"),
        ("open", "invoice-1", 0, "PMS_INVOICE_ISSUED"),
        ("open", "", 5_000, "PMS_GUEST_PAYMENT_PORT_REQUIRED"),
    ] {
        let error = plan_guest_account(
            &actor(),
            &key("guest-account-charge-guard-0001"),
            &facts("account-1", account_state, 0, advance, invoice),
            &charge_add(10_000, "Charge", "", 1),
            NOW,
        )
        .unwrap_err();
        assert_eq!(error.code, expected);
    }

    let mut refunded = facts("account-1", "open", 0, 0, "");
    refunded.account_refunds = vec![record(
        "refund-1",
        "refund",
        json!({"guestAccountId": "account-1", "state": "requested", "amount": 100}),
    )];
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-refund-0001"),
        &refunded,
        &charge_add(10_000, "Charge", "", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_REFUND_PENDING");

    let mismatch = charge_import(1_000, 0, 0, 1_001, 1);
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-import-0001"),
        &facts("account-1", "open", 0, 0, ""),
        &mismatch,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_ACCOUNT_REQUEST_INVALID");

    let error = plan_guest_account(
        &actor(),
        &key("guest-account-price-0001"),
        &facts("account-1", "open", 0, 0, ""),
        &charge_add(0, "Charge", "", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_ACCOUNT_REQUEST_INVALID");
}

#[test]
fn charge_replays_retained_source_key_without_writes() {
    let mut facts = facts("account-1", "open", 0, 0, "");
    facts.source_entry = Some(entry(
        "entry-prior",
        "account-1",
        "charge",
        "",
        100,
        0,
        0,
        100,
    ));
    let plan = plan(&facts, &charge_import(100, 0, 0, 100, 1));
    assert_eq!(plan.result.id, "entry-prior");
    assert_eq!(plan.result.account_id, "account-1");
    assert!(plan.new_entry.is_none());
    assert!(plan.account.is_none());
    assert!(plan.source_key.is_none());
}

#[test]
fn adjustment_reverses_source_and_bounds_credit_balance() {
    let mut base = facts("account-1", "open", 11_770, 0, "");
    base.original_entry = Some(entry(
        "entry-source",
        "account-1",
        "charge",
        "",
        10_000,
        1_000,
        770,
        11_770,
    ));
    let plan = plan(
        &base,
        &adjustment("entry-source", -10_000, -1_000, -770, -11_770, 1),
    );
    assert_eq!(plan.result.due, 0);
    let created = plan.new_entry.as_ref().unwrap();
    assert_eq!(created.data["kind"], "adjustment");
    assert_eq!(created.data["originalId"], "entry-source");
    assert_eq!(created.data["total"], -11_770);
    assert_eq!(plan.account.as_ref().unwrap().data["due"], 0);

    let wrong = adjustment("entry-source", -10_000, -1_000, -700, -11_700, 1);
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-adjust-total-0001"),
        &base,
        &wrong,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_REVERSAL_TOTAL");

    base.original_references = vec![entry(
        "entry-reversal",
        "account-1",
        "adjustment",
        "",
        -10_000,
        -1_000,
        -770,
        -11_770,
    )];
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-adjust-referenced-0001"),
        &base,
        &adjustment("entry-source", -10_000, -1_000, -770, -11_770, 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_ALREADY_REVERSED");

    let error = plan_guest_account(
        &actor(),
        &key("guest-account-adjust-credit-0001"),
        &facts("account-1", "open", 0, 0, ""),
        &adjustment("", -100, 0, 0, -100, 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_CREDIT_REQUIRES_FINANCE");

    let mut opening = facts("account-1", "open", 100, 0, "");
    opening.original_entry = Some(entry(
        "entry-opening",
        "account-1",
        "charge",
        "opening",
        100,
        0,
        0,
        100,
    ));
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-adjust-opening-0001"),
        &opening,
        &adjustment("entry-opening", -100, 0, 0, -100, 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_ALREADY_REVERSED");
}

#[test]
fn transfer_moves_unpaid_item_between_open_accounts() {
    let mut movement = facts("account-1", "open", 20_000, 0, "");
    movement.original_entry = Some(entry(
        "entry-source",
        "account-1",
        "charge",
        "",
        5_000,
        0,
        0,
        5_000,
    ));
    movement.destination = Some(account("account-2", "open", 0, 0, ""));
    let plan = plan(&movement, &transfer("account-2", 1));
    assert_eq!(plan.result.id, plan.new_entry.as_ref().unwrap().id);
    assert_eq!(plan.result.due, 15_000);
    assert_eq!(plan.account.as_ref().unwrap().data["due"], 15_000);
    assert_eq!(plan.destination.as_ref().unwrap().data["due"], 5_000);
    let from_entry = plan.new_entry.as_ref().unwrap();
    assert_eq!(from_entry.data["guestAccountId"], "account-1");
    assert_eq!(from_entry.data["total"], -5_000);
    assert_eq!(from_entry.data["originalId"], "entry-source");
    let to_entry = plan.new_destination_entry.as_ref().unwrap();
    assert_eq!(to_entry.data["guestAccountId"], "account-2");
    assert_eq!(to_entry.data["total"], 5_000);
    assert_eq!(to_entry.data["originalId"], from_entry.id);

    movement.account = Some(account("account-1", "open", 4_000, 0, ""));
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-transfer-paid-0001"),
        &movement,
        &transfer("account-2", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_PAID_CHARGE");

    movement.account = Some(account("account-1", "open", 20_000, 0, ""));
    movement.destination = Some(account("account-1", "open", 0, 0, ""));
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-transfer-same-0001"),
        &movement,
        &transfer("account-1", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_TRANSFER_INVALID");

    movement.destination = Some(account("account-2", "open", 0, 1_000, ""));
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-transfer-advance-0001"),
        &movement,
        &transfer("account-2", 1),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_GUEST_PAYMENT_PORT_REQUIRED");
}

#[test]
fn requires_current_branch_scope_and_cashier_permission() {
    let command = charge_add(10_000, "Charge", "", 1);
    let base = facts("account-1", "open", 0, 0, "");

    let mut selected = actor();
    selected.board.permissions = vec!["pms.read".into()];
    let error = selected
        .require_command(PmsGuestAccountOperation::ChargeAdd, NOW)
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");

    let mut selected = actor();
    selected.board.department_id = Some("front".into());
    let error = selected
        .require_command(PmsGuestAccountOperation::ChargeAdd, NOW)
        .unwrap_err();
    assert_eq!(error.code, "PMS_BRANCH_REQUIRED");

    let error = actor()
        .require_command(PmsGuestAccountOperation::ChargeAdd, 10_000)
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    actor()
        .require_command(PmsGuestAccountOperation::ChargeAdd, NOW)
        .unwrap();

    let mut selected = actor();
    selected.board.branch_id = Some("south".into());
    let error = plan_guest_account(
        &selected,
        &key("guest-account-other-branch-0001"),
        &base,
        &command,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut other_company = base.clone();
    other_company.property.company_id = "other".into();
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-other-company-0001"),
        &other_company,
        &command,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut setup = base.clone();
    setup.property.state = "setup".into();
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-setup-0001"),
        &setup,
        &command,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_SETUP_REQUIRED");

    let mut closing = base.clone();
    closing.property.state = "closing".into();
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-closing-0001"),
        &closing,
        &command,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_DAY_CLOSING");

    let stale = charge_add(10_000, "Charge", "", 2);
    let error = plan_guest_account(
        &actor(),
        &key("guest-account-stale-0001"),
        &base,
        &stale,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_VERSION_CONFLICT");
}

#[test]
fn price_amounts_matches_retained_rounding_and_bounds() {
    assert_eq!(
        price_amounts(10_000, 700, 1000, false).unwrap(),
        (10_000, 1_000, 770, 11_770)
    );
    assert_eq!(
        price_amounts(11_770, 700, 1000, true).unwrap(),
        (10_000, 1_000, 770, 11_770)
    );
    assert!(price_amounts(0, 700, 1000, false).is_err());
    assert!(price_amounts(10_000, 10_001, 1000, false).is_err());
}

struct FakeRepository {
    replay: Option<PmsGuestAccountResult>,
    loaded: Arc<AtomicBool>,
    committed: Arc<AtomicBool>,
    expected_hash: String,
}

impl PmsGuestAccountRepository for FakeRepository {
    type Error = BusinessError;

    fn replay<'a>(
        &'a mut self,
        _key: &'a CommandKey,
        request_hash: &'a str,
        _command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, Option<PmsGuestAccountResult>, Self::Error> {
        assert_eq!(request_hash, self.expected_hash);
        let replay = self.replay.clone();
        Box::pin(async move { Ok(replay) })
    }

    fn load<'a>(
        &'a mut self,
        _actor: &'a PmsGuestAccountActor,
        _command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, PmsGuestAccountFacts, Self::Error> {
        self.loaded.store(true, Ordering::SeqCst);
        Box::pin(async move { Ok(PmsGuestAccountFacts::empty(property())) })
    }

    fn commit<'a>(
        &'a mut self,
        _actor: &'a PmsGuestAccountActor,
        _key: &'a CommandKey,
        _request_hash: &'a str,
        _plan: PmsGuestAccountPlan,
        _now: i64,
    ) -> PmsBoardFuture<'a, PmsGuestAccountResult, Self::Error> {
        self.committed.store(true, Ordering::SeqCst);
        Box::pin(async move { panic!("commit must not run during replay") })
    }
}

#[tokio::test]
async fn application_replays_receipt_before_any_owner_load() {
    let command = update("  New Bill  ", 1);
    let mut normalized = command.clone();
    normalized.normalize();
    let hash = guest_account_request_hash(&actor(), &normalized).unwrap();
    let replayed = PmsGuestAccountResult {
        id: "account-1".into(),
        account_id: "account-1".into(),
        property_id: "property".into(),
        version: 2,
        state: "open".into(),
        due: 0,
        advance: 0,
        deposit: 0,
    };
    let loaded = Arc::new(AtomicBool::new(false));
    let committed = Arc::new(AtomicBool::new(false));
    let mut repository = FakeRepository {
        replay: Some(replayed.clone()),
        loaded: loaded.clone(),
        committed: committed.clone(),
        expected_hash: hash,
    };
    let result = pms_guest_account(
        &actor(),
        &key("guest-account-replay-0001"),
        &command,
        &mut repository,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result, replayed);
    assert!(!loaded.load(Ordering::SeqCst));
    assert!(!committed.load(Ordering::SeqCst));

    let mut repository = FakeRepository {
        replay: Some(PmsGuestAccountResult {
            version: 3,
            ..replayed
        }),
        loaded: Arc::new(AtomicBool::new(false)),
        committed: Arc::new(AtomicBool::new(false)),
        expected_hash: guest_account_request_hash(&actor(), &normalized).unwrap(),
    };
    let error = pms_guest_account(
        &actor(),
        &key("guest-account-replay-bad-0001"),
        &command,
        &mut repository,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
}
