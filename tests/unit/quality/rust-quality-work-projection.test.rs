//! Native Quality work projection over workspace holds, with injected source
//! and same-person facts. Parity fixtures mirror the retained `qualityWork`
//! WorkItem payload so client fields cannot drift between the two paths.
use erpx_server_quality::public::quality_read::{
    HoldStatus, QualityActor, QualityDetailCoreData, QualityFuture, QualityHold,
    QualityHoldProduct, QualityReadRepository, QualityWorkspaceData,
    quality_detail_core_from_source,
};
use erpx_server_quality::public::quality_work::{
    FnSourceAccess, MappedSourceAccess, QualitySourceAccess, SourceAccess, quality_work,
    quality_work_from_workspace, quality_work_with_same_person,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use std::collections::BTreeMap;

fn hold(status: HoldStatus, creator: &str) -> QualityHold {
    QualityHold {
        id: "hold-1".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        product_id: "product".into(),
        lot_id: None,
        reason: "Damaged packaging".into(),
        due_date: "2026-11-10".into(),
        status,
        created_by: creator.into(),
        last_inspector: None,
        last_result: None,
        released_by: None,
        release_reason: None,
        released_at: None,
        version: 3,
        created_at: 1,
        updated_at: 2,
        spec_id: None,
        spec_version: None,
        quantity_scope: "whole".into(),
        held_quantity_milli: Some(1000),
        source_type: None,
        source_id: None,
        cancel_requested_by: None,
        cancel_reason: None,
        cancelled_by: None,
        cancelled_at: None,
        disposition_kind: None,
        disposition_reason: None,
        disposition_requested_by: None,
        disposition_count_id: None,
        disposition_quantity_milli: None,
        disposed_by: None,
        disposed_at: None,
        inspection_count: 0,
    }
}

fn data(status: HoldStatus, creator: &str, eligible: i64) -> QualityWorkspaceData {
    let mut hold_products = BTreeMap::new();
    hold_products.insert(
        "hold-1".to_owned(),
        QualityHoldProduct {
            product_name: "Quality product".into(),
            product_unit: "ea".into(),
            tracking_mode: "none".into(),
            warehouse_name: "Quality warehouse".into(),
            lot_code: None,
            serial_number: None,
            expiry_date: None,
            eligible_count: eligible,
        },
    );
    QualityWorkspaceData {
        holds: vec![hold(status, creator)],
        hold_products,
        stock: Vec::new(),
        lots: Vec::new(),
        products: Vec::new(),
        specifications: Vec::new(),
        specification_metrics: Vec::new(),
        alerts: Vec::new(),
    }
}

fn actor(permissions: &[&str]) -> QualityActor {
    QualityActor {
        user_id: UserId::new("inspector").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    }
}

fn open() -> FnSourceAccess<impl Fn(&str) -> Option<QualitySourceAccess> + Send + Sync> {
    FnSourceAccess::new(|_| None)
}

fn restricted() -> MappedSourceAccess {
    MappedSourceAccess::new().with(
        "hold-1",
        Some(QualitySourceAccess {
            new_work: true,
            settle_existing: false,
            history: true,
            revision: "rev-1".into(),
        }),
    )
}

#[test]
fn native_work_item_is_byte_parity_with_the_retained_work_item_payload() {
    let work = quality_work_from_workspace(
        &actor(&["quality.read", "quality.inspect"]),
        &data(HoldStatus::Held, "maker", 1),
        &open(),
        &|user| user == "inspector",
    )
    .unwrap();
    let json = serde_json::to_string(&work.items).unwrap();
    assert_eq!(
        json,
        r#"[{"id":"hold-1","type":"quality_hold","source":{"kind":"quality_hold","id":"hold-1","href":"/quality/hold-1","label":"Quality hold","version":3},"actionKind":"work","title":"Quality hold · Quality product","detail":"Quality warehouse","href":"/quality/hold-1","status":"held","updatedAt":2,"queue":"todo","canAct":true,"dueAt":1794268800000,"responsibleQueue":"Quality inspection team","nextAction":"Inspect stock and record evidence","blocker":null,"relationships":["approval"]}]"#
    );
}

#[test]
fn same_person_creator_or_merged_alias_suppresses_action() {
    let work = quality_work_from_workspace(
        &actor(&["quality.read", "quality.inspect"]),
        &data(HoldStatus::Held, "alias", 1),
        &open(),
        &|user| user == "inspector" || user == "alias",
    )
    .unwrap();
    assert!(!work.items[0].can_act);
    assert_eq!(work.items[0].queue, "waiting");
    assert_eq!(work.items[0].relationships, vec!["requested"]);
}

#[test]
fn source_gate_without_settle_existing_downgrades_only_availability() {
    let work = quality_work_from_workspace(
        &actor(&["quality.read", "quality.inspect"]),
        &data(HoldStatus::Held, "maker", 1),
        &restricted(),
        &|user| user == "inspector",
    )
    .unwrap();
    let item = &work.items[0];
    assert!(!item.can_act);
    assert_eq!(item.action_kind, "work");
    assert_eq!(item.queue, "waiting");
    assert!(item.relationships.is_empty());
    assert_eq!(item.id, "hold-1");
    assert_eq!(item.source.version, 3);
    assert_eq!(item.due_at, Some(1_794_268_800_000));
}

#[test]
fn terminal_hold_keeps_completed_queue_under_a_restricted_source() {
    let work = quality_work_from_workspace(
        &actor(&["quality.read"]),
        &data(HoldStatus::Released, "maker", 1),
        &restricted(),
        &|user| user == "inspector",
    )
    .unwrap();
    let item = &work.items[0];
    assert_eq!(item.queue, "completed");
    assert!(!item.can_act);
    assert_eq!(item.status, "released");
    assert_eq!(item.next_action, "Inspect stock and record evidence");
}

#[test]
fn blocked_review_and_department_scope_follow_the_retained_policy() {
    let no_recipient = quality_work_from_workspace(
        &actor(&["quality.read", "quality.inspect"]),
        &data(HoldStatus::Held, "maker", 0),
        &open(),
        &|user| user == "inspector",
    )
    .unwrap();
    assert_eq!(
        no_recipient.items[0].blocker,
        Some(
            "No eligible independent recipient; Company administrator must assign quality authority"
        )
    );

    let mut scoped = actor(&["quality.read", "quality.inspect"]);
    scoped.department_id = Some(RecordId::new("department").unwrap());
    let empty = quality_work_from_workspace(
        &scoped,
        &data(HoldStatus::Held, "maker", 1),
        &open(),
        &|user| user == "inspector",
    )
    .unwrap();
    assert!(empty.items.is_empty());
}

struct Repo {
    data: QualityWorkspaceData,
    core: QualityDetailCoreData,
}

impl QualityReadRepository for Repo {
    fn workspace<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
    ) -> QualityFuture<'a, QualityWorkspaceData> {
        let data = self.data.clone();
        Box::pin(async move { Ok(data) })
    }

    fn detail<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
        _id: &'a str,
    ) -> QualityFuture<'a, Option<erpx_server_quality::public::quality_read::QualityDetailData>>
    {
        Box::pin(async move { Ok(None) })
    }

    fn detail_core<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
        _id: &'a str,
    ) -> QualityFuture<'a, Option<QualityDetailCoreData>> {
        let core = self.core.clone();
        Box::pin(async move { Ok(Some(core)) })
    }
}

#[tokio::test]
async fn repository_detail_core_and_injected_predicates_drive_both_ports() {
    let core_data = QualityDetailCoreData {
        hold: hold(HoldStatus::Held, "maker"),
        product: QualityHoldProduct {
            product_name: "Quality product".into(),
            ..QualityHoldProduct::empty()
        },
        inspections: Vec::new(),
        measurements: Vec::new(),
        dispositions: Vec::new(),
        spec_metrics: Vec::new(),
        alerts: Vec::new(),
    };
    let mut repo = Repo {
        data: data(HoldStatus::Held, "alias", 1),
        core: core_data,
    };
    let actor = actor(&["quality.read", "quality.inspect"]);
    let source = restricted();

    let work = quality_work_with_same_person(&actor, &mut repo, &source, |user| {
        user == "inspector" || user == "alias"
    })
    .await
    .unwrap();
    assert_eq!(work.items.len(), 1);
    assert!(!work.items[0].can_act);
    assert_eq!(work.items[0].relationships, vec!["requested"]);

    let core = repo
        .detail_core(&actor.company_id, actor.branch_id.as_ref(), "hold-1")
        .await
        .unwrap()
        .unwrap();
    let detail = quality_detail_core_from_source(&actor, core).unwrap();
    assert_eq!(detail.hold.hold.id, "hold-1");
    assert_eq!(detail.hold.next_action, "Record an independent inspection");

    let default_source = open();
    let default = quality_work(&actor, &mut repo, &default_source)
        .await
        .unwrap();
    assert_eq!(default.items.len(), 1);
    // Exact-id default does not collapse the merged alias; the injected
    // Identity predicate above is what suppresses that action.
    assert!(default.items[0].can_act);
}

#[test]
fn failed_injected_source_access_propagates_as_a_business_error() {
    struct Failing;
    impl SourceAccess for Failing {
        fn source_access(
            &self,
            _hold_id: &str,
        ) -> Result<Option<QualitySourceAccess>, BusinessError> {
            Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "QUALITY_SOURCE_UNAVAILABLE",
                "Source facts unavailable.",
            ))
        }
    }
    let result = quality_work_from_workspace(
        &actor(&["quality.read", "quality.inspect"]),
        &data(HoldStatus::Held, "maker", 1),
        &Failing,
        &|user| user == "inspector",
    );
    let error = result.unwrap_err();
    assert_eq!(error.code, "QUALITY_SOURCE_UNAVAILABLE");
}
