//! Native Planning capacity, resource and calendar decisions.
use erpx_server_planning::public::planning::PlanningActor;
use erpx_server_planning::public::resources::{
    CalendarPlan, CalendarRequest, CalendarRow, PlanningResourceCommandRepository, ResourcePlan,
    ResourceRequest, ResourceRow, save_calendar, save_resource,
};
use erpx_server_planning::public::scenario::{
    CapacityCalendarDay, CapacityFuture, CapacityInput, CapacityOperation,
    PlanningCapacityRepository, ReceiptSnapshot, calculate_capacity, capacity_unavailable,
    consolidate_allocations, default_capacity, next_capacity_date, weekday,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> PlanningActor {
    PlanningActor {
        user_id: "planner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        modules: vec!["planning".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn key() -> CommandKey {
    CommandKey::new("planning-key-0002").unwrap()
}

fn operation() -> CapacityOperation {
    CapacityOperation {
        id: "cut".into(),
        name: "Cut".into(),
        resource_id: "machine".into(),
        status: "active".into(),
        branch_id: "branch".into(),
        setup_minutes: 15,
        run_minutes_per_batch: 100,
        work_weekdays: vec![1, 2, 3, 4, 5],
        daily_minutes: 120,
        resource_version: 7,
    }
}

fn input() -> CapacityInput {
    CapacityInput {
        specification_id: "spec".into(),
        branch_id: "branch".into(),
        factor: 1.5,
        start_date: "2026-10-02".into(),
        due_date: "2026-10-04".into(),
    }
}

#[derive(Default)]
struct FakeCapacity {
    operations: Vec<CapacityOperation>,
    calendar: Option<CapacityCalendarDay>,
    allocated: i64,
}

impl PlanningCapacityRepository for FakeCapacity {
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

#[test]
fn date_helpers_match_javascript_utc_semantics() {
    assert_eq!(weekday("2026-10-02"), Some(5));
    assert_eq!(weekday("2026-10-04"), Some(0));
    assert_eq!(next_capacity_date("2026-12-31"), "2027-01-01");
    assert_eq!(next_capacity_date("2028-02-28"), "2028-02-29");
    assert_eq!(default_capacity("2026-10-03", &[1, 2, 3, 4, 5], 480), 0);
    assert_eq!(default_capacity("2026-10-05", &[1, 2, 3, 4, 5], 480), 480);
    let consolidated = consolidate_allocations(vec![
        erpx_server_planning::public::planning::PlanningContextAllocation {
            resource_id: "machine".into(),
            work_date: "2026-10-02".into(),
            minutes: 20,
            resource_version: 1,
            calendar_version: 0,
            available_minutes: 120,
        },
        erpx_server_planning::public::planning::PlanningContextAllocation {
            resource_id: "machine".into(),
            work_date: "2026-10-02".into(),
            minutes: 30,
            resource_version: 1,
            calendar_version: 0,
            available_minutes: 120,
        },
    ]);
    assert_eq!(consolidated.len(), 1);
    assert_eq!(consolidated[0].minutes, 50);
}

#[tokio::test]
async fn capacity_honors_allocations_weekends_and_the_due_date_window() {
    let mut repository = FakeCapacity {
        operations: vec![operation()],
        allocated: 20,
        ..FakeCapacity::default()
    };
    let plan = calculate_capacity(
        &mut repository,
        &CompanyId::new("company").unwrap(),
        &input(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(plan.required_minutes, 165);
    assert_eq!(plan.feasible_date, "2026-10-05");
    assert_eq!(plan.capacity_minutes, 100);
    assert_eq!(plan.allocations.len(), 2);
    assert_eq!(plan.allocations[0].work_date, "2026-10-02");
    assert_eq!(plan.allocations[0].minutes, 100);
    assert_eq!(plan.allocations[0].available_minutes, 120);
    assert_eq!(plan.allocations[0].resource_version, 7);
    assert_eq!(plan.allocations[1].work_date, "2026-10-05");
    assert_eq!(plan.allocations[1].minutes, 65);
}

#[tokio::test]
async fn calendar_overrides_and_reused_resources_consolidate() {
    let mut repository = FakeCapacity {
        operations: vec![
            CapacityOperation {
                setup_minutes: 0,
                run_minutes_per_batch: 20,
                ..operation()
            },
            CapacityOperation {
                id: "finish".into(),
                name: "Finish".into(),
                setup_minutes: 0,
                run_minutes_per_batch: 30,
                ..operation()
            },
        ],
        calendar: Some(CapacityCalendarDay {
            available_minutes: 100,
            version: 4,
        }),
        allocated: 0,
    };
    let plan = calculate_capacity(
        &mut repository,
        &CompanyId::new("company").unwrap(),
        &CapacityInput {
            factor: 1.0,
            start_date: "2026-10-03".into(),
            ..input()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(plan.required_minutes, 50);
    assert_eq!(plan.allocations.len(), 1);
    assert_eq!(plan.allocations[0].minutes, 50);
    assert_eq!(plan.allocations[0].calendar_version, 4);
    assert_eq!(plan.allocations[0].available_minutes, 100);
}

#[tokio::test]
async fn capacity_refusals_and_empty_routing_are_bounded() {
    for operation in [
        CapacityOperation {
            status: "archived".into(),
            ..operation()
        },
        CapacityOperation {
            branch_id: "foreign".into(),
            ..operation()
        },
    ] {
        let mut repository = FakeCapacity {
            operations: vec![operation],
            ..FakeCapacity::default()
        };
        let error = calculate_capacity(
            &mut repository,
            &CompanyId::new("company").unwrap(),
            &input(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PLANNING_RESOURCE_CHANGED");
    }
    let mut repository = FakeCapacity {
        operations: vec![CapacityOperation {
            daily_minutes: 0,
            ..operation()
        }],
        ..FakeCapacity::default()
    };
    let error = calculate_capacity(
        &mut repository,
        &CompanyId::new("company").unwrap(),
        &input(),
    )
    .await
    .unwrap_err();
    assert_eq!(error, capacity_unavailable());

    let mut empty = FakeCapacity::default();
    assert!(
        calculate_capacity(&mut empty, &CompanyId::new("company").unwrap(), &input())
            .await
            .unwrap()
            .is_none()
    );
}

#[derive(Default)]
struct FakeResources {
    authority: bool,
    row: Option<ResourceRow>,
    active_allocation: bool,
    calendar: Option<CalendarRow>,
    active_version: bool,
    released: bool,
    receipt: Option<ReceiptSnapshot>,
    resource: RefCell<Option<ResourcePlan>>,
    calendar_plan: RefCell<Option<CalendarPlan>>,
}

impl FakeResources {
    fn ready() -> Self {
        Self {
            authority: true,
            row: Some(ResourceRow {
                id: "resource".into(),
                company_id: "company".into(),
                branch_id: "branch".into(),
                code: "CELL".into(),
                name: "Assembly cell".into(),
                daily_minutes: 480,
                work_weekdays_json: "[1,2,3,4,5]".into(),
                status: "active".into(),
                version: 2,
                created_by: "planner".into(),
                created_at: 10,
                updated_at: 10,
            }),
            active_version: true,
            released: true,
            ..FakeResources::default()
        }
    }
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

impl PlanningResourceCommandRepository for FakeResources {
    type Error = BusinessError;

    fn current_authority<'a>(&'a mut self, _: &'a PlanningActor, _: bool, _: i64) -> Fut<'a, bool> {
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }

    fn digest<'a>(&'a mut self, _: &'a str) -> Fut<'a, String> {
        Box::pin(async { Ok("a".repeat(64)) })
    }

    fn receipt<'a>(&'a mut self, _: &'a str) -> Fut<'a, Option<ReceiptSnapshot>> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn resource<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
    ) -> Fut<'a, Option<ResourceRow>> {
        let row = self.row.clone();
        Box::pin(async move { Ok(row) })
    }

    fn has_active_allocation<'a>(&'a mut self, _: &'a str) -> Fut<'a, bool> {
        let active = self.active_allocation;
        Box::pin(async move { Ok(active) })
    }

    fn calendar<'a>(&'a mut self, _: &'a str, _: &'a str) -> Fut<'a, Option<CalendarRow>> {
        let calendar = self.calendar.clone();
        Box::pin(async move { Ok(calendar) })
    }

    fn active_resource_version<'a>(
        &'a mut self,
        _: &'a PlanningActor,
        _: &'a str,
        _: i64,
    ) -> Fut<'a, bool> {
        let active = self.active_version;
        Box::pin(async move { Ok(active) })
    }

    fn released_capacity<'a>(&'a mut self, _: &'a str, _: &'a str, _: i64) -> Fut<'a, bool> {
        let released = self.released;
        Box::pin(async move { Ok(released) })
    }

    fn commit_resource<'a>(&'a mut self, plan: ResourcePlan) -> Fut<'a, serde_json::Value> {
        *self.resource.borrow_mut() = Some(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({"id": plan.resource_id, "version": plan.expected_version + 1}))
        })
    }

    fn commit_calendar<'a>(&'a mut self, plan: CalendarPlan) -> Fut<'a, serde_json::Value> {
        *self.calendar_plan.borrow_mut() = Some(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": plan.resource_id,
                "workDate": plan.work_date,
                "version": plan.expected_version + 1
            }))
        })
    }
}

fn resource_request() -> ResourceRequest {
    serde_json::from_value(serde_json::json!({
        "branchId": "branch",
        "code": "CELL",
        "name": "Assembly cell",
        "dailyMinutes": 480,
        "workWeekdays": [5, 1, 1, 3, 2, 4]
    }))
    .unwrap()
}

fn calendar_request() -> CalendarRequest {
    serde_json::from_value(serde_json::json!({
        "expectedVersion": 0,
        "resourceVersion": 2,
        "workDate": "2026-10-04",
        "availableMinutes": 60,
        "reason": "Verified shutdown"
    }))
    .unwrap()
}

#[tokio::test]
async fn resource_creation_normalizes_weekdays_and_status() {
    let mut repository = FakeResources::ready();
    save_resource(
        &mut repository,
        &actor(),
        &key(),
        None,
        resource_request(),
        NOW,
    )
    .await
    .unwrap();
    let plan = repository.resource.borrow();
    let plan = plan.as_ref().unwrap();
    assert_eq!(plan.resource_id, "production-resource-planning-key-0002");
    assert!(!plan.existing);
    assert_eq!(plan.work_weekdays_json, "[1,2,3,4,5]");
    assert_eq!(plan.status, "active");
    assert_eq!(plan.expected_version, 0);
}

#[tokio::test]
async fn resource_updates_require_cas_and_no_active_allocation() {
    let mut missing_version = FakeResources::ready();
    let error = save_resource(
        &mut missing_version,
        &actor(),
        &key(),
        Some("resource"),
        resource_request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "VERSION_REQUIRED");

    let mut stale = FakeResources::ready();
    let mut request = resource_request();
    request.expected_version = Some(1);
    let error = save_resource(
        &mut stale,
        &actor(),
        &key(),
        Some("resource"),
        request.clone(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_RESOURCE_CHANGED");

    let mut allocated = FakeResources::ready();
    allocated.active_allocation = true;
    request.expected_version = Some(2);
    let error = save_resource(
        &mut allocated,
        &actor(),
        &key(),
        Some("resource"),
        request,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_RESOURCE_ALLOCATED");
}

#[tokio::test]
async fn calendar_fences_resource_version_calendar_version_and_released_capacity() {
    let mut repository = FakeResources::ready();
    save_calendar(
        &mut repository,
        &actor(),
        &key(),
        "resource",
        calendar_request(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        repository
            .calendar_plan
            .borrow()
            .as_ref()
            .unwrap()
            .expected_version,
        0
    );

    let mut stale_calendar = FakeResources::ready();
    stale_calendar.calendar = Some(CalendarRow {
        resource_id: "resource".into(),
        work_date: "2026-10-04".into(),
        available_minutes: 90,
        reason: "Previous shutdown".into(),
        version: 2,
        updated_by: "planner".into(),
        updated_at: 20,
    });
    let error = save_calendar(
        &mut stale_calendar,
        &actor(),
        &key(),
        "resource",
        calendar_request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_RESOURCE_CHANGED");

    let mut over_capacity = FakeResources::ready();
    over_capacity.released = false;
    let error = save_calendar(
        &mut over_capacity,
        &actor(),
        &key(),
        "resource",
        calendar_request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_CALENDAR_CAPACITY_EXCEEDED");
}

#[tokio::test]
async fn resource_scope_and_review_permission_are_checked_before_writes() {
    let mut department = FakeResources::ready();
    let error = save_resource(
        &mut department,
        &PlanningActor {
            department_id: Some(RecordId::new("department").unwrap()),
            ..actor()
        },
        &key(),
        None,
        resource_request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_RESOURCE_SCOPE");

    let mut no_review = FakeResources::ready();
    let error = save_resource(
        &mut no_review,
        &PlanningActor {
            permissions: vec!["records.read".into(), "records.write".into()],
            ..actor()
        },
        &key(),
        None,
        resource_request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PLANNING_REVIEW_REQUIRED");
    assert!(no_review.resource.borrow().is_none());
}

#[test]
fn resource_schema_rejects_oversized_and_duplicate_invalid_weekdays() {
    for body in [
        serde_json::json!({"branchId":"branch","code":"C","name":"Cell","dailyMinutes":480}),
        serde_json::json!({"branchId":"branch","code":"CELL","name":"Cell","dailyMinutes":0}),
        serde_json::json!({"branchId":"branch","code":"CELL","name":"Cell","dailyMinutes":480,"workWeekdays":[]}),
        serde_json::json!({"branchId":"branch","code":"CELL","name":"Cell","dailyMinutes":480,"workWeekdays":[7]}),
        serde_json::json!({"branchId":"branch","code":"CELL","name":"Cell","dailyMinutes":480,"workWeekdays":[1],"status":"retired"}),
    ] {
        let request: ResourceRequest = serde_json::from_value(body).unwrap();
        assert!(request.validate().is_err());
    }
}
