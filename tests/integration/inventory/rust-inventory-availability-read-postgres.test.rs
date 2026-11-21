//! Actual Inventory availability read over leased PostgreSQL, current
//! Account/IAM authority, the rendered-context CAS and the Inventory purchase.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId};
use erpx_service::inventory_availability_read::InventoryAvailabilityReadService;
use std::sync::Arc;

#[derive(Clone)]
struct TestClock(Arc<review::account::fixture::MutableClock>);
impl Clock for TestClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        self.0.now()
    }
}

fn selection(assignment: &str) -> HoldingWorkspaceRequest {
    HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new(assignment).unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}

async fn inventory_setup(
    system: review::account::System,
    with_purchase: bool,
) -> (review::Prepared, InventoryAvailabilityReadService<TestClock>) {
    let p = review::setup_with_finance(system, false).await;
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    let grant_expires = now + 30 * 24 * 60 * 60 * 1_000;
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_inventory_reader','Synthetic Inventory reader',false,1,'active') ON CONFLICT DO NOTHING;\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_inventory_reader',1,'[\"records.read\",\"inventory.read\",\"commerce.read\",\"finance.read\",\"module.inventory.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at)VALUES('person-a','fixture-a','company','Synthetic Agent','TH','active',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at)VALUES('engagement-a','person-a','company','employee','salaried','active','2026-01-01',NULL,'Inventory','THB',1,'synthetic-fixture',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','person-a','active','local',1) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    let session: String = sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id='fixture-a'")
        .fetch_one(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('inventory-fixture-a','fixture-a','custom_inventory_reader','[\"inventory\"]','company',0,$1,3,'holding')")
        .bind(9_007_199_254_740_991_i64)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("UPDATE active_work_contexts SET assignment_id='inventory-fixture-a',assignment_version=3,revision=7,expires_at=$2,updated_at=$3 WHERE session_id=$1")
        .bind(&session)
        .bind(expires)
        .bind(now)
        .execute(pool.raw())
        .await
        .unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    if with_purchase {
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','sales',1,'active',0,$1,1,'Synthetic reviewed inventory purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'inventory-availability-origin-grant','company','sales','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','sales','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed inventory purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let service = InventoryAvailabilityReadService {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
    };
    (p, service)
}

async fn seed_stock(pool: &DatabasePool, _now: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','inventory','Inventory branch','company','company','branch','active'),('branch-two','branch','inventory-two','Second branch','company','company','branch-two','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('inv-wh','company','branch','INV-WH','Main warehouse','active',1),('inv-wh-two','company','branch-two','INV-WH-2','Second warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type)VALUES('inv-product','company','INV-P','Inventory product','pcs',1000,0,'active',1,'none','stock') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version)VALUES('inv-wh','inv-product',10000,1000,1),('inv-wh-two','inv-product',4000,400,1) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('inv-customer','company','customer','INV-C','Inventory customer','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES('inv-order','company','branch','sales','SO-INV-1','inv-customer','inv-wh','THB','2026-11-01','2026-11-10',3000,0,'confirmed',1,'fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES('inv-line','inv-order','inv-product','Reserved line',3000,0,1000,0,3000,0,1,'stock') ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query("INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES('inv-res-active','inv-line','inv-wh','inv-product',3000,0,0,$1,'fixture-a','Active dispatch commitment',0) ON CONFLICT DO NOTHING")
        .bind(now + 3_600_000)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES('inv-res-expired','inv-line','inv-wh','inv-product',1000,0,0,$1,'fixture-a','Expired commitment',0) ON CONFLICT DO NOTHING")
        .bind(now - 1)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,lot_id,held_quantity_milli,version,created_at,updated_at)VALUES('inv-hold','company','branch','inv-wh','inv-product','Quarantine for inspection','2026-11-10','held','fixture-a',NULL,2000,1,0,0) ON CONFLICT DO NOTHING")
        .execute(pool.raw())
        .await
        .unwrap();
    let chart: String = sqlx::query_scalar(
        "SELECT id FROM finance_charts WHERE company_id='company' ORDER BY id LIMIT 1",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    let account: String = sqlx::query_scalar(
        "SELECT id FROM finance_accounts WHERE company_id='company' ORDER BY id LIMIT 1",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    sqlx::query("INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,status,version,total_value_minor,created_by,prepared_by,approved_by,delivery_mode,created_at,updated_at)VALUES('inv-transfer','company','inv-wh-two','inv-wh','branch-two','branch',1,1,'TR-INV-1','2026-11-01','Stock transfer for availability','posted',1,150,'fixture-a','fixture-a','fixture-b','transit',0,0) ON CONFLICT DO NOTHING")
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_transit(id,transfer_id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,status,due_date,currency,chart_id,inventory_account_id,created_by,approved_by,dispatch_date,version,created_at,updated_at)VALUES('inv-transit','inv-transfer','company','inv-wh-two','inv-wh','branch-two','branch','in_transit','2026-11-10','THB',$1,$2,'fixture-a','fixture-b','2026-11-01',1,0,0) ON CONFLICT DO NOTHING")
        .bind(&chart)
        .bind(&account)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_transfer_lines(id,transfer_id,product_id,product_version,description,unit,quantity_milli,value_minor,source_quantity_milli,source_value_minor,source_version,ordinal)VALUES('inv-transfer-line','inv-transfer','inv-product',1,'Stock transfer line','pcs',1500,150,1500,150,1,1) ON CONFLICT DO NOTHING")
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_transit_lines(id,transit_id,source_line_id,product_id,quantity_milli,value_minor,received_milli,lost_milli,settled_value_minor)VALUES('inv-transit-line','inv-transit','inv-transfer-line','inv-product',1500,150,0,0,0) ON CONFLICT DO NOTHING")
        .execute(pool.raw())
        .await
        .unwrap();
}

#[tokio::test]
async fn inventory_availability_reads_current_owner_facts_and_scope() {
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system, true).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_stock(pool, now).await;
        let result = service
            .read(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("inventory-fixture-a")),
            )
            .await;
        let result = match result {
            Ok(value) => value,
            Err(error) => panic!("availability read failed: {error:?}"),
        };
        assert_eq!(result.context.company_id, "company");
        assert_eq!(result.data.len(), 2);
        let main = result
            .data
            .iter()
            .find(|row| row.warehouse_id == "inv-wh")
            .unwrap();
        assert_eq!(main.quantity_milli, 10_000);
        assert_eq!(main.reserved_milli, 3_000);
        // A whole-stock quality hold quarantines every unit, so the larger of
        // reservation or quarantine removes all available stock.
        assert_eq!(main.quarantined_milli, 10_000);
        assert_eq!(main.in_transit_milli, 1_500);
        assert_eq!(main.available_milli(), 0);
        assert_eq!(main.value_minor, 1_000);
        assert!(main.version >= 1);
        let second = result
            .data
            .iter()
            .find(|row| row.warehouse_id == "inv-wh-two")
            .unwrap();
        assert_eq!(second.available_milli(), 4_000);
        assert_eq!(second.in_transit_milli, 0);

        // Branch scope is resolved by the owner, not a caller label.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let scoped = erpx_server_inventory::public::availability_read::read_availability(
            &mut tx,
            "company",
            Some("branch-two"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].warehouse_id, "inv-wh-two");

        // Company isolation: another Company sees no rows even with the same IDs.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let foreign = erpx_server_inventory::public::availability_read::read_availability(
            &mut tx,
            "another-company",
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(foreign.is_empty());
    })
    .await;
}

#[tokio::test]
async fn inventory_availability_requires_current_context_and_purchase() {
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_stock(pool, now).await;

        // A stale rendered-context CAS fails before any read.
        match service
            .read(
                "acme.example.test",
                p.maker.clone(),
                || {
                    let mut request = selection("inventory-fixture-a");
                    request.expected = Some(ExpectedWorkSelection {
                        revision: 99,
                        assignment_id: AssignmentId::new("inventory-fixture-a").unwrap(),
                        assignment_version: 3,
                    });
                    Ok(request)
                },
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(error.kind, erpx_server_shared::public::FailureKind::Conflict));
            }
            other => panic!("stale context must fail: {other:?}"),
        }

        // Without the reviewed package purchase the read fails closed.
        match service
            .read(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("inventory-fixture-a")),
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "PACKAGE_REQUIRED");
            }
            other => panic!("missing purchase must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn inventory_availability_http_route_accepts_real_session_and_rendered_context() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::inventory_availability_http::{
        InventoryAvailabilityHttpTarget, inventory_availability_http_router,
    };
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system, true).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        seed_stock(pool, now).await;
        let router = inventory_availability_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![InventoryAvailabilityHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service: Arc::new(service),
            }],
        )
        .unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/availability")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "inventory-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, 200);
        assert_eq!(value["context"]["companyId"], "company");
        assert_eq!(value["data"].as_array().unwrap().len(), 2);
        let rows = value["data"].as_array().unwrap();
        let main = rows
            .iter()
            .find(|row| row["warehouseId"] == "inv-wh")
            .unwrap();
        assert_eq!(main["quarantinedMilli"], 10_000);
        assert_eq!(main["availableMilli"], 0);
        let second = rows
            .iter()
            .find(|row| row["warehouseId"] == "inv-wh-two")
            .unwrap();
        assert_eq!(second["availableMilli"], 4_000);

        let wrong_origin = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/availability")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://evil.example.test")
            .header("x-erpx-context-company", "company")
            .body(Body::empty())
            .unwrap();
        let denied = router.oneshot(wrong_origin).await.unwrap();
        assert_eq!(denied.status(), 403);
    })
    .await;
}
