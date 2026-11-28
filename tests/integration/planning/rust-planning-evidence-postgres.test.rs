//! Native Planning evidence SEARCH on migrated PostgreSQL.
//! Proves owner SQL, Company/branch isolation, the bounded query/cursor/limit,
//! the metric-exact date/status predicates, full label projection and the
//! composition-supplied eligible set. It does not prove a browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_planning_server::public::evidence::{
    CAPACITY_FEASIBLE_RECOMMENDATIONS, EvidenceActor, EvidenceEligibility, EvidenceQuery,
    PgEvidenceRead, PROPOSED_RECOMMENDATIONS, SHORTAGE_SCENARIOS, search_evidence,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;

fn actor(company: &str, branch: Option<&str>) -> EvidenceActor {
    EvidenceActor {
        user_id: "planner".into(),
        company_id: company.into(),
        branch_id: branch.map(ToOwned::to_owned),
        assignment_id: "assignment".into(),
        department_id: None,
        role: "planner".into(),
        modules: vec!["planning".into()],
        permissions: vec!["records.read".into()],
    }
}

fn query(metric: &str, limit: u32) -> EvidenceQuery {
    EvidenceQuery {
        metric: metric.into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        query: String::new(),
        cursor_id: None,
        limit,
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
         ('foreign-output','foreign','FOUT','Foreign output','each',1000,0,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1),\
         ('warehouse-b','company','branch-b','WHB','Warehouse B','active',1),\
         ('warehouse-f','foreign','foreign-branch','WHF','Foreign warehouse','active',1);\
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('specification','company','Spec','bom','output',10000,10000,'approved','maker','reviewer',2,100,100),\
         ('specification-b','company','Spec B','bom','output',10000,10000,'approved','maker','reviewer',2,100,100),\
         ('specification-f','foreign','Foreign Spec','bom','foreign-output',10000,10000,'approved','maker','reviewer',2,100,100);\
         INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json) VALUES \
         ('scenario','company','branch','warehouse','specification',2,2000,'2026-10-04',120,'[{\"productId\":\"material\",\"requiredMilli\":4000,\"availableMilli\":1000,\"incomingMilli\":2000,\"shortageMilli\":1000}]','calculated','planner',1,100,300,'{}'),\
         ('scenario-b','company','branch-b','warehouse-b','specification-b',2,3000,'2026-10-05',60,'[]','calculated','planner',1,100,200,'{}'),\
         ('scenario-f','foreign','foreign-branch','warehouse-f','specification-f',2,1000,'2026-10-06',30,'[]','calculated','planner',1,100,100,'{}');\
         INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('recommendation','scenario',1,2000,60,'2026-10-04','proposed','planner',NULL,1,100,300),\
         ('recommendation-b','scenario-b',1,3000,30,'2026-10-05','approved','planner','reviewer',2,100,200),\
         ('recommendation-f','scenario-f',1,1000,15,'2026-10-06','proposed','planner',NULL,1,100,100);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn search(
    pool: &DatabasePool,
    actor: EvidenceActor,
    eligibility: EvidenceEligibility,
    query: EvidenceQuery,
) -> Result<Vec<erpx_planning_server::public::evidence::EvidenceRow>, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let eligibility = eligibility.clone();
            let query = query.clone();
            Box::pin(async move {
                let mut repository = PgEvidenceRead::new(connection);
                search_evidence(&actor, &eligibility, &query, &mut repository)
                    .await
                    .map_err(Into::into)
            })
        })
        .await
}

#[tokio::test]
async fn scenario_search_is_scoped_bounded_and_fully_labelled() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let rows = search(
            &pool,
            actor("company", Some("branch")),
            EvidenceEligibility::All,
            query(SHORTAGE_SCENARIOS, 25),
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, "scenario");
        assert_eq!(row.scenario_id, "scenario");
        assert_eq!(row.branch_name.as_deref(), Some("Branch"));
        assert_eq!(row.warehouse_name.as_deref(), Some("Warehouse"));
        assert_eq!(row.specification_name.as_deref(), Some("Spec"));
        assert_eq!(row.output_product_name.as_deref(), Some("Bounded output"));
        assert_eq!(row.demand_quantity_milli, "2000");
        assert_eq!(row.shortage_count, "1");
        assert_eq!(row.scenario_version, 1);
        assert!(row.shortage_items_json.contains("shortageMilli"));

        // Company scope excludes the foreign company row.
        let all_branches = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::All,
            query(SHORTAGE_SCENARIOS, 25),
        )
        .await
        .unwrap();
        assert_eq!(all_branches.len(), 1);
        assert_eq!(all_branches[0].id, "scenario");

        // Bounded text query and limit+1 pagination.
        let mut filtered = query(SHORTAGE_SCENARIOS, 25);
        filtered.query = "spec".into();
        let matched = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::All,
            filtered.clone(),
        )
        .await
        .unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "scenario");
        let paged = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::All,
            query(CAPACITY_FEASIBLE_RECOMMENDATIONS, 1),
        )
        .await
        .unwrap();
        assert_eq!(paged.len(), 2);

        // The composition-supplied eligible set filters before paging.
        let eligible = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::Scenarios(vec!["scenario".into()]),
            query(SHORTAGE_SCENARIOS, 25),
        )
        .await
        .unwrap();
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].id, "scenario");
    })
    .await;
}

#[tokio::test]
async fn recommendation_metrics_use_exact_date_and_status_predicates() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let proposed = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::All,
            query(PROPOSED_RECOMMENDATIONS, 25),
        )
        .await
        .unwrap();
        assert_eq!(proposed.len(), 1);
        assert_eq!(proposed[0].id, "recommendation");
        assert_eq!(proposed[0].recommendation_status.as_deref(), Some("proposed"));
        assert_eq!(proposed[0].feasible_date.as_deref(), Some("2026-10-04"));

        let feasible = search(
            &pool,
            actor("company", None),
            EvidenceEligibility::All,
            query(CAPACITY_FEASIBLE_RECOMMENDATIONS, 25),
        )
        .await
        .unwrap();
        assert_eq!(feasible.len(), 2);
        assert!(feasible.iter().any(|row| row.id == "recommendation-b"));
    })
    .await;
}
