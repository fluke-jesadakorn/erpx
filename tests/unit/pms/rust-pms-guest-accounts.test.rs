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

/// Native booking-refund producer: money rules, guards and effects.
mod booking_refunds {

    use erpx_server_pms::public::refund::{
        PmsBoardFuture, PmsProviderReceipt, PmsProviderRefund, PmsRefundFinanceRow, PmsRefundPart,
    };
    use erpx_server_pms::public::refund::{
        PmsRefundMail, PmsRefundPayment, PmsRefundPrior, PmsRefundProvider, PmsRefundPurchaseGuard,
        PmsRefundRepository, PmsRefundRequest, PmsRefundSelection, PmsRefundSource, approve_refund,
        recover_receipt, refunds_for_finance, request_refund, retry_refund, settle_refund,
    };
    use erpx_server_pms::public::room_board::PmsBoardActor;
    use erpx_server_pms::public::room_board::PmsRecord;
    use erpx_server_shared::public::BusinessError;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn actor(user: &str, branch: Option<&str>) -> PmsBoardActor {
        PmsBoardActor {
            user_id: user.into(),
            company_id: "co-1".into(),
            branch_id: branch.map(str::to_owned),
            department_id: None,
            permissions: vec![
                "pms.read".into(),
                "pms.approve".into(),
                "finance.read".into(),
                "finance.approve".into(),
            ],
            modules: vec!["pms".into()],
            expires_at: i64::MAX,
        }
    }

    fn payment() -> PmsRefundPayment {
        PmsRefundPayment {
            id: "pay-1".into(),
            property_id: "prop-1".into(),
            reservation_id: "res-1".into(),
            company_id: "co-1".into(),
            holding_slug: "acme".into(),
            account_id: "acct-1".into(),
            livemode: false,
            amount_minor: 5000,
            status: "confirmed".into(),
            checkout_id: Some("cs-1".into()),
            clearing_account: Some("edc-1".into()),
            methods_json: "[\"card\"]".into(),
            created_at: 1,
            provider_succeeded_at: None,
        }
    }

    fn account() -> PmsRecord {
        PmsRecord {
            id: "ga-1".into(),
            kind: "guest_account".into(),
            version: 3,
            updated_at: 0,
            data: json!({
                "name": "Guest",
                "guestId": "g-1",
                "reservationId": "res-1",
                "state": "open",
                "due": 0,
                "advance": 5000,
                "advanceTax": 350,
                "deposit": 0,
                "refundReserved": 0,
            }),
        }
    }

    #[derive(Default)]
    struct Store {
        payments: HashMap<String, PmsRefundPayment>,
        requests: HashMap<String, PmsRefundRequest>,
        accounts: HashMap<String, PmsRecord>,
        sources: Vec<PmsRefundSource>,
        previous: Vec<PmsRefundPrior>,
        entries: Vec<PmsRecord>,
        finance: Vec<PmsRefundFinanceRow>,

        write_context: usize,
        history: usize,
        receipt: Option<PmsRecord>,
        stay_account: Option<String>,
        refundable: Vec<PmsRefundRequest>,
        approval: Option<(String, String)>,
    }

    #[derive(Clone)]
    struct Fake {
        store: Arc<Mutex<Store>>,
    }

    impl Fake {
        fn new() -> Self {
            let mut store = Store::default();
            store.payments.insert("pay-1".into(), payment());
            store.accounts.insert("ga-1".into(), account());
            store.sources.push(PmsRefundSource {
                stay_id: "stay-1".into(),
                guest_account_id: "ga-1".into(),
                entry_id: "entry-1".into(),
                amount_minor: 5000,
            });
            store.stay_account = Some("ga-1".into());
            Self {
                store: Arc::new(Mutex::new(store)),
            }
        }
    }

    fn state(store: &Arc<Mutex<Store>>) -> std::sync::MutexGuard<'_, Store> {
        store.lock().expect("store")
    }

    impl PmsRefundRepository for Fake {
        type Error = BusinessError;

        fn payment<'a>(
            &'a mut self,
            payment_id: &'a str,
            property_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
            Box::pin(async move {
                Ok(state(&self.store)
                    .payments
                    .get(payment_id)
                    .filter(|payment| payment.property_id == property_id)
                    .cloned())
            })
        }

        fn payment_by_id<'a>(
            &'a mut self,
            payment_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).payments.get(payment_id).cloned()) })
        }

        fn request<'a>(
            &'a mut self,
            request_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).requests.get(request_id).cloned()) })
        }

        fn locked_request<'a>(
            &'a mut self,
            request_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).requests.get(request_id).cloned()) })
        }

        fn sources<'a>(
            &'a mut self,
            _payment_id: &'a str,
        ) -> PmsBoardFuture<'a, Vec<PmsRefundSource>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).sources.clone()) })
        }

        fn previous<'a>(
            &'a mut self,
            _payment_id: &'a str,
        ) -> PmsBoardFuture<'a, Vec<PmsRefundPrior>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).previous.clone()) })
        }

        fn account<'a>(
            &'a mut self,
            _property_id: &'a str,
            account_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).accounts.get(account_id).cloned()) })
        }

        fn entries<'a>(
            &'a mut self,
            _property_id: &'a str,
            guest_account_id: &'a str,
        ) -> PmsBoardFuture<'a, Vec<PmsRecord>, Self::Error> {
            Box::pin(async move {
                Ok(state(&self.store)
                    .entries
                    .iter()
                    .filter(|entry| {
                        entry
                            .data
                            .get("guestAccountId")
                            .and_then(serde_json::Value::as_str)
                            == Some(guest_account_id)
                    })
                    .cloned()
                    .collect())
            })
        }

        fn property<'a>(
            &'a mut self,
            property_id: &'a str,
        ) -> PmsBoardFuture<
            'a,
            Option<erpx_server_pms::public::refund::PmsRefundProperty>,
            Self::Error,
        > {
            Box::pin(async move {
                Ok(Some(erpx_server_pms::public::refund::PmsRefundProperty {
                    id: property_id.to_owned(),
                    company_id: "co-1".into(),
                    branch_id: "branch-1".into(),
                    business_date: "2026-01-01".into(),
                    config_json: json!({"onlineAccounting": {"approvedBy": "finance-1"}})
                        .to_string(),
                }))
            })
        }

        fn save_account<'a>(
            &'a mut self,
            account: &'a PmsRecord,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                state(&self.store)
                    .accounts
                    .insert(account.id.clone(), account.clone());
                Ok(())
            })
        }

        fn insert_entry<'a>(
            &'a mut self,
            _property_id: &'a str,
            entry: &'a PmsRecord,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                state(&self.store).entries.push(entry.clone());
                Ok(())
            })
        }

        fn insert_request<'a>(
            &'a mut self,
            request: &'a PmsRefundRequest,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                state(&self.store)
                    .requests
                    .insert(request.id.clone(), request.clone());
                Ok(())
            })
        }

        fn mark_payment_refund<'a>(
            &'a mut self,
            payment_id: &'a str,
            state_value: &'a str,
            _requested_at: Option<i64>,
            _approved_by: Option<&'a str>,
            _refund_id: Option<&'a str>,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                let mut store = state(&self.store);
                if let Some(payment) = store.payments.get_mut(payment_id) {
                    let _ = state_value;
                    payment.provider_succeeded_at = payment.provider_succeeded_at.or(Some(0));
                }
                Ok(())
            })
        }

        fn update_request<'a>(
            &'a mut self,
            request_id: &'a str,
            state_value: &'a str,
            approved_by: Option<&'a str>,
            provider_id: Option<&'a str>,
            last_error: Option<&'a str>,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                let mut store = state(&self.store);
                if let Some(request) = store.requests.get_mut(request_id) {
                    request.state = state_value.to_owned();
                    if let Some(value) = approved_by {
                        request.approved_by = Some(value.to_owned());
                    }
                    request.provider_id = provider_id.map(str::to_owned);
                    if state_value == "approved" && last_error.is_some() {
                        request.generation += 1;
                    }
                }
                Ok(())
            })
        }

        fn insert_attempt<'a>(
            &'a mut self,
            _request_id: &'a str,
            _generation: i64,
            _provider_id: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move { Ok(()) })
        }

        fn bind_provider_refund<'a>(
            &'a mut self,
            request_id: &'a str,
            _payment_id: &'a str,
            provider_id: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                let mut store = state(&self.store);
                if let Some(request) = store.requests.get_mut(request_id) {
                    if request.provider_id.is_none() {
                        request.provider_id = Some(provider_id.to_owned());
                        request.state = "processing".into();
                    }
                }
                Ok(())
            })
        }

        fn finance_rows<'a>(
            &'a mut self,
            _company_id: &'a str,
        ) -> PmsBoardFuture<'a, Vec<PmsRefundFinanceRow>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).finance.clone()) })
        }

        fn refundable_requests<'a>(
            &'a mut self,
            _request_id: &'a str,
        ) -> PmsBoardFuture<'a, Vec<PmsRefundRequest>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).refundable.clone()) })
        }

        fn approval_target<'a>(
            &'a mut self,
            _id: &'a str,
            _company_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<(String, String)>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).approval.clone()) })
        }

        fn recovery_payment<'a>(
            &'a mut self,
            _request_id: &'a str,
            _company_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).payments.get("pay-1").cloned()) })
        }

        fn stay_account<'a>(
            &'a mut self,
            _property_id: &'a str,
            _reservation_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<String>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).stay_account.clone()) })
        }

        fn receipt_entry<'a>(
            &'a mut self,
            _property_id: &'a str,
            _payment_id: &'a str,
        ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
            Box::pin(async move { Ok(state(&self.store).receipt.clone()) })
        }

        fn mark_receipt_recovered<'a>(
            &'a mut self,
            payment_id: &'a str,
            provider_intent_id: &'a str,
            succeeded_at: i64,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                let mut store = state(&self.store);
                if let Some(payment) = store.payments.get_mut(payment_id) {
                    payment.provider_succeeded_at = Some(succeeded_at);
                    payment.account_id = provider_intent_id.to_owned();
                }
                Ok(())
            })
        }

        fn append_reservation_history<'a>(
            &'a mut self,
            _property_id: &'a str,
            _reservation_id: &'a str,
            _actor_id: &'a str,
            _action: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                state(&self.store).history += 1;
                Ok(())
            })
        }

        fn write_context<'a>(&'a mut self) -> PmsBoardFuture<'a, (), Self::Error> {
            Box::pin(async move {
                state(&self.store).write_context += 1;
                Ok(())
            })
        }
    }

    struct FakeProvider {
        refund: PmsProviderRefund,
        receipt: PmsProviderReceipt,
    }

    impl FakeProvider {
        fn ok() -> Self {
            Self {
                refund: PmsProviderRefund {
                    id: "re-1".into(),
                    amount: 2000,
                    currency: Some("thb".into()),
                    status: "succeeded".into(),
                    payment_id: Some("pay-1".into()),
                    request_id: Some("req-1".into()),
                },
                receipt: PmsProviderReceipt {
                    session_paid: true,
                    payment_intent_id: "pi-1".into(),
                    succeeded: true,
                    amount_received: 5000,
                    currency: "thb".into(),
                    payment_id: Some("pay-1".into()),
                    succeeded_at: 5,
                },
            }
        }
    }

    impl PmsRefundProvider for FakeProvider {
        fn provider_refund<'a>(
            &'a self,
            _payment: &'a PmsRefundPayment,
            _refund_id: &'a str,
        ) -> PmsBoardFuture<'a, PmsProviderRefund, BusinessError> {
            Box::pin(async move { Ok(self.refund.clone()) })
        }

        fn create_refund<'a>(
            &'a self,
            _payment: &'a PmsRefundPayment,
            _request: &'a PmsRefundRequest,
            _part: &'a [PmsRefundPart],
            _promptpay: bool,
            _email: &'a str,
            _idempotency_key: &'a str,
        ) -> PmsBoardFuture<'a, PmsProviderRefund, BusinessError> {
            Box::pin(async move { Ok(self.refund.clone()) })
        }

        fn promptpay<'a>(
            &'a self,
            _payment: &'a PmsRefundPayment,
        ) -> PmsBoardFuture<'a, bool, BusinessError> {
            Box::pin(async move { Ok(false) })
        }

        fn receipt<'a>(
            &'a self,
            _payment: &'a PmsRefundPayment,
        ) -> PmsBoardFuture<'a, PmsProviderReceipt, BusinessError> {
            Box::pin(async move { Ok(self.receipt.clone()) })
        }

        fn guest_email<'a>(
            &'a self,
            _payment: &'a PmsRefundPayment,
        ) -> PmsBoardFuture<'a, String, BusinessError> {
            Box::pin(async move { Ok("guest@example.com".into()) })
        }
    }

    struct Guard;
    impl PmsRefundPurchaseGuard for Guard {
        fn require_settle_existing<'a>(
            &'a self,
            _property_id: &'a str,
            _permission: Option<&'a str>,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            Box::pin(async move { Ok(()) })
        }
    }

    struct Mail(Arc<Mutex<usize>>);
    impl PmsRefundMail for Mail {
        fn queue_refund<'a>(
            &'a self,
            _reservation_id: &'a str,
            _holding_slug: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            Box::pin(async move {
                *self.0.lock().expect("mail") += 1;
                Ok(())
            })
        }
    }

    fn seed_request(fake: &Fake, state_value: &str, requested_by: &str, parts: &str) {
        let mut store = state(&fake.store);
        store.requests.insert(
            "req-1".into(),
            PmsRefundRequest {
                id: "req-1".into(),
                payment_id: "pay-1".into(),
                amount_minor: 2000,
                allocations_json: parts.into(),
                reason: "Guest asked".into(),
                requested_by: requested_by.into(),
                approved_by: None,
                provider_id: None,
                state: state_value.into(),
                generation: 0,
                legacy_review: false,
            },
        );
        store.approval = Some(("req-1".into(), "prop-1".into()));
    }

    #[tokio::test]
    async fn request_reserves_original_receipt_funding() {
        let mut fake = Fake::new();
        let result = request_refund(
            &mut fake,
            "pay-1",
            "prop-1",
            &[PmsRefundSelection {
                stay_id: "stay-1".into(),
                amount: 2000,
            }],
            "Guest asked",
            "user-1",
            "req-1",
            10,
        )
        .await
        .expect("request");
        assert_eq!(result.state, "requested");
        assert_eq!(result.amount_minor, Some(2000));
        let store = state(&fake.store);
        let account = store.accounts.get("ga-1").expect("account");
        assert_eq!(account.data["refundReserved"], json!(2000));
        assert_eq!(account.data["advance"], json!(5000));
        assert_eq!(account.version, 4);
        let request = store.requests.get("req-1").expect("request row");
        assert_eq!(request.amount_minor, 2000);
        assert!(request.allocations_json.contains("entry-1"));
    }

    #[tokio::test]
    async fn request_replays_the_retained_key() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "requested",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        let result = request_refund(
            &mut fake,
            "pay-1",
            "prop-1",
            &[PmsRefundSelection {
                stay_id: "stay-1".into(),
                amount: 2000,
            }],
            "Guest asked",
            "user-1",
            "req-1",
            10,
        )
        .await
        .expect("replay");
        assert_eq!(result.amount_minor, Some(2000));
        let store = state(&fake.store);
        assert_eq!(store.accounts.get("ga-1").unwrap().version, 3);
    }

    #[tokio::test]
    async fn request_rejects_key_reuse_with_other_amounts() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "requested",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":1500}]",
        );
        let error = request_refund(
            &mut fake,
            "pay-1",
            "prop-1",
            &[PmsRefundSelection {
                stay_id: "stay-1".into(),
                amount: 2000,
            }],
            "Guest asked",
            "user-1",
            "req-1",
            10,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PMS_REFUND_KEY_REUSED");
    }

    #[tokio::test]
    async fn approval_requires_an_independent_manager() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "requested",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        let error = approve_refund(
            &mut fake,
            &Guard,
            &actor("user-1", Some("branch-1")),
            "req-1",
            11,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PMS_INDEPENDENT_REFUND_APPROVAL");
    }

    #[tokio::test]
    async fn approval_records_liability_and_postings() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "requested",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        {
            let mut store = state(&fake.store);
            store.accounts.get_mut("ga-1").unwrap().data["refundReserved"] = json!(2000);
        }
        let outcome = approve_refund(
            &mut fake,
            &Guard,
            &actor("user-2", Some("branch-1")),
            "req-1",
            11,
        )
        .await
        .expect("approve");
        assert_eq!(outcome.result.state, "approved");
        assert_eq!(outcome.effects.postings.len(), 2);
        assert_eq!(outcome.effects.postings[0].lines[0].account, "advance");
        let store = state(&fake.store);
        let account = store.accounts.get("ga-1").unwrap();
        assert_eq!(account.data["advance"], json!(3000));
        assert_eq!(account.data["refundReserved"], json!(0));
        assert_eq!(store.requests.get("req-1").unwrap().state, "approved");
        assert_eq!(store.entries.len(), 1);
    }

    #[tokio::test]
    async fn approval_requires_finance_online_accounts() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "requested",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        // Property config without onlineAccounting approval.
        let mut store = state(&fake.store);
        store.accounts.get_mut("ga-1").unwrap().data["refundReserved"] = json!(2000);
        drop(store);
        struct Unapproved;
        impl PmsRefundRepository for Unapproved {
            type Error = BusinessError;
            fn property<'a>(
                &'a mut self,
                property_id: &'a str,
            ) -> PmsBoardFuture<
                'a,
                Option<erpx_server_pms::public::refund::PmsRefundProperty>,
                Self::Error,
            > {
                Box::pin(async move {
                    Ok(Some(erpx_server_pms::public::refund::PmsRefundProperty {
                        id: property_id.to_owned(),
                        company_id: "co-1".into(),
                        branch_id: "branch-1".into(),
                        business_date: "2026-01-01".into(),
                        config_json: "{}".into(),
                    }))
                })
            }
            // Delegate the rest to the shared fake via a boxed inner store is
            // unnecessary: the online-accounting gate is checked before any read.
            fn payment<'a>(
                &'a mut self,
                _id: &'a str,
                _property: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn payment_by_id<'a>(
                &'a mut self,
                _id: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn request<'a>(
                &'a mut self,
                _id: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn locked_request<'a>(
                &'a mut self,
                id: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error> {
                Box::pin(async move {
                    Ok(Some(PmsRefundRequest { id: id.into(), payment_id: "pay-1".into(), amount_minor: 2000, allocations_json: "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]".into(), reason: "Guest asked".into(), requested_by: "user-1".into(), approved_by: None, provider_id: None, state: "requested".into(), generation: 0, legacy_review: false }))
                })
            }
            fn sources<'a>(
                &'a mut self,
                _id: &'a str,
            ) -> PmsBoardFuture<'a, Vec<PmsRefundSource>, Self::Error> {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn previous<'a>(
                &'a mut self,
                _id: &'a str,
            ) -> PmsBoardFuture<'a, Vec<PmsRefundPrior>, Self::Error> {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn account<'a>(
                &'a mut self,
                _p: &'a str,
                _a: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn entries<'a>(
                &'a mut self,
                _p: &'a str,
                _a: &'a str,
            ) -> PmsBoardFuture<'a, Vec<PmsRecord>, Self::Error> {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn save_account<'a>(
                &'a mut self,
                _a: &'a PmsRecord,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn insert_entry<'a>(
                &'a mut self,
                _p: &'a str,
                _e: &'a PmsRecord,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn insert_request<'a>(
                &'a mut self,
                _r: &'a PmsRefundRequest,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn mark_payment_refund<'a>(
                &'a mut self,
                _p: &'a str,
                _s: &'a str,
                _r: Option<i64>,
                _b: Option<&'a str>,
                _f: Option<&'a str>,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn update_request<'a>(
                &'a mut self,
                _r: &'a str,
                _s: &'a str,
                _b: Option<&'a str>,
                _p: Option<&'a str>,
                _e: Option<&'a str>,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn insert_attempt<'a>(
                &'a mut self,
                _r: &'a str,
                _g: i64,
                _p: &'a str,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn bind_provider_refund<'a>(
                &'a mut self,
                _r: &'a str,
                _p: &'a str,
                _f: &'a str,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn finance_rows<'a>(
                &'a mut self,
                _c: &'a str,
            ) -> PmsBoardFuture<'a, Vec<PmsRefundFinanceRow>, Self::Error> {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn refundable_requests<'a>(
                &'a mut self,
                _r: &'a str,
            ) -> PmsBoardFuture<'a, Vec<PmsRefundRequest>, Self::Error> {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn approval_target<'a>(
                &'a mut self,
                _i: &'a str,
                _c: &'a str,
            ) -> PmsBoardFuture<'a, Option<(String, String)>, Self::Error> {
                Box::pin(async { Ok(Some(("req-1".into(), "prop-1".into()))) })
            }
            fn recovery_payment<'a>(
                &'a mut self,
                _r: &'a str,
                _c: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn stay_account<'a>(
                &'a mut self,
                _p: &'a str,
                _r: &'a str,
            ) -> PmsBoardFuture<'a, Option<String>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn receipt_entry<'a>(
                &'a mut self,
                _p: &'a str,
                _i: &'a str,
            ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error> {
                Box::pin(async { Ok(None) })
            }
            fn mark_receipt_recovered<'a>(
                &'a mut self,
                _p: &'a str,
                _i: &'a str,
                _s: i64,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn append_reservation_history<'a>(
                &'a mut self,
                _p: &'a str,
                _r: &'a str,
                _a: &'a str,
                _x: &'a str,
                _n: i64,
            ) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
            fn write_context<'a>(&'a mut self) -> PmsBoardFuture<'a, (), Self::Error> {
                Box::pin(async { Ok(()) })
            }
        }
        let error = approve_refund(
            &mut Unapproved,
            &Guard,
            &actor("user-2", Some("branch-1")),
            "req-1",
            11,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PMS_ONLINE_ACCOUNTING_REQUIRED");
        let _ = &mut fake;
    }

    #[tokio::test]
    async fn settle_posts_entries_and_queues_refund_mail() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "approved",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        {
            let mut store = state(&fake.store);
            let request = store.requests.get("req-1").expect("request").clone();
            store.refundable = vec![request];
        }
        let mail = Mail(Arc::new(Mutex::new(0)));
        let outcomes = settle_refund(&mut fake, &FakeProvider::ok(), &Guard, &mail, "req-1", 12)
            .await
            .expect("settle");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].result.state, "succeeded");
        assert_eq!(
            outcomes[0].effects.postings[0].lines[0].account,
            "refundPayable"
        );
        assert_eq!(outcomes[0].effects.documents[0].kind, "refund");
        assert_eq!(*mail.0.lock().unwrap(), 1);
        let store = state(&fake.store);
        assert_eq!(store.requests.get("req-1").unwrap().state, "succeeded");
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].data["total"], json!(-2000));
    }

    #[tokio::test]
    async fn settle_records_a_failed_provider_refund_without_posting() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "approved",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        {
            let mut store = state(&fake.store);
            let request = store.requests.get("req-1").expect("request").clone();
            store.refundable = vec![request];
        }
        let mut provider = FakeProvider::ok();
        provider.refund.status = "failed".into();
        let mail = Mail(Arc::new(Mutex::new(0)));
        let outcomes = settle_refund(&mut fake, &provider, &Guard, &mail, "req-1", 12)
            .await
            .expect("settle");
        assert!(outcomes.is_empty());
        let store = state(&fake.store);
        assert_eq!(store.requests.get("req-1").unwrap().state, "failed");
        assert!(store.entries.is_empty());
        assert_eq!(*mail.0.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn settle_keeps_processing_when_provider_is_pending() {
        let mut fake = Fake::new();
        seed_request(
            &fake,
            "approved",
            "user-1",
            "[{\"stayId\":\"stay-1\",\"guestAccountId\":\"ga-1\",\"entryId\":\"entry-1\",\"amount\":2000}]",
        );
        {
            let mut store = state(&fake.store);
            let request = store.requests.get("req-1").expect("request").clone();
            store.refundable = vec![request];
        }
        let mut provider = FakeProvider::ok();
        provider.refund.status = "processing".into();
        let mail = Mail(Arc::new(Mutex::new(0)));
        let outcomes = settle_refund(&mut fake, &provider, &Guard, &mail, "req-1", 12)
            .await
            .expect("settle");
        assert!(outcomes.is_empty());
        let store = state(&fake.store);
        assert_eq!(store.requests.get("req-1").unwrap().state, "processing");
    }

    #[tokio::test]
    async fn finance_list_requires_company_scope() {
        let mut fake = Fake::new();
        state(&fake.store).finance.push(PmsRefundFinanceRow {
            id: "req-1".into(),
            payment_id: "pay-1".into(),
            property_id: "prop-1".into(),
            amount_minor: 2000,
            status: "refund_review".into(),
            refund_state: "requested".into(),
            last_error: None,
            reason: "Guest asked".into(),
            created_at: 3,
            needs_receipt_recovery: true,
        });
        let rows = refunds_for_finance(&mut fake, &actor("user-3", None), 20)
            .await
            .expect("rows");
        assert_eq!(rows.len(), 1);
        let branch = refunds_for_finance(&mut fake, &actor("user-3", Some("branch-1")), 20)
            .await
            .expect("branch");
        assert!(branch.is_empty());
    }

    #[tokio::test]
    async fn retry_refuses_a_pending_provider_refund() {
        let mut fake = Fake::new();
        seed_request(&fake, "failed", "user-1", "[]");
        {
            let mut store = state(&fake.store);
            store.requests.get_mut("req-1").unwrap().provider_id = Some("re-1".into());
        }
        let mut provider = FakeProvider::ok();
        provider.refund.status = "processing".into();
        let error = retry_refund(
            &mut fake,
            &provider,
            &Guard,
            &actor("user-2", Some("branch-1")),
            "req-1",
            "Provider failed once",
            13,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PMS_REFUND_PROVIDER_PENDING");
    }

    #[tokio::test]
    async fn receipt_recovery_rejects_a_mismatched_receipt() {
        let mut fake = Fake::new();
        seed_request(&fake, "requested", "user-1", "[]");
        let mut store = state(&fake.store);
        store.payments.get_mut("pay-1").unwrap().status = "refund_review".into();
        drop(store);
        let mut provider = FakeProvider::ok();
        provider.receipt.amount_received = 4000;
        let error = recover_receipt(
            &mut fake,
            &provider,
            &Guard,
            &actor("finance-1", None),
            "req-1",
            "Recover missing receipt",
            14,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PMS_PAYMENT_MISMATCH");
    }

    #[tokio::test]
    async fn receipt_recovery_posts_the_unallocated_receipt() {
        let mut fake = Fake::new();
        seed_request(&fake, "requested", "user-1", "[]");
        state(&fake.store).payments.get_mut("pay-1").unwrap().status = "refund_review".into();
        let outcome = recover_receipt(
            &mut fake,
            &FakeProvider::ok(),
            &Guard,
            &actor("finance-1", None),
            "req-1",
            "Recover missing receipt",
            14,
        )
        .await
        .expect("recover");
        assert_eq!(outcome.result.state, "receipt_recovered");
        assert_eq!(outcome.effects.postings[0].lines[0].account, "edc");
        let store = state(&fake.store);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(
            store.entries[0].data["sourceKey"],
            json!("online-payment:pay-1")
        );
        assert_eq!(store.history, 1);
    }
}
