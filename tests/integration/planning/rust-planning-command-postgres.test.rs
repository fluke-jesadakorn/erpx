//! Native Planning command persistence on migrated PostgreSQL.
//!
//! Proves owner SQL, serializable transaction guards, receipt/audit/outbox
//! writes, conflict and release decisions and the cross-owner Manufacturing
//! release port. It does not prove the product route merge or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_planning::public::planning::PlanningActor;
use erpx_server_planning::public::resources::{
    CalendarRequest, PgPlanningResources, ResourceRequest, save_calendar, save_resource,
};
use erpx_server_planning::public::scenario::{
    CalculateRequest, CancelRequest, ManufacturingOrderPlan, PgPlanningScenarios,
    PlanningReleaseFactory, PlanningReleasePort, ReleaseFuture, ReviewRequest, calculate_scenario,
    cancel_scenario, review_recommendation,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(user: &str) -> PlanningActor {
    PlanningActor {
        user_id: user.into(),
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

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

async fn setup(pool: &DatabasePool) {
    let root = std::env::var("ERPX_REPO_ROOT").map_or_else(
        |_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        |root| std::path::PathBuf::from(root).join("migrations/postgres"),
    );
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','planner','local:planner','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-planner','planner','holding_owner','[]','company','branch',0,9000000000000,NULL,'holding','company','manual'),\
         ('assignment-reviewer','reviewer','holding_owner','[]','company','branch',0,9000000000000,NULL,'holding','company','manual');\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('output','company','OUT','Bounded output','each',1000,0,'active',1),\
         ('material','company','MAT','Material','each',100,0,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1);\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES \
         ('warehouse','material',1000,1000,1);\
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('specification','company','Bounded output','bom','output',1000,10000,'draft','maker',NULL,1,100,100);\
         INSERT INTO manufacturing_specification_lines(id,specification_id,product_id,quantity_milli,substitution_group,ordinal) VALUES \
         ('line','specification','material',2000,NULL,1);\
         INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at) VALUES \
         ('resource','company','branch','CELL','Assembly cell',480,'[1,2,3,4,5]','active',1,'planner',10,10);\
         INSERT INTO manufacturing_specification_operations(id,specification_id,resource_id,name,setup_minutes,run_minutes_per_batch,ordinal) VALUES \
         ('operation','specification','resource','Cut',0,100,1);\
         UPDATE manufacturing_specifications SET status='approved',reviewed_by='reviewer',version=2,updated_at=100 WHERE id='specification';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn request() -> CalculateRequest {
    serde_json::from_value(serde_json::json!({
        "warehouseId": "warehouse",
        "specificationId": "specification",
        "specificationVersion": 2,
        "demandQuantity": "2",
        "dueDate": "2026-10-01",
        "capacityMinutes": 0
    }))
    .unwrap()
}

async fn calculate(pool: &DatabasePool, key_value: &str) -> Value {
    let key = key(key_value);
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            Box::pin(async move {
                let mut repository = PgPlanningScenarios::new(connection);
                calculate_scenario(&mut repository, &actor("planner"), &key, request(), NOW).await
            })
        })
        .await
        .unwrap()
}

struct TestRelease;

static RELEASE: TestRelease = TestRelease;

struct TestPort<'a> {
    connection: &'a mut PgConnection,
}

impl PlanningReleasePort for TestPort<'_> {
    fn release<'a>(&'a mut self, plan: &'a ManufacturingOrderPlan) -> ReleaseFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO manufacturing_orders\
                 (id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,\
                 specification_version,planned_quantity_milli,status,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,'planned',$9,$10,$10)",
            )
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(&plan.branch_id)
            .bind(&plan.warehouse_id)
            .bind(&plan.recommendation_id)
            .bind(&plan.specification_id)
            .bind(plan.specification_version)
            .bind(plan.planned_quantity_milli)
            .bind(&plan.user_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO manufacturing_order_events\
                 (id,order_id,version,action,actor_id,reason,status,details_json,created_at) \
                 VALUES($1,$2,1,'release',$3,$4,'planned','{}',$5)",
            )
            .bind(&plan.event_id)
            .bind(&plan.order_id)
            .bind(&plan.user_id)
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}

impl PlanningReleaseFactory for TestRelease {
    fn create<'a>(&'a self, connection: &'a mut PgConnection) -> Box<dyn PlanningReleasePort + 'a> {
        Box::new(TestPort { connection })
    }
}

#[tokio::test]
async fn calculate_cancel_and_release_persist_owner_rows_receipts_and_manufacturing_order() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let calculated = calculate(&pool, "planning-key-calc-0001").await;
        assert_eq!(calculated["id"], "plan-planning-key-calc-0001");
        assert_eq!(calculated["status"], "calculated");
        assert_eq!(calculated["shortages"][0]["requiredMilli"], 4_000);
        assert_eq!(calculated["shortages"][0]["availableMilli"], 1_000);
        assert_eq!(calculated["shortages"][0]["shortageMilli"], 3_000);
        assert_eq!(calculated["requiredMinutes"], 200);
        assert_eq!(calculated["capacityMinutes"], 200);
        assert_eq!(
            calculated["context"]["capacityBasis"],
            "approved_routing_resource_calendar"
        );
        assert_eq!(calculated["context"]["allocations"][0]["minutes"], 200);
        assert_eq!(
            calculated["context"]["allocations"][0]["workDate"],
            "2026-10-01"
        );

        let scenario_count: i64 = sqlx::query_scalar("SELECT count(*) FROM planning_scenarios")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let outbox: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!((scenario_count, receipts, audits, outbox), (1, 1, 1, 1));

        // Lost-response replay returns the receipt without writing again.
        let replay = calculate(&pool, "planning-key-calc-0001").await;
        assert_eq!(replay["id"], calculated["id"]);
        let scenarios: i64 = sqlx::query_scalar("SELECT count(*) FROM planning_scenarios")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(scenarios, 1);

        // Cancel the current calculated scenario.
        let cancel: CancelRequest = serde_json::from_value(serde_json::json!({
            "action": "cancel",
            "expectedVersion": 1,
            "reason": "Replaced by a newer plan"
        }))
        .unwrap();
        let cancelled = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let cancel = cancel.clone();
                Box::pin(async move {
                    let mut repository = PgPlanningScenarios::new(connection);
                    cancel_scenario(
                        &mut repository,
                        &actor("planner"),
                        &key("planning-key-cancel-0001"),
                        "plan-planning-key-calc-0001",
                        cancel,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(cancelled["status"], "cancelled");
        assert_eq!(cancelled["version"], 2);
        let status: String =
            sqlx::query_scalar("SELECT status FROM planning_scenarios WHERE id='plan-planning-key-calc-0001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(status, "cancelled");
        let rejected: String = sqlx::query_scalar(
            "SELECT status FROM planning_recommendations WHERE scenario_id='plan-planning-key-calc-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rejected, "rejected");

        // A fresh scenario is approved by the independent reviewer.
        let second = calculate(&pool, "planning-key-calc-0002").await;
        assert_eq!(second["status"], "calculated");
        let recommendation_id = second["recommendationId"].as_str().unwrap().to_owned();
        let approve: ReviewRequest = serde_json::from_value(serde_json::json!({
            "action": "approve",
            "expectedVersion": 1,
            "reason": "Independent capacity review"
        }))
        .unwrap();
        let review_id = recommendation_id.clone();
        let approved = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let approve = approve.clone();
                let review_id = review_id.clone();
                Box::pin(async move {
                    let mut repository = PgPlanningScenarios::with_release(connection, &RELEASE);
                    review_recommendation(
                        &mut repository,
                        &actor("reviewer"),
                        &key("planning-key-review-0001"),
                        &review_id,
                        approve,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(approved["status"], "approved");
        assert_eq!(approved["version"], 2);
        assert_eq!(
            approved["orderId"],
            "mfg-order-planning-key-review-0001"
        );
        let scenario_state: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM planning_scenarios WHERE id='plan-planning-key-calc-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(scenario_state, ("released".to_owned(), 2));
        let order: (String, i64) = sqlx::query_as(
            "SELECT status,planned_quantity_milli FROM manufacturing_orders WHERE recommendation_id=$1",
        )
        .bind(&recommendation_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(order, ("planned".to_owned(), 2_000));
        let allocations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM planning_resource_allocations WHERE scenario_id='plan-planning-key-calc-0002' \
             AND released_at IS NULL",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(allocations, 1);
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM planning_scenario_events WHERE scenario_id='plan-planning-key-calc-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 4);

        // A stale approval fails closed and writes nothing new.
        let stale: ReviewRequest = serde_json::from_value(serde_json::json!({
            "action": "approve",
            "expectedVersion": 1,
            "reason": "Stale review"
        }))
        .unwrap();
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let stale = stale.clone();
                Box::pin(async move {
                    let mut repository = PgPlanningScenarios::with_release(connection, &RELEASE);
                    review_recommendation(
                        &mut repository,
                        &actor("reviewer"),
                        &key("planning-key-review-0002"),
                        "plan-recommendation-planning-key-calc-0002",
                        stale,
                        NOW,
                    )
                    .await
                    .map(|_| ())
                })
            })
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "CONFLICT"),
            other => panic!("expected the conflict, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn resource_and_calendar_commands_persist_versions_and_receipts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let created = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut repository = PgPlanningResources::new(connection);
                    let request: ResourceRequest = serde_json::from_value(serde_json::json!({
                        "branchId": "branch",
                        "code": "PRESS",
                        "name": "Press cell",
                        "dailyMinutes": 240,
                        "workWeekdays": [1, 2, 3, 4, 5]
                    }))
                    .unwrap();
                    save_resource(
                        &mut repository,
                        &actor("planner"),
                        &key("planning-key-resource-0001"),
                        None,
                        request,
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(created["version"], 1);
        assert_eq!(created["status"], "active");
        let resource_id = created["id"].as_str().unwrap().to_owned();
        assert_eq!(
            resource_id,
            "production-resource-planning-key-resource-0001"
        );

        let calendar = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let resource_id = resource_id.clone();
                move |connection| {
                    let resource_id = resource_id.clone();
                    Box::pin(async move {
                        let mut repository = PgPlanningResources::new(connection);
                        let request: CalendarRequest = serde_json::from_value(serde_json::json!({
                            "expectedVersion": 0,
                            "resourceVersion": 1,
                            "workDate": "2026-10-04",
                            "availableMinutes": 60,
                            "reason": "Verified shutdown"
                        }))
                        .unwrap();
                        save_calendar(
                            &mut repository,
                            &actor("planner"),
                            &key("planning-key-calendar-0001"),
                            &resource_id,
                            request,
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap();
        assert_eq!(calendar["version"], 1);
        assert_eq!(calendar["workDate"], "2026-10-04");
        let stored: (i64, String) = sqlx::query_as(
            "SELECT available_minutes,reason FROM production_resource_calendar \
             WHERE resource_id=$1 AND work_date='2026-10-04'",
        )
        .bind(&resource_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, (60, "Verified shutdown".to_owned()));
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 2);

        // A resource with an active allocation cannot be revised.
        sqlx::query(
            "INSERT INTO planning_scenarios\
             (id,company_id,branch_id,warehouse_id,specification_id,specification_version,\
             demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,\
             version,created_at,updated_at,context_json) \
             VALUES('scenario-allocated','company','branch','warehouse','specification',2,2000,\
             '2026-10-01',0,'[]','calculated','planner',1,1,1,'{}')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO planning_resource_allocations\
             (id,scenario_id,resource_id,work_date,minutes,created_by,created_at) \
             VALUES('allocation-allocated','scenario-allocated',$1,'2026-10-04',30,'planner',1)",
        )
        .bind(&resource_id)
        .execute(pool.raw())
        .await
        .unwrap();
        let update: ResourceRequest = serde_json::from_value(serde_json::json!({
            "expectedVersion": 1,
            "branchId": "branch",
            "code": "PRESS",
            "name": "Press cell",
            "dailyMinutes": 300,
            "workWeekdays": [1, 2, 3, 4, 5]
        }))
        .unwrap();
        let error = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), {
                let resource_id = resource_id.clone();
                move |connection| {
                    let resource_id = resource_id.clone();
                    let update = update.clone();
                    Box::pin(async move {
                        let mut repository = PgPlanningResources::new(connection);
                        save_resource(
                            &mut repository,
                            &actor("planner"),
                            &key("planning-key-resource-0002"),
                            Some(&resource_id),
                            update,
                            NOW,
                        )
                        .await
                        .map(|_| ())
                    })
                }
            })
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "PLANNING_RESOURCE_ALLOCATED")
            }
            other => panic!("expected the allocation conflict, got {other:?}"),
        }
    })
    .await;
}
