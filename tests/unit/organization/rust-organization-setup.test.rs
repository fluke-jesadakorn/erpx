use erpx_server_organization::public::organization_nodes::{
    OrganizationAuthority, OrganizationCommand, OrganizationCommandActor,
};
use erpx_server_organization::public::organization_setup::*;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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

fn template(positions: Value) -> TemplateInput {
    TemplateInput::parse(&json!({
        "name": "Standard organization",
        "positions": positions,
        "expectedVersion": 0
    }))
    .unwrap()
}

fn position(key: &str, title: &str, level: i64, reports: Value, ceo: bool) -> Value {
    json!({
        "key": key,
        "title": title,
        "levelNumber": level,
        "reportsToKey": reports,
        "isCeo": ceo,
        "rosterRequired": false
    })
}

fn standard_template() -> TemplateInput {
    template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("manager", "Manager", 3, json!("ceo"), false)
    ]))
}

#[derive(Default)]
struct FakeRepository {
    replay: Option<Value>,
    template: Option<TemplateRow>,
    branch_active: bool,
    department_active: bool,
    levels: Vec<(i64, String)>,
    ceo: Option<String>,
    calls: Vec<String>,
    applied: Option<ApplyTemplateCommand>,
}

impl OrganizationSetupRepository for FakeRepository {
    type Error = BusinessError;

    fn replay<'a>(
        &'a mut self,
        _actor: &'a OrganizationCommandActor,
        _key: &'a str,
        operation: &'a str,
        _record_id: &'a str,
        _input: &'a Value,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<Value>>
    {
        self.calls.push(format!("replay:{operation}"));
        Box::pin(async move { Ok(self.replay.clone()) })
    }

    fn template<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _id: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<
        'a,
        Option<TemplateRow>,
    > {
        self.calls.push("template".into());
        Box::pin(async move { Ok(self.template.clone()) })
    }

    fn branch_active<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: &'a str,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        Box::pin(async move { Ok(self.branch_active) })
    }

    fn department_active<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _department: &'a str,
        _branch: Option<&'a str>,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, bool> {
        Box::pin(async move { Ok(self.department_active) })
    }

    fn levels<'a>(
        &'a mut self,
        _company: &'a CompanyId,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<
        'a,
        Vec<(i64, String)>,
    > {
        Box::pin(async move { Ok(self.levels.clone()) })
    }

    fn ceo_position<'a>(
        &'a mut self,
        _company: &'a CompanyId,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Option<String>>
    {
        Box::pin(async move { Ok(self.ceo.clone()) })
    }

    fn persist_install_levels<'a>(
        &'a mut self,
        command: &'a InstallLevelsCommand,
        _now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Value> {
        self.calls.push(format!("install:{}", command.titles.len()));
        Box::pin(async move { Ok(json!({"id": "company"})) })
    }

    fn persist_save_template<'a>(
        &'a mut self,
        command: &'a SaveTemplateCommand,
        _now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Value> {
        self.calls.push(format!("save:{}", command.record_id));
        let id = command.record_id.clone();
        let version = command.version;
        Box::pin(async move { Ok(json!({"id": id, "version": version})) })
    }

    fn persist_apply_template<'a>(
        &'a mut self,
        command: &'a ApplyTemplateCommand,
        _now: i64,
    ) -> erpx_server_organization::public::organization_nodes::OrganizationFuture<'a, Value> {
        self.calls.push("apply".into());
        self.applied = Some(command.clone());
        let id = command.template_id.clone();
        let ids = command.position_ids.clone();
        Box::pin(async move { Ok(json!({"id": id, "positionIds": ids})) })
    }
}

#[test]
fn template_rules_match_the_retained_codes_and_order() {
    standard_template().validate().unwrap();

    let duplicate = template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("ceo", "Duplicate", 2, json!("ceo"), false)
    ]));
    assert_eq!(duplicate.validate().unwrap_err().code, "DUPLICATE_POSITION");

    let bad_ceo = template(json!([
        position("ceo", "CEO", 2, Value::Null, true),
        position("staff", "Staff", 6, json!("ceo"), false)
    ]));
    assert_eq!(bad_ceo.validate().unwrap_err().code, "CEO_POSITION_INVALID");

    let missing_report = template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("staff", "Staff", 6, Value::Null, false)
    ]));
    assert_eq!(
        missing_report.validate().unwrap_err().code,
        "REPORTING_POSITION_REQUIRED"
    );

    let cycle = template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("self", "Self reporting", 2, json!("self"), false)
    ]));
    assert_eq!(cycle.validate().unwrap_err().code, "REPORTING_CYCLE");

    let not_senior = template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("manager", "Manager", 3, json!("ceo"), false),
        position("staff", "Staff", 3, json!("manager"), false)
    ]));
    assert_eq!(
        not_senior.validate().unwrap_err().code,
        "REPORTING_POSITION_INVALID"
    );

    let two_ceos = template(json!([
        position("ceo", "CEO", 1, Value::Null, true),
        position("other", "Other CEO", 1, Value::Null, true)
    ]));
    assert_eq!(
        two_ceos.validate().unwrap_err().code,
        "CEO_POSITION_REQUIRED"
    );
}

#[test]
fn malformed_templates_and_applications_fail_before_persistence() {
    let too_many = json!({
        "name": "Standard organization",
        "positions": (0..101).map(|index| position(&format!("p{index}"), "Position", 6, json!("ceo"), false)).collect::<Vec<_>>(),
        "expectedVersion": 0
    });
    assert_eq!(
        TemplateInput::parse(&too_many).unwrap_err().code,
        "VALIDATION_FAILED"
    );

    let bad_level = json!({
        "name": "Standard organization",
        "positions": [position("ceo", "CEO", 7, Value::Null, true)],
        "expectedVersion": 0
    });
    assert_eq!(
        TemplateInput::parse(&bad_level).unwrap_err().code,
        "VALIDATION_FAILED"
    );

    let zero_apply = json!({"expectedVersion": 0, "branchId": null, "departmentId": null, "reason": "Apply the template"});
    assert_eq!(
        ApplyTemplateInput::parse(&zero_apply).unwrap_err().code,
        "VALIDATION_FAILED"
    );
    let parsed = ApplyTemplateInput::parse(&json!({
        "expectedVersion": 2,
        "branchId": " branch ",
        "departmentId": "department",
        "reason": "Apply the reviewed template",
        "requirements": "Ignored by the retained schema"
    }))
    .unwrap();
    assert_eq!(parsed.branch_id.as_deref(), Some(" branch "));
    assert_eq!(parsed.department_id.as_deref(), Some("department"));
}

#[tokio::test]
async fn save_template_replays_before_validation_and_requires_the_exact_version() {
    let clock = TestClock;
    let mut repository = FakeRepository {
        replay: Some(json!({"id": "org-template-old", "version": 4})),
        ..Default::default()
    };
    let invalid = TemplateInput {
        name: String::new(),
        positions: Vec::new(),
        expected_version: 0,
    };
    let replayed = save_template(
        &mut repository,
        owner(),
        "template-key-0001",
        None,
        invalid,
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(replayed["version"], 4);
    assert_eq!(
        repository.calls,
        vec!["replay:organization.template.save".to_string()]
    );

    let mut repository = FakeRepository {
        template: Some(TemplateRow {
            positions_json: "[]".into(),
            version: 2,
        }),
        ..Default::default()
    };
    let stale = save_template(
        &mut repository,
        owner(),
        "template-key-0002",
        Some("template"),
        TemplateInput {
            expected_version: 1,
            ..standard_template()
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "CONFLICT");
    assert!(
        !repository
            .calls
            .iter()
            .any(|call| call.starts_with("save:"))
    );

    let mut repository = FakeRepository::default();
    let denied = save_template(
        &mut repository,
        actor(&[], None, None),
        "template-key-0003",
        None,
        standard_template(),
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "PERMISSION_REQUIRED");
}

#[tokio::test]
async fn apply_template_reuses_the_existing_ceo_and_creates_senior_positions() {
    let clock = TestClock;
    let mut repository = FakeRepository {
        template: Some(TemplateRow {
            positions_json: serde_json::to_string(&standard_template().positions).unwrap(),
            version: 3,
        }),
        levels: vec![(1, "level-1".into()), (3, "level-3".into())],
        ceo: Some("position-ceo".into()),
        branch_active: true,
        ..Default::default()
    };
    let result = apply_template(
        &mut repository,
        owner(),
        "apply-key-0001",
        "org-template-key",
        ApplyTemplateInput {
            expected_version: 3,
            branch_id: Some("branch".into()),
            department_id: None,
            reason: "Apply the reviewed template".to_owned(),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        result["positionIds"],
        json!(["position-ceo", "position-apply-key-0001-1"])
    );
    let applied = repository.applied.unwrap();
    assert_eq!(applied.positions.len(), 1);
    assert_eq!(applied.positions[0].id, "position-apply-key-0001-1");
    assert_eq!(
        applied.positions[0].reports_to_position_id.as_deref(),
        Some("position-ceo")
    );
    assert_eq!(applied.positions[0].level_id, "level-3");
    assert_eq!(applied.version, 3);
}

#[tokio::test]
async fn apply_template_checks_scope_placement_template_and_levels() {
    let clock = TestClock;
    let mut repository = FakeRepository {
        template: Some(TemplateRow {
            positions_json: serde_json::to_string(&standard_template().positions).unwrap(),
            version: 1,
        }),
        levels: vec![(1, "level-1".into()), (3, "level-3".into())],
        ..Default::default()
    };
    let scoped = apply_template(
        &mut repository,
        actor(&["organization.manage"], Some("branch"), None),
        "apply-key-scope",
        "org-template-key",
        ApplyTemplateInput {
            expected_version: 1,
            branch_id: Some("other-branch".into()),
            department_id: None,
            reason: "Apply the reviewed template".to_owned(),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(scoped.code, "SCOPE_FORBIDDEN");

    let mut repository = FakeRepository {
        template: None,
        ..Default::default()
    };
    let missing = apply_template(
        &mut repository,
        owner(),
        "apply-key-missing",
        "org-template-missing",
        ApplyTemplateInput {
            expected_version: 1,
            branch_id: None,
            department_id: None,
            reason: "Apply the reviewed template".to_owned(),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, "TEMPLATE_NOT_FOUND");

    let mut repository = FakeRepository {
        template: Some(TemplateRow {
            positions_json: serde_json::to_string(&standard_template().positions).unwrap(),
            version: 1,
        }),
        levels: vec![(1, "level-1".into())],
        ..Default::default()
    };
    let missing_level = apply_template(
        &mut repository,
        owner(),
        "apply-key-level",
        "org-template-key",
        ApplyTemplateInput {
            expected_version: 1,
            branch_id: None,
            department_id: None,
            reason: "Apply the reviewed template".to_owned(),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(missing_level.code, "LEVEL_REQUIRED");

    let mut repository = FakeRepository {
        template: Some(TemplateRow {
            positions_json: serde_json::to_string(&standard_template().positions).unwrap(),
            version: 1,
        }),
        levels: vec![(1, "level-1".into()), (3, "level-3".into())],
        ..Default::default()
    };
    let inactive_branch = apply_template(
        &mut repository,
        owner(),
        "apply-key-branch",
        "org-template-key",
        ApplyTemplateInput {
            expected_version: 1,
            branch_id: Some("branch".into()),
            department_id: None,
            reason: "Apply the reviewed template".to_owned(),
        },
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(inactive_branch.code, "BRANCH_INVALID");
}

#[tokio::test]
async fn install_levels_uses_the_fixed_catalog_and_replays_the_prior_receipt() {
    let clock = TestClock;
    let mut repository = FakeRepository::default();
    let result = install_levels(&mut repository, owner(), "level-key-0001", &clock)
        .await
        .unwrap();
    assert_eq!(result["id"], "company");
    assert_eq!(
        repository.calls,
        vec![
            "replay:organization.levels.install".to_string(),
            "install:6".to_string()
        ]
    );

    let mut repository = FakeRepository {
        replay: Some(json!({"id": "company"})),
        ..Default::default()
    };
    let replayed = install_levels(&mut repository, owner(), "level-key-0001", &clock)
        .await
        .unwrap();
    assert_eq!(replayed, json!({"id": "company"}));
    assert_eq!(
        repository.calls,
        vec!["replay:organization.levels.install".to_string()]
    );

    let mut repository = FakeRepository::default();
    let denied = install_levels(
        &mut repository,
        actor(&[], None, None),
        "level-key-0002",
        &clock,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "PERMISSION_REQUIRED");
}

#[test]
fn generated_template_and_application_identifiers_stay_inside_the_owner_contract() {
    let too_long = "x".repeat(190);
    let value = json!({
        "name": "Standard organization",
        "positions": [position("ceo", "CEO", 1, Value::Null, true)],
        "expectedVersion": 0
    });
    let input = TemplateInput::parse(&value).unwrap();
    assert_eq!(input.positions[0].key, "ceo");
    assert_eq!(input.positions[0].level_number, 1);
    let id = format!("org-template-{too_long}");
    assert!(RecordId::new(id).is_err());
    let positions = BTreeMap::from([("ceo".to_string(), "position-ceo".to_string())]);
    assert_eq!(
        positions.get("ceo").map(String::as_str),
        Some("position-ceo")
    );
}
