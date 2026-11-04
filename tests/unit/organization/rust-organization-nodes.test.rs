use erpx_server_organization::public::organization_nodes::*;
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};
use serde_json::json;

fn input(value: serde_json::Value) -> NodeChangeInput {
    NodeChangeInput::parse(&value).unwrap()
}

fn actor(
    user: &str,
    scope: OrganizationScopeKind,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> OrganizationActor {
    OrganizationActor {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new(user).unwrap(),
        assignment_id: erpx_server_shared::public::AssignmentId::new("assignment").unwrap(),
        role: "custom_organization".into(),
        branch_id: branch.map(|id| RecordId::new(id).unwrap()),
        department_id: department.map(|id| RecordId::new(id).unwrap()),
        scope_kind: scope,
        assignment_scope_kind: scope,
        permissions: permissions.iter().map(|value| (*value).into()).collect(),
        authority: None,
    }
}

fn create_branch() -> serde_json::Value {
    json!({
        "action": "create",
        "kind": "branch",
        "slug": "north",
        "displayName": "Northern branch",
        "branchId": null,
        "targetNodeId": null,
        "expectedNodeVersion": null,
        "expectedVersion": 0,
        "reason": "Open the approved northern operation"
    })
}

fn create_department() -> serde_json::Value {
    json!({
        "action": "create",
        "kind": "department",
        "slug": "service-team",
        "displayName": "Service",
        "branchId": "branch",
        "targetNodeId": null,
        "expectedNodeVersion": null,
        "expectedVersion": 0,
        "reason": "Create the reviewed service team"
    })
}

fn relocate() -> serde_json::Value {
    json!({
        "action": "relocate",
        "kind": "department",
        "slug": null,
        "displayName": "Service",
        "branchId": "destination",
        "targetNodeId": "department",
        "expectedNodeVersion": 1,
        "expectedVersion": 0,
        "reason": "Move the team to its operating branch",
        "previewSnapshot": "a".repeat(64)
    })
}

#[test]
fn parses_the_retained_node_change_input_and_rejects_reserved_or_malformed_values() {
    let parsed = input(create_branch());
    assert_eq!(parsed.action, NodeChangeAction::Create);
    assert_eq!(parsed.kind, NodeLifecycleKind::Branch);
    assert_eq!(parsed.slug.as_deref(), Some("north"));
    assert!(parsed.validate_structure().is_ok());

    let reserved = json!({"action":"create","kind":"branch","slug":"finance","displayName":"Proxy","expectedVersion":0,"reason":"Shadow a product"});
    assert_eq!(
        NodeChangeInput::parse(&reserved).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let bad_slug = json!({"action":"create","kind":"branch","slug":"North_Branch","displayName":"Capital","expectedVersion":0,"reason":"Invalid code"});
    assert_eq!(
        NodeChangeInput::parse(&bad_slug).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let short_reason = json!({"action":"create","kind":"branch","slug":"north","displayName":"North","expectedVersion":0,"reason":"a"});
    assert_eq!(
        NodeChangeInput::parse(&short_reason).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let uppercase_preview = json!({"action":"relocate","kind":"department","targetNodeId":"department","branchId":"destination","displayName":"Service","expectedNodeVersion":1,"expectedVersion":0,"reason":"Move it","previewSnapshot":"A".repeat(64)});
    assert_eq!(
        NodeChangeInput::parse(&uppercase_preview).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn create_and_lifecycle_structure_keep_branch_and_department_placement_exact() {
    let mut created = input(create_branch());
    created.target_node_id = Some(RecordId::new("existing").unwrap());
    assert_eq!(
        created.validate_structure().unwrap_err().code,
        "NEW_NODE_INVALID"
    );
    let mut no_slug = input(create_branch());
    no_slug.slug = None;
    assert_eq!(no_slug.validate_structure().unwrap_err().code, "SLUG_REQUIRED");
    let branch_parent = json!({"action":"create","kind":"branch","slug":"north","displayName":"North","branchId":"branch","expectedVersion":0,"reason":"Invalid parent"});
    assert_eq!(
        input(branch_parent).validate_structure().unwrap_err().code,
        "BRANCH_PARENT_INVALID"
    );
    let department_without_branch = json!({"action":"create","kind":"department","slug":"service-team","displayName":"Service","branchId":null,"expectedVersion":0,"reason":"Invalid placement"});
    assert_eq!(
        input(department_without_branch).validate_structure().unwrap_err().code,
        "DEPARTMENT_BRANCH_REQUIRED"
    );
    let mut renamed = input(json!({"action":"rename","kind":"department","targetNodeId":"department","branchId":"branch","displayName":"Service","expectedNodeVersion":1,"expectedVersion":0,"reason":"Rename the unit"}));
    renamed.expected_node_version = None;
    assert_eq!(
        renamed.validate_structure().unwrap_err().code,
        "NODE_VERSION_REQUIRED"
    );
}

#[test]
fn target_facts_refuse_stale_wrong_placement_and_unsafe_lifecycle_actions() {
    let input = input(json!({"action":"rename","kind":"department","targetNodeId":"department","branchId":"branch","displayName":"Service","expectedNodeVersion":1,"expectedVersion":0,"reason":"Rename the unit"}));
    let facts = NodeFacts {
        id_kind: NodeLifecycleKind::Department,
        branch_id: Some(RecordId::new("branch").unwrap()),
        status: NodeStatus::Active,
        hierarchy_revision: 1,
    };
    assert!(
        validate_target_facts(
            &input,
            Some(facts.clone()),
            true,
            Some(&RecordId::new("branch").unwrap())
        )
        .is_ok()
    );
    assert_eq!(
        validate_target_facts(&input, Some(facts.clone()), true, Some(&RecordId::new("other").unwrap()))
            .unwrap_err()
            .code,
        "NODE_PLACEMENT_CHANGED"
    );
    let mut suspended = input.clone();
    suspended.action = NodeChangeAction::Suspend;
    assert_eq!(
        validate_target_facts(&suspended, Some(NodeFacts { status: NodeStatus::Suspended, ..facts.clone() }), true, Some(&RecordId::new("branch").unwrap()))
            .unwrap_err()
            .code,
        "NODE_STATE_CHANGED"
    );
    let mut reactivate = input.clone();
    reactivate.action = NodeChangeAction::Reactivate;
    assert_eq!(
        validate_target_facts(&reactivate, Some(facts.clone()), true, Some(&RecordId::new("branch").unwrap()))
            .unwrap_err()
            .code,
        "NODE_STATE_CHANGED"
    );
    let mut absorbed_suspended = input.clone();
    absorbed_suspended.action = NodeChangeAction::Reactivate;
    assert!(
        validate_target_facts(&absorbed_suspended, Some(NodeFacts { status: NodeStatus::Archived, ..facts.clone() }), true, Some(&RecordId::new("branch").unwrap()))
            .is_ok()
    );
    let mut relocate = input.clone();
    relocate.action = NodeChangeAction::Relocate;
    relocate.target_node_id = Some(RecordId::new("department").unwrap());
    relocate.expected_node_version = Some(1);
    relocate.branch_id = Some(RecordId::new("destination").unwrap());
    assert!(
        validate_target_facts(&relocate, Some(facts.clone()), true, Some(&RecordId::new("destination").unwrap()))
            .is_ok()
    );
    let moved = NodeFacts {
        branch_id: Some(RecordId::new("destination").unwrap()),
        ..facts.clone()
    };
    assert_eq!(
        validate_target_facts(&relocate, Some(moved.clone()), true, Some(&RecordId::new("destination").unwrap()))
            .unwrap_err()
            .code,
        "RELOCATION_UNCHANGED"
    );
    let stale = NodeFacts {
        branch_id: Some(RecordId::new("branch").unwrap()),
        hierarchy_revision: 2,
        ..facts.clone()
    };
    assert_eq!(
        validate_target_facts(&relocate, Some(stale), true, Some(&RecordId::new("destination").unwrap()))
            .unwrap_err()
            .code,
        "NODE_VERSION_CHANGED"
    );
    let mut branch_relocation = relocate.clone();
    branch_relocation.kind = NodeLifecycleKind::Branch;
    branch_relocation.branch_id = None;
    assert_eq!(
        validate_target_facts(&branch_relocation, Some(NodeFacts { id_kind: NodeLifecycleKind::Branch, ..facts.clone() }), true, None)
            .unwrap_err()
            .code,
        "RELOCATION_REQUIRES_DEPARTMENT"
    );
}

#[test]
fn relocation_fences_and_impact_refuse_children_slug_conflicts_and_live_dependencies() {
    assert_eq!(
        validate_relocation_children(true).unwrap_err().code,
        "RELOCATION_HAS_CHILDREN"
    );
    assert!(validate_relocation_children(false).is_ok());
    assert_eq!(
        validate_relocation_slug_conflict(true).unwrap_err().code,
        "RELOCATION_SLUG_CONFLICT"
    );
    assert!(validate_relocation_slug_conflict(false).is_ok());
    assert_eq!(
        validate_reserved_draft(true).unwrap_err().code,
        "ORGANIZATION_DRAFT_RESERVED"
    );
    assert_eq!(
        validate_preview_snapshot(Some(&"b".repeat(64)), &"a".repeat(64))
            .unwrap_err()
            .code,
        "ORGANIZATION_RELOCATION_CHANGED"
    );
    assert!(validate_preview_snapshot(None, &"a".repeat(64)).is_ok());
    assert_eq!(
        validate_lifecycle_impact(NodeChangeAction::Suspend, NodeImpact { staff: 1, ..NodeImpact::default() })
            .unwrap_err()
            .code,
        "ORGANIZATION_NODE_IN_USE"
    );
    assert!(validate_lifecycle_impact(NodeChangeAction::Rename, NodeImpact { staff: 1, ..NodeImpact::default() }).is_ok());
    assert!(validate_lifecycle_impact(NodeChangeAction::Archive, NodeImpact::default()).is_ok());
}

#[test]
fn independent_review_requires_another_fresh_company_authority_and_exact_version() {
    let requester = UserId::new("requester").unwrap();
    let reviewer = actor(
        "reviewer",
        OrganizationScopeKind::Holding,
        None,
        None,
        &["organization.manage"],
    );
    assert!(
        validate_review_state(
            &requester,
            None,
            &reviewer,
            NodeChangeState::Submitted,
            1,
            1,
            true
        )
        .is_ok()
    );
    assert_eq!(
        validate_review_state(
            &requester,
            None,
            &reviewer,
            NodeChangeState::Submitted,
            2,
            1,
            true
        )
        .unwrap_err()
        .code,
        "STALE_VERSION"
    );
    assert_eq!(
        validate_review_state(
            &requester,
            None,
            &reviewer,
            NodeChangeState::Draft,
            1,
            1,
            true
        )
        .unwrap_err()
        .code,
        "CHANGE_NOT_SUBMITTED"
    );
    assert_eq!(
        validate_review_state(
            &reviewer.user_id,
            None,
            &reviewer,
            NodeChangeState::Submitted,
            1,
            1,
            true
        )
        .unwrap_err()
        .code,
        "INDEPENDENT_REVIEW_REQUIRED"
    );
    assert_eq!(
        validate_review_state(
            &requester,
            None,
            &reviewer,
            NodeChangeState::Submitted,
            1,
            1,
            false
        )
        .unwrap_err()
        .code,
        "INDEPENDENT_REVIEW_REQUIRED"
    );
    assert_eq!(
        validate_change_owner(&requester, &reviewer).unwrap_err().code,
        "CHANGE_OWNER_REQUIRED"
    );
    assert_eq!(
        validate_editable(NodeChangeState::Submitted).unwrap_err().code,
        "CHANGE_LOCKED"
    );
    assert_eq!(
        validate_cancellable(NodeChangeState::Submitted, 2, 1).unwrap_err().code,
        "CONFLICT"
    );
    assert!(validate_cancellable(NodeChangeState::Submitted, 1, 1).is_ok());
}

#[test]
fn business_unit_scope_and_sharing_follow_the_current_placement() {
    let company = actor("owner", OrganizationScopeKind::Holding, None, None, &["organization.manage"]);
    assert_eq!(visible_node_scope(&company), VisibleNodeScope::Company);
    let branch = actor("branch", OrganizationScopeKind::Branch, Some("branch"), None, &[]);
    assert_eq!(
        visible_node_scope(&branch),
        VisibleNodeScope::Branch(Some(RecordId::new("branch").unwrap()))
    );
    let department = actor("department", OrganizationScopeKind::Department, Some("branch"), Some("department"), &[]);
    assert_eq!(
        visible_node_scope(&department),
        VisibleNodeScope::Department(Some(RecordId::new("department").unwrap()))
    );
    assert_eq!(
        channel_visible("company", "company", None, None),
        true
    );
    assert_eq!(
        channel_visible("branch", "branch", Some("branch"), None),
        true
    );
    assert_eq!(
        channel_visible("branch", "branch", Some("other"), None),
        false
    );
    assert_eq!(
        channel_visible("department", "department", Some("branch"), Some("department")),
        true
    );
    assert_eq!(
        channel_visible("department", "department", Some("branch"), Some("other")),
        false
    );
    assert_eq!(channel_visible("team", "team", None, None), false);
    let audience = ChannelAudience {
        active_person: true,
        active_single_engagement: true,
        active_assignment: true,
        kind_matches: true,
        placement_matches: true,
    };
    assert!(audience.visible());
    assert!(!ChannelAudience { active_assignment: false, ..audience }.visible());
    assert!(company.can_manage_units());
    assert!(!branch.can_manage_units());
    assert_eq!(branch.company_authority().unwrap_err().code, "COMPANY_AUTHORITY_REQUIRED");
}

#[test]
fn relocation_snapshot_json_keeps_the_retained_ordered_evidence() {
    let snapshot = RelocationSnapshot {
        engagements: vec![("engagement".into(), 3, "active".into())],
        positions: vec![("position".into(), 2)],
        assignments: vec![("assignment".into(), 4)],
        rules: vec![("rule".into(), 1)],
        head: Some((
            2,
            Some("head-position".into()),
            Some("person".into()),
            Some(1_700_000_000_000),
        )),
        legacy_head: Some((1, "person".into())),
        flow: Some(("flow".into(), 2)),
    };
    assert_eq!(
        relocation_snapshot_json(&snapshot),
        "{\"engagements\":[{\"id\":\"engagement\",\"version\":3,\"status\":\"active\"}],\"positions\":[{\"id\":\"position\",\"version\":2}],\"assignments\":[{\"id\":\"assignment\",\"version\":4}],\"rules\":[{\"id\":\"rule\",\"version\":1}],\"head\":{\"version\":2,\"position_id\":\"head-position\",\"person_id\":\"person\",\"effective_at\":1700000000000},\"legacyHead\":{\"version\":1,\"person_id\":\"person\"},\"flow\":{\"id\":\"flow\",\"version\":2}}"
    );
}
