use chrono::{Duration, TimeZone, Utc};
use erpx_kernel::{ActorAttribution, ActorKind, HumanDecisionProof, SessionId};

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at(minute: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).single().unwrap() + Duration::minutes(minute)
}
fn scope(branch: u128) -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: id(1),
        company_id: id(2),
        branch_id: id(branch),
    }
}
fn human_decision(
    principal: u128,
    authority_scope: AuthorityScope,
    decision_ref: String,
    decided_at: DateTime<Utc>,
) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: id(principal).to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(900)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        authority_scope,
        decision_ref,
        decided_at,
    )
    .unwrap()
}
fn workflow() -> WorkflowTemplate {
    WorkflowTemplate::publish(
        WorkflowTemplateId::try_from_uuid(id(10)).unwrap(),
        scope(3),
        "Delivery workflow",
        vec![
            WorkflowStateDefinition {
                code: "proposed".into(),
                core_state: CoreWorkState::Proposed,
            },
            WorkflowStateDefinition {
                code: "working".into(),
                core_state: CoreWorkState::InProgress,
            },
            WorkflowStateDefinition {
                code: "closed".into(),
                core_state: CoreWorkState::Closed,
            },
        ],
        vec![
            WorkflowTransitionDefinition {
                from: "proposed".into(),
                to: "working".into(),
            },
            WorkflowTransitionDefinition {
                from: "working".into(),
                to: "closed".into(),
            },
        ],
        at(0),
    )
    .unwrap()
}
fn project() -> Project {
    Project::propose(
        ProjectId::try_from_uuid(id(20)).unwrap(),
        scope(3),
        "PRJ-20",
        "Factory upgrade",
        ProjectClass::Capital,
        "capital-policy-v1",
        &workflow(),
        id(30),
        at(0),
    )
    .unwrap()
}
fn node(value: u128, parent: Option<ProjectNodeId>, kind: NodeKind) -> ProjectNode {
    ProjectNode::new(
        ProjectNodeId::try_from_uuid(id(value)).unwrap(),
        parent,
        kind,
        format!("Node {value}"),
        id(100 + value),
        vec![id(300 + value)],
        None,
        None,
        "proposed",
        CoreWorkState::Proposed,
        None,
        at(1),
    )
    .unwrap()
}

#[test]
fn hierarchy_rejects_excess_depth_and_cycle_without_mutation() {
    let mut value = project();
    let flow = workflow();
    let phase = ProjectNodeId::try_from_uuid(id(40)).unwrap();
    let item = ProjectNodeId::try_from_uuid(id(41)).unwrap();
    let subitem = ProjectNodeId::try_from_uuid(id(42)).unwrap();
    value
        .add_node(node(40, None, NodeKind::Phase), &flow, at(1))
        .unwrap();
    value
        .add_node(node(41, Some(phase), NodeKind::Task), &flow, at(2))
        .unwrap();
    value
        .add_node(node(42, Some(item), NodeKind::Issue), &flow, at(3))
        .unwrap();
    assert_eq!(
        value.add_node(node(43, Some(subitem), NodeKind::Task), &flow, at(4)),
        Err(ProjectDomainError::InvalidHierarchy)
    );
    let before = value.clone();
    assert_eq!(
        value.move_node(subitem, Some(subitem), 1, at(4)),
        Err(ProjectDomainError::HierarchyCycle)
    );
    assert_eq!(value, before);
}

#[test]
fn workflow_mapping_is_versioned_and_withdrawal_blocks_new_transition() {
    let mut value = project();
    let mut flow = workflow();
    let phase = ProjectNodeId::try_from_uuid(id(40)).unwrap();
    value
        .add_node(node(40, None, NodeKind::Epic), &flow, at(1))
        .unwrap();
    value
        .transition_node(phase, 1, "working", &flow, at(2))
        .unwrap();
    assert_eq!(
        value.nodes().get(&phase).unwrap().core_state(),
        CoreWorkState::InProgress
    );
    flow.withdraw(at(3)).unwrap();
    assert_eq!(
        value.transition_node(phase, 2, "closed", &flow, at(3)),
        Err(ProjectDomainError::InvalidTransition)
    );
}

#[test]
fn promotion_pins_exact_comment_revision() {
    let source = CommentRevisionRef {
        thread_id: id(80),
        comment_id: id(81),
        revision: 3,
        content_digest: "a".repeat(64),
    };
    let value = ProjectNode::new(
        ProjectNodeId::try_from_uuid(id(82)).unwrap(),
        Some(ProjectNodeId::try_from_uuid(id(40)).unwrap()),
        NodeKind::Issue,
        "Promoted issue",
        id(83),
        Vec::new(),
        None,
        None,
        "proposed",
        CoreWorkState::Proposed,
        Some(source.clone()),
        at(1),
    )
    .unwrap();
    assert_eq!(value.promotion_source(), Some(&source));
}

fn endpoint(project: u128, node: u128, branch: u128, owner: u128) -> DependencyEndpoint {
    DependencyEndpoint {
        project_id: ProjectId::try_from_uuid(id(project)).unwrap(),
        project_version: 4,
        node_id: ProjectNodeId::try_from_uuid(id(node)).unwrap(),
        node_version: 2,
        scope: scope(branch),
        owner_id: id(owner),
    }
}

#[test]
fn cross_branch_dependency_needs_both_exact_owner_acceptances() {
    let mut value = DependencyAgreement::propose(
        DependencyId::try_from_uuid(id(120)).unwrap(),
        endpoint(100, 101, 3, 110),
        endpoint(102, 103, 4, 111),
        DependencyKind::FinishToStart,
        id(110),
        at(0),
    )
    .unwrap();
    let source_decision = human_decision(
        110,
        scope(3),
        dependency_acceptance_decision_ref(value.id(), DependencySide::Source),
        at(1),
    );
    value
        .accept(DependencySide::Source, 1, &source_decision)
        .unwrap();
    assert_eq!(value.status(), DependencyStatus::Proposed);
    let wrong_owner_decision = human_decision(
        999,
        scope(4),
        dependency_acceptance_decision_ref(value.id(), DependencySide::Target),
        at(2),
    );
    assert_eq!(
        value.accept(DependencySide::Target, 1, &wrong_owner_decision),
        Err(ProjectDomainError::DependencyAuthorityDenied)
    );
    let target_decision = human_decision(
        111,
        scope(4),
        dependency_acceptance_decision_ref(value.id(), DependencySide::Target),
        at(2),
    );
    value
        .accept(DependencySide::Target, 1, &target_decision)
        .unwrap();
    assert_eq!(value.status(), DependencyStatus::Accepted);
    value.revoke(id(111), at(3)).unwrap();
    assert_eq!(value.status(), DependencyStatus::Revoked);
}

#[test]
fn typed_handoff_enforces_maker_checker_and_owner_receipt() {
    let mut value = ProjectHandoff::propose(
        HandoffId::try_from_uuid(id(130)).unwrap(),
        ProjectId::try_from_uuid(id(20)).unwrap(),
        ProjectNodeId::try_from_uuid(id(41)).unwrap(),
        scope(3),
        HandoffKind::AssetCapitalization,
        "b".repeat(64),
        "capitalization-130",
        CorrelationId::try_from_uuid(id(131)).unwrap(),
        id(132),
        at(0),
    )
    .unwrap();
    let maker_decision = human_decision(
        132,
        scope(3),
        handoff_approval_decision_ref(value.id()),
        at(1),
    );
    assert_eq!(
        value.approve(&maker_decision),
        Err(ProjectDomainError::HandoffAuthorityDenied)
    );
    let checker_decision = human_decision(
        133,
        scope(3),
        handoff_approval_decision_ref(value.id()),
        at(1),
    );
    value.approve(&checker_decision).unwrap();
    value.mark_ready(at(2)).unwrap();
    assert_eq!(
        value.reconcile(HandoffOutcome {
            owner_context: ContextId::Finance,
            owner_receipt_id: "receipt-1".into(),
            owner_record: None,
            accepted: true,
            reason_code: "accepted".into(),
            reconciled_at: at(3),
        }),
        Err(ProjectDomainError::InvalidHandoffOutcome)
    );
    value
        .reconcile(HandoffOutcome {
            owner_context: ContextId::Assets,
            owner_receipt_id: "receipt-2".into(),
            owner_record: None,
            accepted: true,
            reason_code: "capitalized".into(),
            reconciled_at: at(3),
        })
        .unwrap();
    assert_eq!(value.status(), HandoffStatus::Accepted);
}

#[test]
fn dependency_and_handoff_decisions_require_exact_scope_reference_and_time() {
    let mut dependency = DependencyAgreement::propose(
        DependencyId::try_from_uuid(id(140)).unwrap(),
        endpoint(100, 101, 3, 110),
        endpoint(102, 103, 4, 111),
        DependencyKind::FinishToStart,
        id(110),
        at(2),
    )
    .unwrap();
    let wrong_scope = human_decision(
        110,
        scope(4),
        dependency_acceptance_decision_ref(dependency.id(), DependencySide::Source),
        at(3),
    );
    assert_eq!(
        dependency.accept(DependencySide::Source, 1, &wrong_scope),
        Err(ProjectDomainError::HumanDecisionRequired)
    );
    let wrong_ref = human_decision(
        110,
        scope(3),
        "projects.dependency:other:accept:source".into(),
        at(3),
    );
    assert_eq!(
        dependency.accept(DependencySide::Source, 1, &wrong_ref),
        Err(ProjectDomainError::HumanDecisionRequired)
    );
    let stale = human_decision(
        110,
        scope(3),
        dependency_acceptance_decision_ref(dependency.id(), DependencySide::Source),
        at(1),
    );
    assert_eq!(
        dependency.accept(DependencySide::Source, 1, &stale),
        Err(ProjectDomainError::InvalidDependencyTransition)
    );

    let mut handoff = ProjectHandoff::propose(
        HandoffId::try_from_uuid(id(141)).unwrap(),
        ProjectId::try_from_uuid(id(20)).unwrap(),
        ProjectNodeId::try_from_uuid(id(41)).unwrap(),
        scope(3),
        HandoffKind::AssetCapitalization,
        "c".repeat(64),
        "capitalization-141",
        CorrelationId::try_from_uuid(id(142)).unwrap(),
        id(132),
        at(2),
    )
    .unwrap();
    let wrong_ref = human_decision(
        133,
        scope(3),
        "projects.handoff:other:approve".into(),
        at(3),
    );
    assert_eq!(
        handoff.approve(&wrong_ref),
        Err(ProjectDomainError::HumanDecisionRequired)
    );
}

#[test]
fn project_and_item_have_stable_routes() {
    let project_id = Uuid::new_v4();
    let node_id = Uuid::new_v4();
    let flow = workflow();
    let mut value = Project::propose(
        ProjectId::try_from_uuid(project_id).unwrap(),
        scope(3),
        "PRJ-REF",
        "Referenced project",
        ProjectClass::Internal,
        "internal-policy",
        &flow,
        id(30),
        at(0),
    )
    .unwrap();
    value
        .add_node(
            ProjectNode::new(
                ProjectNodeId::try_from_uuid(node_id).unwrap(),
                None,
                NodeKind::Epic,
                "Referenced epic",
                id(31),
                Vec::new(),
                None,
                None,
                "proposed",
                CoreWorkState::Proposed,
                None,
                at(1),
            )
            .unwrap(),
            &flow,
            at(1),
        )
        .unwrap();
    assert_eq!(
        value.record_ref().unwrap().canonical_detail_route(),
        format!("/project/project/{project_id}")
    );
    assert_eq!(
        value
            .node_record_ref(ProjectNodeId::try_from_uuid(node_id).unwrap())
            .unwrap()
            .canonical_detail_route(),
        format!("/project/item/{node_id}")
    );
}
