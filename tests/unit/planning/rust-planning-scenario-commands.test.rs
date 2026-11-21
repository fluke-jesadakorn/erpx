//! Native Planning scenario command decisions and application policy.
use erpx_server_planning::public::planning::PlanningActor;
use erpx_server_planning::public::scenario::{
    CalculatePlan, CalculateRequest, CancelPlan, CapacityCalendarDay, CapacityFuture,
    CapacityOperation, CapacityReservationPlan, ComparisonRow, DemandSourceRow, IncomingRow,
    PlanningCapacityRepository, PlanningScenarioCommandRepository, ReceiptSnapshot,
    RecommendationRow, ReviewPlan, ReviewRequest, ScenarioRow, SpecificationLineRow,
    SpecificationSource, StockRow, WarehouseSource, calculate_scenario, cancel_scenario,
    parse_quantity_milli, review_recommendation,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> PlanningActor {
    PlanningActor {
        user_id: "planner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["planning".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn reviewer() -> PlanningActor {
    PlanningActor {
        user_id: "reviewer".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        ..actor()
    }
}

fn key() -> CommandKey {
    CommandKey::new("planning-key-0001").unwrap()
}

fn request() -> CalculateRequest {
    serde_json::from_value(serde_json::json!({
        "warehouseId": "warehouse",
        "specificationId": "specification",
        "specificationVersion": 2,
        "demandQuantity": "2",
        "dueDate": "2026-10-01",
        "capacityMinutes": 10
    }))
    .unwrap()
}

#[derive(Default)]
struct FakeScenario {
    specification: Option<SpecificationSource>,
    warehouse: Option<WarehouseSource>,
    demand: Option<DemandSourceRow>,
    lines: Vec<SpecificationLineRow>,
    stock: BTreeMap<String, StockRow>,
    incoming: Vec<IncomingRow>,
    comparison: Option<ComparisonRow>,
    scenario: Option<ScenarioRow>,
    recommendation: Option<RecommendationRow>,
    receipt: Option<ReceiptSnapshot>,
    spec_current: bool,
    operations: Vec<CapacityOperation>,
    calendar: Option<CapacityCalendarDay>,
    allocated: i64,
    calls: RefCell<Vec<&'static str>>,
    calculated: RefCell<Vec<CalculatePlan>>,
    cancelled: RefCell<Vec<CancelPlan>>,
    reviewed: RefCell<Vec<ReviewPlan>>,
}

fn good_scenario() -> FakeScenario {
    FakeScenario {
        specification: Some(SpecificationSource {
            id: "specification".into(),
            status: "approved".into(),
            output_product_id: "output".into(),
            batch_quantity_milli: 1_000,
        }),
        warehouse: Some(WarehouseSource {
            id: "warehouse".into(),
            branch_id: "branch".into(),
        }),
        lines: vec![SpecificationLineRow {
            product_id: "material".into(),
            quantity_milli: 2_000,
        }],
        stock: BTreeMap::from([(
            "material".to_owned(),
            StockRow {
                available: 1_000,
                version: 3,
            },
        )]),
        incoming: vec![IncomingRow {
            id: "purchase".into(),
            version: 2,
            product_id: "material".into(),
            incoming_milli: 2_000,
            due_date: "2026-10-01".into(),
        }],
        spec_current: true,
        ..FakeScenario::default()
    }
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

impl PlanningScenarioCommandRepository for FakeScenario {
    type Error = BusinessError;

    fn current_authority<'a>(&'a mut self, _: &'a PlanningActor, _: bool, _: i64) -> Fut<'a, bool> {
        self.calls.borrow_mut().push("authority");
        Box::pin(async { Ok(true) })
    }

    fn digest<'a>(&'a mut self, _: &'a str) -> Fut<'a, String> {
        Box::pin(async { Ok("a".repeat(64)) })
    }

    fn receipt<'a>(&'a mut self, _: &'a str) -> Fut<'a, Option<ReceiptSnapshot>> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn specification<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: i64,
    ) -> Fut<'a, Option<SpecificationSource>> {
        self.calls.borrow_mut().push("specification");
        let value = self.specification.clone();
        Box::pin(async move { Ok(value) })
    }

    fn active_warehouse<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
    ) -> Fut<'a, Option<WarehouseSource>> {
        self.calls.borrow_mut().push("warehouse");
        let value = self.warehouse.clone();
        Box::pin(async move { Ok(value) })
    }

    fn demand_source<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: &'a str,
        _: i64,
        _: &'a str,
    ) -> Fut<'a, Option<DemandSourceRow>> {
        self.calls.borrow_mut().push("demand");
        let value = self.demand.clone();
        Box::pin(async move { Ok(value) })
    }

    fn specification_lines<'a>(&'a mut self, _: &'a str) -> Fut<'a, Vec<SpecificationLineRow>> {
        self.calls.borrow_mut().push("lines");
        let value = self.lines.clone();
        Box::pin(async move { Ok(value) })
    }

    fn available_stock<'a>(
        &'a mut self,
        _: &'a str,
        product_id: &'a str,
    ) -> Fut<'a, Option<StockRow>> {
        self.calls.borrow_mut().push("stock");
        let value = self.stock.get(product_id).cloned();
        Box::pin(async move { Ok(value) })
    }

    fn incoming_supply<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> Fut<'a, Vec<IncomingRow>> {
        self.calls.borrow_mut().push("incoming");
        let value = self.incoming.clone();
        Box::pin(async move { Ok(value) })
    }

    fn comparison<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: &'a str,
    ) -> Fut<'a, Option<ComparisonRow>> {
        let value = self.comparison.clone();
        Box::pin(async move { Ok(value) })
    }

    fn guard_context<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: &'a erpx_server_planning::public::planning::PlanningContext,
    ) -> Fut<'a, ()> {
        self.calls.borrow_mut().push("guard");
        Box::pin(async { Ok(()) })
    }

    fn commit_calculate<'a>(&'a mut self, plan: CalculatePlan) -> Fut<'a, serde_json::Value> {
        self.calls.borrow_mut().push("commit");
        self.calculated.borrow_mut().push(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({"id": plan.id, "recommendationId": plan.recommendation_id}))
        })
    }

    fn scenario<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
    ) -> Fut<'a, Option<ScenarioRow>> {
        let value = self.scenario.clone();
        Box::pin(async move { Ok(value) })
    }

    fn commit_cancel<'a>(&'a mut self, plan: CancelPlan) -> Fut<'a, serde_json::Value> {
        self.cancelled.borrow_mut().push(plan.clone());
        Box::pin(
            async move { Ok(serde_json::json!({"id": plan.scenario_id, "status": "cancelled"})) },
        )
    }

    fn scoped_recommendation<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
    ) -> Fut<'a, Option<RecommendationRow>> {
        let value = self.recommendation.clone();
        Box::pin(async move { Ok(value) })
    }

    fn recommendation_specification_current<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: i64,
    ) -> Fut<'a, bool> {
        let value = self.spec_current;
        Box::pin(async move { Ok(value) })
    }

    fn commit_review<'a>(&'a mut self, plan: ReviewPlan) -> Fut<'a, serde_json::Value> {
        self.reviewed.borrow_mut().push(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": plan.recommendation.id,
                "status": plan.status,
                "version": plan.expected_version + 1,
                "orderId": plan.order.as_ref().map(|order| order.order_id.clone())
            }))
        })
    }
}

impl PlanningCapacityRepository for FakeScenario {
    type Error = BusinessError;

    fn operations<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> CapacityFuture<'a, Vec<CapacityOperation>, Self::Error> {
        let operations = self.operations.clone();
        Box::pin(async move { Ok(operations) })
    }

    fn calendar<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> CapacityFuture<'a, Option<CapacityCalendarDay>, Self::Error> {
        let calendar = self.calendar.clone();
        Box::pin(async move { Ok(calendar) })
    }

    fn allocated_minutes<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> CapacityFuture<'a, i64, Self::Error> {
        let allocated = self.allocated;
        Box::pin(async move { Ok(allocated) })
    }
}

fn recommendation() -> RecommendationRow {
    RecommendationRow {
        id: "recommendation".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        scenario_id: "scenario".into(),
        scenario_version: 1,
        warehouse_id: "warehouse".into(),
        specification_id: "specification".into(),
        specification_version: 2,
        recommended_quantity_milli: 2_000,
        required_minutes: 20,
        feasible_date: "2026-10-02".into(),
        status: "proposed".into(),
        created_by: "planner".into(),
        reviewed_by: None,
        version: 1,
        context_json: r#"{"allocations":[{"resourceId":"resource","workDate":"2026-10-04","minutes":60,"resourceVersion":2,"calendarVersion":0,"availableMinutes":120}]}"#.into(),
    }
}

#[test]
fn quantities_and_request_defaults_match_the_retained_schema() {
    assert_eq!(parse_quantity_milli("2").unwrap(), 2_000);
    assert_eq!(parse_quantity_milli("2.5").unwrap(), 2_500);
    assert_eq!(parse_quantity_milli("2.500").unwrap(), 2_500);
    for value in ["", "-1", "1.", ".5", "1.0000", "1,5", "abc"] {
        assert!(parse_quantity_milli(value).is_none(), "{value}");
    }
    let input = request().validate().unwrap();
    assert_eq!(input.start_date, "2026-10-01");
    assert_eq!(input.manual_minutes_per_unit, 10);
    assert_eq!(input.demand_milli, 2_000);
    assert!(
        serde_json::from_value::<CalculateRequest>(serde_json::json!({
            "warehouseId": "warehouse",
            "specificationId": "specification",
            "specificationVersion": 2,
            "dueDate": "2026-10-01",
            "capacityMinutes": 10
        }))
        .unwrap()
        .validate()
        .is_err()
    );
    assert!(
        serde_json::from_value::<CalculateRequest>(serde_json::json!({
            "warehouseId": "warehouse",
            "specificationId": "specification",
            "specificationVersion": 2,
            "demandQuantity": "1",
            "dueDate": "2026-10-01",
            "startDate": "2026-10-02"
        }))
        .unwrap()
        .validate()
        .is_err()
    );
}

#[tokio::test]
async fn calculate_derives_shortages_and_manual_capacity_without_capacity_rows() {
    let mut repository = good_scenario();
    let value = calculate_scenario(&mut repository, &actor(), &key(), request(), NOW)
        .await
        .unwrap();
    assert_eq!(value["id"], "plan-planning-key-0001");
    let plans = repository.calculated.borrow();
    let plan = &plans[0];
    assert_eq!(plan.shortages.len(), 1);
    assert_eq!(plan.shortages[0].required_milli, 4_000);
    assert_eq!(plan.shortages[0].available_milli, 1_000);
    assert_eq!(plan.shortages[0].incoming_milli, 2_000);
    assert_eq!(plan.shortages[0].shortage_milli, 1_000);
    assert_eq!(plan.required_minutes, 20);
    assert_eq!(plan.capacity_minutes, 10);
    assert_eq!(plan.feasible_date, "2026-10-02");
    assert_eq!(
        plan.context.capacity_basis.as_deref(),
        Some("manual_capacity_assumption")
    );
    assert_eq!(plan.context.stock.len(), 1);
    assert_eq!(plan.context.stock[0].version, 3);
    assert_eq!(plan.context.incoming_supply.len(), 1);
    assert_eq!(plan.context.incoming_supply[0].quantity_milli, 2_000);
}

#[tokio::test]
async fn calculation_uses_the_capacity_plan_and_comparison_deltas() {
    let mut repository = good_scenario();
    repository.comparison = Some(ComparisonRow {
        id: "previous".into(),
        version: 2,
        shortages_json: r#"[{"productId":"material","requiredMilli":1000,"availableMilli":0,"incomingMilli":0,"shortageMilli":1000}]"#.into(),
        required_minutes: 5,
        feasible_date: "2026-10-03".into(),
    });
    let mut request = request();
    request.comparison_scenario_id = Some("previous".into());
    repository.operations = vec![CapacityOperation {
        id: "cut".into(),
        name: "Cut".into(),
        resource_id: "resource".into(),
        status: "active".into(),
        branch_id: "branch".into(),
        setup_minutes: 15,
        run_minutes_per_batch: 100,
        work_weekdays: vec![1, 2, 3, 4, 5],
        daily_minutes: 120,
        resource_version: 7,
    }];
    let value = calculate_scenario(&mut repository, &actor(), &key(), request, NOW)
        .await
        .unwrap();
    assert_eq!(value["id"], "plan-planning-key-0001");
    let plans = repository.calculated.borrow();
    let plan = &plans[0];
    assert_eq!(plan.required_minutes, 215);
    assert_eq!(plan.feasible_date, "2026-10-02");
    assert_eq!(
        plan.context.capacity_basis.as_deref(),
        Some("approved_routing_resource_calendar")
    );
    assert_eq!(plan.context.allocations.len(), 2);
    let comparison = plan.context.comparison.as_ref().unwrap();
    assert_eq!(comparison.shortage_delta_milli, 0);
    assert_eq!(comparison.required_minutes_delta, 210);
}

#[tokio::test]
async fn module_denial_and_source_changes_precede_reads_and_writes() {
    let mut denied = good_scenario();
    let error = calculate_scenario(
        &mut denied,
        &PlanningActor {
            modules: vec!["manufacturing".into()],
            ..actor()
        },
        &key(),
        request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_MODULE_FORBIDDEN");
    assert!(denied.calls.borrow().is_empty());

    let mut retired = good_scenario();
    retired.specification.as_mut().unwrap().status = "retired".into();
    let error = calculate_scenario(&mut retired, &actor(), &key(), request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PLANNING_SOURCE_CHANGED");

    let mut missing = good_scenario();
    missing.warehouse = None;
    let error = calculate_scenario(&mut missing, &actor(), &key(), request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "WAREHOUSE_NOT_FOUND");

    let mut demand = good_scenario();
    demand.demand = None;
    let mut input = request();
    input.sales_order_id = Some("sales".into());
    input.sales_order_version = Some(2);
    let error = calculate_scenario(&mut demand, &actor(), &key(), input, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PLANNING_DEMAND_SOURCE_CHANGED");
}

#[tokio::test]
async fn lost_response_replay_returns_the_receipt_without_source_reads() {
    let mut repository = good_scenario();
    repository.receipt = Some(ReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: r#"{"id":"prior"}"#.into(),
        operation: "planning.scenario.create".into(),
        actor_id: "planner".into(),
        record_id: "plan-planning-key-0001".into(),
        authority_json: actor().authority_json(),
    });
    let value = calculate_scenario(&mut repository, &actor(), &key(), request(), NOW)
        .await
        .unwrap();
    assert_eq!(value["id"], "prior");
    assert_eq!(repository.calls.borrow().as_slice(), ["authority"]);
}

#[tokio::test]
async fn cancellation_requires_owner_or_reviewer_and_an_unreleased_current_version() {
    let mut repository = FakeScenario {
        scenario: Some(ScenarioRow {
            id: "scenario".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            warehouse_id: "warehouse".into(),
            specification_id: "specification".into(),
            specification_version: 2,
            demand_quantity_milli: 2_000,
            due_date: "2026-10-01".into(),
            capacity_minutes: 10,
            status: "calculated".into(),
            created_by: "planner".into(),
            version: 1,
            context_json: "{}".into(),
        }),
        ..FakeScenario::default()
    };
    let request: erpx_server_planning::public::scenario::CancelRequest =
        serde_json::from_value(serde_json::json!({
            "action": "cancel",
            "expectedVersion": 1,
            "reason": "Replaced by a newer plan"
        }))
        .unwrap();
    cancel_scenario(
        &mut repository,
        &reviewer(),
        &key(),
        "scenario",
        request.clone(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(repository.cancelled.borrow()[0].expected_version, 1);

    let mut released = FakeScenario {
        scenario: Some(ScenarioRow {
            status: "released".into(),
            ..repository.scenario.clone().unwrap()
        }),
        ..FakeScenario::default()
    };
    let error = cancel_scenario(
        &mut released,
        &actor(),
        &key(),
        "scenario",
        request.clone(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_SCENARIO_RELEASED");

    let mut not_owner = FakeScenario {
        scenario: Some(ScenarioRow {
            created_by: "someone-else".into(),
            ..released.scenario.clone().unwrap()
        }),
        ..FakeScenario::default()
    };
    not_owner.scenario.as_mut().unwrap().status = "calculated".into();
    let error = cancel_scenario(&mut not_owner, &actor(), &key(), "scenario", request, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PLANNING_OWNER_REQUIRED");
}

#[tokio::test]
#[allow(clippy::await_holding_refcell_ref)]
async fn review_requires_an_independent_current_version_and_reserves_capacity_on_approval() {
    let approve: ReviewRequest = serde_json::from_value(serde_json::json!({
        "action": "approve",
        "expectedVersion": 1,
        "reason": "Independent capacity review"
    }))
    .unwrap();
    let mut repository = FakeScenario {
        recommendation: Some(recommendation()),
        spec_current: true,
        ..FakeScenario::default()
    };
    review_recommendation(
        &mut repository,
        &reviewer(),
        &key(),
        "recommendation",
        approve,
        NOW,
    )
    .await
    .unwrap();
    let plans = repository.reviewed.borrow();
    let plan = &plans[0];
    assert_eq!(plan.status, "approved");
    assert_eq!(plan.reservations.len(), 1);
    assert_eq!(
        plan.reservations[0],
        CapacityReservationPlan {
            id: "planning-capacity-planning-key-0001-0".into(),
            resource_id: "resource".into(),
            work_date: "2026-10-04".into(),
            minutes: 60,
            available_minutes: 120,
        }
    );
    assert_eq!(
        plan.order.as_ref().unwrap().order_id,
        "mfg-order-planning-key-0001"
    );
    drop(plans);

    let reject: ReviewRequest = serde_json::from_value(serde_json::json!({
        "action": "reject",
        "expectedVersion": 1,
        "reason": "Independent capacity rejection"
    }))
    .unwrap();
    let mut repository = FakeScenario {
        recommendation: Some(recommendation()),
        spec_current: true,
        ..FakeScenario::default()
    };
    review_recommendation(
        &mut repository,
        &reviewer(),
        &key(),
        "recommendation",
        reject,
        NOW,
    )
    .await
    .unwrap();
    let plans = repository.reviewed.borrow();
    assert_eq!(plans[0].status, "rejected");
    assert!(plans[0].reservations.is_empty());
    assert!(plans[0].order.is_none());
    drop(plans);

    for row in [
        RecommendationRow {
            version: 2,
            ..recommendation()
        },
        RecommendationRow {
            created_by: "reviewer".into(),
            ..recommendation()
        },
    ] {
        let mut repository = FakeScenario {
            recommendation: Some(row),
            spec_current: true,
            ..FakeScenario::default()
        };
        let approve: ReviewRequest = serde_json::from_value(serde_json::json!({
            "action": "approve",
            "expectedVersion": 1,
            "reason": "Independent capacity review"
        }))
        .unwrap();
        let error = review_recommendation(
            &mut repository,
            &reviewer(),
            &key(),
            "recommendation",
            approve,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CONFLICT");
        assert!(repository.reviewed.borrow().is_empty());
    }
}
