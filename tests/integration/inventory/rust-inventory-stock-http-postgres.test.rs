//! Real Account/Holding session HTTP reads for the native Inventory stock
//! register and receipt source. These cases prove routing, cookie/CAS handling
//! and owner-scoped data; they do not prove browser behavior or Commerce writes.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId};
use erpx_service::inventory_stock_read::InventoryStockRegisterService;
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
) -> (review::Prepared, InventoryStockRegisterService<TestClock>) {
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
    let session: String =
        sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id='fixture-a'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('inventory-fixture-a','fixture-a','custom_inventory_reader','[\"inventory\"]','company',0,$1,3,'holding') ON CONFLICT DO NOTHING")
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
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','sales',1,'active',0,$1,1,'Synthetic reviewed inventory purchase','public synthetic evidence',$2) ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'inventory-stock-http-origin-grant','company','sales','fixture-a','synthetic-request',NULL,jsonb_build_object('companyId','company','packageId','sales','packageVersion',1,'state','active','startsAt',0,'endsAt',$1,'version',1,'reason','Synthetic reviewed inventory purchase','evidence','public synthetic evidence'),$2 ON CONFLICT DO NOTHING")
        .bind(grant_expires)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','branch','Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('http-wh','company','branch','HTTP-WH','HTTP warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type)VALUES('http-product','company','HTTP-P','HTTP product','pcs',2500,0,'active',1,'lot','stock') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version)VALUES('http-wh','http-product',2000,2000,1) ON CONFLICT DO NOTHING;\
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at)VALUES('http-lot','company','http-product','lot','HTTP-LOT','fixture-a',0) ON CONFLICT DO NOTHING;\
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('http-lot','http-wh',2000,2000) ON CONFLICT DO NOTHING;\
         INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,created_at,updated_at)VALUES('http-count','company','branch','http-wh','HTTP-CNT','2026-11-04','HTTP count','draft',1,0,'fixture-a','fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('http-supplier','company','supplier','HTTP-SUP','HTTP Supplier','THB',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES('http-order','company','branch','purchase','HTTP-PO','http-supplier','http-wh','THB','2026-11-01','2026-11-10',2500,0,'confirmed',3,'fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES('http-line','http-order','http-product','HTTP product',2000,0,2500,0,2500,0,1,'stock') ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    let service = InventoryStockRegisterService {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: Arc::new(TestClock(p.system.scenario.clock.clone())),
    };
    (p, service)
}

#[tokio::test]
async fn stock_register_and_receipt_source_http_accept_a_real_session() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
    use erpx_service::inventory_stock_http::{
        InventoryStockHttpTarget, inventory_stock_http_router,
    };
    use tower::ServiceExt;
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system).await;
        let router = inventory_stock_http_router(
            Arc::new(review::account::fixture::account::policy()),
            Arc::new(FieldServiceReadScopeHeaders),
            vec![InventoryStockHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service: Arc::new(service),
            }],
        )
        .unwrap();
        let request = |uri: &str| {
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("host", "acme.example.test")
                .header(
                    "cookie",
                    format!("__Host-erpx-product={}", p.maker_token.expose()),
                )
                .header("origin", "https://acme.example.test")
                .header("x-erpx-context-company", "company")
                .header("x-erpx-context-assignment", "inventory-fixture-a")
                .header("x-erpx-context-revision", "7")
                .header("x-erpx-context-assignment-version", "3")
                .body(Body::empty())
                .unwrap()
        };
        let register = router
            .clone()
            .oneshot(request("/api/v1/holdings/acme/inventory/stock-register"))
            .await
            .unwrap();
        assert_eq!(register.status(), 200);
        let body = to_bytes(register.into_body(), 256 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["context"]["companyId"], "company");
        assert_eq!(value["data"]["currency"], "THB");
        assert_eq!(value["data"]["stock"][0]["sku"], "HTTP-P");
        assert_eq!(value["data"]["lots"][0]["lotCode"], "HTTP-LOT");
        assert_eq!(value["data"]["counts"][0]["id"], "http-count");
        assert_eq!(value["data"]["totals"]["consistent"], true);

        let branch = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/holdings/acme/inventory/stock-register")
                    .header("host", "acme.example.test")
                    .header(
                        "cookie",
                        format!("__Host-erpx-product={}", p.maker_token.expose()),
                    )
                    .header("origin", "https://acme.example.test")
                    .header("x-erpx-context-company", "company")
                    .header("x-erpx-context-assignment", "inventory-fixture-a")
                    .header("x-erpx-context-revision", "7")
                    .header("x-erpx-context-assignment-version", "3")
                    .header("x-erpx-workspace-branch", "branch")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(branch.status(), 200);
        let body = to_bytes(branch.into_body(), 256 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["stock"][0]["warehouseId"], "http-wh");
        assert_eq!(value["data"]["lots"][0]["warehouseId"], "http-wh");

        let receipt = router
            .clone()
            .oneshot(request(
                "/api/v1/holdings/acme/inventory/receipts/http-order",
            ))
            .await
            .unwrap();
        assert_eq!(receipt.status(), 200);
        let body = to_bytes(receipt.into_body(), 256 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["order"]["number"], "HTTP-PO");
        assert_eq!(value["data"]["lines"][0]["remainingMilli"], 2_000);
        assert_eq!(value["data"]["lines"][0]["lots"][0]["lotCode"], "HTTP-LOT");

        let foreign_company = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/receipts/http-order")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "other-company")
            .header("x-erpx-context-assignment", "inventory-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router
                .clone()
                .oneshot(foreign_company)
                .await
                .unwrap()
                .status(),
            409
        );

        let missing_order = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/receipts/missing-order")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "inventory-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router
                .clone()
                .oneshot(missing_order)
                .await
                .unwrap()
                .status(),
            404
        );

        let wrong_origin = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/stock-register")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://evil.example.test")
            .header("x-erpx-context-company", "company")
            .body(Body::empty())
            .unwrap();
        assert_eq!(router.oneshot(wrong_origin).await.unwrap().status(), 403);
    })
    .await;
}
