use erpx_server_operations::public::field_exceptions::*;
fn row() -> Exception {
    Exception {
        id: "exception".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        area: "service".into(),
        source_kind: "field_service_order".into(),
        source_id: "service".into(),
        source_version: 1,
        source_reference_json: "{}".into(),
        policy_id: "policy".into(),
        policy_snapshot_json: "{}".into(),
        title: "Review visit".into(),
        issue: "Evidence missing".into(),
        priority: "normal".into(),
        status: "open".into(),
        owner_id: "owner".into(),
        escalation_owner_id: None,
        resolution: None,
        resolved_by: None,
        reviewed_by: None,
        version: 1,
        created_by: "maker".into(),
        created_at: 0,
        updated_at: 0,
        due_at: 100,
    }
}
fn actor(user: &str, review: bool) -> Actor {
    Actor {
        user: user.into(),
        can_review: review,
        same_person_owner: false,
        same_person_resolver: false,
    }
}
fn input(action: Action) -> TransitionInput {
    TransitionInput {
        action,
        expected_version: 1,
        reason: "Reviewed actual source".into(),
        owner_id: None,
        escalation_owner_id: None,
        resolution: None,
        priority: None,
    }
}
#[test]
fn owner_cannot_skip_acknowledgement_or_complete_another_owners_exception() {
    let r = row();
    assert_eq!(
        transition(&actor("owner", false), &r, &input(Action::Start))
            .unwrap_err()
            .code,
        "OPERATIONS_STATE_CHANGED"
    );
    assert_eq!(
        transition(&actor("other", true), &r, &input(Action::Acknowledge))
            .unwrap_err()
            .code,
        "OPERATIONS_OWNER_REQUIRED"
    );
    assert_eq!(
        transition(&actor("owner", false), &r, &input(Action::Acknowledge))
            .unwrap()
            .status,
        "acknowledged"
    );
}
#[test]
fn resolution_requires_evidence_and_enters_independent_review() {
    let mut r = row();
    r.status = "in_progress".into();
    let mut i = input(Action::Resolve);
    assert!(transition(&actor("owner", false), &r, &i).is_err());
    i.resolution = Some("\u{feff}Evidence reviewed\u{feff}".into());
    let next = transition(&actor("owner", false), &r, &i).unwrap();
    assert_eq!(next.status, "awaiting_review");
    assert_eq!(next.resolution.as_deref(), Some("Evidence reviewed"));
    assert_eq!(next.resolved_by.as_deref(), Some("owner"));
}
#[test]
fn independent_review_rejects_same_person_aliases_even_when_the_user_id_differs() {
    let mut r = row();
    r.status = "awaiting_review".into();
    r.resolution = Some("evidence".into());
    r.resolved_by = Some("resolver".into());
    for action in [Action::Approve, Action::Reject] {
        for (owner, resolver) in [(true, false), (false, true)] {
            let mut a = actor("different-account", true);
            a.same_person_owner = owner;
            a.same_person_resolver = resolver;
            assert_eq!(
                transition(&a, &r, &input(action)).unwrap_err().code,
                "OPERATIONS_INDEPENDENT_REVIEW"
            );
        }
    }
    let p = transition(&actor("reviewer", true), &r, &input(Action::Approve)).unwrap();
    assert_eq!(p.status, "resolved");
    assert_eq!(p.reviewed_by.as_deref(), Some("reviewer"));
}
#[test]
fn reassigning_pending_review_resets_resolution_and_preserves_scope_in_the_owner_row() {
    let mut r = row();
    r.status = "awaiting_review".into();
    r.resolution = Some("old evidence".into());
    r.resolved_by = Some("owner".into());
    r.reviewed_by = Some("reviewer".into());
    let mut i = input(Action::Assign);
    i.owner_id = Some("new-owner".into());
    let p = transition(&actor("reviewer", true), &r, &i).unwrap();
    assert_eq!(p.status, "in_progress");
    assert_eq!(p.owner, "new-owner");
    assert!(p.resolution.is_none() && p.resolved_by.is_none() && p.reviewed_by.is_none());
    assert_eq!(r.branch_id.as_deref(), Some("branch"));
}
#[test]
fn escalation_routes_review_without_changing_the_assigned_owner() {
    let r = row();
    let mut i = input(Action::Escalate);
    i.escalation_owner_id = Some("reviewer".into());
    let p = transition(&actor("owner", false), &r, &i).unwrap();
    assert_eq!(p.status, "escalated");
    assert_eq!(p.owner, "owner");
    assert_eq!(p.escalation_owner.as_deref(), Some("reviewer"));
}
#[test]
fn stale_versions_and_terminal_transitions_are_rejected_without_overflow() {
    let mut r = row();
    r.status = "closed".into();
    assert!(transition(&actor("owner", true), &r, &input(Action::Cancel)).is_err());
    let mut i = input(Action::Reopen);
    i.expected_version = 2;
    assert!(transition(&actor("reviewer", true), &r, &i).is_err());
    r.version = 9_007_199_254_740_991;
    i.expected_version = r.version;
    assert!(transition(&actor("reviewer", true), &r, &i).is_err());
}

#[test]
fn field_policy_preserves_input_bounds_and_deduplicates_only_storage() {
    use erpx_server_operations::public::field_policy::*;
    let input = DraftPolicy {
        area: " service ".into(),
        name: " Native queue ".into(),
        target_minutes: 60,
        source_kinds: vec!["field_service_order".into(), "field_service_order".into()],
    };
    let row = input.draft("policy", "company", "maker", 1, 0).unwrap();
    assert_eq!(row.area, "service");
    assert_eq!(row.name, "Native queue");
    assert_eq!(row.source_kinds_json, "[\"field_service_order\"]");
    assert_eq!(input.source_kinds.len(), 2);
    assert!(
        input
            .draft(&"x".repeat(181), "company", "maker", 1, 0)
            .is_err()
    );
    let mut bad = input.clone();
    bad.source_kinds.push("support_ticket".into());
    assert_eq!(
        bad.validate().unwrap_err().code,
        "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED"
    );
    bad = input.clone();
    bad.area = "A".into();
    assert!(bad.validate().is_err());
    bad = input;
    bad.target_minutes = 10081;
    assert!(bad.validate().is_err());
}
#[test]
fn policy_review_needs_an_independent_creator_and_exact_current_revision() {
    use erpx_server_operations::public::field_policy::*;
    let row = DraftPolicy {
        area: "service".into(),
        name: "Native queue".into(),
        target_minutes: 60,
        source_kinds: vec!["field_service_order".into()],
    }
    .draft("policy", "company", "maker", 1, 0)
    .unwrap();
    assert!(review_policy(&row, PolicyAction::Publish, 1, true, "Review policy").is_err());
    assert_eq!(
        review_policy(&row, PolicyAction::Publish, 1, false, "Review policy").unwrap(),
        2
    );
    assert!(review_policy(&row, PolicyAction::Publish, 2, false, "Review policy").is_err());
    let mut published = row;
    published.status = "published".into();
    published.version = 2;
    assert_eq!(
        review_policy(&published, PolicyAction::Retire, 2, true, "Retire policy").unwrap(),
        3
    );
}
#[test]
fn open_uses_the_exact_published_policy_and_bounded_due_date() {
    use erpx_server_operations::public::field_policy::*;
    let mut policy = DraftPolicy {
        area: "service".into(),
        name: "Native queue".into(),
        target_minutes: 60,
        source_kinds: vec!["field_service_order".into()],
    }
    .draft("policy", "company", "maker", 1, 0)
    .unwrap();
    policy.status = "published".into();
    policy.version = 2;
    let mut input = OpenException {
        source_id: "order".into(),
        source_version: 1,
        policy_id: "policy".into(),
        policy_version: 2,
        title: "Visit issue".into(),
        issue: "Review evidence".into(),
        priority: "normal".into(),
        owner_id: "person".into(),
    };
    assert_eq!(input.due_at(&policy, 1000).unwrap(), 3601000);
    assert!(input.due_at(&policy, 9_007_199_254_740_000).is_err());
    input.policy_version = 1;
    assert_eq!(
        input.due_at(&policy, 0).unwrap_err().code,
        "OPERATIONS_POLICY_UNAVAILABLE"
    );
}

#[test]
fn my_actions_uses_live_independence_and_preserves_queue_relationships() {
    let mut r = row();
    let mut a = actor("reviewer", true);
    r.status = "awaiting_review".into();
    let w = exception_work(&r, &a, false);
    assert!(w.can_act);
    assert_eq!(w.queue, "todo");
    assert_eq!(w.relationships, vec!["approval"]);
    a.same_person_owner = true;
    assert!(!exception_work(&r, &a, true).can_act);
    a.same_person_owner = false;
    a.same_person_resolver = true;
    assert!(!exception_work(&r, &a, true).can_act);
    a = actor("owner", false);
    r.status = "open".into();
    assert!(exception_work(&r, &a, true).can_act);
    assert!(!exception_work(&r, &a, false).can_act);
    assert_eq!(exception_work(&r, &a, true).relationships, vec!["assigned"]);
    for state in ["closed", "cancelled"] {
        r.status = state.into();
        let w = exception_work(&r, &a, true);
        assert!(!w.can_act);
        assert_eq!(w.queue, "completed");
    }
}
