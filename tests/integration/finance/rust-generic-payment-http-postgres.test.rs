//! Native generic payment create HTTP ingress, independent of Field receipt
//! configuration, under a normal Account session and Finance banking purchase.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::account_holding_bindings::{
    ConfiguredAccountHolding, ConfiguredHoldingRegistry,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::finance_generic_payment_http::{GenericPaymentTarget, finance_generic_payment_router};
use erpx_service::finance_journal_create::FinanceJournalCreateConfiguration;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use std::sync::Arc;
use tower::ServiceExt;

async fn seed(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0),\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('generic-supplier','company','supplier','SUP-G','Synthetic Supplier','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,source_type)VALUES\
         ('generic-bill','company','generic-supplier','payable','BILL-G','2026-01-02','2026-01-31','THB',10000,0,10000,10000,10000,10000,'bank','ap','posted',1,'fixture-a',0,0,'supplier_bill')\
         ON CONFLICT DO NOTHING;",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router { router_for(p, &["company"]) }

fn router_for(p: &review::Prepared, companies:&[&str]) -> axum::Router {
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
    let targets=companies.iter().map(|company| GenericPaymentTarget {
        host:"acme.example.test".into(),holding_slug:"acme".into(),authorizer:p.authorizer.clone(),
        configuration:FinanceJournalCreateConfiguration::new(FinanceJournalEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new(*company).unwrap(),Arc::new(current_package_catalogue().unwrap())).unwrap(),clock:p.system.scenario.clock.clone(),
    }).collect();
    finance_generic_payment_router(Arc::new(policy),Arc::new(registry),Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),targets).unwrap()
}

#[tokio::test]
async fn generic_payment_http_creates_a_draft_without_field_receipt_configuration() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/payments/generic")
            .header("host", "acme.example.test")
            .header("cookie", format!("__Host-erpx-product={}", p.maker_token.expose()))
            .header("origin", "https://acme.example.test")
            .header("idempotency-key", "generic-payment-http")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"kind":"payment","partyId":"generic-supplier","paymentNumber":"PAY-G-1","paymentDate":"2026-01-10","currency":"THB","amount":"100.00","bankAccount":"bank","settlementInvoiceId":"generic-bill"}"#,
            ))
            .unwrap();
        let response = router(&p).oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        assert_eq!(value["data"]["status"], "draft");
        let row: (String, String, String, i64, String) = sqlx::query_as(
            "SELECT kind,party_id,bank_account,amount_minor,status FROM finance_payments WHERE company_id='company' AND payment_number='PAY-G-1'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(row, ("payment".into(), "generic-supplier".into(), "bank".into(), 10000, "draft".into()));
        let (audit, outbox): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM audit_events WHERE command_id='generic-payment-http'),\
             (SELECT count(*)::bigint FROM outbox_events WHERE command_id='generic-payment-http')",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!((audit, outbox), (1, 1));
    })
    .await;
}

#[tokio::test]
async fn two_company_targets_require_normal_sessions_and_isolate_payment_effects() {
    review::account::with_system(|system| async move {
        let p=review::setup_with_finance(system,true).await;seed(&p).await;let db=&p.system.holdings[0];
        // The second selected context belongs to fixture-b's real SSO child.
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company-b','company','company-b','Company B',NULL,'company-b',NULL,'active'); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('finance-company-b','fixture-b','custom_finance_review','[\"finance\"]','company-b',0,9000000000000,1,'holding'); UPDATE active_work_contexts SET company_id='company-b',assignment_id='finance-company-b',assignment_version=1,revision=8 WHERE user_id='fixture-b'; INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart-b','company-b','Company B chart','THB','fixture-b',0,0); INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('period-b','company-b','January','2026-01-01','2026-01-31',0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at) VALUES('bank-b','company-b','chart-b','1000','Bank B','asset','THB',1,'bank','active',1,0,0),('ap-b','company-b','chart-b','2100','AP B','liability','THB',1,'payable','active',1,0,0); INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES('supplier-b','company-b','supplier','SUP-B','Supplier B','THB','active',1,0,0); INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,source_type) VALUES('bill-b','company-b','supplier-b','payable','BILL-B','2026-01-02','2026-01-31','THB',10000,0,10000,10000,10000,10000,'bank-b','ap-b','posted',1,'fixture-b',0,0,'supplier_bill'); INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('rate-b','company-b','THB','2026-01-01',2,'fixture-b',1); INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company-b','finance',1,'active',0,9000000000000,1,'Reviewed Company B purchase','Synthetic Company B evidence',0); INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'finance-origin-grant-b','company-b','finance','fixture-b','synthetic-request-b',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),0 FROM company_package_grants WHERE company_id='company-b' AND package_id='finance'").execute(db.raw()).await.unwrap();
        let app=router_for(&p,&["company","company-b"]);
        let make=|token:Option<&str>,company:Option<&str>,assignment:&str,revision:&str,version:&str,key:&str,party:&str,bank:&str,invoice:&str|{
            let mut b=Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/payments/generic").header("host","acme.example.test").header("origin","https://acme.example.test").header("idempotency-key",key).header("x-erpx-context-assignment",assignment).header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version",version).header("content-type","application/json");
            if let Some(token)=token {b=b.header("cookie",format!("__Host-erpx-product={token}"));}if let Some(company)=company {b=b.header("x-erpx-context-company",company);}
            b.body(Body::from(serde_json::json!({"kind":"payment","partyId":party,"paymentNumber":format!("PAY-{key}"),"paymentDate":"2026-01-10","currency":"THB","amount":"100.00","bankAccount":bank,"settlementInvoiceId":invoice}).to_string())).unwrap()
        };
        let a=p.maker_token.expose();let b=p.reviewer_token.expose();
        let mut duplicate=make(None,Some("company"),"finance-fixture-a","7","3","missing-duplicate","generic-supplier","bank","generic-bill");duplicate.headers_mut().append("x-erpx-context-company","company-b".parse().unwrap());
        for request in [make(None,None,"finance-fixture-a","7","3","missing-company","generic-supplier","bank","generic-bill"),make(None,Some("unknown"),"finance-fixture-a","7","3","missing-unknown","generic-supplier","bank","generic-bill"),duplicate] {
            let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
        }
        for (token,company,assignment,revision,version,key,party,bank,invoice) in [(a,"company","finance-fixture-a","7","3","payment-company-a","generic-supplier","bank","generic-bill"),(b,"company-b","finance-company-b","8","1","payment-company-b","supplier-b","bank-b","bill-b")] {
            let response=app.clone().oneshot(make(Some(token),Some(company),assignment,revision,version,key,party,bank,invoice)).await.unwrap();let status=response.status();let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(status,StatusCode::CREATED,"{value}");
            let replay=app.clone().oneshot(make(Some(token),Some(company),assignment,revision,version,key,party,bank,invoice)).await.unwrap();assert_eq!(replay.status(),StatusCode::CREATED);let replay:serde_json::Value=serde_json::from_slice(&to_bytes(replay.into_body(),65536).await.unwrap()).unwrap();assert_eq!(replay,value);
            let row:(String,String,String,String,i64,String)=sqlx::query_as("SELECT company_id,party_id,bank_account,payment_number,amount_minor,status FROM finance_payments WHERE payment_number=$1").bind(format!("PAY-{key}")).fetch_one(db.raw()).await.unwrap();assert_eq!(row,(company.into(),party.into(),bank.into(),format!("PAY-{key}"),10000,"draft".into()));
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(1,1,1));
            let authority:(String,String,String)=sqlx::query_as("SELECT r.actor_id,r.authority_json::jsonb->>'companyId',a.authority_json::jsonb->>'companyId' FROM command_receipts r JOIN audit_events a ON a.command_id=r.command_id WHERE r.command_id=$1").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(authority,((if company=="company"{"fixture-a"}else{"fixture-b"}).into(),company.into(),company.into()));
        }
        let mut duplicate=make(Some(a),Some("company"),"finance-fixture-a","7","3","duplicate-company","generic-supplier","bank","generic-bill");duplicate.headers_mut().append("x-erpx-context-company","company-b".parse().unwrap());
        for (request,status,code) in [(make(Some(a),None,"finance-fixture-a","7","3","selected-missing","generic-supplier","bank","generic-bill"),StatusCode::CONFLICT,"WORK_COMPANY_REQUIRED"),(make(Some(a),Some("unknown"),"finance-fixture-a","7","3","selected-unknown","generic-supplier","bank","generic-bill"),StatusCode::CONFLICT,"WORK_CONTEXT_CHANGED"),(make(Some(a),Some("company-b"),"finance-company-b","8","1","selected-forged","supplier-b","bank-b","bill-b"),StatusCode::CONFLICT,"WORK_ROLE_REQUIRED"),(make(Some(a),Some("company"),"finance-fixture-a","6","3","selected-stale","generic-supplier","bank","generic-bill"),StatusCode::CONFLICT,"WORK_CONTEXT_CHANGED"),(duplicate,StatusCode::UNPROCESSABLE_ENTITY,"AMBIGUOUS_REQUEST_AUTHORITY")] {let response=app.clone().oneshot(request).await.unwrap();let actual=response.status();let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(actual,status,"expected {code}: {value}");assert_eq!(value["error"]["code"],code,"expected {code}: {value}");}
        let foreign_snapshot_sql="SELECT jsonb_build_object('invoice',(SELECT to_jsonb(i) FROM finance_invoices i WHERE i.company_id='company-b' AND i.id='bill-b'),'payment',(SELECT to_jsonb(p) FROM finance_payments p WHERE p.company_id='company-b' AND p.payment_number='PAY-payment-company-b'),'money',COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id='company-b'),'[]'::jsonb),'receipts',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.command_id) FROM command_receipts r WHERE r.command_id='payment-company-b'),'[]'::jsonb),'audits',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a WHERE a.command_id='payment-company-b'),'[]'::jsonb),'outbox',COALESCE((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM outbox_events o WHERE o.command_id='payment-company-b'),'[]'::jsonb))";
        let foreign_before:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(foreign_snapshot_sql)).fetch_one(db.raw()).await.unwrap();
        let response=app.clone().oneshot(make(Some(a),Some("company"),"finance-fixture-a","7","3","foreign-reference","supplier-b","bank-b","bill-b")).await.unwrap();
        let status=response.status();let denied:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY,"{denied}");assert_eq!(denied["error"]["code"],"PAYMENT_SETTLEMENT_SOURCE_INVALID");
        let foreign_after:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(foreign_snapshot_sql)).fetch_one(db.raw()).await.unwrap();assert_eq!(foreign_after,foreign_before,"foreign invoice, payment, Money and effects must remain unchanged");
        // Inject an allowed source mutation from a deferred outbox trigger,
        // after admission and owner effects. The owner retained-image guard is
        // evaluated before the final authority predicate and may fail first.
        let drift_snapshot_sql="SELECT jsonb_build_object('invoice',(SELECT to_jsonb(i) FROM finance_invoices i WHERE i.company_id='company' AND i.id='generic-bill'),'payments',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM finance_payments p WHERE p.company_id='company'),'[]'::jsonb),'money',COALESCE((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id='company'),'[]'::jsonb),'receipts',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.command_id) FROM command_receipts r),'[]'::jsonb),'audits',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a),'[]'::jsonb),'outbox',COALESCE((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM outbox_events o),'[]'::jsonb))";
        let drift_before:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(drift_snapshot_sql)).fetch_one(db.raw()).await.unwrap();
        sqlx::raw_sql("CREATE SEQUENCE generic_payment_drift_calls; CREATE FUNCTION generic_payment_drift_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='payment-invoice-drift' THEN PERFORM nextval('generic_payment_drift_calls'); UPDATE finance_invoices SET status='voided',updated_at=updated_at+1 WHERE id='generic-bill' AND company_id='company'; END IF; RETURN NEW; END; $$; CREATE CONSTRAINT TRIGGER generic_payment_drift_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION generic_payment_drift_fault()").execute(db.raw()).await.unwrap();
        let response=app.clone().oneshot(make(Some(a),Some("company"),"finance-fixture-a","7","3","payment-invoice-drift","generic-supplier","bank","generic-bill")).await.unwrap();let status=response.status();let denied:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(status,StatusCode::CONFLICT,"post-admission drift: {denied}");assert_eq!(denied["error"]["code"],"PAYMENT_SOURCE_CHANGED");
        let (called,count):(bool,i64)=sqlx::query_as("SELECT is_called,last_value FROM generic_payment_drift_calls").fetch_one(db.raw()).await.unwrap();assert!(called && count>=1,"deferred drift must actually run");
        let drift_after:serde_json::Value=sqlx::query_scalar(sqlx::AssertSqlSafe(drift_snapshot_sql)).fetch_one(db.raw()).await.unwrap();assert_eq!(drift_after,drift_before,"post-admission source drift must roll back invoice, payment, Money and ledger effects");
        sqlx::raw_sql("DROP TRIGGER generic_payment_drift_fault ON outbox_events; DROP FUNCTION generic_payment_drift_fault(); DROP SEQUENCE generic_payment_drift_calls").execute(db.raw()).await.unwrap();
        for key in ["missing-duplicate","missing-company","missing-unknown","duplicate-company","selected-missing","selected-unknown","selected-forged","selected-stale","foreign-reference","payment-invoice-drift"] {let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1),(SELECT count(*) FROM finance_payments WHERE payment_number=$2)").bind(key).bind(format!("PAY-{key}")).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0),"{key}");}
    }).await;
}
