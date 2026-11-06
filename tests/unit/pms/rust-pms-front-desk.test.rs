//! Native PMS front-desk policy and application facts. No database, browser,
//! command, purchase or Workflow acceptance is inferred.
use erpx_server_pms::public::front_desk::*;
use erpx_server_pms::public::room_board::{
    PmsBoardActor, PmsBoardEntitlement, PmsBoardEntitlementPort, PmsBoardFuture, PmsPropertyRow,
    PmsRecord,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::{Arc, Mutex};

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.frontdesk".into()],
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

fn data_fixture() -> PmsFrontDeskData {
    PmsFrontDeskData {
        property: property_row("property"),
        records: vec![
            record(
                "type-1",
                "room_type",
                serde_json::json!({"name":"Deluxe","capacity":2}),
            ),
            record(
                "room-1",
                "room",
                serde_json::json!({"number":"101","typeId":"type-1","active":true}),
            ),
            record(
                "rate-1",
                "rate",
                serde_json::json!({"name":"Rack","typeId":"type-1","active":true}),
            ),
            record(
                "stay-1",
                "stay",
                serde_json::json!({
                    "reservationId":"reservation-1","guestId":"guest-1","roomId":"room-1",
                    "guestAccountId":"account-1","arrival":"2026-11-04","departure":"2026-11-06",
                    "state":"reserved","adults":2,
                    "prices":[{"date":"2026-11-04","net":100,"service":10,"tax":7,"total":117}],
                    "upgrades":[{"effectiveDate":"2026-11-04"}],
                    "bookingPolicy":{"deadline":1}
                }),
            ),
            record(
                "reservation-1",
                "reservation",
                serde_json::json!({"number":"R-1","guestId":"guest-1","state":"confirmed"}),
            ),
            record(
                "guest-1",
                "guest",
                serde_json::json!({"name":"Synthetic Guest","email":"guest@example.test"}),
            ),
            record(
                "account-1",
                "guest_account",
                serde_json::json!({"name":"Synthetic Guest","guestId":"guest-1","state":"open","due":117,"advance":0,"deposit":0}),
            ),
        ],
        ids: vec!["stay-1".into()],
        has_next: false,
        documents: vec![serde_json::json!({
            "id": "document-1",
            "guest_account_id": "account-1",
            "kind": "receipt",
            "created_at": 1
        })],
        from: "2026-10-05".into(),
        to: "2027-01-03".into(),
    }
}

#[derive(Clone)]
struct Repo {
    properties: Vec<PmsPropertyRow>,
    data: Option<PmsFrontDeskData>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl PmsFrontDeskRepository for Repo {
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
        let rows = self
            .properties
            .iter()
            .filter(|row| {
                row.company_id == company_id
                    && branch_id.is_none_or(|branch| row.branch_id == branch)
            })
            .cloned()
            .collect();
        Box::pin(async move { Ok(rows) })
    }
    fn front_desk<'a>(
        &'a mut self,
        property_id: &'a str,
        _business_date: &'a str,
        selection: &'a PmsFrontDeskSelection,
        visibility: erpx_server_pms::public::room_board::PmsBoardVisibility,
    ) -> PmsBoardFuture<'a, PmsFrontDeskData, Self::Error> {
        self.seen.lock().unwrap().push(format!(
            "front-desk:{property_id}:{}:{}:{}:{}",
            selection.page, selection.status, visibility.guest_details, visibility.financial
        ));
        let data = self.data.clone().expect("front-desk fixture");
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

fn selection() -> PmsFrontDeskSelection {
    PmsFrontDeskSelection {
        property_id: Some("property".into()),
        status: "arrivals".into(),
        q: "Synthetic".into(),
        page: 0,
        selected_id: Some("stay-1".into()),
        from: Some("2026-11-01".into()),
        to: Some("2026-11-10".into()),
    }
}

#[test]
fn front_desk_requires_permission_module_branch_context_and_frontdesk_role() {
    let mut missing = actor();
    missing.permissions = vec!["pms.frontdesk".into()];
    assert_eq!(
        missing.require_front_desk(0).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );
    let mut module = actor();
    module.modules = vec!["finance".into()];
    assert_eq!(
        module.require_front_desk(0).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let mut department = actor();
    department.department_id = Some("dept".into());
    assert_eq!(
        department.require_front_desk(0).unwrap_err().code,
        "PMS_BRANCH_REQUIRED"
    );
    let mut expired = actor();
    expired.expires_at = 10;
    let error = expired.require_front_desk(10).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    let mut reservation = actor();
    reservation.permissions = vec!["pms.read".into(), "pms.reservations".into()];
    assert_eq!(
        reservation.require_front_desk(0).unwrap_err().code,
        "PMS_FRONTDESK_REQUIRED"
    );
    let mut cashier = actor();
    cashier.permissions = vec!["pms.read".into(), "pms.cashier".into()];
    assert_eq!(
        cashier.require_front_desk(0).unwrap_err().code,
        "PMS_FRONTDESK_REQUIRED"
    );
    assert!(actor().require_front_desk(0).is_ok());
}

#[test]
fn selection_is_bounded_to_front_desk_queues_and_ninety_three_days() {
    assert!(selection().validate().is_ok());
    for (index, value) in [
        PmsFrontDeskSelection {
            page: 10_001,
            ..selection()
        },
        PmsFrontDeskSelection {
            status: "payment_pending".into(),
            ..selection()
        },
        PmsFrontDeskSelection {
            status: "reserved".into(),
            ..selection()
        },
        PmsFrontDeskSelection {
            q: "q".repeat(201),
            ..selection()
        },
        PmsFrontDeskSelection {
            from: Some("2026-11-10".into()),
            to: Some("2026-11-01".into()),
            ..selection()
        },
        PmsFrontDeskSelection {
            from: Some("2026-01-01".into()),
            to: Some("2026-05-01".into()),
            ..selection()
        },
        PmsFrontDeskSelection {
            from: Some("2026-02-30".into()),
            to: Some("2026-03-01".into()),
            ..selection()
        },
        PmsFrontDeskSelection {
            from: Some("2026-11-01".into()),
            to: None,
            ..selection()
        },
        PmsFrontDeskSelection {
            property_id: Some(String::new()),
            ..selection()
        },
        PmsFrontDeskSelection {
            selected_id: Some(String::new()),
            ..selection()
        },
    ]
    .into_iter()
    .enumerate()
    {
        if index == 2 {
            assert!(value.validate().is_ok(), "reserved is a valid stay state");
        } else {
            assert!(
                value.validate().is_err(),
                "case {index} should fail: {value:?}"
            );
        }
    }
}

#[test]
fn front_desk_text_bounds_use_utf16_units_after_ecma_trim() {
    let mut base = selection();
    base.q = format!("{}😀", "a".repeat(198));
    assert!(base.validate().is_ok(), "198 a + emoji is 200 UTF-16 units");
    base.q = format!("{}😀", "a".repeat(199));
    assert!(base.validate().is_err(), "199 a + emoji is 201 UTF-16 units");
    let mut base = selection();
    base.q = format!("  \u{00A0}{}{} ", "a".repeat(200), "\u{3000}");
    assert!(
        base.validate().is_ok(),
        "ECMAScript whitespace around 200 units still passes"
    );
    base.normalize();
    assert_eq!(base.q, "a".repeat(200));
    let mut base = selection();
    base.q = format!("{}{}", "\u{0085}", "a".repeat(200));
    assert!(
        base.validate().is_err(),
        "U+0085 NEL is not in the ECMAScript trim set"
    );
    base.q = format!("{}{}", "\u{FEFF}", "a".repeat(200));
    assert!(
        base.validate().is_ok(),
        "BOM (ZWNBSP) is trimmed by ECMAScript"
    );
    base.normalize();
    assert_eq!(base.q, "a".repeat(200));
    let mut base = selection();
    base.property_id = Some("中".repeat(150));
    assert!(
        base.validate().is_ok(),
        "150 CJK are 150 UTF-16 units but 450 UTF-8 bytes"
    );
    base.property_id = Some("😀".repeat(101));
    assert!(base.validate().is_err());
    base.property_id = None;
    base.selected_id = Some("😀".repeat(101));
    assert!(base.validate().is_err());
    base.selected_id = Some("😀".repeat(100));
    assert!(base.validate().is_ok());
}

#[tokio::test]
async fn front_desk_normalizes_edge_whitespace_before_repo_and_metadata() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data_fixture()),
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let mut raw = selection();
    raw.q = "  Synthetic  ".into();
    let workspace = pms_front_desk(&actor(), &raw, &mut repo, &mut packages, 1_000)
        .await
        .unwrap();
    let meta = workspace.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap();
    assert_eq!(meta.q, "Synthetic");
    assert_eq!(raw.q, "  Synthetic  ");
}

#[tokio::test]
async fn front_desk_keeps_guest_links_and_redacts_financial_facts_without_cashier() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data_fixture()),
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let workspace = pms_front_desk(&actor(), &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap();
    let snapshot = workspace.snapshot.as_ref().expect("snapshot");
    assert_eq!(snapshot.property.id, "property");
    assert_eq!(snapshot.property.config["chartId"], "");
    let kinds: std::collections::BTreeSet<&str> = snapshot
        .records
        .iter()
        .map(|row| row.kind.as_str())
        .collect();
    for kind in [
        "stay",
        "guest",
        "reservation",
        "guest_account",
        "room",
        "room_type",
        "rate",
    ] {
        assert!(kinds.contains(kind), "missing {kind}");
    }
    let stay = snapshot
        .records
        .iter()
        .find(|row| row.kind == "stay")
        .unwrap();
    assert_eq!(stay.data["prices"], serde_json::json!([]));
    let account = snapshot
        .records
        .iter()
        .find(|row| row.kind == "guest_account")
        .unwrap();
    assert_eq!(account.data["due"], 0);
    assert!(workspace.documents.is_empty());
    let meta = snapshot.list_meta.as_ref().unwrap();
    assert_eq!(meta.section, "front-desk");
    assert_eq!(meta.kind, "stay");
    assert_eq!(meta.ids, ["stay-1"]);
    assert_eq!(meta.selected_id.as_deref(), Some("stay-1"));
    assert!(meta.room_summary.is_none());
    assert_eq!(workspace.front_ready, Some(true));
    assert!(workspace.staff.is_empty());
    assert!(workspace.readiness.is_empty());
    assert!(
        repo.seen
            .lock()
            .unwrap()
            .contains(&"front-desk:property:0:arrivals:true:false".to_owned())
    );
    let value = workspace.value();
    assert_eq!(value["snapshot"]["listMeta"]["kind"], "stay");
    assert_eq!(value["snapshot"]["listMeta"]["section"], "front-desk");
}

#[tokio::test]
async fn cashier_retains_financial_facts_and_only_visible_documents() {
    let mut cashier = actor();
    cashier.permissions.push("pms.cashier".into());
    let mut data = data_fixture();
    data.documents.push(serde_json::json!({
        "id": "document-foreign",
        "guest_account_id": "account-foreign",
        "kind": "receipt",
        "created_at": 1
    }));
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data),
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let workspace = pms_front_desk(&cashier, &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap();
    let snapshot = workspace.snapshot.as_ref().unwrap();
    let stay = snapshot
        .records
        .iter()
        .find(|row| row.kind == "stay")
        .unwrap();
    assert_ne!(stay.data["prices"], serde_json::json!([]));
    let account = snapshot
        .records
        .iter()
        .find(|row| row.kind == "guest_account")
        .unwrap();
    assert_eq!(account.data["due"], 117);
    assert_eq!(workspace.documents.len(), 1);
    assert_eq!(workspace.documents[0]["id"], "document-1");
}

#[tokio::test]
async fn foreign_company_branch_and_missing_property_fail_closed() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data_fixture()),
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let mut south = actor();
    south.branch_id = Some("south".into());
    let error = pms_front_desk(&south, &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut other = actor();
    other.company_id = "other".into();
    let error = pms_front_desk(&other, &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut missing = selection();
    missing.property_id = Some("missing".into());
    let error = pms_front_desk(&actor(), &missing, &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PMS_NOT_FOUND");

    let mut foreign = data_fixture();
    foreign.property.company_id = "other".into();
    let mut foreign_repo = Repo {
        properties: vec![property_row("property")],
        data: Some(foreign),
        seen: Arc::default(),
    };
    let error = pms_front_desk(
        &actor(),
        &selection(),
        &mut foreign_repo,
        &mut packages,
        1_000,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PMS_FRONT_DESK_READ_INVALID");
}

#[tokio::test]
async fn hidden_selected_ids_and_malformed_documents_fail_validation() {
    let mut data = data_fixture();
    data.ids = vec!["stay-1".into(), "missing".into()];
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data),
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let error = pms_front_desk(&actor(), &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PMS_FRONT_DESK_READ_INVALID");

    let mut data = data_fixture();
    data.documents = vec![serde_json::json!({"id": "document-1", "kind": "receipt"})];
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data),
        seen: Arc::default(),
    };
    let error = pms_front_desk(&actor(), &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PMS_FRONT_DESK_READ_INVALID");
}

#[tokio::test]
async fn enforced_package_without_history_is_forbidden() {
    let mut repo = Repo {
        properties: vec![property_row("property")],
        data: Some(data_fixture()),
        seen: Arc::default(),
    };
    let mut packages = Packages(PmsBoardEntitlement {
        enforced: true,
        history_allowed: false,
    });
    let error = pms_front_desk(&actor(), &selection(), &mut repo, &mut packages, 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_REQUIRED");
}

#[tokio::test]
async fn empty_scope_is_an_explicit_empty_workspace() {
    let mut repo = Repo {
        properties: Vec::new(),
        data: None,
        seen: Arc::default(),
    };
    let mut packages = Packages::allowed();
    let mut empty = selection();
    empty.property_id = None;
    let workspace = pms_front_desk(&actor(), &empty, &mut repo, &mut packages, 1_000)
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
