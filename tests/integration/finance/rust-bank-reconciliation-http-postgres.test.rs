//! Native Finance bank HTTP import/reconcile under the actual Account session.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::finance_bank_http::{FinanceBankTarget, finance_bank_router};
use erpx_service::finance_journal_create::FinanceJournalCreateConfiguration;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use std::sync::Arc;
use tower::ServiceExt;

async fn seed(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0),\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('bank-supplier','company','supplier','SUP-B','Synthetic Supplier','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('bank-journal','company','chart','period','J-BANK','2026-01-01','THB','posted','fixture-a','fixture-a',0,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,version,created_by,created_at,updated_at)VALUES\
         ('bank-payment','company','bank-supplier','receipt','RC-BANK','2026-01-01','THB',1500,1500,'bank','released','bank-journal',3,'fixture-a',0,0)\
         ON CONFLICT DO NOTHING;",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    router_for(p, false)
}
fn router_for(p: &review::Prepared, two_companies: bool) -> axum::Router {
    let policy = review::account::fixture::account::policy();
    let registry = ConfiguredHoldingRegistry::new(
        &policy,
        vec![
            ConfiguredAccountHolding {
                holding_id: HoldingId::new("h001").unwrap(),
                slug: "acme".into(),
                write_binding: "HOLDING_H001_DB".into(),
                read_binding: "HOLDING_H001_READ_DB".into(),
                files_binding: "HOLDING_H001_FILES".into(),
            },
            ConfiguredAccountHolding {
                holding_id: HoldingId::new("h002").unwrap(),
                slug: "northstar".into(),
                write_binding: "HOLDING_H002_DB".into(),
                read_binding: "HOLDING_H002_READ_DB".into(),
                files_binding: "HOLDING_H002_FILES".into(),
            },
        ],
    )
    .unwrap();
    let configuration = FinanceJournalCreateConfiguration::new(
        FinanceJournalEnvironment::Development,
        HoldingId::new("h001").unwrap(),
        CompanyId::new("company").unwrap(),
        Arc::new(current_package_catalogue().unwrap()),
    )
    .unwrap();
    let mut targets=vec![FinanceBankTarget {
        host:"acme.example.test".into(),holding_slug:"acme".into(),authorizer:p.authorizer.clone(),configuration,clock:p.system.scenario.clock.clone(),
    }];
    if two_companies {targets.push(FinanceBankTarget {host:"acme.example.test".into(),holding_slug:"acme".into(),authorizer:p.authorizer.clone(),configuration:FinanceJournalCreateConfiguration::new(FinanceJournalEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("foreign").unwrap(),Arc::new(current_package_catalogue().unwrap())).unwrap(),clock:p.system.scenario.clock.clone()});}
    finance_bank_router(
        Arc::new(policy),
        Arc::new(registry),
        targets,
    )
    .unwrap()
}

#[tokio::test]
async fn bank_http_imports_and_reconciles_under_normal_finance_session() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let app = router(&p);
        let cookie = format!("__Host-erpx-product={}", p.maker_token.expose());
        let import = r#"{"bankAccount":"bank","statementNumber":"ST-HTTP","statementDate":"2026-01-31","opening":"0.00","closing":"15.00","closingFxRate":"1.0","sourceHash":"http-source","rows":[{"externalId":"HTTP-1","transactionDate":"2026-01-05","description":"Settlement","amount":"15.00","currency":"THB"}]}"#;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/bank/statements/generic-import")
            .header("host", "acme.example.test")
            .header("cookie", &cookie)
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "bank-http-import")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(import))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["id"], "statement-bank-http-import");
        assert_eq!(value["data"]["imported"], 1);
        assert_eq!(value["data"]["status"], "imported");

        let reconcile = r#"{"expectedVersion":1,"matchedType":"payment","matchedId":"bank-payment","proof":"Reviewed the statement row against the released receipt.","attachmentSetIds":[]}"#;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/bank/transactions/bankrow-bank-http-import-0/reconcile")
            .header("host", "acme.example.test")
            .header("cookie", &cookie)
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "bank-http-reconcile")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(reconcile))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["status"], "matched");
        assert_eq!(value["data"]["version"], 2);
        let matched: (String, String, i64) = sqlx::query_as(
            "SELECT match_status,matched_id,version FROM finance_bank_transactions WHERE id='bankrow-bank-http-import-0'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(matched, ("matched".into(), "bank-payment".into(), 2));
    })
    .await;
}

#[tokio::test]
async fn bank_http_two_companies_preserve_auth_and_write_isolation() {
    review::account::with_system(|system| async move {
        let p=review::setup_with_finance(system,true).await;seed(&p).await;
        let db=p.system.holdings[0].raw();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('finance-foreign-b','fixture-b','custom_finance_review','[\"finance\"]','foreign',0,9000000000000,3,'holding'); UPDATE active_work_contexts SET company_id='foreign',assignment_id='finance-foreign-b',assignment_version=3,revision=7 WHERE user_id='fixture-b'; INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('foreign-chart','foreign','Foreign','THB','fixture-b',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at) VALUES('foreign-bank','foreign','foreign-chart','1000','Foreign bank','asset','THB',1,'bank','active',1,0,0)").execute(db).await.unwrap();
        let mut tx=db.begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) SELECT 'foreign',package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at FROM company_package_grants WHERE company_id='company' AND package_id='finance'").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'foreign-finance-grant',company_id,package_id,'fixture-b','foreign-fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),updated_at FROM company_package_grants WHERE company_id='foreign' AND package_id='finance'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let app=router_for(&p,true);
        let make=|token:Option<&str>,company:&str,assignment:&str,revision:&str,key:&str,bank:&str|{let body=serde_json::json!({"bankAccount":bank,"statementNumber":key,"statementDate":"2026-01-31","opening":"0.00","closing":"15.00","closingFxRate":"1.0","sourceHash":key,"rows":[{"externalId":key,"transactionDate":"2026-01-05","description":"Settlement","amount":"15.00","currency":"THB"}]});let mut b=Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/bank/statements/generic-import").header("host","acme.example.test").header("origin","https://acme.example.test").header("x-erpx-context-company",company).header("x-erpx-context-assignment",assignment).header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version","3").header("idempotency-key",key);if let Some(t)=token{b=b.header("cookie",format!("__Host-erpx-product={t}"));}b.body(Body::from(body.to_string())).unwrap()};
        let a=p.maker_token.expose();let b=p.reviewer_token.expose();
        for (token,company,assignment,key,bank) in [(a,"company","finance-fixture-a","bank-company","bank"),(b,"foreign","finance-foreign-b","bank-foreign","foreign-bank")] {let response=app.clone().oneshot(make(Some(token),company,assignment,"7",key,bank)).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let replay=app.clone().oneshot(make(Some(token),company,assignment,"7",key,bank)).await.unwrap();assert_eq!(replay.status(),StatusCode::CREATED);assert_eq!(to_bytes(replay.into_body(),65536).await.unwrap(),bytes);}
        let rows:Vec<(String,String)>=sqlx::query_as("SELECT id,company_id FROM finance_bank_statements WHERE id IN ('statement-bank-company','statement-bank-foreign') ORDER BY id").fetch_all(db).await.unwrap();assert_eq!(rows,vec![("statement-bank-company".into(),"company".into()),("statement-bank-foreign".into(),"foreign".into())]);
        for key in ["bank-company","bank-foreign"] {let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_bank_transactions WHERE statement_id='statement-'||$1),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db).await.unwrap();assert_eq!(counts,(1,1,1,1),"{key}");}
        let snapshot_sql="SELECT jsonb_build_array((SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM finance_bank_statements x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM finance_bank_transactions x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.command_id),'[]') FROM command_receipts x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM audit_events x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM outbox_events x))";
        let before:serde_json::Value=sqlx::query_scalar(snapshot_sql).fetch_one(db).await.unwrap();
        let mut missing=make(Some(a),"company","finance-fixture-a","7","bank-missing","bank");missing.headers_mut().remove("x-erpx-context-company");
        let mut duplicate=make(Some(a),"company","finance-fixture-a","7","bank-duplicate","bank");duplicate.headers_mut().append("x-erpx-context-company","foreign".parse().unwrap());
        for (label,request,expected,code) in [("unauthenticated",make(None,"unknown","finance-fixture-a","7","bank-no-auth","bank"),StatusCode::UNAUTHORIZED,"UNAUTHENTICATED"),("foreign selection without actor role",make(Some(a),"foreign","finance-fixture-a","7","bank-forged","foreign-bank"),StatusCode::CONFLICT,"WORK_ROLE_REQUIRED"),("stale revision",make(Some(a),"company","finance-fixture-a","6","bank-stale","bank"),StatusCode::CONFLICT,"WORK_CONTEXT_CHANGED"),("missing selection",missing,StatusCode::CONFLICT,"WORK_COMPANY_REQUIRED"),("duplicate selection",duplicate,StatusCode::UNPROCESSABLE_ENTITY,"AMBIGUOUS_REQUEST_AUTHORITY"),("foreign account",make(Some(a),"company","finance-fixture-a","7","bank-cross-reference","foreign-bank"),StatusCode::UNPROCESSABLE_ENTITY,"BANK_ACCOUNT_INVALID")] {let response=app.clone().oneshot(request).await.unwrap();let status=response.status();let body=to_bytes(response.into_body(),65536).await.unwrap();let v:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(status,expected,"{label}: {v}");assert_eq!(v["error"]["code"],code,"{label}: {v}");}
        let after:serde_json::Value=sqlx::query_scalar(snapshot_sql).fetch_one(db).await.unwrap();assert_eq!(after,before,"rejected commands must leave statements, transactions, receipts, audit and outbox unchanged");
    }).await;
}
