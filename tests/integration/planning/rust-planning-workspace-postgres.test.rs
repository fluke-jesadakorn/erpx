//! Native Planning workspace reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company/branch isolation, serializable
//! transaction guards, rollback and persisted-source readback. They do not
//! prove a planning command, capacity reservation, recommendation release,
//! cross-owner Manufacturing/Inventory/Finance/Projects effect or browser
//! journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_manufacturing::public::planning::{
    PgPlanningWorkspaceRead, PlanningActor, PlanningWorkspace, planning_workspace,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use sqlx::Connection;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor(company: &str, branch: Option<&str>) -> PlanningActor {
    PlanningActor {
        user_id: "planner".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        permissions: vec!["records.read".into()],
        modules: vec!["planning".into()],
        expires_at: NOW.get() + 60_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('branch-b','branch','branch-b','Branch B',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('output','company','OUT','Bounded output','each',1000,0,'active',1),\
         ('material','company','MAT','Material','each',100,0,'active',1),\
         ('foreign-output','foreign','FOUT','Foreign output','each',1000,0,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('warehouse-b','company','branch-b','WHB','Warehouse B','active',1),\
         ('warehouse-f','foreign','foreign-branch','WHF','Foreign warehouse','active',1);\
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('specification','company','Bounded output','bom','output',10000,10000,'approved','maker','reviewer',2,100,100),\
         ('specification-b','company','Bounded output B','bom','output',10000,10000,'approved','maker','reviewer',2,100,100),\
         ('specification-f','foreign','Foreign output','bom','foreign-output',10000,10000,'approved','maker','reviewer',2,100,100);\
         INSERT INTO production_resources(id,company_id,branch_id,code,name,daily_minutes,work_weekdays_json,status,version,created_by,created_at,updated_at) VALUES \
         ('resource','company','branch','CELL','Assembly cell',480,'[1,2,3,4,5]','active',1,'planner',10,20),\
         ('resource-b','company','branch-b','CELLB','Assembly cell B',240,'[1,2,3,4,5]','active',1,'planner',10,20),\
         ('resource-f','foreign','foreign-branch','CELLF','Foreign cell',480,'[1,2,3,4,5]','active',1,'planner',10,20);\
         INSERT INTO production_resource_calendar(resource_id,work_date,available_minutes,reason,version,updated_by,updated_at) VALUES \
         ('resource','2026-10-04',120,'Verified shutdown',1,'planner',20),\
         ('resource-b','2026-10-04',60,'Branch B shutdown',1,'planner',20),\
         ('resource-f','2026-10-04',90,'Foreign shutdown',1,'planner',20);\
         INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json) VALUES \
         ('scenario','company','branch','warehouse','specification',2,2000,'2026-10-04',120,\
          '[{\"productId\":\"material\",\"requiredMilli\":4000,\"availableMilli\":1000,\"incomingMilli\":2000,\"shortageMilli\":1000}]',\
          'calculated','planner',1,100,300,\
          '{\"capacityBasis\":\"approved_routing_resource_calendar\",\"comparison\":{\"scenarioId\":\"scenario-b\",\"sourceVersion\":1,\"shortageDeltaMilli\":-500,\"requiredMinutesDelta\":30,\"feasibleDate\":\"2026-10-05\",\"currentFeasibleDate\":\"2026-10-04\"}}'),\
         ('scenario-b','company','branch-b','warehouse-b','specification-b',2,3000,'2026-10-05',60,'[]','calculated','planner',1,100,200,'{}'),\
         ('scenario-f','foreign','foreign-branch','warehouse-f','specification-f',2,1000,'2026-10-06',30,'[]','calculated','planner',1,100,100,'{}');\
         INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('recommendation','scenario',1,2000,60,'2026-10-04','proposed','planner',NULL,1,100,300),\
         ('recommendation-b','scenario-b',1,3000,30,'2026-10-05','approved','planner','reviewer',2,100,200),\
         ('recommendation-f','scenario-f',1,1000,15,'2026-10-06','proposed','planner',NULL,1,100,100);\
         INSERT INTO planning_resource_allocations(id,scenario_id,resource_id,work_date,minutes,created_by,created_at,released_at) VALUES \
         ('allocation','scenario','resource','2026-10-04',60,'planner',150,NULL),\
         ('allocation-released','scenario','resource','2026-10-05',30,'planner',150,200),\
         ('allocation-b','scenario-b','resource-b','2026-10-04',20,'planner',150,NULL),\
         ('allocation-f','scenario-f','resource-f','2026-10-04',10,'planner',150,NULL);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read(
    pool: &DatabasePool,
    actor: PlanningActor,
) -> Result<PlanningWorkspace, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut repository = PgPlanningWorkspaceRead::new(connection);
                planning_workspace(&actor, &mut repository, NOW).await
            })
        })
        .await
}

#[tokio::test]
async fn reads_current_branch_workspace_without_writing_owner_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let result = read(&pool, actor("company", Some("branch"))).await.unwrap();
        assert_eq!(
            result
                .scenarios
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["scenario"]
        );
        let scenario = &result.scenarios[0];
        assert_eq!(scenario.company_id, "company");
        assert_eq!(scenario.branch_id, "branch");
        assert_eq!(scenario.specification_version, 2);
        assert_eq!(scenario.demand_quantity_milli, 2_000);
        assert_eq!(scenario.shortages.len(), 1);
        assert_eq!(scenario.shortages[0].product_id, "material");
        assert_eq!(scenario.shortages[0].shortage_milli, 1_000);
        assert_eq!(
            scenario.context.capacity_basis.as_deref(),
            Some("approved_routing_resource_calendar")
        );
        assert_eq!(
            scenario
                .context
                .comparison
                .as_ref()
                .unwrap()
                .shortage_delta_milli,
            -500
        );

        assert_eq!(result.recommendations.len(), 1);
        assert_eq!(result.recommendations[0].id, "recommendation");
        assert_eq!(result.recommendations[0].status, "proposed");
        assert_eq!(result.recommendations[0].feasible_date, "2026-10-04");
        assert_eq!(result.recommendations[0].reviewed_by, None);

        assert_eq!(
            result
                .resources
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["resource"]
        );
        assert_eq!(result.resources[0].work_weekdays, vec![1, 2, 3, 4, 5]);
        assert_eq!(result.calendar.len(), 1);
        assert_eq!(result.calendar[0].available_minutes, 120);
        assert_eq!(result.allocations.len(), 1);
        assert_eq!(result.allocations[0].id, "allocation");

        let scenarios: i64 = sqlx::query_scalar("SELECT count(*) FROM planning_scenarios")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let recommendations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM planning_recommendations")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let resources: i64 = sqlx::query_scalar("SELECT count(*) FROM production_resources")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let calendar: i64 = sqlx::query_scalar("SELECT count(*) FROM production_resource_calendar")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let allocations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM planning_resource_allocations")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            (scenarios, recommendations, resources, calendar, allocations),
            (3, 3, 3, 3, 4)
        );
    })
    .await;
}

#[tokio::test]
async fn company_wide_read_excludes_foreign_company() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let result = read(&pool, actor("company", None)).await.unwrap();
        assert_eq!(
            result
                .scenarios
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["scenario", "scenario-b"]
        );
        assert!(
            result
                .scenarios
                .iter()
                .all(|row| row.company_id == "company")
        );
        assert_eq!(result.recommendations.len(), 2);
        assert_eq!(
            result
                .resources
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["resource", "resource-b"]
        );
        assert_eq!(result.calendar.len(), 2);
        assert_eq!(result.allocations.len(), 2);

        let foreign = read(&pool, actor("foreign", Some("foreign-branch")))
            .await
            .unwrap();
        assert_eq!(foreign.scenarios.len(), 1);
        assert_eq!(foreign.scenarios[0].id, "scenario-f");
        assert_eq!(foreign.resources[0].id, "resource-f");
        assert_eq!(foreign.recommendations[0].id, "recommendation-f");
    })
    .await;
}

#[tokio::test]
async fn read_requires_a_serializable_caller_transaction() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = PgPlanningWorkspaceRead::new(&mut connection)
            .workspace(&actor("company", Some("branch")))
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "PLANNING_WORKSPACE_TRANSACTION_REQUIRED")
            }
            other => panic!("expected a transaction guard, got {other:?}"),
        }
        drop(connection);

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut transaction = connection
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await
            .unwrap();
        let error = PgPlanningWorkspaceRead::new(&mut transaction)
            .workspace(&actor("company", Some("branch")))
            .await
            .unwrap_err();
        match error {
            PgTransactionError::Persistence(error) => {
                assert_eq!(error.code, "PLANNING_WORKSPACE_SERIALIZABLE_REQUIRED")
            }
            other => panic!("expected a serializable guard, got {other:?}"),
        }
        transaction.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn policy_and_rollback_leave_only_persisted_source_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut denied = actor("company", Some("branch"));
        denied.permissions.clear();
        match read(&pool, denied).await {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.kind, FailureKind::Forbidden);
                assert_eq!(error.code, "PLANNING_PERMISSION_REQUIRED");
            }
            other => panic!("expected the policy failure, got {other:?}"),
        }

        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json) VALUES \
                         ('scenario-uncommitted','company','branch','warehouse','specification',2,1000,'2026-10-07',0,'[]','calculated','planner',1,500,500,'{}');",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut repository = PgPlanningWorkspaceRead::new(connection);
                    let current =
                        planning_workspace(&actor("company", Some("branch")), &mut repository, NOW)
                            .await?;
                    assert!(
                        current
                            .scenarios
                            .iter()
                            .any(|row| row.id == "scenario-uncommitted")
                    );
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PLANNING_TEST_LATE_FAILURE",
                        "Late failure after the owner read.",
                    )
                    .into())
                })
            })
            .await;
        match result {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "PLANNING_TEST_LATE_FAILURE")
            }
            other => panic!("expected the controlled late failure, got {other:?}"),
        }

        let uncommitted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM planning_scenarios WHERE id='scenario-uncommitted'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(uncommitted, 0);
        let readback = read(&pool, actor("company", Some("branch"))).await.unwrap();
        assert_eq!(readback.scenarios.len(), 1);
        assert_eq!(readback.scenarios[0].id, "scenario");
        assert_eq!(readback.allocations.len(), 1);
    })
    .await;
}
