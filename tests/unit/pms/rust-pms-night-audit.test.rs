// Native PMS day-close command domain/application facts. No database, browser,
// adapter, persisted receipt or HTTP acceptance is inferred.
mod domain {
    #[path = "../../../../packages/server-pms/src/domain/night_audit.rs"]
    pub mod night_audit;
    #[path = "../../../../packages/server-pms/src/domain/room_board.rs"]
    pub mod room_board;
}
mod application {
    #[path = "../../../../packages/server-pms/src/application/night_audit.rs"]
    pub mod night_audit;
    #[path = "../../../../packages/server-pms/src/application/room_board.rs"]
    pub mod room_board;
}

use application::night_audit::night_audit_request_hash;
use domain::night_audit::{
    NIGHT_ACCOUNT_KINDS, PmsNightAuditActor, PmsNightAuditCommand, PmsNightAuditFacts,
    PmsNightAuditGuard, PmsNightAuditOperation, PmsNightDayControlFacts, PmsNightJobPlan,
    PmsNightSetupFacts, PmsNightSetupScope, next_business_date, night_blockers, plan_night_audit,
    require_setup,
};
use domain::room_board::{PmsBoardActor, PmsPropertyRow, PmsRecord};
use erpx_server_shared::public::CommandKey;
use serde_json::{Value, json};

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn actor() -> PmsNightAuditActor {
    PmsNightAuditActor {
        board: PmsBoardActor {
            user_id: "night-auditor".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            permissions: vec!["pms.read".into(), "pms.audit".into(), "pms.reports".into()],
            modules: vec!["pms".into()],
            expires_at: 10_000,
        },
        assignment_id: "assignment".into(),
        role: "pms_night_auditor".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn config() -> String {
    let accounts: serde_json::Map<String, Value> = NIGHT_ACCOUNT_KINDS
        .iter()
        .map(|kind| ((*kind).to_owned(), Value::String(format!("account-{kind}"))))
        .collect();
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

fn property_row() -> PmsPropertyRow {
    PmsPropertyRow {
        id: "property".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        name: "Hotel".into(),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        config_json: config(),
        version: 1,
    }
}

fn record(id: &str, kind: &str, data: Value) -> PmsRecord {
    PmsRecord {
        id: id.into(),
        kind: kind.into(),
        version: 1,
        updated_at: 1,
        data,
    }
}

fn room_type(id: &str) -> PmsRecord {
    record(
        id,
        "room_type",
        json!({"name": id, "capacity": 2, "amenities": ""}),
    )
}

fn room(id: &str, type_id: &str, cleanliness: &str, cycle: i64) -> PmsRecord {
    record(
        id,
        "room",
        json!({
            "number": id,
            "typeId": type_id,
            "building": "Main",
            "floor": "1",
            "active": true,
            "cleanliness": cleanliness,
            "cleaningCycle": cycle,
            "dnd": false
        }),
    )
}

fn rate(id: &str, active: bool) -> PmsRecord {
    record(
        id,
        "rate",
        json!({
            "name": id, "typeId": "type-1", "from": "2026-01-01", "to": "2027-12-31",
            "price": 100, "extraAdult": 0, "includedGuests": 2, "minNights": 1,
            "maxNights": 30, "stopSell": false, "closedArrival": false,
            "closedDeparture": false, "market": "public", "partyId": "",
            "active": active
        }),
    )
}

fn stay(id: &str, state: &str, arrival: &str, departure: &str, room_id: &str) -> PmsRecord {
    record(
        id,
        "stay",
        json!({
            "reservationId": format!("reservation-{id}"),
            "guestId": format!("guest-{id}"),
            "typeId": "type-1",
            "rateId": "rate-1",
            "roomId": room_id,
            "arrival": arrival,
            "departure": departure,
            "adults": 2,
            "state": state,
            "guestAccountId": format!("account-{id}"),
            "prices": [],
            "segments": []
        }),
    )
}

fn shift(id: &str, state: &str, variance: i64) -> PmsRecord {
    record(
        id,
        "shift",
        json!({
            "name": id, "userId": "cashier", "date": "2026-11-04", "state": state,
            "opening": 0, "cash": 0, "counted": 0, "variance": variance,
            "handover": "", "reviewedBy": "", "movements": []
        }),
    )
}

fn night_record(id: &str, date: &str, state: &str, message: &str) -> PmsRecord {
    record(
        id,
        "night",
        json!({"date": date, "state": state, "initiatedBy": "night-auditor", "message": message}),
    )
}

fn block(id: &str, kind: &str, state: &str, from: &str, release: &str) -> PmsRecord {
    record(
        id,
        "block",
        json!({
            "name": id, "typeId": "type-1", "roomId": if kind == "maintenance" { "room-a" } else { "" },
            "from": from, "to": "2026-11-30", "quantity": 1, "releaseDate": release,
            "state": state, "kind": kind, "pickedStayIds": []
        }),
    )
}

fn stayover_task(id: &str, room_id: &str, stay_id: &str, date: &str, cycle: i64) -> PmsRecord {
    record(
        id,
        "task",
        json!({
            "roomCycle": cycle, "roomId": room_id, "type": "stayover", "stayId": stay_id,
            "date": date, "assigneeId": "", "priority": "normal", "state": "pending",
            "checklist": ["Bathroom"], "checked": [], "notes": "",
            "completedBy": "", "inspectedBy": ""
        }),
    )
}

fn entry(id: &str, source_key: &str) -> PmsRecord {
    record(
        id,
        "entry",
        json!({
            "guestAccountId": "account-stay-1", "stayId": "stay-1", "date": "2026-11-04",
            "kind": "charge", "description": "Room charge", "net": 100, "service": 0,
            "tax": 0, "total": 100, "method": "", "reference": "",
            "sourceKey": source_key, "originalId": "", "shiftId": "",
            "actorId": "night-auditor", "at": 1
        }),
    )
}

fn facts() -> PmsNightAuditFacts {
    PmsNightAuditFacts {
        property: property_row(),
        night: None,
        nights_on_date: Vec::new(),
        stays: Vec::new(),
        shifts: Vec::new(),
        room_types: vec![room_type("type-1")],
        rooms: vec![room("room-a", "type-1", "clean", 0)],
        rates: vec![rate("rate-1", true)],
        blocks: Vec::new(),
        stayover_tasks: Vec::new(),
        night_entries: Vec::new(),
    }
}

fn scope_and_setup(facts: &PmsNightAuditFacts) -> (PmsNightSetupScope, PmsNightSetupFacts) {
    (
        PmsNightSetupScope::from_property(&facts.property.view().unwrap()),
        PmsNightSetupFacts::default(),
    )
}

fn start(expected_version: i64) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Start,
        property_id: "property".into(),
        expected_version: Some(expected_version),
        night_id: None,
        worker: false,
    }
}

fn resume(night_id: &str, expected_version: i64) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Resume,
        property_id: "property".into(),
        expected_version: Some(expected_version),
        night_id: Some(night_id.into()),
        worker: false,
    }
}

fn step(night_id: &str) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Step,
        property_id: "property".into(),
        expected_version: None,
        night_id: Some(night_id.into()),
        worker: true,
    }
}

#[test]
fn start_creates_a_pending_night_and_closes_the_day() {
    let facts = facts();
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let key = key("night-start-0001");
    let plan = plan_night_audit(&actor(), &key, &facts, guard, &start(1), 5_000).unwrap();
    assert_eq!(plan.result.property_id, "property");
    assert_eq!(plan.result.version, 2);
    assert!(plan.result.id.starts_with("pms-night-"));
    assert_eq!(plan.property_version, 2);
    assert_eq!(plan.property_state, "closing");
    assert_eq!(plan.property_business_date, "2026-11-04");
    assert_eq!(plan.job, PmsNightJobPlan::Enqueue);
    assert!(plan.night.is_none());
    assert!(plan.tasks.is_empty());
    assert!(plan.blocks.is_empty());
    let night = plan.new_night.unwrap();
    assert_eq!(night.id, plan.result.id);
    assert_eq!(night.version, 1);
    assert_eq!(night.updated_at, 5_000);
    assert_eq!(night.data["date"], "2026-11-04");
    assert_eq!(night.data["state"], "pending");
    assert_eq!(night.data["initiatedBy"], "night-auditor");
    assert_eq!(night.data["message"], "");

    // One command key maps to one night record for the business date.
    let second = self::facts();
    let (scope, setup) = scope_and_setup(&second);
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let again = plan_night_audit(&actor(), &key, &second, guard, &start(1), 5_000).unwrap();
    assert_eq!(again.result.id, plan.result.id);
}

#[test]
fn start_preserves_the_retained_readiness_blockers() {
    let mut facts = facts();
    facts.room_types.clear();
    facts.rooms.clear();
    facts.rates = vec![rate("rate-1", false)];
    let property = facts.property.clone();
    facts.property.config_json = json!({
        "currency": "USD",
        "minorUnit": 2,
        "chartId": "",
        "accounts": {}
    })
    .to_string();
    assert_eq!(property.business_date, "2026-11-04");
    assert_eq!(
        night_blockers(&facts).unwrap(),
        vec![
            "Add a room type".to_owned(),
            "Add rooms".to_owned(),
            "Add a rate plan".to_owned(),
            "Have Finance approve the setup".to_owned(),
            "Complete the chart of accounts setup".to_owned(),
            "The pilot supports THB".to_owned(),
        ]
    );
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let error = plan_night_audit(
        &actor(),
        &key("night-start-blocked"),
        &facts,
        guard,
        &start(1),
        5_000,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_NIGHT_BLOCKED");
}

#[test]
fn start_reports_overdue_stays_and_shift_variances_in_order() {
    let mut facts = facts();
    facts.stays = vec![
        stay(
            "reserved-overdue-1",
            "reserved",
            "2026-11-04",
            "2026-11-06",
            "",
        ),
        stay(
            "inhouse-overdue-1",
            "in_house",
            "2026-11-01",
            "2026-11-04",
            "room-a",
        ),
    ];
    facts.shifts = vec![
        shift("morning", "open", 0),
        shift("evening", "closed", -500),
    ];
    assert_eq!(
        night_blockers(&facts).unwrap(),
        vec![
            "Process reserved: Not checked in / No-show".to_owned(),
            "Process inhouse-: Checkout due".to_owned(),
            "Close/review shift variance for morning".to_owned(),
            "Close/review shift variance for evening".to_owned(),
        ]
    );
}

#[test]
fn start_requires_a_clear_cross_owner_day_control_report() {
    let facts = facts();
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts {
        open_shifts: 1,
        pending_hotel_transfers: 2,
        unbalanced_journals: 1,
        ..PmsNightDayControlFacts::default()
    };
    assert!(!controls.is_clear());
    assert_eq!(
        controls.blockers(),
        vec![
            "1 restaurant cash session(s) still open".to_owned(),
            "2 hotel bank transfer(s) awaiting Finance verification".to_owned(),
            "1 restaurant or hotel journal(s) are unbalanced".to_owned(),
        ]
    );
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let error = plan_night_audit(
        &actor(),
        &key("night-start-controls"),
        &facts,
        guard,
        &start(1),
        5_000,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_DAY_RECONCILIATION_BLOCKED");
}

#[test]
fn start_rejects_a_second_job_for_the_same_business_date() {
    let mut facts = facts();
    facts.nights_on_date = vec![night_record("night-other", "2026-11-04", "completed", "")];
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let error = plan_night_audit(
        &actor(),
        &key("night-start-dup"),
        &facts,
        guard,
        &start(1),
        5_000,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_NIGHT_EXISTS");
    assert_eq!(
        error.message,
        "This business date already has a day-close job"
    );
}

#[test]
fn start_requires_the_current_version_state_and_setup() {
    let facts = facts();
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let guard = || PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-start-v"),
            &facts,
            guard(),
            &start(2),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_VERSION_CONFLICT"
    );
    let mut no_version = start(1);
    no_version.expected_version = None;
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-start-v2"),
            &facts,
            guard(),
            &no_version,
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_VERSION_CONFLICT"
    );

    let mut setup_state = facts.clone();
    setup_state.property.state = "setup".into();
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-start-setup"),
            &setup_state,
            guard(),
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_SETUP_REQUIRED"
    );

    let mut closing = facts.clone();
    closing.property.state = "closing".into();
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-start-closing"),
            &closing,
            guard(),
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_DAY_CLOSING"
    );
}

#[test]
fn setup_scope_keeps_the_retained_account_order_and_errors() {
    let facts = facts();
    let property = facts.property.view().unwrap();
    let scope = PmsNightSetupScope::from_property(&property);
    assert_eq!(scope.chart_id, "chart");
    assert_eq!(scope.currency, "THB");
    assert_eq!(scope.accounts.len(), NIGHT_ACCOUNT_KINDS.len());
    assert_eq!(
        scope.accounts[0],
        ("cash".to_owned(), "account-cash".to_owned())
    );
    assert!(scope.booking_fee_account.is_none());
    assert!(scope.online_accounts.is_empty());
    assert!(require_setup(&property, &scope, &PmsNightSetupFacts::default()).is_ok());

    let invalid_chart = PmsNightSetupFacts {
        chart_valid: false,
        ..PmsNightSetupFacts::default()
    };
    assert_eq!(
        require_setup(&property, &scope, &invalid_chart)
            .unwrap_err()
            .code,
        "PMS_CHART_INVALID"
    );

    let mut duplicate = property.clone();
    duplicate.config["accounts"]["bank"] = Value::String("account-cash".into());
    assert_eq!(
        require_setup(&duplicate, &scope, &PmsNightSetupFacts::default())
            .unwrap_err()
            .code,
        "PMS_ACCOUNTS_DISTINCT"
    );

    let mut invalid_account = PmsNightSetupFacts::default();
    invalid_account.accounts_valid[3] = false;
    assert_eq!(
        require_setup(&property, &scope, &invalid_account)
            .unwrap_err()
            .code,
        "PMS_ACCOUNT_INVALID"
    );

    let mut fees = property.clone();
    fees.config["bookingFees"] = json!({"account": "fee-account"});
    let fee_scope = PmsNightSetupScope::from_property(&fees);
    assert_eq!(
        fee_scope.booking_fee_account.as_deref(),
        Some("fee-account")
    );
    let invalid_fee = PmsNightSetupFacts {
        booking_fee_account_valid: false,
        ..PmsNightSetupFacts::default()
    };
    assert_eq!(
        require_setup(&fees, &fee_scope, &invalid_fee)
            .unwrap_err()
            .code,
        "PMS_FEE_ACCOUNT_INVALID"
    );

    let mut online = property.clone();
    online.config["onlineAccounting"] =
        json!({"refundPayable": "online-refund", "unallocated": "online-unallocated"});
    let online_scope = PmsNightSetupScope::from_property(&online);
    assert_eq!(online_scope.online_accounts.len(), 2);
    let invalid_online = PmsNightSetupFacts {
        online_accounts_valid: false,
        ..PmsNightSetupFacts::default()
    };
    assert_eq!(
        require_setup(&online, &online_scope, &invalid_online)
            .unwrap_err()
            .code,
        "PMS_ONLINE_ACCOUNT_INVALID"
    );
}

#[test]
fn resume_requeues_a_blocked_job_and_refuses_a_closed_day() {
    let mut facts = facts();
    facts.property.state = "closing".into();
    facts.night = Some(night_record(
        "night-1",
        "2026-11-04",
        "blocked",
        "PMS_JOB_FAILED",
    ));
    facts.nights_on_date = vec![night_record(
        "night-1",
        "2026-11-04",
        "blocked",
        "PMS_JOB_FAILED",
    )];
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let guard = || PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    let plan = plan_night_audit(
        &actor(),
        &key("night-resume-1"),
        &facts,
        guard(),
        &resume("night-1", 1),
        5_000,
    )
    .unwrap();
    assert_eq!(plan.result.id, "night-1");
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.job, PmsNightJobPlan::Enqueue);
    assert_eq!(plan.property_state, "closing");
    assert_eq!(plan.property_business_date, "2026-11-04");
    assert!(plan.new_night.is_none());
    let night = plan.night.unwrap();
    assert_eq!(night.version, 2);
    assert_eq!(night.data["state"], "pending");
    assert_eq!(night.data["message"], "");
    assert_eq!(night.data["initiatedBy"], "night-auditor");

    let mut completed = facts.clone();
    completed.night = Some(night_record("night-1", "2026-11-04", "completed", ""));
    completed.nights_on_date = vec![night_record("night-1", "2026-11-04", "completed", "")];
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-resume-2"),
            &completed,
            guard(),
            &resume("night-1", 1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NIGHT_CLOSED"
    );

    let mut open = facts.clone();
    open.property.state = "open".into();
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-resume-3"),
            &open,
            guard(),
            &resume("night-1", 1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NIGHT_CLOSED"
    );

    // The adapter loads the target with `record_not_found`; a loaded record
    // whose identity differs from the command is a corrupt owner read.
    let mut missing = facts.clone();
    missing.night = None;
    missing.nights_on_date.clear();
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-resume-4"),
            &missing,
            guard(),
            &resume("night-missing", 1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_RECORD_NOT_FOUND"
    );
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-resume-5"),
            &facts,
            guard(),
            &resume("night-other", 1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NIGHT_AUDIT_RECORD_INVALID"
    );
}

#[test]
fn step_fails_closed_until_the_room_night_is_posted() {
    let mut facts = facts();
    facts.property.state = "closing".into();
    facts.night = Some(night_record("night-1", "2026-11-04", "pending", ""));
    facts.stays = vec![stay(
        "stay-1",
        "in_house",
        "2026-11-03",
        "2026-11-06",
        "room-a",
    )];
    let error = plan_night_audit(
        &actor(),
        &key("night-step-1"),
        &facts,
        PmsNightAuditGuard::Worker,
        &step("night-1"),
        5_000,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_NIGHT_POSTING_PORT_REQUIRED");
    assert_eq!(
        error.message,
        "Post the outstanding room nights through the Finance posting port before closing the day."
    );

    // The retained night charge closes the same day without a second posting.
    let mut posted = facts.clone();
    posted.night_entries = vec![entry("entry-1", "night:stay-1:2026-11-04")];
    let plan = plan_night_audit(
        &actor(),
        &key("night-step-2"),
        &posted,
        PmsNightAuditGuard::Worker,
        &step("night-1"),
        5_000,
    )
    .unwrap();
    assert_eq!(plan.result.id, "night-1");
    assert_eq!(plan.result.version, 2);
}

#[test]
fn step_completes_the_day_with_metrics_tasks_and_released_blocks() {
    let mut facts = facts();
    facts.property.state = "closing".into();
    facts.night = Some(night_record("night-1", "2026-11-04", "pending", ""));
    facts.stays = vec![
        stay("stay-1", "in_house", "2026-11-03", "2026-11-06", "room-a"),
        stay(
            "stay-2",
            "checked_out",
            "2026-11-03",
            "2026-11-05",
            "room-a",
        ),
        stay("stay-3", "reserved", "2026-11-10", "2026-11-12", ""),
    ];
    facts.rooms = vec![
        room("room-a", "type-1", "clean", 0),
        room("room-b", "type-1", "dirty", 2),
        room("room-c", "type-1", "clean", 0),
    ];
    facts.rates = vec![rate("rate-1", true)];
    facts.blocks = vec![
        block(
            "block-group-due",
            "group",
            "active",
            "2026-11-20",
            "2026-11-05",
        ),
        block(
            "block-group-later",
            "group",
            "active",
            "2026-11-20",
            "2026-11-06",
        ),
        block(
            "block-maintenance",
            "maintenance",
            "active",
            "2026-11-04",
            "",
        ),
    ];
    facts.night_entries = vec![entry("entry-1", "night:stay-1:2026-11-04")];
    let plan = plan_night_audit(
        &actor(),
        &key("night-step-complete"),
        &facts,
        PmsNightAuditGuard::Worker,
        &step("night-1"),
        5_000,
    )
    .unwrap();
    assert_eq!(plan.result.id, "night-1");
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.property_version, 2);
    assert_eq!(plan.property_state, "open");
    assert_eq!(plan.property_business_date, "2026-11-05");
    assert_eq!(plan.job, PmsNightJobPlan::Complete);
    let night = plan.night.unwrap();
    assert_eq!(night.version, 2);
    assert_eq!(night.data["state"], "completed");
    assert_eq!(night.data["completedAt"], 5_000);
    // Capacity = three active rooms minus the one under active maintenance.
    assert_eq!(night.data["roomMetrics"]["capacity"], 2);
    // Sold = the in-house stay plus the checked-out stay covering the date.
    assert_eq!(night.data["roomMetrics"]["sold"], 2);
    assert_eq!(plan.tasks.len(), 1);
    let task = &plan.tasks[0];
    assert_eq!(task.kind, "task");
    assert_eq!(task.version, 1);
    assert_eq!(task.data["roomId"], "room-a");
    assert_eq!(task.data["type"], "stayover");
    assert_eq!(task.data["stayId"], "stay-1");
    assert_eq!(task.data["date"], "2026-11-05");
    assert_eq!(task.data["state"], "pending");
    assert_eq!(task.data["priority"], "normal");
    assert_eq!(task.data["roomCycle"], 0);
    assert_eq!(plan.blocks.len(), 1);
    assert_eq!(plan.blocks[0].id, "block-group-due");
    assert_eq!(plan.blocks[0].version, 2);
    assert_eq!(plan.blocks[0].data["state"], "released");
}

#[test]
fn step_reuses_an_existing_stayover_task() {
    let mut facts = facts();
    facts.property.state = "closing".into();
    facts.night = Some(night_record("night-1", "2026-11-04", "pending", ""));
    facts.stays = vec![stay(
        "stay-1",
        "in_house",
        "2026-11-03",
        "2026-11-06",
        "room-a",
    )];
    facts.rooms = vec![room("room-a", "type-1", "clean", 4)];
    facts.stayover_tasks = vec![stayover_task(
        "task-existing",
        "room-a",
        "stay-1",
        "2026-11-05",
        4,
    )];
    facts.night_entries = vec![entry("entry-1", "night:stay-1:2026-11-04")];
    let plan = plan_night_audit(
        &actor(),
        &key("night-step-dedupe"),
        &facts,
        PmsNightAuditGuard::Worker,
        &step("night-1"),
        5_000,
    )
    .unwrap();
    assert!(plan.tasks.is_empty());
}

#[test]
fn step_rejects_the_wrong_state_and_missing_records() {
    let mut facts = facts();
    facts.property.state = "open".into();
    facts.night = Some(night_record("night-1", "2026-11-04", "pending", ""));
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-step-open"),
            &facts,
            PmsNightAuditGuard::Worker,
            &step("night-1"),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NIGHT_CLOSED"
    );

    let mut completed = facts.clone();
    completed.property.state = "closing".into();
    completed.night = Some(night_record("night-1", "2026-11-04", "completed", ""));
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-step-completed"),
            &completed,
            PmsNightAuditGuard::Worker,
            &step("night-1"),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NIGHT_CLOSED"
    );

    let mut missing = facts.clone();
    missing.night = None;
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-step-missing"),
            &missing,
            PmsNightAuditGuard::Worker,
            &step("night-missing"),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_RECORD_NOT_FOUND"
    );

    // The worker entry point is the only one that accepts `night.step`.
    assert_eq!(
        plan_night_audit(
            &actor(),
            &key("night-step-user"),
            &facts,
            PmsNightAuditGuard::Worker,
            &PmsNightAuditCommand {
                worker: false,
                ..step("night-1")
            },
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_WORKER_ONLY"
    );
}

#[test]
fn command_permissions_preserve_the_retained_audit_gate() {
    let facts = facts();
    let (scope, setup) = scope_and_setup(&facts);
    let controls = PmsNightDayControlFacts::default();
    let mut actor = actor();
    actor.board.permissions = vec!["pms.read".into()];
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    assert_eq!(
        plan_night_audit(
            &actor,
            &key("night-start-perm"),
            &facts,
            guard,
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "PERMISSION_REQUIRED"
    );

    let mut expired = self::actor();
    expired.board.expires_at = 5_000;
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    assert_eq!(
        plan_night_audit(
            &expired,
            &key("night-start-expired"),
            &facts,
            guard,
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "WORK_CONTEXT_CHANGED"
    );

    let mut departmental = self::actor();
    departmental.board.department_id = Some("department".into());
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    assert_eq!(
        plan_night_audit(
            &departmental,
            &key("night-start-dept"),
            &facts,
            guard,
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_BRANCH_REQUIRED"
    );

    let mut foreign = self::actor();
    foreign.board.company_id = "other".into();
    let guard = PmsNightAuditGuard::User {
        controls: &controls,
        setup: &setup,
        scope: &scope,
    };
    assert_eq!(
        plan_night_audit(
            &foreign,
            &key("night-start-foreign"),
            &facts,
            guard,
            &start(1),
            5_000
        )
        .unwrap_err()
        .code,
        "PMS_NOT_FOUND"
    );
}

#[test]
fn request_hash_is_stable_and_bound_to_the_command() {
    let actor = actor();
    let first = night_audit_request_hash(&actor, &start(1)).unwrap();
    assert_eq!(first, night_audit_request_hash(&actor, &start(1)).unwrap());
    assert_eq!(first.len(), 64);
    assert_ne!(first, night_audit_request_hash(&actor, &start(2)).unwrap());
    assert_ne!(
        first,
        night_audit_request_hash(&actor, &resume("night-1", 1)).unwrap()
    );
    let mut worker = start(1);
    worker.worker = true;
    assert_ne!(first, night_audit_request_hash(&actor, &worker).unwrap());
    assert_eq!(next_business_date("2026-11-04").unwrap(), "2026-11-05");
}
