use erpx_server_organization::public::organization_nodes::{
    OrganizationActor, OrganizationCommand, OrganizationCommandActor, OrganizationScopeKind,
};
use erpx_server_organization::public::position_moves::*;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde_json::{json, Value};

fn input() -> PositionMoveInput {
    PositionMoveInput::parse(&json!({
        "positionId": "position-2",
        "expectedVersion": 3,
        "targetKind": "level",
        "targetId": "level-3",
        "effectiveAt": 1_800_003_600_000_i64,
        "reason": "Promote the reviewed position",
        "managerAssignments": []
    }))
    .unwrap()
}

fn person(id: &str, position: &str, level: i64) -> MovePerson {
    MovePerson {
        person_id: id.into(),
        engagement_id: format!("engagement-{id}"),
        legal_name: format!("Person {id}"),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        position_id: Some(position.into()),
        manager_person_id: Some("ceo-person".into()),
        level_id: Some(format!("level-{level}")),
        level_number: Some(level),
        status: "active".into(),
        person_status: "active".into(),
        start_date: "2024-01-01".into(),
        end_date: None,
    }
}

fn facts() -> PositionMoveFacts {
    let mut ceo = person("ceo-person", "ceo", 1);
    ceo.manager_person_id = None;
    PositionMoveFacts {
        people: vec![ceo, person("holder", "position-2", 2)],
        positions: vec![
            MovePosition {
                id: "ceo".into(),
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
                title: "Chief".into(),
                level_id: "level-1".into(),
                level_number: 1,
                reports_to_position_id: None,
                is_ceo: 1,
                active: 1,
                version: 1,
            },
            MovePosition {
                id: "position-2".into(),
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
                title: "Director".into(),
                level_id: "level-2".into(),
                level_number: 2,
                reports_to_position_id: Some("ceo".into()),
                is_ceo: 0,
                active: 1,
                version: 3,
            },
        ],
        nodes: vec![
            MoveNode {
                id: "branch".into(),
                kind: "branch".into(),
                branch_id: None,
                status: "active".into(),
                hierarchy_revision: 1,
            },
            MoveNode {
                id: "department".into(),
                kind: "department".into(),
                branch_id: Some("branch".into()),
                status: "active".into(),
                hierarchy_revision: 1,
            },
        ],
        levels: vec![
            MoveLevel {
                id: "level-2".into(),
                level_number: 2,
                active: 1,
                version: 1,
            },
            MoveLevel {
                id: "level-3".into(),
                level_number: 3,
                active: 1,
                version: 1,
            },
        ],
        members: vec![MoveMember {
            id: "engagement-holder".into(),
            person_id: "holder".into(),
            version: 1,
            status: "active".into(),
            start_date: "2024-01-01".into(),
            end_date: None,
            currency: "THB".into(),
        }],
        open_placements: vec![],
        pending: false,
        defaults: vec![],
        leave: vec![],
        rules: vec![],
        date: "2027-01-15".into(),
    }
}

fn preview(has_people_manage: bool) -> PreparedPositionMove {
    prepare_move(
        Some("branch"),
        Some("department"),
        has_people_manage,
        &input(),
        &facts(),
        1_800_003_600_000 + PREVIEW_TTL_MS,
    )
    .unwrap()
}

#[test]
fn preview_keeps_scope_version_and_whole_position_blockers() {
    let prepared = preview(true);
    assert!(prepared.preview.ready, "{:?}", prepared.preview.blockers);
    assert_eq!(prepared.preview.affected_count, 1);
    assert_eq!(prepared.candidate.level_number, 3);
    assert_eq!(
        prepared.preview.people[0].after.level_id.as_deref(),
        Some("level-3")
    );
    assert_eq!(prepared.preview.before.level_id.as_deref(), Some("level-2"));

    let mut stale = input();
    stale.expected_version = 2;
    let prepared = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &stale,
        &facts(),
        PREVIEW_TTL_MS,
    )
    .unwrap();
    assert!(prepared
        .preview
        .blockers
        .contains(&"The position changed. Reload it and preview again.".to_owned()));

    let mut outside = input();
    outside.position_id = "foreign".into();
    assert_eq!(
        prepare_move(None, None, true, &outside, &facts(), PREVIEW_TTL_MS)
            .unwrap_err()
            .code,
        "POSITION_NOT_FOUND"
    );
}

#[test]
fn preview_requires_people_manage_when_holders_exist() {
    let error = prepare_move(
        Some("branch"),
        Some("department"),
        false,
        &input(),
        &facts(),
        PREVIEW_TTL_MS,
    )
    .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(error.kind, FailureKind::Forbidden);
}

#[test]
fn department_target_uses_the_active_branch_and_rejects_foreign_scope() {
    let mut move_input = input();
    move_input.target_kind = PositionMoveTargetKind::Department;
    move_input.target_id = "department".into();
    let prepared = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &move_input,
        &facts(),
        PREVIEW_TTL_MS,
    )
    .unwrap();
    assert_eq!(
        prepared.candidate.department_id.as_deref(),
        Some("department")
    );
    assert_eq!(prepared.candidate.branch_id.as_deref(), Some("branch"));

    let mut foreign = facts();
    foreign.nodes[1].branch_id = Some("other".into());
    let error = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &move_input,
        &foreign,
        PREVIEW_TTL_MS,
    )
    .unwrap_err();
    assert_eq!(error.code, "DEPARTMENT_INVALID");
}

#[test]
fn graph_issues_keep_the_retained_reporting_messages() {
    let data = facts();
    let candidate = data.positions[1].clone();
    let mut circular = data.people[1].clone();
    circular.manager_person_id = Some(circular.person_id.clone());
    let issues = position_move_graph_issues(
        &data.people,
        &data.positions,
        &candidate,
        &[circular],
        "2027-01-15",
    );
    assert!(issues
        .iter()
        .any(|issue| issue.contains("Correct the circular reporting relationship.")));

    let mut ceo = data.positions[0].clone();
    ceo.reports_to_position_id = Some("position-2".into());
    let issues = position_move_graph_issues(&data.people, &data.positions, &ceo, &[], "2027-01-15");
    assert!(issues
        .iter()
        .any(|issue| issue.contains("must remain Level 1")));
}

#[test]
fn leave_impact_matches_the_retained_cases() {
    let policy = LeaveDefault {
        level_id: "level-3".into(),
        policy_version_id: "policy".into(),
        effective_from: "2027-01-01".into(),
        version: 1,
    };
    let unchanged = position_move_leave_impact(
        None,
        Some(&policy),
        Some("level-3"),
        "level-3",
        "2027-01-15",
    );
    assert_eq!(unchanged.leave_effective_from, None);
    assert_eq!(
        unchanged.leave_note,
        "Actual Level is unchanged; existing leave remains."
    );
    let custom = Leave {
        engagement_id: "engagement".into(),
        id: "leave".into(),
        effective_from: "2027-01-01".into(),
        effective_to: None,
        version: 1,
        policy_key: "custom".into(),
    };
    assert_eq!(
        position_move_leave_impact(
            Some(&custom),
            Some(&policy),
            Some("level-2"),
            "level-3",
            "2027-01-15"
        )
        .leave_note,
        "Custom leave policy is preserved."
    );
    let impact = position_move_leave_impact(
        None,
        Some(&policy),
        Some("level-2"),
        "level-3",
        "2027-01-15",
    );
    assert_eq!(impact.leave_effective_from.as_deref(), Some("2028-01-01"));
    assert!(impact.blocker.is_none());
    let overlapping = position_move_leave_impact(
        Some(&Leave {
            effective_from: "2028-01-01".into(),
            policy_key: "level-2-default".into(),
            ..custom
        }),
        Some(&policy),
        Some("level-2"),
        "level-3",
        "2027-01-15",
    );
    assert_eq!(
        overlapping.blocker.as_deref(),
        Some("Review the overlapping future leave assignment.")
    );
}

#[test]
fn preview_token_is_deterministic_and_changes_with_the_snapshot() {
    let first = preview(true);
    let second = preview(true);
    assert_eq!(first.preview.preview_token, second.preview.preview_token);
    assert_eq!(first.snapshot_hash, second.snapshot_hash);
    let mut changed = facts();
    changed.people[1].level_number = Some(4);
    let third = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &input(),
        &changed,
        PREVIEW_TTL_MS,
    )
    .unwrap();
    assert_ne!(first.preview.preview_token, third.preview.preview_token);
    assert_eq!(first.preview.preview_token.len(), 64);
}

fn actor() -> OrganizationActor {
    OrganizationActor {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("owner").unwrap(),
        assignment_id: AssignmentId::new("assignment-owner").unwrap(),
        role: "custom_organization".into(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: Some(RecordId::new("department").unwrap()),
        scope_kind: OrganizationScopeKind::Branch,
        assignment_scope_kind: OrganizationScopeKind::Branch,
        permissions: vec!["organization.manage".into(), "people.manage".into()],
        authority: None,
    }
}

fn command_actor() -> OrganizationCommandActor {
    let actor = actor();
    OrganizationCommandActor {
        holding_id: actor.holding_id.clone(),
        command: OrganizationCommand {
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            role: actor.role.clone(),
            company_id: actor.company_id.clone(),
            branch_id: Some("branch".into()),
            department_id: Some("department".into()),
            authority: None,
            iam_json: json!({
                "principalId": "principal-owner",
                "permissions": ["organization.manage", "people.manage"],
                "grantablePermissions": [],
                "personId": "owner",
                "personUserIds": ["owner"],
                "freshUntil": null
            })
            .to_string(),
            assignment_scope_kind: "branch".into(),
            assignment_scope_node_id: Some("branch".into()),
        },
        modules: vec!["people".into()],
        permissions: vec!["organization.manage".into(), "people.manage".into()],
    }
}

struct Fake {
    facts: PositionMoveFacts,
    replay: Option<Value>,
    scheduled: Option<Value>,
    rows: Vec<PositionMoveRecord>,
    cancelled: Option<(String, i64, i64)>,
}

impl Fake {
    fn new() -> Self {
        Self {
            facts: facts(),
            replay: None,
            scheduled: None,
            rows: Vec::new(),
            cancelled: None,
        }
    }
}

impl PositionMoveRepository for Fake {
    type Error = BusinessError;

    fn authorize<'a>(
        &'a mut self,
        _actor: &'a OrganizationActor,
        _permission: &'a str,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }

    fn local_date<'a>(
        &'a mut self,
        _timestamp: i64,
        _timezone: &'a str,
    ) -> PositionMoveFuture<'a, String, BusinessError> {
        Box::pin(async { Ok("2027-01-15".into()) })
    }

    fn facts<'a>(
        &'a mut self,
        _actor: &'a OrganizationActor,
        _position_id: &'a str,
        _date: &'a str,
        _exclude_move_id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, PositionMoveFacts, BusinessError> {
        Box::pin(async move { Ok(self.facts.clone()) })
    }

    fn replay<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _key: &'a str,
        _operation: &'a str,
        _record_id: &'a str,
        _input: &'a Value,
    ) -> PositionMoveFuture<'a, Option<Value>, BusinessError> {
        Box::pin(async move { Ok(self.replay.clone()) })
    }

    fn schedule<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _input: &'a PositionMoveInput,
        _key: &'a str,
        _payload: &'a Value,
        _prepared: &'a PreparedPositionMove,
        _now: i64,
    ) -> PositionMoveFuture<'a, Value, BusinessError> {
        Box::pin(async move {
            let value =
                json!({"id": "position-move-key-12345678", "version": 1, "status": "scheduled"});
            self.scheduled = Some(value.clone());
            Ok(value)
        })
    }

    fn rows<'a>(
        &'a mut self,
        _actor: &'a OrganizationActor,
        _id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, Vec<PositionMoveRecord>, BusinessError> {
        Box::pin(async move { Ok(self.rows.clone()) })
    }

    fn cancel<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        id: &'a str,
        expected_version: i64,
        _key: &'a str,
        now: i64,
    ) -> PositionMoveFuture<'a, Value, BusinessError> {
        Box::pin(async move {
            self.cancelled = Some((id.to_owned(), expected_version, now));
            Ok(json!({"id": id, "version": expected_version + 1, "status": "cancelled"}))
        })
    }
}

#[test]
fn preview_command_returns_the_reviewed_token() {
    let mut repository = Fake::new();
    let clock = FixedClock(erpx_server_shared::public::TimestampMillis::new(
        1_800_000_000_000,
    ));
    let result = futures_lite_preview(&mut repository, &actor(), &input(), &clock);
    assert!(result.ready, "{:?}", result.blockers);
    assert_eq!(result.preview_token.len(), 64);
}

fn futures_lite_preview(
    repository: &mut Fake,
    actor: &OrganizationActor,
    input: &PositionMoveInput,
    clock: &FixedClock,
) -> PositionMovePreview {
    // The repository futures are immediately ready; block on them with a
    // bounded local executor instead of adding a runtime dependency.
    block(preview_position_move(repository, actor, input, clock)).unwrap()
}

fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    match std::future::Future::poll(future.as_mut(), &mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("future stalled"),
    }
}

#[test]
fn submit_replays_before_expiry_and_rejects_stale_tokens() {
    let mut repository = Fake::new();
    let clock = FixedClock(erpx_server_shared::public::TimestampMillis::new(
        1_800_000_000_000,
    ));
    let command = command_actor();
    let preview = preview(true);
    let expires_at = 1_800_000_000_000 + PREVIEW_TTL_MS;

    repository.replay = Some(json!({"id": "prior", "version": 1, "status": "scheduled"}));
    let replay = block(submit_position_move(
        &mut repository,
        &command,
        &input(),
        &preview.preview.preview_token,
        expires_at,
        "key-replay-1234",
        &clock,
    ))
    .unwrap();
    assert_eq!(replay["id"], "prior");
    assert!(repository.scheduled.is_none());

    repository.replay = None;
    let stale = block(submit_position_move(
        &mut repository,
        &command,
        &input(),
        "0".repeat(64).as_str(),
        expires_at,
        "key-stale-1234",
        &clock,
    ))
    .unwrap_err();
    assert_eq!(stale.code, "POSITION_MOVE_CHANGED");

    let expired = block(submit_position_move(
        &mut repository,
        &command,
        &input(),
        &preview.preview.preview_token,
        1_800_000_000_000 - 1,
        "key-expired-1234",
        &clock,
    ))
    .unwrap_err();
    assert_eq!(expired.code, "POSITION_MOVE_PREVIEW_EXPIRED");
}

#[test]
fn cancel_locks_applied_or_effective_moves() {
    let mut repository = Fake::new();
    let clock = FixedClock(erpx_server_shared::public::TimestampMillis::new(
        1_800_000_000_000,
    ));
    let command = command_actor();
    let preview = preview(true);
    let record = PositionMoveRecord {
        id: "position-move-key".into(),
        company_id: "company".into(),
        position_id: "position-2".into(),
        status: "scheduled".into(),
        version: 1,
        effective_at: 1_800_000_000_000 + 60_000,
        definition: input(),
        preview: preview.preview.clone(),
        created_by: "owner".into(),
        created_at: 1_800_000_000_000,
        updated_at: 1_800_000_000_000,
        applied_at: None,
        last_error: None,
    };
    repository.rows = vec![record.clone()];
    let result = block(cancel_position_move(
        &mut repository,
        &command,
        "position-move-key",
        1,
        "key-cancel-1234",
        &clock,
    ))
    .unwrap();
    assert_eq!(result["status"], "cancelled");
    assert_eq!(
        repository.cancelled,
        Some(("position-move-key".into(), 1, 1_800_000_000_000))
    );

    repository.cancelled = None;
    repository.rows = vec![PositionMoveRecord {
        effective_at: 1_800_000_000_000,
        ..record
    }];
    let locked = block(cancel_position_move(
        &mut repository,
        &command,
        "position-move-key",
        1,
        "key-cancel-5678",
        &clock,
    ))
    .unwrap_err();
    assert_eq!(locked.code, "POSITION_MOVE_LOCKED");
}

#[test]
fn rows_hide_holder_details_without_people_manage() {
    let mut repository = Fake::new();
    let clock = FixedClock(erpx_server_shared::public::TimestampMillis::new(
        1_800_000_000_000,
    ));
    let preview = preview(true);
    repository.rows = vec![PositionMoveRecord {
        id: "position-move-key".into(),
        company_id: "company".into(),
        position_id: "position-2".into(),
        status: "scheduled".into(),
        version: 1,
        effective_at: 1_800_000_000_000 + 60_000,
        definition: input(),
        preview: preview.preview.clone(),
        created_by: "owner".into(),
        created_at: 1_800_000_000_000,
        updated_at: 1_800_000_000_000,
        applied_at: None,
        last_error: None,
    }];
    let mut reader = actor();
    reader.permissions = vec!["organization.manage".into()];
    let rows = block(position_moves(&mut repository, &reader, None)).unwrap();
    assert!(rows.is_empty());
    let _ = clock;
}
