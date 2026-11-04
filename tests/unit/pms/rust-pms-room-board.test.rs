//! Native PMS room-board policy and application facts. No database, browser,
//! command, purchase or Workflow acceptance is inferred.
use erpx_server_pms::public::room_board::*;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.housekeeping".into()],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn property_row(id: &str) -> PmsPropertyRow {
    PmsPropertyRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        name: format!("Hotel {id}"),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        config_json: r#"{"country":"TH","currency":"THB","minorUnit":2,"timezone":"Asia/Bangkok","checkIn":"14:00","checkOut":"12:00","cancellationPolicy":"","chartId":"chart","accounts":{"cash":"cash","bank":"bank","edc":"edc","receivable":"receivable","advance":"advance","deposit":"deposit","revenue":"revenue","service":"service","tax":"tax","prepaidTax":"prepaidTax","inventory":"inventory","consumption":"consumption","commission":"commission","cashDifference":"cashDifference"},"taxBps":700,"serviceBps":1000,"taxInclusive":false,"advanceTax":true,"preparedBy":"maker","approvedBy":"reviewer","revision":1}"#.into(),
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

fn room(id: &str, number: &str, cleanliness: &str, dnd: bool) -> PmsRecord {
    record(
        id,
        "room",
        serde_json::json!({
            "number": number,
            "typeId": "type-1",
            "building": "Main",
            "floor": "1",
            "active": true,
            "cleanliness": cleanliness,
            "dnd": dnd,
            "observedOccupied": null
        }),
    )
}

fn board_fixture() -> PmsBoardData {
    PmsBoardData {
        property: property_row("property"),
        records: vec![
            room("room-1", "101", "dirty", true),
            room("room-2", "102", "clean", false),
            record(
                "type-1",
                "room_type",
                serde_json::json!({"name":"Deluxe","capacity":2,"amenities":""}),
            ),
            record(
                "rate-1",
                "rate",
                serde_json::json!({"name":"Rack","typeId":"type-1","active":true}),
            ),
            record(
                "stay-1",
                "stay",
                serde_json::json!({"roomId":"room-1","state":"in_house","prices":[{"date":"2026-11-04","net":100,"service":10,"tax":7,"total":117}],"bookingPolicy":{"deadline":1},"upgrades":[{"effectiveDate":"2026-11-04"}]}),
            ),
            record(
                "task-1",
                "task",
                serde_json::json!({"roomId":"room-1","state":"pending","priority":"urgent"}),
            ),
            record(
                "block-1",
                "block",
                serde_json::json!({"roomId":"room-1","state":"active","kind":"maintenance"}),
            ),
            record("guest-1", "guest", serde_json::json!({"name":"Guest"})),
        ],
        ids: vec!["room-1".into(), "room-2".into()],
        has_next: false,
        room_summary: PmsRoomSummary {
            total: 2,
            active: 2,
            dirty: 1,
            clean: 1,
            inspected: 0,
            dnd: 1,
            occupied: 1,
        },
        setup: PmsBoardSetup {
            has_room_types: true,
            has_active_rooms: true,
            has_active_rates: true,
        },
        from: "2026-10-05".into(),
        to: "2027-01-03".into(),
    }
}

#[derive(Clone)]
struct Repo {
    properties: Vec<PmsPropertyRow>,
    data: Option<PmsBoardData>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl PmsBoardRepository for Repo {
    type Error = BusinessError;
    fn properties<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsPropertyRow>, Self::Error> {
        self.seen.lock().unwrap().push(format!(
            "properties:{company_id}:{}",
            branch_id.unwrap_or("-")
        ));
        let rows = self.properties.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn board<'a>(
        &'a mut self,
        property_id: &'a str,
        _business_date: &'a str,
        selection: &'a PmsBoardSelection,
        visibility: PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsBoardData, Self::Error> {
        self.seen.lock().unwrap().push(format!(
            "board:{property_id}:{}:{}:{}",
            selection.page, visibility.guest_details, visibility.pricing
        ));
        let data = self.data.clone().expect("board fixture");
        Box::pin(async move { Ok(data) })
    }
}

struct Packages(PmsBoardEntitlement);
impl Packages {
    fn allowed() -> Self {
        Self(PmsBoardEntitlement {
            enforced: true,
            history_allowed: true,
        })
    }
}
impl PmsBoardEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error> {
        let value = self.0;
        Box::pin(async move { Ok(value) })
    }
}
struct Staff(Vec<PmsStaff>);
impl PmsStaffDirectory for Staff {
    type Error = BusinessError;
    fn staff<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsStaff>, Self::Error> {
        let rows = self.0.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn staff_fixture() -> Staff {
    Staff(vec![
        PmsStaff {
            id: "staff-2".into(),
            name: "Zulu".into(),
        },
        PmsStaff {
            id: "staff-1".into(),
            name: "Alpha".into(),
        },
    ])
}

fn selection() -> PmsBoardSelection {
    PmsBoardSelection {
        section: PmsBoardSection::Housekeeping,
        property_id: Some("property".into()),
        status: "dirty".into(),
        q: "101".into(),
        building: "Main".into(),
        floor: "1".into(),
        type_id: Some("type-1".into()),
        page: 1,
        selected_id: Some("room-1".into()),
        from: Some("2026-11-01".into()),
        to: Some("2026-11-10".into()),
    }
}

#[test]
fn board_requires_permission_module_branch_context_and_room_board_role() {
    let mut missing = actor();
    missing.permissions = vec!["pms.housekeeping".into()];
    assert_eq!(
        missing.require_board(0).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );
    let mut module = actor();
    module.modules = vec!["finance".into()];
    assert_eq!(
        module.require_board(0).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let mut department = actor();
    department.department_id = Some("dept".into());
    assert_eq!(
        department.require_board(0).unwrap_err().code,
        "PMS_BRANCH_REQUIRED"
    );
    let mut expired = actor();
    expired.expires_at = 10;
    let error = expired.require_board(10).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    let mut reader = actor();
    reader.permissions = vec!["pms.read".into()];
    assert_eq!(
        reader.require_board(0).unwrap_err().code,
        "PMS_HOUSEKEEPING_REQUIRED"
    );
    let mut inspector = actor();
    inspector.permissions = vec!["pms.read".into(), "pms.inspect".into()];
    assert!(
        inspector
            .require_section(PmsBoardSection::Housekeeping)
            .is_ok()
    );
    assert_eq!(
        inspector
            .require_section(PmsBoardSection::Rooms)
            .unwrap_err()
            .code,
        "PMS_HOUSEKEEPING_REQUIRED"
    );
    let mut maintenance = actor();
    maintenance.permissions = vec!["pms.read".into(), "pms.maintenance".into()];
    assert!(maintenance.require_section(PmsBoardSection::Rooms).is_ok());
    assert_eq!(
        maintenance
            .require_section(PmsBoardSection::Housekeeping)
            .unwrap_err()
            .code,
        "PMS_HOUSEKEEPING_REQUIRED"
    );
}

#[test]
fn selection_is_bounded_and_date_ranges_stay_within_ninety_three_days() {
    assert!(selection().validate().is_ok());
    for (index, value) in [
        PmsBoardSelection {
            page: 10_001,
            ..selection()
        },
        PmsBoardSelection {
            status: "occupied".into(),
            ..selection()
        },
        PmsBoardSelection {
            q: "q".repeat(201),
            ..selection()
        },
        PmsBoardSelection {
            from: Some("2026-11-10".into()),
            to: Some("2026-11-01".into()),
            ..selection()
        },
        PmsBoardSelection {
            from: Some("2026-01-01".into()),
            to: Some("2026-05-01".into()),
            ..selection()
        },
        PmsBoardSelection {
            from: Some("2026-02-30".into()),
            to: Some("2026-03-01".into()),
            ..selection()
        },
        PmsBoardSelection {
            from: Some("2026-11-01".into()),
            to: None,
            ..selection()
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            value.validate().is_err(),
            "case {index} should fail: {value:?}"
        );
    }
}

#[tokio::test]
async fn housekeeping_board_hides_guest_and_rate_facts_and_redacts_stay_prices() {
    let repo = Repo {
        properties: vec![property_row("property")],
        data: Some(board_fixture()),
        seen: Arc::default(),
    };
    let mut repo = repo.clone();
    let mut staff = staff_fixture();
    let mut packages = Packages::allowed();
    let workspace = pms_room_board(
        &actor(),
        &selection(),
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap();
    let snapshot = workspace.snapshot.as_ref().expect("snapshot");
    assert_eq!(snapshot.property.id, "property");
    assert_eq!(snapshot.property.config["chartId"], "");
    assert_eq!(snapshot.records.len(), 6);
    assert!(!snapshot.records.iter().any(|row| row.kind == "rate"));
    assert!(!snapshot.records.iter().any(|row| row.kind == "guest"));
    let stay = snapshot
        .records
        .iter()
        .find(|row| row.kind == "stay")
        .unwrap();
    assert_eq!(stay.data["prices"], serde_json::json!([]));
    assert!(stay.data.get("bookingPolicy").is_none());
    assert_eq!(
        workspace.permissions,
        vec!["pms.read".to_owned(), "pms.housekeeping".to_owned()]
    );
    assert_eq!(workspace.staff[0].name, "Alpha");
    assert_eq!(workspace.staff[1].name, "Zulu");
    assert_eq!(workspace.available_payment_methods, vec!["cash".to_owned()]);
    assert!(
        repo.seen
            .lock()
            .unwrap()
            .contains(&"board:property:1:false:false".to_owned())
    );
    let meta = snapshot.list_meta.as_ref().unwrap();
    assert_eq!(meta.section, "housekeeping");
    assert_eq!(meta.ids, ["room-1", "room-2"]);
    assert_eq!(meta.selected_id.as_deref(), Some("room-1"));
    assert_eq!(meta.room_summary.as_ref().unwrap().occupied, 1);
    let value = workspace.value();
    assert_eq!(value["snapshot"]["records"][0]["updatedAt"], 1);
    assert_eq!(value["snapshot"]["property"]["companyId"], "company");
    assert_eq!(value["defaultDate"].as_str().unwrap().len(), 10);
}

#[tokio::test]
async fn manager_board_retains_pricing_and_guest_records_and_visible_metadata() {
    let mut manager = actor();
    manager.permissions = vec![
        "pms.read".into(),
        "pms.housekeeping".into(),
        "pms.inspect".into(),
        "pms.frontdesk".into(),
        "pms.reservations".into(),
        "pms.cashier".into(),
        "pms.configure".into(),
    ];
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(board_fixture()),
        seen: Arc::default(),
    };
    let mut staff = staff_fixture();
    let mut packages = Packages::allowed();
    let workspace = pms_room_board(
        &manager,
        &selection(),
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap();
    let snapshot = workspace.snapshot.as_ref().unwrap();
    assert!(snapshot.records.iter().any(|row| row.kind == "rate"));
    assert!(snapshot.records.iter().any(|row| row.kind == "guest"));
    assert_ne!(
        snapshot
            .records
            .iter()
            .find(|row| row.kind == "stay")
            .unwrap()
            .data["prices"],
        serde_json::json!([])
    );
    assert_eq!(
        snapshot.list_meta.as_ref().unwrap().selected_id.as_deref(),
        Some("room-1")
    );
}

#[tokio::test]
async fn foreign_requested_property_is_a_scoped_not_found_and_empty_scope_is_explicit() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(board_fixture()),
        seen: Arc::default(),
    };
    let mut staff = staff_fixture();
    let mut packages = Packages::allowed();
    let mut foreign = selection();
    foreign.property_id = Some("other".into());
    let error = pms_room_board(
        &actor(),
        &foreign,
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut empty = Repo {
        properties: Vec::new(),
        data: None,
        seen: Arc::default(),
    };
    let mut empty_selection = selection();
    empty_selection.property_id = None;
    let workspace = pms_room_board(
        &actor(),
        &empty_selection,
        &mut empty,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap();
    assert!(workspace.snapshot.is_none());
    assert_eq!(workspace.front_ready, Some(false));
    assert!(
        workspace
            .readiness
            .contains(&"Add a hotel property".to_owned())
    );
}

#[tokio::test]
async fn hidden_selected_record_is_removed_and_foreign_records_fail_validation() {
    let mut data = board_fixture();
    data.ids = vec!["room-1".into(), "missing".into()];
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data),
        seen: Arc::default(),
    };
    let mut staff = staff_fixture();
    let mut packages = Packages::allowed();
    let error = pms_room_board(
        &actor(),
        &selection(),
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PMS_BOARD_READ_INVALID");

    let mut foreign = board_fixture();
    foreign.property.company_id = "other".into();
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(foreign),
        seen: Arc::default(),
    };
    let error = pms_room_board(
        &actor(),
        &selection(),
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PMS_BOARD_READ_INVALID");
}

#[tokio::test]
async fn enforced_package_without_history_is_forbidden() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(board_fixture()),
        seen: Arc::default(),
    };
    let mut staff = staff_fixture();
    let mut packages = Packages(PmsBoardEntitlement {
        enforced: true,
        history_allowed: false,
    });
    let error = pms_room_board(
        &actor(),
        &selection(),
        &mut repo,
        &mut staff,
        &mut packages,
        1_000,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_REQUIRED");
}

#[test]
fn staff_and_records_are_bounded() {
    let rows = (0..=1_000)
        .map(|index| PmsStaff {
            id: format!("staff-{index}"),
            name: format!("Staff {index}"),
        })
        .collect();
    assert_eq!(staff(rows).unwrap_err().code, "PMS_BOARD_READ_LIMIT");
    let rows = vec![
        PmsStaff {
            id: "staff-2".into(),
            name: "Zulu".into(),
        },
        PmsStaff {
            id: "staff-1".into(),
            name: "Alpha".into(),
        },
        PmsStaff {
            id: "staff-1".into(),
            name: "Alpha".into(),
        },
    ];
    let rows = staff(rows).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "staff-1");
    assert_eq!(rows[1].id, "staff-2");
}

#[test]
fn readiness_and_booking_facts_follow_the_property_config() {
    let property = property_row("property").view().unwrap();
    let records = vec![
        record(
            "type-1",
            "room_type",
            serde_json::json!({"name":"Deluxe","capacity":2,"amenities":""}),
        ),
        record(
            "room-1",
            "room",
            serde_json::json!({"active":true,"cleanliness":"clean"}),
        ),
        record("rate-1", "rate", serde_json::json!({"active":true})),
    ];
    assert_eq!(readiness(&property, &records), Vec::<String>::new());
    assert!(booking_readiness(&property).accounting);
    assert!(!booking_readiness(&property).email);
    assert!(!booking_readiness(&property).connect);
}

#[test]
fn serialized_workspace_matches_the_next_transport_field_names() {
    let value = serde_json::to_value(PmsWorkspace {
        properties: vec![property_row("property").summary()],
        snapshot: None,
        default_date: "2026-11-04".into(),
        readiness: Vec::new(),
        front_ready: None,
        can_review_booking_media: false,
        booking_readiness: PmsBookingReadiness::default(),
        can_manage_setup: false,
        permissions: Vec::new(),
        available_payment_methods: vec!["cash".into()],
        user_id: "user".into(),
        branches: Vec::new(),
        staff: Vec::new(),
        accounts: Vec::new(),
        charts: Vec::new(),
        parties: Vec::new(),
        products: Vec::new(),
        warehouses: Vec::new(),
        documents: Vec::new(),
        transfer_requests: Vec::new(),
        charge_corrections: Vec::new(),
        stripe_connection_ready: false,
        stripe_payments: Vec::new(),
        counter_refunds: Vec::new(),
        booking_refunds: Vec::new(),
        booking_delivery: Vec::new(),
        ledger: None,
        control_balances: None,
        day_controls: None,
    })
    .unwrap();
    let keys: BTreeSet<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    for key in [
        "properties",
        "snapshot",
        "defaultDate",
        "readiness",
        "canReviewBookingMedia",
        "bookingReadiness",
        "canManageSetup",
        "permissions",
        "availablePaymentMethods",
        "userId",
        "branches",
        "staff",
        "accounts",
        "charts",
        "parties",
        "products",
        "warehouses",
        "documents",
        "transferRequests",
        "chargeCorrections",
        "stripeConnectionReady",
        "stripePayments",
        "counterRefunds",
        "bookingRefunds",
        "bookingDelivery",
        "ledger",
        "controlBalances",
        "dayControls",
    ] {
        assert!(keys.contains(key), "missing {key}");
    }
    assert!(!keys.contains("frontReady"));
    assert_eq!(value["properties"][0]["branchId"], "branch");
    assert_eq!(value["properties"][0]["businessDate"], "2026-11-04");
}
