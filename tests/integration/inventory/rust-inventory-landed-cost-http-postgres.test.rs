//! Native HTTP landed-cost authority with real Account/IAM and leased Postgres.
#[allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;

use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
use erpx_commercial_server::public::purchases::current_package_catalogue;
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{Clock, CompanyId, HoldingId};
use erpx_platform_composition::{
    field_service_reservation_read_http::FieldServiceReadScopeHeaders,
    inventory_landed_cost_commands::InventoryLandedCostCommand,
    inventory_owner_commands_http::{InventoryLandedCostHttpTarget, inventory_landed_cost_http_router},
};
use std::sync::Arc;
use tower::ServiceExt;

async fn seed_sources(pool: &DatabasePool, now: i64) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('branch','branch','branch','Branch','company','company','branch','active');
      INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','landed_finance','Landed finance',false,1,'active');
      INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','landed_finance',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"module.inventory.read\"]','[]','[\"holding\"]','fixture',0);
      UPDATE assignments SET role='landed_finance',modules_json='[\"finance\",\"inventory\"]' WHERE id IN ('finance-fixture-a','finance-fixture-b');
      INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','company','Primary','THB','fixture-a',0,0);
      INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES('period','company','FY26','2026-01-01','2026-12-31','open',0);
      INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('inventory','company','chart','100','Inventory','asset','THB',0,0),('freight','company','chart','500','Freight','expense','THB',0,0);
      INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','inventory','inventory','inventory','inventory','inventory','inventory','inventory','inventory','inventory');
      INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','company','branch','W1','Warehouse');
      INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('product','company','P1','Product',1000,'none');
      INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product',1000,1000);
      INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('supplier','company','supplier','S1','Supplier','THB',0,0);
      INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,memo,status,version,created_by,created_at,updated_at) VALUES('receipt-journal','company','chart','period','JV1','2026-11-01','THB','purchase_receipt','Receipt','posted',1,'fixture-a',0,0);
      INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('purchase','company','branch','purchase','PO1','supplier','warehouse','THB','2026-11-01','2026-11-02','confirmed',1000,0,1,'fixture-a',0,0);
      INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES('line','purchase','product','Product',1000,1000,1000,0,1000,0,1);
      INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,journal_id,version) VALUES('receipt','purchase','GR1','2026-11-01','fixture-a',0,'receipt-journal',1);
      INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES('receipt-line','receipt','line',1000,1000,1000,0);")
      .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','supply',1,'active',0,$1,1,'Fixture purchase','Fixture evidence',$2),('company','supply-advanced',1,'active',0,$1,1,'Fixture purchase','Fixture evidence',$2)")
        .bind(now + 3_600_000).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'grant-'||package_id,'company',package_id,'fixture-a','fixture-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id IN ('supply','supply-advanced')")
        .bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn post(router: &axum::Router, p: &review::Prepared, actor: &str, path: &str, key: &str, body: &str) -> (StatusCode, serde_json::Value) {
    let token = if actor == "fixture-a" { &p.maker_token } else { &p.reviewer_token };
    let request = Request::builder().method("POST").uri(path)
        .header("host", "acme.example.test").header("origin", "https://acme.example.test")
        .header("cookie", format!("__Host-erpx-product={}", token.expose()))
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", format!("finance-{actor}"))
        .header("x-erpx-context-revision", "7").header("x-erpx-context-assignment-version", "3")
        .header("idempotency-key", key).header("content-type", "application/json")
        .body(Body::from(body.to_owned())).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn linked_person_cannot_review_but_can_cancel_original_landed_cost() {
    for people_fallback in [false, true] {
        review::account::with_system(move |system| async move {
            let p = review::setup_without_finance_references(system).await;
            let pool = &p.system.holdings[0];
            seed_sources(pool, p.system.scenario.clock.now().get()).await;
            let command = InventoryLandedCostCommand {
                authorizer: p.authorizer.clone(), holding: HoldingId::new("h001").unwrap(),
                company: CompanyId::new("company").unwrap(),
                catalogue: Arc::new(current_package_catalogue().unwrap()),
                clock: p.system.scenario.clock.clone(),
            };
            let router = inventory_landed_cost_http_router(
                Arc::new(review::account::fixture::account::policy()), Arc::new(FieldServiceReadScopeHeaders),
                vec![InventoryLandedCostHttpTarget { host: "acme.example.test".into(), holding_slug: "acme".into(), command: Arc::new(command) }],
            ).unwrap();
            let create_body = r#"{"fulfillmentId":"receipt","costDate":"2026-11-02","description":"Freight invoice","allocationMethod":"quantity","amount":"3.00","creditAccountId":"freight"}"#;
            let (status, created) = post(&router, &p, "fixture-a", "/api/v1/holdings/acme/landed-costs", "create-landed-alias", create_body).await;
            assert_eq!(status, StatusCode::CREATED, "native creation: {created}");
            assert_eq!(created["data"], serde_json::json!({
                "id": "landed-create-landed-alias", "status": "submitted", "version": 1
            }));
            let (status, replay) = post(&router, &p, "fixture-a", "/api/v1/holdings/acme/landed-costs", "create-landed-alias", create_body).await;
            assert_eq!(status, StatusCode::CREATED, "native create replay: {replay}");
            assert_eq!(replay, created);
            let (status, conflict) = post(&router, &p, "fixture-a", "/api/v1/holdings/acme/landed-costs", "create-landed-alias", &create_body.replace("3.00", "4.00")).await;
            assert_eq!(status, StatusCode::CONFLICT, "changed input: {conflict}");
            assert_eq!(conflict["error"]["code"], "IDEMPOTENCY_CONFLICT");
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_landed_costs WHERE id='landed-create-landed-alias'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(count, 1);
            sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','fixture-b','company','Reviewer','TH','fixture-b',0,0);
              INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','fixture-a','local:fixture-a','person','active','local') ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=EXCLUDED.person_id")
                .execute(pool.raw()).await.unwrap();
            sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','fixture-b','local:fixture-b',$1,'active','local') ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=EXCLUDED.person_id")
                .bind(if people_fallback { None } else { Some("person") }).execute(pool.raw()).await.unwrap();
            let id = "landed-create-landed-alias";
            for action in ["approve", "reject", "post"] {
                let key = format!("alias-{action}");
                let (status, result) = post(&router, &p, "fixture-b", &format!("/api/v1/holdings/acme/landed-costs/{id}/{action}"), &key, r#"{"expectedVersion":1,"reason":"Independent freight review"}"#).await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{action}, fallback={people_fallback}: {result}");
                assert_eq!(result["error"]["code"], "INDEPENDENT_FINANCE_REQUIRED");
                let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1").bind(&key).fetch_one(pool.raw()).await.unwrap();
                assert_eq!(receipts, 0);
            }
            let state: (String,i64) = sqlx::query_as("SELECT status,version FROM inventory_landed_costs WHERE id=$1").bind(id).fetch_one(pool.raw()).await.unwrap();
            assert_eq!(state, ("submitted".into(),1));
            let (status, cancelled) = post(&router, &p, "fixture-b", &format!("/api/v1/holdings/acme/landed-costs/{id}/cancel"), "alias-cancel", r#"{"expectedVersion":1,"reason":"Cancel original freight"}"#).await;
            assert_eq!(status, StatusCode::OK, "same-person cancel: {cancelled}");
            assert_eq!(cancelled["data"], serde_json::json!({"id":id,"status":"cancelled","version":2,"journalId":null}));
            let (status, replay) = post(&router, &p, "fixture-b", &format!("/api/v1/holdings/acme/landed-costs/{id}/cancel"), "alias-cancel", r#"{"expectedVersion":1,"reason":"Cancel original freight"}"#).await;
            assert_eq!(status, StatusCode::OK, "native cancel replay: {replay}");
            assert_eq!(replay, cancelled);
        }).await;
    }
}
