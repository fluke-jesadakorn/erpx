//! Payroll history over actual Account/IAM and native HTTP on leased databases.
#[allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
use erpx_platform_shared_server::public::{Clock, CompanyId, HoldingId};
use erpx_people_payroll_server::public::payroll_summary_http::{PayrollReadHttpTarget, people_payroll_router};
use erpx_platform_composition::{field_service_reservation_read_http::FieldServiceReadScopeHeaders, people_payroll::{PeoplePayrollBackend, PeoplePayrollConfiguration, PeoplePayrollService}};
use std::sync::Arc;
use tower::ServiceExt;

async fn get(router: &axum::Router, cookie: Option<&str>, query: &str, branch: bool) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder().method("GET").uri(format!("/api/v1/holdings/acme/people/payroll/runs-page{query}"))
        .header("host", "acme.example.test");
    if let Some(value) = cookie { request = request.header("cookie", format!("__Host-erpx-product={value}")); }
    if branch { request = request.header("x-erpx-workspace-branch", "branch"); }
    let response = router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 512 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn history_http_uses_current_account_and_company_payroll_authority() {
    assert_eq!(std::env::var("ERPX_PEOPLE_PACKAGE_ENFORCEMENT").unwrap_or_else(|_| "disabled".into()), "disabled", "run this case in a disabled-mode process");
    review::account::with_system(|system| async move {
        let p = review::setup_without_finance_references(system).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('branch','branch','branch','Branch','company','company','branch','active');
          INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','history_reader','Payroll reader',false,1,'active'),('*','payment_reader','Payment reader',false,1,'active');
          INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','history_reader',1,'[\"payroll.read\",\"module.people_payroll.read\"]','[]','[\"holding\",\"branch\"]','fixture',0),('*','payment_reader',1,'[\"payroll.payment.read\",\"module.people_payroll.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);
          UPDATE assignments SET role='history_reader',modules_json='[\"people_payroll\"]' WHERE id='finance-fixture-a';
          UPDATE assignments SET role='payment_reader',modules_json='[\"people_payroll\"]' WHERE id='finance-fixture-b';
          INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES('group','company','Monthly','TH','THB','monthly','Asia/Bangkok','fixture-a',0,0),('foreign-group','foreign','Foreign','US','USD','monthly','UTC','other',0,0);
          INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) SELECT 'run-'||lpad(n::text,2,'0'),'company','group','2026-10-01','2026-10-31','2026-10-31','THB','preview','draft',n,'{}',1,'fixture-a',5,5 FROM generate_series(1,31) n;
          INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,totals_json,version,created_by,created_at,updated_at) VALUES('foreign-run','foreign','foreign-group','2026-10-01','2026-10-31','2026-10-31','USD','preview','draft',1,'{}',1,'other',5,5)")
            .execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','people',1,'active',0,$1,1,'Synthetic purchase','Fixture evidence',$2),('company','people-payroll',1,'active',0,$1,1,'Synthetic purchase','Fixture evidence',$2)")
            .bind(now+3_600_000).bind(now).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'grant-'||package_id,'company',package_id,'fixture-a','fixture-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id IN ('people','people-payroll')")
            .bind(now).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let policy = Arc::new(review::account::fixture::account::policy());
        let scopes = Arc::new(FieldServiceReadScopeHeaders);
        let service = Arc::new(PeoplePayrollService::new(p.authorizer.clone(), PeoplePayrollConfiguration::new(HoldingId::new("h001").unwrap(), CompanyId::new("company").unwrap()), p.system.scenario.clock.clone()));
        let backend = Arc::new(PeoplePayrollBackend::new(policy, scopes, service));
        let router = people_payroll_router(vec![PayrollReadHttpTarget {host:"acme.example.test".into(), holding_slug:"acme".into(), origin:"https://acme.example.test".into(), backend}]).unwrap();
        let (status, first) = get(&router, Some(p.maker_token.expose()), "", false).await;
        assert_eq!(status, StatusCode::OK, "actual payroll reader: {first}");
        assert_eq!(first["data"]["items"].as_array().unwrap().len(), 30);
        assert_eq!(first["data"]["items"][0]["id"], "run-31");
        assert_eq!(first["data"]["items"][0]["company_id"], "company");
        assert_eq!(first["context"]["userId"], "fixture-a");
        let cursor = first["data"]["nextCursor"].as_str().unwrap();
        let (status, second) = get(&router, Some(p.maker_token.expose()), &format!("?cursor={cursor}"), false).await;
        assert_eq!(status, StatusCode::OK, "next page: {second}");
        assert_eq!(second["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(second["data"]["items"][0]["id"], "run-01");
        assert!(second["data"]["nextCursor"].is_null());
        let (status, invalid) = get(&router, Some(p.maker_token.expose()), "?cursor=invalid", false).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "invalid cursor: {invalid}");
        assert_eq!(invalid["error"]["code"], "INVALID_CURSOR");
        let (status, payment_only) = get(&router, Some(p.reviewer_token.expose()), "", false).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "payment-only actor: {payment_only}");
        assert_eq!(payment_only["error"]["code"], "PERMISSION_REQUIRED");
        let (status, scoped) = get(&router, Some(p.maker_token.expose()), "", true).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "branch scope: {scoped}");
        assert_eq!(scoped["error"]["code"], "COMPANY_PAYROLL_SCOPE_REQUIRED");
        let (status, _) = get(&router, None, "", false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }).await;
}
