//! Actual Inventory movement-history read over leased PostgreSQL, current
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


async fn seed_masters(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','inventory','Inventory branch','company','company','branch','active'),('branch-two','branch','inventory-two','Second branch','company','company','branch-two','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('inv-wh','company','branch','INV-WH','Main warehouse','active',1),('inv-wh-two','company','branch-two','INV-WH-2','Second warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type)VALUES('inv-product','company','INV-P','Inventory product','pcs',1000,0,'active',1,'none','stock') ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_movement(pool: &DatabasePool, id: &str, warehouse: &str, kind: &str, created_at: i64) {
    sqlx::query("INSERT INTO pos_stock_moves(id,company_id,warehouse_id,product_id,source_id,source_number,kind,quantity_milli,value_minor,movement_date,created_at)VALUES($1,'company',$2,'inv-product',$3,$4,$5,1000,100,'2026-11-04',$6) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(warehouse)
        .bind(format!("source-{id}"))
        .bind(format!("NUMBER-{id}"))
        .bind(kind)
        .bind(created_at)
        .execute(pool.raw())
        .await
        .unwrap();
}

#[tokio::test]
async fn inventory_movements_read_all_sources_and_paginate() {
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system, true).await;
        let pool = &p.system.holdings[0];
        seed_masters(pool).await;
        seed_movement(pool, "move-sale", "inv-wh", "sale", 100).await;
        seed_movement(pool, "move-return", "inv-wh", "return", 200).await;
        seed_movement(pool, "move-kitchen", "inv-wh-two", "kitchen", 300).await;

        let result = service
            .read_movements(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("inventory-fixture-a")),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.data.len(), 3);
        assert_eq!(result.data[0].id, "move-kitchen");
        assert_eq!(result.data[0].source_type, "pos_kitchen");
        assert_eq!(result.data[0].order_number, "NUMBER-move-kitchen");
        assert_eq!(result.data[1].source_type, "pos_return");
        assert_eq!(result.data[2].source_type, "pos_sale");
        assert!(result.next_cursor.is_none());

        // Branch scope is resolved by the owner.
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let scoped = erpx_server_inventory::public::movement_read::read_movements(
            &mut tx, "company", Some("branch-two"), None,
        ).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(scoped.0.len(), 1);
        assert_eq!(scoped.0[0].warehouse_id, "inv-wh-two");

        // Fifty rows plus one overflow row produce a next cursor.
        for index in 0..51 {
            seed_movement(pool, &format!("move-page-{index}"), "inv-wh", "sale", 1_000 + index).await;
        }
        let first = service
            .read_movements(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("inventory-fixture-a")),
                None,
            )
            .await
            .unwrap();
        assert_eq!(first.data.len(), 50);
        let cursor = first.next_cursor.clone().unwrap();
        let second = service
            .read_movements(
                "acme.example.test",
                p.maker.clone(),
                || Ok(selection("inventory-fixture-a")),
                Some(cursor),
            )
            .await
            .unwrap();
        assert_eq!(second.data.len(), 4);
        assert!(second.next_cursor.is_none());
    })
    .await;
}

#[tokio::test]
async fn inventory_movements_require_current_context_and_purchase() {
    review::account::with_system(|system| async move {
        let (p, service) = inventory_setup(system, true).await;
        let pool = &p.system.holdings[0];
        seed_masters(pool).await;
        seed_movement(pool, "move-auth", "inv-wh", "sale", 100).await;
        match service
            .read_movements(
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
                None,
            )
            .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert!(matches!(error.kind, erpx_server_shared::public::FailureKind::Conflict));
            }
            other => panic!("stale context must fail: {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn inventory_movements_http_route_accepts_real_session_and_rendered_context() {
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
        seed_masters(pool).await;
        seed_movement(pool, "move-http", "inv-wh", "sale", 100).await;
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
            .uri("/api/v1/holdings/acme/inventory/movements")
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
        assert_eq!(value["data"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"][0]["sourceType"], "pos_sale");
        assert_eq!(value["data"][0]["orderNumber"], "NUMBER-move-http");

        let invalid = Request::builder()
            .method("GET")
            .uri("/api/v1/holdings/acme/inventory/movements?cursor=not-a-cursor")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "inventory-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(router.oneshot(invalid).await.unwrap().status(), 422);
    })
    .await;
}
