use erpx_server_organization::public::organization_nodes::{
    OrganizationActor, OrganizationCommandActor, OrganizationScopeKind,
};
use erpx_server_organization::public::position_moves::*;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde_json::json;

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
        nodes: Vec::new(),
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

fn input() -> PositionMoveInput {
    PositionMoveInput::parse(&json!({
        "positionId": "position-2",
        "expectedVersion": 3,
        "targetKind": "level",
        "targetId": "level-3",
        "effectiveAt": 1_800_003_600_000_i64,
        "reason": "Promote the reviewed position"
    }))
    .unwrap()
}

fn claimed(facts: &PositionMoveFacts, snapshot_hash: Option<String>) -> ClaimedPositionMove {
    let prepared = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &input(),
        facts,
        1_800_003_600_000 + PREVIEW_TTL_MS,
    )
    .unwrap();
    ClaimedPositionMove {
        record: PositionMoveRecord {
            id: "position-move-apply-0001".into(),
            company_id: "company".into(),
            position_id: "position-2".into(),
            status: "scheduled".into(),
            version: 1,
            effective_at: 1_800_003_600_000,
            definition: input(),
            preview: prepared.preview,
            created_by: "owner".into(),
            created_at: 1_800_000_000_000,
            updated_at: 1_800_000_000_000,
            applied_at: None,
            last_error: None,
        },
        actor: actor(),
        authorizing_assignment_id: "assignment-owner".into(),
        created_by: "owner".into(),
        snapshot_hash: snapshot_hash.unwrap_or(prepared.snapshot_hash),
    }
}

struct FakeDrain {
    claimed: Option<ClaimedPositionMove>,
    facts: PositionMoveFacts,
    valid: bool,
    applied_position: u32,
    applied_holders: Vec<String>,
    completed: bool,
    staged: Vec<LeaveAssignmentPlan>,
    policy: Option<LeavePolicyFact>,
    existing: Option<ExistingLeaveFact>,
}

impl FakeDrain {
    fn new(claimed: Option<ClaimedPositionMove>, policy: Option<LeavePolicyFact>) -> Self {
        Self {
            claimed,
            facts: facts(),
            valid: true,
            applied_position: 0,
            applied_holders: Vec::new(),
            completed: false,
            staged: Vec::new(),
            policy,
            existing: None,
        }
    }
}

impl PositionMoveRepository for FakeDrain {
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
        _input: &'a serde_json::Value,
    ) -> PositionMoveFuture<'a, Option<serde_json::Value>, BusinessError> {
        Box::pin(async { Ok(None) })
    }

    fn schedule<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _input: &'a PositionMoveInput,
        _key: &'a str,
        _payload: &'a serde_json::Value,
        _prepared: &'a PreparedPositionMove,
        _now: i64,
    ) -> PositionMoveFuture<'a, serde_json::Value, BusinessError> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "UNUSED",
                "unused",
            ))
        })
    }

    fn rows<'a>(
        &'a mut self,
        _actor: &'a OrganizationActor,
        _id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, Vec<PositionMoveRecord>, BusinessError> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn cancel<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _id: &'a str,
        _expected_version: i64,
        _key: &'a str,
        _now: i64,
    ) -> PositionMoveFuture<'a, serde_json::Value, BusinessError> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "UNUSED",
                "unused",
            ))
        })
    }
}

impl PositionMoveDrainRepository for FakeDrain {
    fn claim<'a>(
        &'a mut self,
        _now: i64,
    ) -> PositionMoveFuture<'a, Option<ClaimedPositionMove>, BusinessError> {
        Box::pin(async move { Ok(self.claimed.clone()) })
    }

    fn assignment_valid<'a>(
        &'a mut self,
        _claimed: &'a ClaimedPositionMove,
        _affected_count: usize,
        _now: i64,
    ) -> PositionMoveFuture<'a, bool, BusinessError> {
        Box::pin(async move { Ok(self.valid) })
    }

    fn set_payroll_date<'a>(
        &'a mut self,
        _date: &'a str,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }

    fn apply_position<'a>(
        &'a mut self,
        _company_id: &'a str,
        _candidate: &'a MovePosition,
        _now: i64,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.applied_position += 1;
            Ok(())
        })
    }

    fn apply_holder<'a>(
        &'a mut self,
        _claimed: &'a ClaimedPositionMove,
        _input: &'a PositionMoveInput,
        holder: &'a PositionMovePersonWire,
        _original: &'a MoveMember,
        _now: i64,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.applied_holders.push(holder.engagement_id.clone());
            Ok(())
        })
    }

    fn leave_policy<'a>(
        &'a mut self,
        _company_id: &'a str,
        _level_id: &'a str,
    ) -> PositionMoveFuture<'a, Option<LeavePolicyFact>, BusinessError> {
        Box::pin(async move { Ok(self.policy.clone()) })
    }

    fn existing_leave<'a>(
        &'a mut self,
        _company_id: &'a str,
        _engagement_id: &'a str,
        _date: &'a str,
    ) -> PositionMoveFuture<'a, Option<ExistingLeaveFact>, BusinessError> {
        Box::pin(async move { Ok(self.existing.clone()) })
    }

    fn stage_leave<'a>(
        &'a mut self,
        _actor: &'a OrganizationActor,
        _command_id: &'a str,
        _engagement_id: &'a str,
        plan: &'a LeaveAssignmentPlan,
        _now: i64,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.staged.push(plan.clone());
            Ok(())
        })
    }

    fn complete<'a>(
        &'a mut self,
        _claimed: &'a ClaimedPositionMove,
        _input: &'a PositionMoveInput,
        _affected_count: usize,
        _now: i64,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.completed = true;
            Ok(())
        })
    }

    fn needs_review<'a>(
        &'a mut self,
        _id: &'a str,
        _code: &'a str,
        _now: i64,
    ) -> PositionMoveFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
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
fn empty_claim_does_nothing() {
    let mut repository = FakeDrain::new(None, None);
    match block(drain_once(&mut repository, 1_800_003_600_001)).unwrap() {
        DrainOutcome::Empty => {}
        DrainOutcome::Applied { .. } => panic!("unexpected apply"),
    }
    assert_eq!(repository.applied_position, 0);
    assert!(!repository.completed);
}

#[test]
fn changed_authority_fails_closed_before_effects() {
    let facts = facts();
    let mut repository = FakeDrain::new(Some(claimed(&facts, None)), None);
    repository.valid = false;
    let error = block(drain_once(&mut repository, 1_800_003_600_001)).unwrap_err();
    match error {
        DrainFailure::Business { move_id, error } => {
            assert_eq!(move_id.as_deref(), Some("position-move-apply-0001"));
            assert_eq!(error.code, "POSITION_MOVE_AUTHORITY_CHANGED");
            assert_eq!(error.kind, FailureKind::Conflict);
        }
        DrainFailure::Repository { .. } => panic!("unexpected repository failure"),
    }
    assert_eq!(repository.applied_position, 0);
    assert!(repository.applied_holders.is_empty());
    assert!(!repository.completed);
}

#[test]
fn changed_snapshot_fails_closed_with_review_changed() {
    let facts = facts();
    let mut repository = FakeDrain::new(Some(claimed(&facts, Some("a".repeat(64)))), None);
    let error = block(drain_once(&mut repository, 1_800_003_600_001)).unwrap_err();
    match error {
        DrainFailure::Business { move_id, error } => {
            assert_eq!(move_id.as_deref(), Some("position-move-apply-0001"));
            assert_eq!(error.code, "POSITION_MOVE_REVIEW_CHANGED");
        }
        DrainFailure::Repository { .. } => panic!("unexpected repository failure"),
    }
    assert_eq!(repository.applied_position, 0);
    assert!(!repository.completed);
}

#[test]
fn applies_position_holders_leave_and_completion_in_order() {
    let facts = facts();
    let mut repository = FakeDrain::new(
        Some(claimed(&facts, None)),
        Some(LeavePolicyFact {
            policy_version_id: "policy-level-3".into(),
            effective_from: "2020-01-01".into(),
        }),
    );
    let outcome = block(drain_once(&mut repository, 1_800_003_600_001)).unwrap();
    match outcome {
        DrainOutcome::Applied { move_id } => {
            assert_eq!(move_id, "position-move-apply-0001");
        }
        DrainOutcome::Empty => panic!("expected apply"),
    }
    assert_eq!(repository.applied_position, 1);
    assert_eq!(repository.applied_holders, vec!["engagement-holder"]);
    assert_eq!(repository.staged.len(), 1);
    assert_eq!(repository.staged[0].effective, "2028-01-01");
    assert_eq!(repository.staged[0].end, "2027-12-31");
    assert!(repository.completed);
}

#[test]
fn unchanged_level_skips_leave_recommendation() {
    let mut facts = facts();
    facts.levels = vec![MoveLevel {
        id: "level-2".into(),
        level_number: 2,
        active: 1,
        version: 1,
    }];
    let mut move_input = input();
    move_input.target_id = "level-2".into();
    let prepared = prepare_move(
        Some("branch"),
        Some("department"),
        true,
        &move_input,
        &facts,
        1_800_003_600_000 + PREVIEW_TTL_MS,
    )
    .unwrap();
    let mut repository = FakeDrain::new(
        Some(ClaimedPositionMove {
            record: PositionMoveRecord {
                id: "position-move-apply-0002".into(),
                company_id: "company".into(),
                position_id: "position-2".into(),
                status: "scheduled".into(),
                version: 1,
                effective_at: 1_800_003_600_000,
                definition: move_input,
                preview: prepared.preview,
                created_by: "owner".into(),
                created_at: 1_800_000_000_000,
                updated_at: 1_800_000_000_000,
                applied_at: None,
                last_error: None,
            },
            actor: actor(),
            authorizing_assignment_id: "assignment-owner".into(),
            created_by: "owner".into(),
            snapshot_hash: prepared.snapshot_hash,
        }),
        None,
    );
    repository.facts = facts;
    block(drain_once(&mut repository, 1_800_003_600_001)).unwrap();
    assert_eq!(repository.applied_position, 1);
    assert!(repository.staged.is_empty());
    assert!(repository.completed);
}

#[test]
fn leave_default_recommendation_matches_the_retained_rules() {
    let policy = LeavePolicyFact {
        policy_version_id: "policy".into(),
        effective_from: "2027-06-01".into(),
    };
    assert_eq!(
        leave_default_recommendation(Some("level-2"), "level-3", "2027-01-15", None, None).unwrap(),
        None
    );
    let custom = ExistingLeaveFact {
        id: "leave".into(),
        effective_from: "2027-01-01".into(),
        effective_to: None,
        policy_key: "custom".into(),
    };
    assert_eq!(
        leave_default_recommendation(
            Some("level-2"),
            "level-3",
            "2027-01-15",
            Some(&policy),
            Some(&custom)
        )
        .unwrap(),
        None
    );
    let managed = ExistingLeaveFact {
        policy_key: "level-2-default".into(),
        ..custom.clone()
    };
    assert_eq!(
        leave_default_recommendation(
            Some("level-3"),
            "level-3",
            "2027-01-15",
            Some(&policy),
            Some(&managed)
        )
        .unwrap(),
        None
    );
    let plan = leave_default_recommendation(
        Some("level-2"),
        "level-3",
        "2027-01-15",
        Some(&policy),
        Some(&managed),
    )
    .unwrap()
    .unwrap();
    assert_eq!(plan.effective, "2028-01-01");
    assert_eq!(plan.end, "2027-12-31");
    assert_eq!(plan.end_assignment_id.as_deref(), Some("leave"));

    let overlap = ExistingLeaveFact {
        effective_from: "2028-06-01".into(),
        ..managed
    };
    assert_eq!(
        leave_default_recommendation(
            Some("level-2"),
            "level-3",
            "2027-01-15",
            Some(&policy),
            Some(&overlap)
        )
        .unwrap_err()
        .code,
        "LEAVE_ASSIGNMENT_OVERLAP"
    );

    let plan = leave_default_recommendation(
        None,
        "level-3",
        "2027-01-15",
        Some(&LeavePolicyFact {
            effective_from: "2027-06-01".into(),
            ..policy
        }),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(plan.effective, "2027-06-01");
    assert_eq!(plan.end, "2027-05-31");
}
