use erpx_server_organization::public::organization_nodes::{
    OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
};
use erpx_server_organization::public::position::*;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn actor(
    permissions: &[&str],
    branch: Option<&str>,
    department: Option<&str>,
) -> OrganizationCommandActor {
    OrganizationCommandActor {
        holding_id: HoldingId::new("h001").unwrap(),
        command: OrganizationCommand {
            user_id: UserId::new("owner").unwrap(),
            assignment_id: AssignmentId::new("assignment-owner").unwrap(),
            role: "custom_organization".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: branch.map(str::to_owned),
            department_id: department.map(str::to_owned),
            authority: Some(OrganizationAuthority {
                session_id: "session-owner".into(),
                revision: 7,
                assignment_version: 1,
            }),
            iam_json: json!({
                "principalId": "principal-owner",
                "permissions": permissions,
                "grantablePermissions": [],
                "personId": "person-owner",
                "personUserIds": ["owner"],
                "freshUntil": null
            })
            .to_string(),
            assignment_scope_kind: if department.is_some() {
                "department".into()
            } else if branch.is_some() {
                "branch".into()
            } else {
                "holding".into()
            },
            assignment_scope_node_id: department.or(branch).map(str::to_owned),
        },
        modules: vec!["people".into()],
        permissions: permissions.iter().map(|value| (*value).into()).collect(),
    }
}

fn owner() -> OrganizationCommandActor {
    actor(&["organization.manage"], None, None)
}

fn base_input() -> Value {
    json!({
        "requirements": "",
        "title": "Operations Manager",
        "branchId": null,
        "departmentId": null,
        "levelId": "level-3",
        "reportsToPositionId": "position-ceo",
        "isCeo": false,
        "active": true,
        "expectedVersion": 0,
        "reason": "Create the operations manager"
    })
}

fn input() -> PositionInput {
    PositionInput::parse(&base_input()).unwrap()
}

fn position(
    id: &str,
    level_number: i64,
    reports_to: Option<&str>,
    is_ceo: i64,
    active: i64,
    roster_required: i64,
) -> PositionRosterPosition {
    PositionRosterPosition {
        id: id.into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        title: id.into(),
        level_id: format!("level-{level_number}"),
        level_number,
        reports_to_position_id: reports_to.map(str::to_owned),
        requirements: Some(String::new()),
        is_ceo,
        roster_required,
        active,
        version: 1,
    }
}

fn person(id: &str) -> PositionRosterPerson {
    PositionRosterPerson {
        person_id: id.into(),
        user_id: Some(format!("user-{id}")),
        legal_name: id.into(),
        email: None,
        engagement_id: format!("engagement-{id}"),
        engagement_version: 1,
        person_version: 1,
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        position_id: None,
        manager_person_id: None,
        level_id: None,
        level_number: None,
        level_name: None,
        status: "active".into(),
        person_status: "active".into(),
        start_date: "2020-01-01".into(),
        end_date: None,
        job_title: "Staff".into(),
        issues: Vec::new(),
    }
}

#[derive(Clone, Debug, Default)]
struct Persisted {
    id: Option<String>,
    title: String,
    guard: String,
    roster_required: bool,
    exiting: Vec<String>,
    version: i64,
    now: i64,
}

struct FakeRepository {
    replay: Option<Value>,
    roster: PositionRoster,
    branch_active: bool,
    department_active: bool,
    level: Option<i64>,
    guard: PositionGraphGuard,
    exiting: Vec<String>,
    calls: Vec<String>,
    persisted: Option<Persisted>,
}

impl FakeRepository {
    fn new() -> Self {
        Self {
            replay: None,
            roster: PositionRoster {
                people: Vec::new(),
                positions: vec![position("position-ceo", 1, None, 1, 1, 0)],
            },
            branch_active: true,
            department_active: true,
            level: Some(3),
            guard: PositionGraphGuard {
                snapshot: "organization-snapshot".into(),
            },
            exiting: Vec::new(),
            calls: Vec::new(),
            persisted: None,
        }
    }
}

impl OrganizationPositionRepository for FakeRepository {
    type Error = BusinessError;

    fn replay<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _key: &'a str,
        _operation: &'a str,
        _record_id: &'a str,
        _input: &'a PositionInput,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<Value>>
    {
        self.calls.push("replay".into());
        let prior = self.replay.clone();
        Box::pin(async move { Ok(prior) })
    }

    fn roster<'a>(
        &'a mut self,
        _company: &'a CompanyId,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, PositionRoster>
    {
        self.calls.push("roster".into());
        let roster = self.roster.clone();
        Box::pin(async move { Ok(roster) })
    }

    fn branch_active<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        self.calls.push("branch_active".into());
        Box::pin(async move { Ok(self.branch_active) })
    }

    fn department_active<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _department: &'a str,
        _branch: Option<&'a str>,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        self.calls.push("department_active".into());
        Box::pin(async move { Ok(self.department_active) })
    }

    fn position_level<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _level: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<i64>>
    {
        self.calls.push("position_level".into());
        let level = self.level;
        Box::pin(async move { Ok(level) })
    }

    fn graph_guard<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _key: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<
        'a,
        PositionGraphGuard,
    > {
        self.calls.push("graph_guard".into());
        let guard = self.guard.clone();
        Box::pin(async move { Ok(guard) })
    }

    fn exiting_engagements<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _position: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Vec<String>>
    {
        self.calls.push("exiting_engagements".into());
        let exiting = self.exiting.clone();
        Box::pin(async move { Ok(exiting) })
    }

    fn persist<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        id: Option<&'a str>,
        input: &'a PositionInput,
        key: &'a str,
        guard: &'a PositionGraphGuard,
        roster_required: bool,
        exiting: &'a [String],
        now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Value> {
        self.calls.push("persist".into());
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| format!("position-{key}"));
        let version = input.expected_version + 1;
        self.persisted = Some(Persisted {
            id: Some(id.clone()),
            title: input.title.clone(),
            guard: guard.snapshot.clone(),
            roster_required,
            exiting: exiting.to_vec(),
            version,
            now,
        });
        Box::pin(async move { Ok(json!({"id": id, "version": version})) })
    }
}

#[derive(Default)]
struct FakeRosterExit {
    calls: Vec<(Vec<String>, String, String, i64)>,
}

impl RosterExitPort for FakeRosterExit {
    fn execute<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        engagement_ids: &'a [String],
        key: &'a str,
        reason: &'a str,
        now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, ()> {
        self.calls.push((
            engagement_ids.to_vec(),
            key.to_owned(),
            reason.to_owned(),
            now,
        ));
        Box::pin(async move { Ok(()) })
    }
}

struct FakeUnit<'a> {
    repository: &'a mut FakeRepository,
    roster_exit: &'a mut FakeRosterExit,
}

impl OrganizationPositionRepository for FakeUnit<'_> {
    type Error = BusinessError;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a PositionInput,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<Value>>
    {
        self.repository.replay(actor, key, operation, record_id, input)
    }
    fn roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, PositionRoster>
    {
        self.repository.roster(company)
    }
    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        self.repository.branch_active(company, branch)
    }
    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        self.repository.department_active(company, department, branch)
    }
    fn position_level<'a>(
        &'a mut self,
        company: &'a CompanyId,
        level: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<i64>>
    {
        self.repository.position_level(company, level)
    }
    fn graph_guard<'a>(
        &'a mut self,
        company: &'a CompanyId,
        key: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<
        'a,
        PositionGraphGuard,
    > {
        self.repository.graph_guard(company, key)
    }
    fn exiting_engagements<'a>(
        &'a mut self,
        company: &'a CompanyId,
        position: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Vec<String>>
    {
        self.repository.exiting_engagements(company, position)
    }
    fn persist<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        id: Option<&'a str>,
        input: &'a PositionInput,
        key: &'a str,
        guard: &'a PositionGraphGuard,
        roster_required: bool,
        exiting: &'a [String],
        now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Value> {
        self.repository
            .persist(actor, id, input, key, guard, roster_required, exiting, now)
    }
}

impl RosterExitPort for FakeUnit<'_> {
    fn execute<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        engagement_ids: &'a [String],
        key: &'a str,
        reason: &'a str,
        now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, ()> {
        self.roster_exit.execute(actor, engagement_ids, key, reason, now)
    }
}

async fn save(
    repository: &mut FakeRepository,
    roster_exit: &mut FakeRosterExit,
    actor: OrganizationCommandActor,
    id: Option<&str>,
    input: PositionInput,
) -> Result<Value, BusinessError> {
    let mut unit = FakeUnit {
        repository,
        roster_exit,
    };
    save_position(
        &mut unit,
        actor,
        "command-key-0001",
        id,
        input,
        &TestClock,
    )
    .await
}

fn code(result: Result<Value, BusinessError>) -> &'static str {
    result.expect_err("expected a business denial").code
}

#[test]
fn parse_applies_retained_defaults_and_trimming() {
    let mut value = base_input();
    value["requirements"] = json!("  Keep the site running  ");
    value["title"] = json!("  Operations Manager  ");
    value["reason"] = json!("  Create the operations manager  ");
    value.as_object_mut().unwrap().remove("isCeo");
    value.as_object_mut().unwrap().remove("active");
    let parsed = PositionInput::parse(&value).unwrap();
    assert_eq!(parsed.requirements, "Keep the site running");
    assert_eq!(parsed.title, "Operations Manager");
    assert_eq!(parsed.reason, "Create the operations manager");
    assert!(!parsed.is_ceo);
    assert!(parsed.active);
    assert_eq!(parsed.roster_required, None);
    assert_eq!(parsed.expected_version, 0);
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert_eq!(
        serialized,
        r#"{"requirements":"Keep the site running","title":"Operations Manager","branchId":null,"departmentId":null,"levelId":"level-3","reportsToPositionId":"position-ceo","isCeo":false,"active":true,"expectedVersion":0,"reason":"Create the operations manager"}"#
    );
    let keys = parsed
        .to_value()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            "active",
            "branchId",
            "departmentId",
            "expectedVersion",
            "isCeo",
            "levelId",
            "reason",
            "reportsToPositionId",
            "requirements",
            "title"
        ]
    );
    let with_roster = PositionInput::parse(&json!({
        "title": "Operations Manager",
        "branchId": null,
        "departmentId": null,
        "levelId": "level-3",
        "reportsToPositionId": "position-ceo",
        "rosterRequired": true,
        "expectedVersion": 2,
        "reason": "Require a roster"
    }))
    .unwrap();
    assert_eq!(with_roster.roster_required, Some(true));
    assert_eq!(with_roster.to_value()["rosterRequired"], json!(true));
}

#[test]
fn parse_rejects_every_out_of_bound_or_incomplete_field() {
    let cases = [
        ("title", json!(" A ")),
        ("title", json!("x".repeat(161))),
        ("requirements", json!("x".repeat(3001))),
        ("levelId", json!("")),
        ("levelId", json!("x".repeat(201))),
        ("branchId", json!("x".repeat(201))),
        ("departmentId", json!("x".repeat(201))),
        ("reportsToPositionId", json!("x".repeat(201))),
        ("expectedVersion", json!(-1)),
        ("expectedVersion", json!(1.5)),
        ("reason", json!("ab")),
        ("reason", json!("x".repeat(1001))),
        ("rosterRequired", Value::Null),
        ("requirements", Value::Null),
        ("isCeo", Value::Null),
        ("active", Value::Null),
        ("isCeo", json!("yes")),
        ("active", json!("yes")),
    ];
    for (field, value) in cases {
        let mut payload = base_input();
        payload[field] = value;
        assert_eq!(
            PositionInput::parse(&payload).unwrap_err().code,
            "VALIDATION_FAILED",
            "{field}"
        );
    }
    for field in [
        "title",
        "branchId",
        "departmentId",
        "levelId",
        "reportsToPositionId",
        "expectedVersion",
        "reason",
    ] {
        let mut payload = base_input();
        payload.as_object_mut().unwrap().remove(field);
        assert_eq!(
            PositionInput::parse(&payload).unwrap_err().code,
            "VALIDATION_FAILED",
            "missing {field}"
        );
    }
    assert_eq!(
        PositionInput::parse(&json!([])).unwrap_err().code,
        "VALIDATION_FAILED"
    );
}

#[tokio::test]
async fn permission_is_checked_before_replay() {
    let mut repository = FakeRepository::new();
    repository.replay = Some(json!({"id": "prior", "version": 4}));
    let mut exit = FakeRosterExit::default();
    let result = save(
        &mut repository,
        &mut exit,
        actor(&["people.manage"], None, None),
        None,
        input(),
    )
    .await;
    assert_eq!(code(result), "PERMISSION_REQUIRED");
    assert!(repository.calls.is_empty());
}

#[tokio::test]
async fn replay_short_circuits_before_every_source_read() {
    let mut repository = FakeRepository::new();
    repository.replay = Some(json!({"id": "position-prior", "version": 4}));
    let mut exit = FakeRosterExit::default();
    let result = save(&mut repository, &mut exit, owner(), None, input())
        .await
        .unwrap();
    assert_eq!(result, json!({"id": "position-prior", "version": 4}));
    assert_eq!(repository.calls, vec!["replay"]);
    assert!(repository.persisted.is_none());
}

#[tokio::test]
async fn an_unknown_position_id_is_not_found() {
    let mut repository = FakeRepository::new();
    let mut exit = FakeRosterExit::default();
    let result = save(
        &mut repository,
        &mut exit,
        owner(),
        Some("missing"),
        input(),
    )
    .await;
    assert_eq!(code(result), "POSITION_NOT_FOUND");
}

#[tokio::test]
async fn current_scope_is_checked_before_the_input_placement() {
    let mut repository = FakeRepository::new();
    let mut current = position("position-current", 3, Some("position-ceo"), 0, 1, 0);
    current.branch_id = Some("branch-b".into());
    repository.roster.positions.push(current);
    let mut exit = FakeRosterExit::default();
    let result = save(
        &mut repository,
        &mut exit,
        actor(&["organization.manage"], Some("branch-a"), None),
        Some("position-current"),
        input(),
    )
    .await;
    assert_eq!(code(result), "SCOPE_FORBIDDEN");
    assert!(!repository.calls.contains(&"branch_active".to_owned()));
}

#[tokio::test]
async fn placement_branch_and_department_guards_keep_their_codes() {
    let mut repository = FakeRepository::new();
    repository.branch_active = false;
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.branch_id = Some("missing-branch".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "BRANCH_INVALID");

    let mut repository = FakeRepository::new();
    repository.department_active = false;
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.branch_id = Some("branch".into());
    payload.department_id = Some("missing-department".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "DEPARTMENT_INVALID");
}

#[tokio::test]
async fn level_guard_precedes_the_ceo_and_reporting_guards() {
    let mut repository = FakeRepository::new();
    repository.level = None;
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.is_ceo = true;
    payload.reports_to_position_id = None;
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "LEVEL_REQUIRED");
}

#[tokio::test]
async fn ceo_and_reporting_guards_match_the_retained_order() {
    for level in [2, 3] {
        let mut repository = FakeRepository::new();
        repository.level = Some(level);
        let mut exit = FakeRosterExit::default();
        let mut payload = input();
        payload.is_ceo = true;
        let result = save(&mut repository, &mut exit, owner(), None, payload).await;
        assert_eq!(code(result), "CEO_POSITION_INVALID");
    }

    let mut repository = FakeRepository::new();
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.reports_to_position_id = None;
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "REPORTING_POSITION_REQUIRED");
}

#[tokio::test]
async fn reporting_chain_detects_cycles_before_invalid_or_less_senior_managers() {
    let mut repository = FakeRepository::new();
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.reports_to_position_id = Some("position-command-key-0001".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "REPORTING_CYCLE");

    let mut repository = FakeRepository::new();
    repository.roster.positions = vec![
        position("position-ceo", 1, None, 1, 1, 0),
        position(
            "position-parent",
            1,
            Some("position-command-key-0001"),
            0,
            1,
            0,
        ),
    ];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.reports_to_position_id = Some("position-parent".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "REPORTING_CYCLE");
}

#[tokio::test]
async fn inactive_and_less_senior_reporting_positions_are_rejected() {
    let mut repository = FakeRepository::new();
    repository.roster.positions = vec![position("position-inactive", 1, None, 0, 0, 0)];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.reports_to_position_id = Some("position-inactive".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "REPORTING_POSITION_INVALID");

    let mut repository = FakeRepository::new();
    repository.roster.positions = vec![position("position-equal", 3, None, 0, 1, 0)];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.reports_to_position_id = Some("position-equal".into());
    let result = save(&mut repository, &mut exit, owner(), None, payload).await;
    assert_eq!(code(result), "MANAGER_LEVEL_INVALID");
}

#[tokio::test]
async fn archive_in_use_checks_holders_then_reporting_positions() {
    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        0,
    ));
    let mut holder = person("holder");
    holder.position_id = Some("position-current".into());
    holder.branch_id = None;
    repository.roster.people.push(holder);
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.active = false;
    let result = save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await;
    assert_eq!(code(result), "POSITION_IN_USE");

    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        0,
    ));
    repository.roster.positions.push(position(
        "position-report",
        5,
        Some("position-current"),
        0,
        1,
        0,
    ));
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.active = false;
    let result = save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await;
    assert_eq!(code(result), "POSITION_IN_USE");
}

#[tokio::test]
async fn reporting_positions_must_remain_less_senior() {
    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        0,
    ));
    repository.roster.positions.push(position(
        "position-report",
        3,
        Some("position-current"),
        0,
        1,
        0,
    ));
    let mut exit = FakeRosterExit::default();
    let result = save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        input(),
    )
    .await;
    assert_eq!(code(result), "POSITION_LEVEL_INVALID");
}

#[tokio::test]
async fn active_holders_reject_newly_added_reporting_issues() {
    let mut repository = FakeRepository::new();
    let mut current = position("position-current", 3, Some("position-ceo"), 0, 1, 0);
    current.branch_id = Some("branch-a".into());
    repository.roster.positions.push(current);
    let mut holder = person("holder");
    holder.position_id = Some("position-current".into());
    holder.branch_id = Some("branch-a".into());
    repository.roster.people.push(holder);
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.branch_id = Some("branch-b".into());
    let error = save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "POSITION_IN_USE");
    assert_eq!(error.message, "Position does not match staff placement.");
}

#[tokio::test]
async fn create_persists_defaults_and_the_graph_guard() {
    let mut repository = FakeRepository::new();
    let mut exit = FakeRosterExit::default();
    let result = save(&mut repository, &mut exit, owner(), None, input())
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({"id": "position-command-key-0001", "version": 1})
    );
    let persisted = repository.persisted.unwrap();
    assert_eq!(persisted.id.as_deref(), Some("position-command-key-0001"));
    assert_eq!(persisted.title, "Operations Manager");
    assert_eq!(persisted.guard, "organization-snapshot");
    assert!(!persisted.roster_required);
    assert!(persisted.exiting.is_empty());
    assert_eq!(persisted.version, 1);
    assert_eq!(persisted.now, 1_800_000_000_000);
    assert!(exit.calls.is_empty());
    assert_eq!(
        repository.calls,
        vec![
            "replay",
            "graph_guard",
            "roster",
            "position_level",
            "persist"
        ]
    );
}

#[tokio::test]
async fn roster_required_defaults_from_current_and_exits_only_on_transition() {
    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        1,
    ));
    repository.exiting = vec!["engagement-1".into(), "engagement-2".into()];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.expected_version = 1;
    payload.roster_required = None;
    save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await
    .unwrap();
    assert!(repository.persisted.as_ref().unwrap().roster_required);
    assert!(exit.calls.is_empty());

    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        1,
    ));
    repository.exiting = vec!["engagement-1".into(), "engagement-2".into()];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.expected_version = 1;
    payload.roster_required = Some(false);
    save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await
    .unwrap();
    assert!(!repository.persisted.as_ref().unwrap().roster_required);
    assert_eq!(
        exit.calls,
        vec![(
            vec!["engagement-1".to_owned(), "engagement-2".to_owned()],
            "command-key-0001".to_owned(),
            "Create the operations manager".to_owned(),
            1_800_000_000_000
        )]
    );
    assert!(repository.calls.contains(&"exiting_engagements".to_owned()));
    assert_eq!(repository.calls.last().map(String::as_str), Some("persist"));
}

#[tokio::test]
async fn roster_exit_is_not_called_without_the_required_transition() {
    let mut repository = FakeRepository::new();
    repository.roster.positions.push(position(
        "position-current",
        3,
        Some("position-ceo"),
        0,
        1,
        0,
    ));
    repository.exiting = vec!["engagement-1".into()];
    let mut exit = FakeRosterExit::default();
    let mut payload = input();
    payload.expected_version = 1;
    payload.roster_required = Some(true);
    save(
        &mut repository,
        &mut exit,
        owner(),
        Some("position-current"),
        payload,
    )
    .await
    .unwrap();
    assert!(exit.calls.is_empty());
    assert!(!repository.calls.contains(&"exiting_engagements".to_owned()));
}

#[test]
fn reporting_issues_keep_the_retained_messages_and_order() {
    let active_position = position("position-current", 3, None, 0, 1, 0);
    let mut holder = person("holder");
    holder.position_id = Some("position-current".into());
    holder.level_number = Some(3);

    let mut no_level = holder.clone();
    no_level.level_number = None;
    assert_eq!(
        position_reporting_issues(
            &no_level,
            &[no_level.clone()],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        ),
        vec![
            "Assign one of the six staff levels.",
            "Assign a reporting manager."
        ]
    );

    let mut no_position = holder.clone();
    no_position.position_id = Some("missing".into());
    assert_eq!(
        position_reporting_issues(
            &no_position,
            &[no_position.clone()],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        ),
        vec!["Assign an active position.", "Assign a reporting manager."]
    );

    let mut mismatched = holder.clone();
    mismatched.branch_id = Some("branch-a".into());
    mismatched.level_number = Some(3);
    let mut mismatched_position = active_position.clone();
    mismatched_position.branch_id = Some("branch-b".into());
    assert_eq!(
        position_reporting_issues(
            &mismatched,
            &[mismatched.clone()],
            std::slice::from_ref(&mismatched_position),
            "2027-01-15"
        )[0],
        "Position does not match staff placement."
    );

    let ceo_position = position("position-ceo", 1, None, 1, 1, 0);
    let mut ceo_holder = holder.clone();
    ceo_holder.position_id = Some("position-ceo".into());
    ceo_holder.level_number = Some(2);
    assert_eq!(
        position_reporting_issues(
            &ceo_holder,
            &[ceo_holder.clone()],
            std::slice::from_ref(&ceo_position),
            "2027-01-15"
        ),
        vec!["The CEO must have Level 1."]
    );
    let mut ceo_level_one = ceo_holder.clone();
    ceo_level_one.level_number = Some(1);
    assert!(
        position_reporting_issues(
            &ceo_level_one,
            &[ceo_level_one.clone()],
            std::slice::from_ref(&ceo_position),
            "2027-01-15"
        )
        .is_empty()
    );

    let mut holder_with_manager = holder.clone();
    holder_with_manager.manager_person_id = Some("manager".into());
    assert_eq!(
        position_reporting_issues(
            &holder_with_manager,
            &[holder_with_manager.clone()],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        )[0],
        "The reporting manager needs one active engagement in this company."
    );

    let mut manager = person("manager");
    manager.position_id = Some("position-current".into());
    manager.level_number = Some(3);
    assert_eq!(
        position_reporting_issues(
            &holder_with_manager,
            &[holder_with_manager.clone(), manager.clone()],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        ),
        vec!["The reporting manager must have a more senior level."]
    );

    let mut cycle_manager = manager.clone();
    cycle_manager.level_number = Some(2);
    cycle_manager.manager_person_id = Some("holder".into());
    assert_eq!(
        position_reporting_issues(
            &holder_with_manager,
            &[holder_with_manager.clone(), cycle_manager],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        ),
        vec!["Correct the circular reporting relationship."]
    );

    let mut foreign_manager = manager.clone();
    foreign_manager.company_id = "foreign".into();
    assert_eq!(
        position_reporting_issues(
            &holder_with_manager,
            &[holder_with_manager.clone(), foreign_manager],
            std::slice::from_ref(&active_position),
            "2027-01-15"
        ),
        vec!["The reporting manager needs one active engagement in this company."]
    );
}

#[test]
fn manager_issues_matches_the_retained_scope_projection() {
    let mut holder = person("holder");
    holder.user_id = Some("user-holder".into());
    let mut report = person("report");
    report.manager_person_id = Some("holder".into());
    report.branch_id = Some("branch-a".into());

    assert_eq!(
        manager_issues(
            &holder,
            &[holder.clone(), report.clone()],
            &[],
            true,
            None,
            None
        ),
        vec![
            "People Manager access is missing for one or more reporting teams. Open the profile and review scoped role assignments."
        ]
    );
    assert!(
        manager_issues(
            &holder,
            &[holder.clone(), report.clone()],
            &[PositionManagerGrant {
                user_id: "user-holder".into(),
                scope_kind: "holding".into(),
                scope_node_id: None,
            }],
            true,
            None,
            None
        )
        .is_empty()
    );
    assert!(
        manager_issues(
            &holder,
            &[holder.clone(), report.clone()],
            &[PositionManagerGrant {
                user_id: "user-holder".into(),
                scope_kind: "branch".into(),
                scope_node_id: Some("branch-a".into()),
            }],
            true,
            None,
            None
        )
        .is_empty()
    );
    assert!(
        manager_issues(
            &holder,
            &[holder.clone(), report.clone()],
            &[],
            false,
            None,
            None
        )
        .is_empty()
    );
    assert!(
        manager_issues(
            &holder,
            &[holder.clone(), report],
            &[],
            true,
            Some("branch-b"),
            None
        )
        .is_empty()
    );
}
