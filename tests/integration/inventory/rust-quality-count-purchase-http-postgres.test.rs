//! Real HTTP Account/IAM/Commercial tests for Quality Count purchase fallback.
//! These assert target behavior and intentionally fail until command integration.
#[path = "rust-quality-count-origin-postgres.test.rs"]
mod origin;
#[path = "../../support/finance/review-composition.rs"]
mod review;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_commercial_server::public::purchases::current_package_catalogue;
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{Clock, CompanyId, HoldingId};
use erpx_platform_composition::{
    field_service_reservation_read_http::FieldServiceReadScopeHeaders,
    inventory_count_commands::InventoryCountCommand,
    inventory_count_http::{InventoryCountHttpTarget, inventory_count_http_router},
};
use std::sync::Arc;
use tower::ServiceExt;

async fn grant(pool: &DatabasePool, package: &str, now: i64) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company',$1,1,'active',0,$2,1,'Fixture purchase','Fixture evidence',$3)")
        .bind(package).bind(now+3_600_000).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT $1,'company',package_id,'fixture-b','fixture-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id=$3")
        .bind(format!("grant-{package}")).bind(now).bind(package).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
async fn setup(
    system: review::account::System,
    packages: &[&str],
) -> (review::Prepared, axum::Router) {
    let p = review::setup_service_customer_without_finance_references(system).await;
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('branch','branch','branch','Branch','company','company','branch','active'),('other-branch','branch','other-branch','Other','company','company','other-branch','active');
       INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','company','branch','W1','Warehouse');
       INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','quality_count_reviewer','Quality Count reviewer',false,1,'active');
       INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','quality_count_reviewer',1,'[\"inventory.read\",\"inventory.prepare\",\"inventory.approve\",\"module.inventory.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);
       UPDATE assignments SET role='quality_count_reviewer',modules_json='[\"inventory\"]' WHERE id='finance-fixture-b'").execute(pool.raw()).await.unwrap();
    origin::seed_customer_return(pool).await;
    origin::seed_return_count(
        pool,
        "plain",
        "quality-return-approval-key-0",
        "plain-product",
        None,
    )
    .await;
    for package in packages {
        grant(pool, package, now).await;
    }
    let command = InventoryCountCommand {
        authorizer: p.authorizer.clone(),
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap(),
        catalogue: Arc::new(current_package_catalogue().unwrap()),
        clock: p.system.scenario.clock.clone(),
    };
    let router = inventory_count_http_router(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        vec![InventoryCountHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            command: Arc::new(command),
        }],
    )
    .unwrap();
    (p, router)
}
async fn deactivate_with_history(pool: &DatabasePool, package: &str, now: i64) {
    let mut tx = pool.raw().begin().await.unwrap();
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id=$1").bind(package).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1,updated_at=$2 WHERE company_id='company' AND package_id=$1").bind(package).bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT $1,'company',package_id,'fixture-b','fixture-change',$2,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$3 FROM company_package_grants WHERE company_id='company' AND package_id=$4").bind(format!("inactive-{package}")).bind(before).bind(now).bind(package).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
async fn setup_posting(pool: &DatabasePool) {
    sqlx::raw_sql("UPDATE finance_accounts SET currency='THB' WHERE id IN ('return-debit','return-credit');
        INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('count-variance','company','chart','5400','Count variance','expense','THB',0,0);
        INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('count-period','company','November','2026-11-01','2026-11-30',0);
        INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','return-debit','return-debit','return-credit','return-debit','return-debit','return-debit','return-debit','return-debit','return-debit')").execute(pool.raw()).await.unwrap();
}

async fn action(
    router: &axum::Router,
    p: &review::Prepared,
    action: &str,
    key: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/holdings/acme/stock-counts/quality-scrap-plain/{action}"
        ))
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header(
            "cookie",
            format!("__Host-erpx-product={}", p.reviewer_token.expose()),
        )
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-b")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .header("idempotency-key", key)
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"expectedVersion":1,"reason":"Independent Count review"}"#,
        ))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

#[tokio::test]
async fn http_count_purchase_sales_fallback_and_replay_require_current_actor() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &["sales"]).await;
        let (status, body) = action(&router, &p, "reject", "return-reject").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "actual customer-return substitute should authorize reject: {body}"
        );
        let now=p.system.scenario.clock.now().get();
        deactivate_with_history(&p.system.holdings[0],"sales",now).await;
        let history:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM company_package_history_rights WHERE company_id='company' AND package_id='sales' AND retained_at<=$1)").bind(now).fetch_one(p.system.holdings[0].raw()).await.unwrap();assert!(history,"actual retained sales history");
        let (status, replay) = action(&router, &p, "reject", "return-reject").await;
        assert_eq!(status, StatusCode::OK, "same History replay: {replay}");
        sqlx::query("UPDATE inventory_quality_holds SET source_id='forged' WHERE id='quality-return-approval-key-0'").execute(p.system.holdings[0].raw()).await.unwrap();
        assert_eq!(action(&router,&p,"reject","return-reject").await.0,StatusCode::FORBIDDEN,"History replay must revalidate the original return source");
        sqlx::query("UPDATE inventory_quality_holds SET source_id='plain-return-line' WHERE id='quality-return-approval-key-0'").execute(p.system.holdings[0].raw()).await.unwrap();
        let now = p.system.scenario.clock.now().get();
        sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='finance-fixture-b'")
            .bind(now)
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_ne!(
            action(&router, &p, "reject", "return-reject").await.0,
            StatusCode::OK,
            "replay must revalidate current actor"
        );
    })
    .await;
}

#[tokio::test]
async fn http_count_purchase_supply_grant_has_precedence() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &["supply"]).await;
        let (status, body) = action(&router, &p, "reject", "supply-reject").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Supply purchase must authorize independent Quality Count review: {body}"
        );
    })
    .await;
}

#[tokio::test]
async fn http_count_purchase_missing_and_forged_sources_do_not_authorize() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &[]).await;
        let (status, _) = action(&router, &p, "reject", "missing-grants").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='missing-grants'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
        let (status, _) = action(&router, &p, "post", "missing-post-grants").await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "Post requires purchased stock and Finance capability"
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='missing-post-grants'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
    review::account::with_system(|system|async move {
        let (p,router)=setup(system,&["sales"]).await;
        sqlx::query("UPDATE inventory_quality_holds SET source_id='forged' WHERE id='quality-return-approval-key-0'").execute(p.system.holdings[0].raw()).await.unwrap();
        let (status,_)=action(&router,&p,"reject","forged-return").await;assert_eq!(status,StatusCode::FORBIDDEN);
        let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='forged-return'").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(receipts,0);
    }).await;
}

#[tokio::test]
async fn http_count_purchase_new_work_does_not_use_return_fallback() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &["sales"]).await;
        let (status, _) = action(&router, &p, "submit", "return-submit").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='return-submit'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

#[tokio::test]
async fn http_count_purchase_sales_financial_post_and_history_replay() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &["sales"]).await;
        setup_posting(&p.system.holdings[0]).await;
        let (status, body) = action(&router, &p, "post", "return-post").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "actual return Post requires stock and finance.sales-posting: {body}"
        );
        assert!(
            body["data"]["journalId"].as_str().is_some(),
            "post must return its actual journal"
        );
        let now = p.system.scenario.clock.now().get();
        deactivate_with_history(&p.system.holdings[0], "sales", now).await;
        let (status, body) = action(&router, &p, "post", "return-post").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Post replay must use retained history for both stock and finance: {body}"
        );
    })
    .await;
}

// Exercise the mounted generic Count command while the Quality-specific path
// remains guarded. Both users are distinct account IDs for the same person.
async fn seed_reviewer_alias(pool: &DatabasePool, use_people_fallback: bool) {
    sqlx::raw_sql("UPDATE inventory_counts SET quality_hold_id=NULL WHERE id='quality-scrap-plain';
        INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('reviewer-person','fixture-b','company','Reviewer','TH','fixture-b',0,0);
        INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','maker','local:maker','reviewer-person','active','local') ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=EXCLUDED.person_id")
        .execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','fixture-b','local:fixture-b',$1,'active','local') ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=EXCLUDED.person_id")
        .bind(if use_people_fallback { None } else { Some("reviewer-person") })
        .execute(pool.raw()).await.unwrap();
}

#[tokio::test]
async fn http_count_reviewer_alias_and_people_fallback_are_denied_without_writes() {
    for fallback in [false, true] {
        review::account::with_system(move |system| async move {
            let (p, router) = setup(system, &["supply"]).await;
            let pool = &p.system.holdings[0];
            seed_reviewer_alias(pool, fallback).await;
            let key = if fallback { "alias-people" } else { "alias-member" };
            let (status, body) = action(&router, &p, "reject", key).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "same-person review must be denied (People fallback={fallback}): {body}");
            assert_eq!(body["error"]["code"], "COUNT_ACTION_FORBIDDEN", "must reach the Count reviewer predicate: {body}");
            let state: (String, i64) = sqlx::query_as("SELECT status,version::bigint FROM inventory_counts WHERE id='quality-scrap-plain'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(state, ("approved".into(), 1));
            let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1").bind(key).fetch_one(pool.raw()).await.unwrap();
            assert_eq!(receipts, 0);
        }).await;
    }
}

#[tokio::test]
async fn http_count_reviewer_alias_drift_rolls_back_entire_command() {
    review::account::with_system(|system| async move {
        let (p, router) = setup(system, &["supply"]).await;
        let pool = &p.system.holdings[0];
        seed_reviewer_alias(pool, true).await;
        // Change the actual IAM alias after Count UPDATE inside the same unit.
        sqlx::raw_sql("UPDATE iam_members SET person_id='different-person' WHERE company_id='company' AND user_id='maker';
            CREATE FUNCTION test_count_alias_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
              UPDATE iam_members SET person_id='reviewer-person' WHERE company_id='company' AND user_id='maker';
              RETURN NEW;
            END $$;
            CREATE TRIGGER test_count_alias_drift AFTER UPDATE OF status ON inventory_counts FOR EACH ROW WHEN (NEW.id='quality-scrap-plain') EXECUTE FUNCTION test_count_alias_drift()")
            .execute(pool.raw()).await.unwrap();
        let (status, body) = action(&router, &p, "reject", "alias-drift").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "post-admission alias drift must abort: {body}");
        assert_eq!(body["error"]["code"], "WORK_AUTHORITY_CHANGED", "must detect changed authority: {body}");
        let state: (String, i64) = sqlx::query_as("SELECT status,version::bigint FROM inventory_counts WHERE id='quality-scrap-plain'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(state, ("approved".into(), 1));
        let person: Option<String> = sqlx::query_scalar("SELECT person_id FROM iam_members WHERE company_id='company' AND user_id='maker'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(person.as_deref(), Some("different-person"));
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='alias-drift'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(receipts, 0);
        sqlx::raw_sql("DROP TRIGGER test_count_alias_drift ON inventory_counts; DROP FUNCTION test_count_alias_drift()")
            .execute(pool.raw()).await.unwrap();
        let (status, body) = action(&router, &p, "reject", "alias-drift").await;
        assert_eq!(status, StatusCode::OK, "independent reviewer must succeed without injected drift: {body}");
    }).await;
}
