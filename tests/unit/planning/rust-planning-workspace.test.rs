//! Native Planning workspace policy, source validation and bounded read tests.
use erpx_server_manufacturing::public::planning::{
    MAX_CALENDAR, MAX_RECOMMENDATIONS, MAX_SCENARIOS, PlanningActor, PlanningAllocation,
    PlanningCalendarDay, PlanningComparison, PlanningContext, PlanningContextAllocation,
    PlanningReadFuture, PlanningRecommendation, PlanningResource, PlanningScenario,
    PlanningShortage, PlanningWorkspace, PlanningWorkspaceRepository, parse_context,
    parse_shortages, parse_work_weekdays, planning_workspace, validate_workspace,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> PlanningActor {
    PlanningActor {
        user_id: "planner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["records.read".into()],
        modules: vec!["planning".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn shortage() -> PlanningShortage {
    PlanningShortage {
        product_id: "material".into(),
        required_milli: 4_000,
        available_milli: 1_000,
        incoming_milli: 2_000,
        shortage_milli: 1_000,
    }
}

fn context() -> PlanningContext {
    PlanningContext {
        demand_source: None,
        incoming_supply: vec![],
        stock: vec![],
        allocations: vec![PlanningContextAllocation {
            resource_id: "resource".into(),
            work_date: "2026-10-04".into(),
            minutes: 60,
            resource_version: 1,
            calendar_version: 0,
            available_minutes: 120,
        }],
        operations: vec![],
        start_date: None,
        capacity_basis: Some("approved_routing_resource_calendar".into()),
        comparison_scenario_id: Some("other".into()),
        comparison: Some(PlanningComparison {
            scenario_id: "other".into(),
            source_version: 2,
            shortage_delta_milli: -500,
            required_minutes_delta: 30,
            feasible_date: "2026-10-05".into(),
            current_feasible_date: "2026-10-04".into(),
        }),
    }
}

fn scenario() -> PlanningScenario {
    PlanningScenario {
        id: "scenario".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        specification_id: "specification".into(),
        specification_version: 2,
        demand_quantity_milli: 2_000,
        due_date: "2026-10-04".into(),
        capacity_minutes: 120,
        status: "calculated".into(),
        created_by: "planner".into(),
        version: 1,
        created_at: 100,
        updated_at: 100,
        shortages: vec![shortage()],
        context: context(),
    }
}

fn recommendation() -> PlanningRecommendation {
    PlanningRecommendation {
        id: "recommendation".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        scenario_id: "scenario".into(),
        scenario_version: 1,
        recommended_quantity_milli: 2_000,
        required_minutes: 60,
        feasible_date: "2026-10-04".into(),
        status: "proposed".into(),
        created_by: "planner".into(),
        reviewed_by: None,
        version: 1,
        created_at: 100,
        updated_at: 100,
    }
}

fn resource() -> PlanningResource {
    PlanningResource {
        id: "resource".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        code: "CELL".into(),
        name: "Assembly cell".into(),
        daily_minutes: 480,
        work_weekdays: vec![1, 2, 3, 4, 5],
        status: "active".into(),
        version: 1,
        created_by: "planner".into(),
        created_at: 10,
        updated_at: 10,
    }
}

fn calendar() -> PlanningCalendarDay {
    PlanningCalendarDay {
        resource_id: "resource".into(),
        work_date: "2026-10-04".into(),
        available_minutes: 60,
        reason: "Verified shutdown".into(),
        version: 1,
        updated_by: "planner".into(),
        updated_at: 20,
    }
}

fn allocation() -> PlanningAllocation {
    PlanningAllocation {
        id: "allocation".into(),
        scenario_id: "scenario".into(),
        resource_id: "resource".into(),
        work_date: "2026-10-04".into(),
        minutes: 60,
        created_by: "planner".into(),
        created_at: 30,
    }
}

fn workspace() -> PlanningWorkspace {
    PlanningWorkspace {
        scenarios: vec![scenario()],
        recommendations: vec![recommendation()],
        resources: vec![resource()],
        calendar: vec![calendar()],
        allocations: vec![allocation()],
    }
}

#[derive(Default)]
struct Probe {
    result: Option<Result<PlanningWorkspace, BusinessError>>,
    calls: usize,
}

impl PlanningWorkspaceRepository for Probe {
    type Error = BusinessError;

    fn workspace<'a>(
        &'a mut self,
        _: &'a PlanningActor,
    ) -> PlanningReadFuture<'a, PlanningWorkspace, Self::Error> {
        self.calls += 1;
        let result = self.result.take().unwrap_or_else(|| Ok(workspace()));
        Box::pin(async move { result })
    }
}

#[tokio::test]
async fn policy_denies_before_any_owner_read() {
    let mut probe = Probe::default();
    for (current, kind, code) in [
        (
            PlanningActor {
                permissions: vec![],
                ..actor()
            },
            FailureKind::Forbidden,
            "PLANNING_PERMISSION_REQUIRED",
        ),
        (
            PlanningActor {
                modules: vec!["manufacturing".into()],
                ..actor()
            },
            FailureKind::Forbidden,
            "PLANNING_MODULE_FORBIDDEN",
        ),
        (
            PlanningActor {
                expires_at: NOW.get(),
                ..actor()
            },
            FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
        ),
    ] {
        let error = planning_workspace(&current, &mut probe, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.kind, kind);
        assert_eq!(error.code, code);
    }
    assert_eq!(probe.calls, 0);

    let mut expired = actor();
    expired.expires_at = 0;
    let error = planning_workspace(&expired, &mut probe, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(probe.calls, 0);
}

#[tokio::test]
async fn bounded_workspace_serializes_exact_owner_fields() {
    let mut probe = Probe::default();
    let result = planning_workspace(&actor(), &mut probe, NOW).await.unwrap();
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "allocations",
            "calendar",
            "recommendations",
            "resources",
            "scenarios"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    assert_eq!(
        value["scenarios"][0]["shortages"][0]["shortageMilli"],
        1_000
    );
    assert_eq!(
        value["scenarios"][0]["context"]["capacityBasis"],
        "approved_routing_resource_calendar"
    );
    assert_eq!(value["recommendations"][0]["status"], "proposed");
    assert_eq!(
        value["resources"][0]["work_weekdays"],
        serde_json::json!([1, 2, 3, 4, 5])
    );
    assert_eq!(value["calendar"][0]["available_minutes"], 60);
    assert_eq!(value["allocations"][0]["minutes"], 60);
    assert_eq!(probe.calls, 1);
}

#[test]
fn foreign_company_and_branch_rows_fail_closed() {
    let mut foreign = workspace();
    foreign.scenarios[0].company_id = "foreign".into();
    let error = validate_workspace(&actor(), foreign).unwrap_err();
    assert_eq!(error.code, "PLANNING_SOURCE_INVALID");

    let mut other_branch = workspace();
    other_branch.scenarios[0].branch_id = "other".into();
    let error = validate_workspace(&actor(), other_branch).unwrap_err();
    assert_eq!(error.code, "PLANNING_SOURCE_INVALID");

    let mut foreign_resource = workspace();
    foreign_resource.resources[0].company_id = "foreign".into();
    let error = validate_workspace(&actor(), foreign_resource).unwrap_err();
    assert_eq!(error.code, "PLANNING_SOURCE_INVALID");
}

#[test]
fn duplicate_orphan_and_incoherent_rows_fail_closed() {
    let mutations: [fn(&mut PlanningWorkspace); 10] = [
        |value| value.scenarios.push(value.scenarios[0].clone()),
        |value| value.recommendations[0].scenario_id = "missing".into(),
        |value| {
            value.recommendations.push(PlanningRecommendation {
                id: "recommendation-2".into(),
                ..value.recommendations[0].clone()
            });
        },
        |value| value.recommendations[0].company_id = "foreign".into(),
        |value| value.recommendations[0].branch_id = "other".into(),
        |value| value.resources.push(value.resources[0].clone()),
        |value| value.calendar[0].resource_id = "missing".into(),
        |value| value.calendar.push(value.calendar[0].clone()),
        |value| value.allocations[0].scenario_id = "missing".into(),
        |value| value.allocations[0].resource_id = "missing".into(),
    ];
    for mutate in mutations {
        let mut value = workspace();
        mutate(&mut value);
        assert_eq!(
            validate_workspace(&actor(), value).unwrap_err().code,
            "PLANNING_SOURCE_INVALID"
        );
    }
}

#[test]
fn shortage_context_and_review_state_corruption_fail_closed() {
    let mut changed = workspace();
    changed.scenarios[0].shortages[0].shortage_milli = 999;
    assert_eq!(
        validate_workspace(&actor(), changed).unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut unknown_shortage = workspace();
    unknown_shortage.scenarios[0].shortages[0]
        .product_id
        .clear();
    assert_eq!(
        validate_workspace(&actor(), unknown_shortage)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut context = workspace();
    context.scenarios[0].context.capacity_basis = Some(String::new());
    assert_eq!(
        validate_workspace(&actor(), context).unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut comparison = workspace();
    comparison.scenarios[0]
        .context
        .comparison
        .as_mut()
        .unwrap()
        .source_version = 0;
    assert_eq!(
        validate_workspace(&actor(), comparison).unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut self_reviewed = workspace();
    self_reviewed.recommendations[0].status = "approved".into();
    self_reviewed.recommendations[0].reviewed_by = Some("planner".into());
    assert_eq!(
        validate_workspace(&actor(), self_reviewed)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut cancelled_owner = workspace();
    cancelled_owner.recommendations[0].status = "rejected".into();
    cancelled_owner.recommendations[0].reviewed_by = Some("planner".into());
    assert!(
        validate_workspace(&actor(), cancelled_owner).is_ok(),
        "a planner may cancel their own unreleased scenario"
    );

    let mut proposed_with_reviewer = workspace();
    proposed_with_reviewer.recommendations[0].reviewed_by = Some("reviewer".into());
    assert_eq!(
        validate_workspace(&actor(), proposed_with_reviewer)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );

    let mut invalid_weekdays = workspace();
    invalid_weekdays.resources[0].work_weekdays = vec![1, 1];
    assert_eq!(
        validate_workspace(&actor(), invalid_weekdays)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );
}

#[test]
fn source_limits_are_bounded() {
    let mut too_many_scenarios = workspace();
    too_many_scenarios.scenarios = vec![scenario(); MAX_SCENARIOS + 1];
    assert_eq!(
        validate_workspace(&actor(), too_many_scenarios)
            .unwrap_err()
            .code,
        "PLANNING_WORKSPACE_LIMIT"
    );

    let mut too_many_recommendations = workspace();
    too_many_recommendations.recommendations = vec![recommendation(); MAX_RECOMMENDATIONS + 1];
    assert_eq!(
        validate_workspace(&actor(), too_many_recommendations)
            .unwrap_err()
            .code,
        "PLANNING_WORKSPACE_LIMIT"
    );

    let mut too_many_calendar = workspace();
    too_many_calendar.calendar = vec![calendar(); MAX_CALENDAR + 1];
    assert_eq!(
        validate_workspace(&actor(), too_many_calendar)
            .unwrap_err()
            .code,
        "PLANNING_WORKSPACE_LIMIT"
    );
}

#[tokio::test]
async fn repository_failures_propagate_without_partial_presentation() {
    let mut probe = Probe {
        result: Some(Err(BusinessError::new(
            FailureKind::Unavailable,
            "PLANNING_SOURCE_UNAVAILABLE",
            "The planning source is unavailable.",
        ))),
        ..Probe::default()
    };
    let error = planning_workspace(&actor(), &mut probe, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PLANNING_SOURCE_UNAVAILABLE");
    assert_eq!(probe.calls, 1);
}

#[test]
fn stored_json_helpers_reject_malformed_owner_facts() {
    assert_eq!(
        parse_shortages(
            r#"[{"productId":"material","requiredMilli":4000,"availableMilli":1000,"incomingMilli":2000,"shortageMilli":1000}]"#
        )
        .unwrap()
        .len(),
        1
    );
    for value in [
        "{}",
        "[] trailing",
        r#"[{"productId":"material","requiredMilli":1}]"#,
        r#"[{"productId":"material","requiredMilli":1,"availableMilli":0,"incomingMilli":0,"shortageMilli":2}]"#,
        r#"[{"productId":"material","requiredMilli":1,"availableMilli":0,"incomingMilli":0,"shortageMilli":1,"extra":true}]"#,
    ] {
        assert_eq!(
            parse_shortages(value).unwrap_err().code,
            "PLANNING_SOURCE_INVALID"
        );
    }
    assert!(parse_context(r#"{"capacityBasis":"manual_capacity_assumption"}"#).is_ok());
    assert_eq!(
        parse_context("[]").unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );
    assert_eq!(parse_work_weekdays("[1,2]").unwrap(), vec![1, 2]);
    assert_eq!(
        parse_work_weekdays("[1,\"2\"]").unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );
}

#[test]
fn text_limits_count_utf16_units_with_js_trim_edges() {
    // 32 non-BMP characters are 64 UTF-16 code units (the code limit) but
    // 128 UTF-8 bytes, so this passes only with UTF-16 length parity.
    let mut code_at_limit = workspace();
    code_at_limit.resources[0].code = "😀".repeat(32);
    assert!(validate_workspace(&actor(), code_at_limit).is_ok());

    // 81 emoji exceed the 160-unit name limit even though code-point
    // counting would accept them.
    let mut name_over_limit = workspace();
    name_over_limit.resources[0].name = "😀".repeat(81);
    assert_eq!(
        validate_workspace(&actor(), name_over_limit)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );

    // U+FEFF is trimmed by ECMAScript trim but not by Rust trim; a value with
    // a leading zero-width no-break space must fail the edge check.
    let mut bom_prefix = workspace();
    bom_prefix.resources[0].code = "\u{feff}CELL".to_owned();
    assert_eq!(
        validate_workspace(&actor(), bom_prefix).unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );

    // U+00A0 trailing whitespace still fails the JS edge check.
    let mut nbsp_trailing = workspace();
    nbsp_trailing.resources[0].name = "Assembly cell\u{00a0}".to_owned();
    assert_eq!(
        validate_workspace(&actor(), nbsp_trailing)
            .unwrap_err()
            .code,
        "PLANNING_SOURCE_INVALID"
    );

    // Interior C0 control fails closed.
    let mut control = workspace();
    control.resources[0].name = "Assembly\u{0007}cell".to_owned();
    assert_eq!(
        validate_workspace(&actor(), control).unwrap_err().code,
        "PLANNING_SOURCE_INVALID"
    );
}
