//! Commerce HTTP selected-Company composition using real Account sign-in,
//! Holding SSO, Identity authority and leased PostgreSQL.
#![allow(dead_code)]
#[cfg(feature = "private-field-api")]
#[path = "../../support/runtime/private-session-config.rs"]
mod private_session_config;
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commerce::public::order_http::{
    CommerceOrderHttpTarget, commerce_order_http_router,
};
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::{
    commerce::{CommerceBackend, CommerceConfiguration, CommerceService},
    finance_journal_detail_http::FinanceJournalReadScopeHeaders,
};
use std::sync::Arc;
use tower::ServiceExt;

fn request(
    token: Option<&str>,
    company: Option<&str>,
    assignment: Option<&str>,
    revision: Option<&str>,
    assignment_version: Option<&str>,
    order: &str,
    write: bool,
    key: &str,
) -> Request<Body> {
    let path = if write {
        format!("/api/v1/holdings/acme/commerce/orders/{order}/transition")
    } else {
        format!("/api/v1/holdings/acme/commerce/orders/{order}")
    };
    let mut b = Request::builder()
        .method(if write { "POST" } else { "GET" })
        .uri(path)
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test");
    if let Some(token) = token {
        b = b.header("cookie", format!("__Host-erpx-product={token}"));
    }
    if let Some(v) = company {
        b = b.header("x-erpx-context-company", v);
    }
    if let Some(v) = assignment {
        b = b.header("x-erpx-context-assignment", v);
    }
    if let Some(v) = revision {
        b = b.header("x-erpx-context-revision", v);
    }
    if let Some(v) = assignment_version {
        b = b.header("x-erpx-context-assignment-version", v);
    }
    if write {
        b = b
            .header("content-type", "application/json")
            .header("idempotency-key", key);
    }
    b.body(Body::from(if write {
        r#"{"action":"submit","expectedVersion":1}"#
    } else {
        ""
    }))
    .unwrap()
}
async fn json(response: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let data = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&data).unwrap_or_default())
}

async fn snapshot(
    db: &erpx_server_runtime::public::postgres::DatabasePool,
    id: &str,
) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('order',to_jsonb(o),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM public.commerce_order_lines l WHERE l.order_id=o.id),'[]'::jsonb)) FROM public.commerce_orders o WHERE o.id=$1").bind(id).fetch_one(db.raw()).await.unwrap()
}
async fn money_snapshot(
    db: &erpx_server_runtime::public::postgres::DatabasePool,
    company: &str,
) -> serde_json::Value {
    sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('source_type',source_type,'source_id',source_id,'amount_kind',amount_kind,'transaction_date',transaction_date::text,'currency',currency,'currency_scale',currency_scale,'amount_units',amount_units::text,'usd_units',usd_units::text,'fx_rate_id',fx_rate_id,'used_latest_fallback',used_latest_fallback) ORDER BY source_type,source_id,amount_kind),'[]'::jsonb) FROM public.money_transaction_snapshots WHERE company_id=$1 AND source_type IN ('commerce_order_revision','commerce_order_line_revision')").bind(company).fetch_one(db.raw()).await.unwrap()
}
fn expected_money(
    id: &str,
    line: &str,
    version: i64,
    rate: &str,
    usd: &str,
) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    for (source, source_id, kind, amount, converted) in [
        (
            "commerce_order_line_revision",
            format!("{line}:{version}"),
            "subtotal",
            "1000",
            usd,
        ),
        (
            "commerce_order_line_revision",
            format!("{line}:{version}"),
            "tax",
            "0",
            "0",
        ),
        (
            "commerce_order_line_revision",
            format!("{line}:{version}"),
            "unit_price",
            "1000",
            usd,
        ),
        (
            "commerce_order_revision",
            format!("{id}:{version}"),
            "total",
            "1000",
            usd,
        ),
    ] {
        rows.push(serde_json::json!({"source_type":source,"source_id":source_id,"amount_kind":kind,"transaction_date":"2026-11-10","currency":"THB","currency_scale":2,"amount_units":amount,"usd_units":converted,"fx_rate_id":rate,"used_latest_fallback":false}));
    }
    rows
}
async fn assert_saved_effects(
    db: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
    id: &str,
    company: &str,
    actor: &str,
    assignment: &str,
    operation: &str,
    version: i64,
) {
    let row:(String,String,String,String,serde_json::Value,String,String,String,serde_json::Value)=sqlx::query_as("SELECT r.operation,r.actor_id,r.record_id,r.authority_json::jsonb->>'companyId',r.result_json::jsonb,a.actor_id,a.assignment_id,a.authority_json::jsonb->>'companyId',o.payload_json::jsonb FROM command_receipts r JOIN audit_events a ON a.command_id=r.command_id JOIN outbox_events o ON o.command_id=r.command_id WHERE r.command_id=$1").bind(key).fetch_one(db.raw()).await.unwrap();
    assert_eq!(
        (&*row.0, &*row.1, &*row.2, &*row.3),
        (operation, actor, id, company)
    );
    assert_eq!(row.4["id"], id);
    assert_eq!(row.4["version"], version);
    assert_eq!((&*row.5, &*row.6, &*row.7), (actor, assignment, company));
    assert_eq!(row.8["id"], id);
    assert_eq!(row.8["companyId"], company);
    assert_eq!(row.8["version"], version);
    assert_eq!(row.8["authority"]["assignmentId"], assignment);
}

#[tokio::test]
async fn two_companies_use_normal_sso_and_selected_context_without_cross_company_effects() {
    review::account::with_system(|system|async move {
        let p=review::setup_with_finance(system,false).await;let db=&p.system.holdings[0];
        // Both accounts have genuine SSO child sessions created by the shared
        // fixture. Seed selected Company/role facts in their leased Holding.
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'),('branch','branch','branch','Branch','company','company',NULL,'active'),('foreign-branch','branch','foreign-branch','Foreign branch','foreign','foreign',NULL,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_finance_review',2,'[\"commerce.read\",\"commerce.sales\",\"commerce.approve\",\"module.sales.read\"]','[]','[\"holding\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review'; UPDATE assignments SET modules_json='[\"sales\"]',version=4 WHERE id='finance-fixture-a'; UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'; INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('company','fixture-a','local:fixture-a','active','local',1),('foreign','fixture-b','local:fixture-b','active','local',1); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('sales-foreign','fixture-b','custom_finance_review','[\"sales\"]','foreign',0,9000000000000,1,'holding'); UPDATE active_work_contexts SET company_id='foreign',assignment_id='sales-foreign',assignment_version=1,revision=8 WHERE assignment_id='finance-fixture-b'; INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-a','company','customer','PA','Customer A','THB',30,0,'active',1,0,0),('party-b','foreign','customer','PB','Customer B','THB',30,0,'active',1,0,0); INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES('product-a','company','P-A','Service A','each',1000,0,'active',1,'none','service'),('product-b','foreign','P-B','Service B','each',1000,0,'active',1,'none','service'); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('order-a','company','branch','quote','QA','party-a','THB','2026-11-01','2026-11-08','','draft',1000,0,1,'fixture-a',100,100),('order-b','foreign','foreign-branch','quote','QB','party-b','THB','2026-11-01','2026-11-08','','draft',1000,0,1,'fixture-b',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('line-a','order-a','product-a','Service',1000,0,1000,0,1000,0,0,'service'),('line-b','order-b','product-b','Service',1000,0,1000,0,1000,0,0,'service')").execute(db.raw()).await.unwrap();
        // Real per-Company charts, distinct accounting references and
        // reviewed active price lists for the actual Commerce save path.
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('foreign-chart','foreign','Foreign chart','THB','fixture-b',0,0); INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) SELECT prefix||'-'||n,chart,company,'AC-'||n,'Account '||n,category,'THB','active',0,0 FROM (VALUES('a','chart','company'),('b','foreign-chart','foreign')) c(prefix,chart,company) CROSS JOIN (VALUES('1','asset'),('2','liability'),('3','revenue'),('4','asset'),('5','liability'),('6','expense'),('7','asset'),('8','liability'),('9','asset')) a(n,category); INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','a-1','a-2','a-3','a-4','a-5','a-6','a-7','a-8','a-9'),('foreign','foreign-chart','THB','b-1','b-2','b-3','b-4','b-5','b-6','b-7','b-8','b-9'); INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,status,version,created_by,prepared_by,approved_by,approval_reason,activated_at,created_at,updated_at) VALUES('price-a','company','Company list','THB','party-a','2026-11-01','2026-11-30',0,'draft',1,'fixture-a','fixture-a',NULL,NULL,NULL,100,100),('price-b','foreign','Foreign list','THB','party-b','2026-11-01','2026-11-30',0,'draft',1,'fixture-b','fixture-b',NULL,NULL,NULL,100,100); INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES('price-item-a','price-a','product-a',1000,0,0),('price-item-b','price-b','product-b',1000,0,0); UPDATE sales_price_lists SET status='active',approved_by='fixture-b',approval_reason='Reviewed price',activated_at=1000 WHERE id='price-a'; UPDATE sales_price_lists SET status='active',approved_by='fixture-a',approval_reason='Reviewed price',activated_at=1000 WHERE id='price-b'").execute(db.raw()).await.unwrap();
        // Confirmed immutable quotes belong to the selected Company and are
        // effective on the order date; the two rates make isolation visible.
        sqlx::raw_sql("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('commerce-fx-a','company','THB','2026-11-10',40,'fixture-b',1000,'Reviewed Company quote'),('commerce-fx-b','foreign','THB','2026-11-10',50,'fixture-a',1000,'Reviewed foreign quote')").execute(db.raw()).await.unwrap();
        // Actual reservation rows for both admitted draft order kinds.
        sqlx::raw_sql("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES('cancel-stock','company','C-STOCK','Cancel stock','each',1000,0,'active',1,'none','stock'); INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('cancel-warehouse','company','branch','CW','Cancel warehouse','active',1); INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES('cancel-warehouse','cancel-stock',10000,10000,1); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('cancel-quote','company','branch','quote','QT-CANCEL','party-a','cancel-warehouse','THB','2026-11-10','2026-11-15','','draft',1000,0,1,'fixture-a',100,100),('cancel-sales','company','branch','sales','SO-CANCEL','party-a','cancel-warehouse','THB','2026-11-10','2026-11-15','','draft',1000,0,1,'fixture-a',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('cancel-line-q','cancel-quote','cancel-stock','Stock',1000,0,1000,0,1000,0,0,'stock'),('cancel-line-s','cancel-sales','cancel-stock','Stock',1000,0,1000,0,1000,0,0,'stock'); INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at) VALUES('cancel-res-q','cancel-line-q','cancel-warehouse','cancel-stock',1000,200,100,4102444800000,'fixture-a','source',100),('cancel-res-s','cancel-line-s','cancel-warehouse','cancel-stock',1000,200,100,4102444800000,'fixture-a','source',100)").execute(db.raw()).await.unwrap();
        // An actual Identity member alias refers to the same persisted person
        // as the signed-in selected manager; a different person is a control.
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('cancel-actor-person','fixture-a','company','Selected manager','TH','active','fixture-a',0,0),('cancel-other-person',NULL,'company','Other creator','TH','active','fixture-a',0,0); UPDATE iam_members SET person_id='cancel-actor-person' WHERE company_id='company' AND user_id='fixture-a'; INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company','fixture-a-alias','local:fixture-a-alias','cancel-actor-person','active','local',1),('company','fixture-other','local:fixture-other','cancel-other-person','active','local',1); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('alias-approve','company','branch','quote','QT-ALIAS-A','party-a','THB','2026-11-10','2026-11-15','','submitted',1000,0,1,'fixture-a-alias',100,100),('alias-cancel','company','branch','quote','QT-ALIAS-C','party-a','THB','2026-11-10','2026-11-15','','confirmed',1000,0,1,'fixture-a-alias',100,100),('other-approve','company','branch','quote','QT-OTHER-A','party-a','THB','2026-11-10','2026-11-15','','submitted',1000,0,1,'fixture-other',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('alias-cancel-line','alias-cancel','product-a','Service',1000,0,1000,0,1000,0,0,'service')").execute(db.raw()).await.unwrap();
        // Distinct real customer credit limits let the stale-version control
        // reach CAS while the limited customer fails the earlier Finance guard.
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('credit-limited','company','customer','CREDIT-LIMIT','Credit limited','THB',30,100,'active',1,0,0); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('credit-stale','company','branch','sales','SO-CREDIT-STALE','credit-limited','THB','2026-11-10','2026-11-15','','submitted',1000,0,2,'fixture-other',100,100),('unlimited-stale','company','branch','sales','SO-UNLIMITED-STALE','party-a','THB','2026-11-10','2026-11-15','','submitted',1000,0,2,'fixture-other',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('credit-stale-line','credit-stale','product-a','Service',1000,0,1000,0,1000,0,0,'service'),('unlimited-stale-line','unlimited-stale','product-a','Service',1000,0,1000,0,1000,0,0,'service')").execute(db.raw()).await.unwrap();
        let config=CommerceConfiguration::for_companies(HoldingId::new("h001").unwrap(),vec![CompanyId::new("company").unwrap(),CompanyId::new("foreign").unwrap()]).unwrap();
        let service=CommerceService::new(p.authorizer.clone(),config,p.system.scenario.clock.clone());let backend=CommerceBackend::new(Arc::new(review::account::fixture::account::policy()),Arc::new(FinanceJournalReadScopeHeaders),Arc::new(service));
        let app=commerce_order_http_router(vec![CommerceOrderHttpTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),origin:"https://acme.example.test".into(),backend:Arc::new(backend)}]).unwrap();
        let a=p.maker_token.expose();let b=p.reviewer_token.expose();
        // Finance party writers must finish before a selected Commerce save
        // can establish the source snapshot, including an absent party key.
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-race','company','customer','PR','Race customer','THB',30,0,'active',1,0,0)").execute(db.raw()).await.unwrap();
        let party_save=|party:&str,key:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/commerce/orders").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={a}")).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision","7").header("x-erpx-context-assignment-version","4").header("content-type","application/json").header("idempotency-key",key).body(Body::from(serde_json::json!({"kind":"quote","partyId":party,"orderDate":"2026-11-10","dueDate":"2026-11-15","note":"Party lock check","lines":[{"productId":"product-a","quantity":"1","price":"10","taxBps":0}]}).to_string())).unwrap();
        for (party,key,commit_writer) in [("party-race","party-source-race",true),("party-absent","party-source-absence",false)] {
            let before_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();let before_money=money_snapshot(db,"company").await;
            let mut writer=db.raw().begin().await.unwrap();
            if commit_writer {sqlx::query("UPDATE finance_parties SET status='blocked',version=version+1 WHERE company_id='company' AND id='party-race'").execute(&mut *writer).await.unwrap();}else{sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-absent','company','customer','PABS','Uncommitted customer','THB',30,0,'active',1,0,0)").execute(&mut *writer).await.unwrap();}
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.finance_parties'::regclass AND mode='RowExclusiveLock' AND granted)").bind(pid).fetch_one(db.raw()).await.unwrap();assert!(held,"actual party writer must hold the conflicting relation lock");
            let (status,denied)=json(app.clone().oneshot(party_save(party,key)).await.unwrap()).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"party contention {denied}");assert_eq!(denied["error"]["code"],"COMMERCE_UNAVAILABLE");
            let blocked_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();assert_eq!(blocked_orders,before_orders);assert_eq!(money_snapshot(db,"company").await,before_money);let blocked_effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(blocked_effects,(0,0,0));
            if commit_writer {writer.commit().await.unwrap();}else{writer.rollback().await.unwrap();}
            let (status,denied)=json(app.clone().oneshot(party_save(party,key)).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"fresh party after lock {denied}");assert_eq!(denied["error"]["code"],"CONFLICT");
            let after_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();assert_eq!(after_orders,before_orders);assert_eq!(money_snapshot(db,"company").await,before_money);let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }
        let state:String=sqlx::query_scalar("SELECT status FROM finance_parties WHERE id='party-race' AND company_id='company'").fetch_one(db.raw()).await.unwrap();assert_eq!(state,"blocked");let absent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_parties WHERE id='party-absent' AND company_id='company')").fetch_one(db.raw()).await.unwrap();assert!(!absent);
        for (token,company,assignment,revision,version,order) in [(a,"company","finance-fixture-a","7","4","order-a"),(b,"foreign","sales-foreign","8","1","order-b")] {
            let (status,value)=json(app.clone().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),order,false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"{value}");assert_eq!(value["context"]["companyId"],company);assert_eq!(value["data"]["order"]["id"],order);
        }
        #[cfg(feature="private-field-api")]
        {
            use erpx_service::private_development_api::{assemble_development_api,DevelopmentApiConfiguration};
            use erpx_server_identity_access::public::SystemAuthenticationSecrets;
            let mut document=private_session_config::document(&p.system.control,&p.system.holdings).await;
            document["commerceCompanies"]=serde_json::json!([{"holdingId":"h001","companyId":"company"},{"holdingId":"h001","companyId":"foreign"}]);
            let config=DevelopmentApiConfiguration::parse(&document.to_string()).expect("two distinct Company targets should parse");
            let runtime=assemble_development_api(config,p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.expect("full private development assembly");
            for (token,company,assignment,revision,version,order) in [(a,"company","finance-fixture-a","7","4","order-a"),(b,"foreign","sales-foreign","8","1","order-b")] {
                let (status,value)=json(runtime.router().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),order,false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"full assembly {value}");assert_eq!(value["context"]["companyId"],company);assert_eq!(value["data"]["order"]["id"],order);
            }
            runtime.close().await;
        }
        for (token,company,assignment,revision,version,foreign_order) in [(a,"company","finance-fixture-a","7","4","order-b"),(b,"foreign","sales-foreign","8","1","order-a")] {let (status,_)=json(app.clone().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),foreign_order,false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::NOT_FOUND);}
        let (status,_)=json(app.clone().oneshot(request(None,Some("company"),None,None,None,"order-a",false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::UNAUTHORIZED);
        for bad in [request(Some(a),None,None,None,None,"order-a",true,"missing-context"),request(Some(a),Some("company"),Some("finance-fixture-a"),Some("6"),Some("4"),"order-a",true,"stale-context"),request(Some(a),Some("foreign"),Some("sales-foreign"),Some("8"),Some("1"),"order-b",true,"forged-context")] {let (status,_)=json(app.clone().oneshot(bad).await.unwrap()).await;assert!(status==StatusCode::CONFLICT||status==StatusCode::FORBIDDEN,"unexpected status {status}");}
        // Context header defects are captured and resolved only after the real
        // Account session. A malformed workspace slug follows the same order.
        let mut duplicate_unauth=request(None,Some("company"),Some("finance-fixture-a"),Some("7"),Some("4"),"order-a",true,"unauth-duplicate");duplicate_unauth.headers_mut().append("x-erpx-context-company","foreign".parse().unwrap());
        let mut invalid_slug_unauth=request(None,Some("company"),Some("finance-fixture-a"),Some("7"),Some("4"),"order-a",true,"unauth-slug");invalid_slug_unauth.headers_mut().insert("x-erpx-workspace-branch","Invalid!".parse().unwrap());
        for bad in [request(None,Some("company"),None,None,None,"order-a",true,"unauth-partial"),duplicate_unauth,invalid_slug_unauth] {let (status,value)=json(app.clone().oneshot(bad).await.unwrap()).await;assert_eq!(status,StatusCode::UNAUTHORIZED,"{value}");assert_eq!(value["error"]["code"],"UNAUTHENTICATED");}
        let mut duplicate=request(Some(a),Some("company"),Some("finance-fixture-a"),Some("7"),Some("4"),"order-a",true,"duplicate-context");duplicate.headers_mut().append("x-erpx-context-company","foreign".parse().unwrap());
        let mut invalid_slug=request(Some(a),Some("company"),Some("finance-fixture-a"),Some("7"),Some("4"),"order-a",true,"invalid-slug");invalid_slug.headers_mut().insert("x-erpx-workspace-branch","Invalid!".parse().unwrap());
        for (bad,expected_code) in [(request(Some(a),Some("company"),Some("finance-fixture-a"),None,Some("4"),"order-a",true,"partial-context"),"WORK_CONTEXT_CHANGED"),(request(Some(a),Some("company"),Some("finance-fixture-a"),Some("7"),Some("3"),"order-a",true,"stale-assignment"),"WORK_CONTEXT_CHANGED"),(request(Some(a),Some("company"),Some("sales-foreign"),Some("7"),Some("4"),"order-a",true,"wrong-assignment"),"WORK_CONTEXT_CHANGED"),(duplicate,"WORK_CONTEXT_CHANGED")] {let (status,value)=json(app.clone().oneshot(bad).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"{value}");assert_eq!(value["error"]["code"],expected_code);}
        let (status,value)=json(app.clone().oneshot(invalid_slug).await.unwrap()).await;assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY,"{value}");assert_eq!(value["error"]["code"],"INVALID_WORKSPACE_SELECTOR");
        for (token,company,assignment,revision,version,order,key) in [(a,"company","finance-fixture-a","7","4","order-a","submit-company-a"),(b,"foreign","sales-foreign","8","1","order-b","submit-company-b")] {let (status,value)=json(app.clone().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),order,true,key)).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"{value}");assert_eq!(value["context"]["companyId"],company);assert_eq!(value["data"]["state"],"submitted");let (replay_status,replayed)=json(app.clone().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),order,true,key)).await.unwrap()).await;assert_eq!(replay_status,StatusCode::OK,"{replayed}");assert_eq!(replayed,value);}
        // Normal Account SSO and the selected Identity context derive aliases;
        // request bodies and headers provide no same-person claims.
        let transition=|order:&str,action:&str,key:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/commerce/orders/{order}/transition")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={a}")).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision","7").header("x-erpx-context-assignment-version","4").header("content-type","application/json").header("idempotency-key",key).body(Body::from(serde_json::json!({"action":action,"expectedVersion":1}).to_string())).unwrap();
        // Service fulfillment uses the actual selected SSO actor and Finance
        // exact Money keys, including a distinct source-type collision.
        sqlx::raw_sql("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('http-service-order','company','branch','sales','SO-HTTP-SERVICE','party-a','THB','2026-11-10','2026-11-15','','confirmed',1000,0,1,'fixture-other',100,100),('http-service-rollback','company','branch','sales','SO-HTTP-ROLLBACK','party-a','THB','2026-11-10','2026-11-15','','confirmed',1000,0,1,'fixture-other',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('http-service-line','http-service-order','product-a','Service',1000,0,1000,0,1000,0,0,'service'),('http-service-rollback-line','http-service-rollback','product-a','Service',1000,0,1000,0,1000,0,0,'service'); INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES('company','unrelated_owner','delivery-http-service-complete-0','subtotal','2026-11-10','THB',2,777,19,'commerce-fx-a',false,1)").execute(db.raw()).await.unwrap();
        let fulfill=|order:&str,line:&str,key:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/commerce/orders/{order}/fulfill")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={a}")).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision","7").header("x-erpx-context-assignment-version","4").header("content-type","application/json").header("idempotency-key",key).body(Body::from(serde_json::json!({"completionMode":"service","expectedVersion":1,"date":"2026-11-10","evidenceNote":"Customer accepted the completed service","lines":[{"lineId":line,"quantity":"1"}]}).to_string())).unwrap();
        let (status,completed)=json(app.clone().oneshot(fulfill("http-service-order","http-service-line","http-service-complete")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"service completion {completed}");assert_eq!(completed["data"]["fulfillmentId"],"delivery-http-service-complete");assert_eq!(completed["data"]["state"],"fulfilled");assert_eq!(completed["data"]["version"],2);
        let (status,replayed)=json(app.clone().oneshot(fulfill("http-service-order","http-service-line","http-service-complete")).await.unwrap()).await;assert_eq!(status,StatusCode::OK);assert_eq!(replayed,completed);
        let persisted:(String,i64,i64,String,String,i64,i64,i64,i64)=sqlx::query_as("SELECT o.state,o.version,l.fulfilled_milli,f.completion_kind,f.completion_note,fl.quantity_milli,fl.value_minor,fl.subtotal_minor,fl.tax_minor FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id JOIN commerce_fulfillments f ON f.order_id=o.id JOIN commerce_fulfillment_lines fl ON fl.fulfillment_id=f.id AND fl.order_line_id=l.id WHERE o.id='http-service-order' AND f.id='delivery-http-service-complete'").fetch_one(db.raw()).await.unwrap();assert_eq!(persisted,("fulfilled".into(),2,1000,"service".into(),"Customer accepted the completed service".into(),1000,0,1000,0));
        let money:Vec<(String,String,String,String,Option<String>)>=sqlx::query_as("SELECT source_type,amount_kind,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id='company' AND source_id='delivery-http-service-complete-0' ORDER BY source_type,amount_kind").fetch_all(db.raw()).await.unwrap();assert_eq!(money,vec![("commerce_fulfillment_line".into(),"subtotal".into(),"1000".into(),"25".into(),Some("commerce-fx-a".into())),("commerce_fulfillment_line".into(),"tax".into(),"0".into(),"0".into(),Some("commerce-fx-a".into())),("commerce_fulfillment_line".into(),"total".into(),"1000".into(),"25".into(),Some("commerce-fx-a".into())),("unrelated_owner".into(),"subtotal".into(),"777".into(),"19".into(),Some("commerce-fx-a".into()))]);
        let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='http-service-complete'),(SELECT count(*) FROM audit_events WHERE command_id='http-service-complete'),(SELECT count(*) FROM outbox_events WHERE command_id='http-service-complete')").fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(1,1,1));
        sqlx::raw_sql("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES('http-service-duplicate','company','branch','sales','SO-HTTP-DUPLICATE','party-a','THB','2026-11-10','2026-11-15','','confirmed',1000,0,1,'fixture-other',100,100); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('http-service-duplicate-line','http-service-duplicate','product-a','Service',1000,0,1000,0,1000,0,0,'service'); INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES('company','commerce_fulfillment_line','delivery-http-service-duplicate-key-0','subtotal','2026-11-10','THB',2,777,19,'commerce-fx-a',false,1)").execute(db.raw()).await.unwrap();
        let duplicate_before=snapshot(db,"http-service-duplicate").await;let duplicate_money:serde_json::Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY source_type,source_id,amount_kind),'[]'::jsonb) FROM money_transaction_snapshots m WHERE company_id='company' AND source_id='delivery-http-service-duplicate-key-0'").fetch_one(db.raw()).await.unwrap();
        let (status,denied)=json(app.clone().oneshot(fulfill("http-service-duplicate","http-service-duplicate-line","http-service-duplicate-key")).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"same-type Money collision {denied}");assert_eq!(denied["error"]["code"],"CONFLICT");assert_eq!(snapshot(db,"http-service-duplicate").await,duplicate_before);
        let duplicate_after:serde_json::Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY source_type,source_id,amount_kind),'[]'::jsonb) FROM money_transaction_snapshots m WHERE company_id='company' AND source_id='delivery-http-service-duplicate-key-0'").fetch_one(db.raw()).await.unwrap();assert_eq!(duplicate_after,duplicate_money);
        let duplicate_effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='http-service-duplicate'),(SELECT count(*) FROM commerce_fulfillment_lines WHERE fulfillment_id='delivery-http-service-duplicate-key'),(SELECT count(*) FROM command_receipts WHERE command_id='http-service-duplicate-key'),(SELECT count(*) FROM audit_events WHERE command_id='http-service-duplicate-key'),(SELECT count(*) FROM outbox_events WHERE command_id='http-service-duplicate-key')").fetch_one(db.raw()).await.unwrap();assert_eq!(duplicate_effects,(0,0,0,0,0));
        let before=snapshot(db,"http-service-rollback").await;
        sqlx::raw_sql("CREATE SEQUENCE http_service_rollback_calls; CREATE FUNCTION http_service_rollback_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='http-service-rollback-key' THEN PERFORM nextval('http_service_rollback_calls'); RAISE EXCEPTION 'synthetic deferred outbox failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER http_service_rollback_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION http_service_rollback_fault()").execute(db.raw()).await.unwrap();
        let (status,failed)=json(app.clone().oneshot(fulfill("http-service-rollback","http-service-rollback-line","http-service-rollback-key")).await.unwrap()).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"deferred rollback {failed}");assert_eq!(failed["error"]["code"],"COMMERCE_UNAVAILABLE");let called:bool=sqlx::query_scalar("SELECT is_called FROM http_service_rollback_calls").fetch_one(db.raw()).await.unwrap();assert!(called);assert_eq!(snapshot(db,"http-service-rollback").await,before);
        let rollback:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='http-service-rollback'),(SELECT count(*) FROM commerce_fulfillment_lines WHERE fulfillment_id='delivery-http-service-rollback-key'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='commerce_fulfillment_line' AND source_id='delivery-http-service-rollback-key-0'),(SELECT count(*) FROM command_receipts WHERE command_id='http-service-rollback-key'),(SELECT count(*) FROM audit_events WHERE command_id='http-service-rollback-key')+(SELECT count(*) FROM outbox_events WHERE command_id='http-service-rollback-key')").fetch_one(db.raw()).await.unwrap();assert_eq!(rollback,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER http_service_rollback_fault ON outbox_events; DROP FUNCTION http_service_rollback_fault(); DROP SEQUENCE http_service_rollback_calls").execute(db.raw()).await.unwrap();
        for (order,action,key,code) in [("alias-approve","approve","alias-denied-approve","INDEPENDENT_APPROVER_REQUIRED"),("alias-cancel","cancel","alias-denied-cancel","INDEPENDENT_CANCELLER_REQUIRED")] {
            let before=snapshot(db,order).await;let (status,denied)=json(app.clone().oneshot(transition(order,action,key)).await.unwrap()).await;assert_eq!(status,StatusCode::FORBIDDEN,"{denied}");assert_eq!(denied["error"]["code"],code);assert_eq!(snapshot(db,order).await,before);
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }
        let (status,approved)=json(app.clone().oneshot(transition("other-approve","approve","other-approved")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"independent creator control {approved}");assert_eq!(approved["data"]["state"],"confirmed");
        // A fresh quotation conversion writes the TS-compatible sales-order
        // revision total in the same unit, replays without duplicates and a
        // stale conversion leaves both the order and Money unchanged.
        let (status,converted)=json(app.clone().oneshot(transition("alias-cancel","convert","convert-fresh")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"fresh conversion {converted}");assert_eq!(converted["data"]["state"],"converted");assert_eq!(converted["data"]["version"],2);let sales_id=converted["data"]["salesId"].as_str().expect("converted sales id").to_owned();
        let (replay_status,replayed)=json(app.clone().oneshot(transition("alias-cancel","convert","convert-fresh")).await.unwrap()).await;assert_eq!(replay_status,StatusCode::OK,"conversion replay {replayed}");assert_eq!(replayed,converted);
        let mut convert_replay_writer=db.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE public.money_transaction_snapshots IN ROW EXCLUSIVE MODE").execute(&mut *convert_replay_writer).await.unwrap();
        let (contended_replay_status,contended_replay)=json(app.clone().oneshot(transition("alias-cancel","convert","convert-fresh")).await.unwrap()).await;assert_eq!(contended_replay_status,StatusCode::OK,"conversion replay under Finance contention {contended_replay}");assert_eq!(contended_replay,converted);
        convert_replay_writer.rollback().await.unwrap();
        let converted_money:Vec<(String,String,String,String,String,String,Option<String>)>=sqlx::query_as("SELECT source_type,source_id,amount_kind,currency,currency_scale::text,amount_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='commerce_order_revision' AND source_id=$1 ORDER BY amount_kind").bind(format!("{sales_id}:1")).fetch_all(db.raw()).await.unwrap();assert_eq!(converted_money,[("commerce_order_revision".into(),format!("{sales_id}:1"),"total".into(),"THB".into(),"2".into(),"1000".into(),Some("commerce-fx-a".into()))]);
        let sales_row:(String,i64,Option<String>)=sqlx::query_as("SELECT state,version,source_id FROM commerce_orders WHERE id=$1").bind(&sales_id).fetch_one(db.raw()).await.unwrap();assert_eq!(sales_row,("draft".into(),1,Some("alias-cancel".into())));
        let money_before_stale=money_snapshot(db,"company").await;
        // A conflicting Finance writer must not preempt the stale version
        // guard: Money validation is source-only before the version assertion.
        let mut finance_writer=db.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE public.money_transaction_snapshots IN ROW EXCLUSIVE MODE").execute(&mut *finance_writer).await.unwrap();
        let order_before_stale=snapshot(db,"alias-cancel").await;
        let (stale_convert_status,stale_convert)=json(app.clone().oneshot(transition("alias-cancel","convert","convert-stale")).await.unwrap()).await;assert_eq!(stale_convert_status,StatusCode::CONFLICT,"stale conversion {stale_convert}");assert_eq!(stale_convert["error"]["code"],"CONFLICT");assert_eq!(snapshot(db,"alias-cancel").await,order_before_stale);assert_eq!(money_snapshot(db,"company").await,money_before_stale,"stale conversion leaves Money unchanged");
        finance_writer.rollback().await.unwrap();
        let convert_effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind("convert-stale").fetch_one(db.raw()).await.unwrap();assert_eq!(convert_effects,(0,0,0));
        // The original TS/PG oracle establishes Money validation before a
        // stale quotation guard. Keep actual source rows and all guards.
        for (id,currency) in [("convert-missing-fx","NZD"),("convert-wrong-scale","JPY"),("convert-fx-drift","THB")] {
            sqlx::query("INSERT INTO commerce_orders SELECT (jsonb_populate_record(NULL::commerce_orders,to_jsonb(o)||jsonb_build_object('id',$1::text,'number',$1::text,'currency',$2::text,'state','confirmed','version',2))).* FROM commerce_orders o WHERE o.id='alias-cancel'").bind(id).bind(currency).execute(db.raw()).await.unwrap();
            sqlx::query("INSERT INTO commerce_order_lines SELECT (jsonb_populate_record(NULL::commerce_order_lines,to_jsonb(l)||jsonb_build_object('id',$1::text,'order_id',$2::text))).* FROM commerce_order_lines l WHERE l.id='alias-cancel-line'").bind(format!("{id}-line")).bind(id).execute(db.raw()).await.unwrap();
        }
        let nzd:i64=sqlx::query_scalar("SELECT count(*) FROM company_fx_rates WHERE company_id='company' AND currency='NZD'").fetch_one(db.raw()).await.unwrap();assert_eq!(nzd,0);
        for (id,key,code) in [("convert-missing-fx","convert-stale-missing-fx","MANUAL_RATE_REQUIRED"),("convert-wrong-scale","convert-stale-wrong-scale","CURRENCY_SCALE_MISMATCH")] {
            let before=snapshot(db,id).await;let money=money_snapshot(db,"company").await;
            let (status,denied)=json(app.clone().oneshot(transition(id,"convert",key)).await.unwrap()).await;assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY,"{denied}");assert_eq!(denied["error"]["code"],code);assert_eq!(snapshot(db,id).await,before);assert_eq!(money_snapshot(db,"company").await,money);
            let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_orders WHERE source_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id=$2),(SELECT count(*) FROM audit_events WHERE command_id=$2),(SELECT count(*) FROM outbox_events WHERE command_id=$2)").bind(id).bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0));
        }
        // A deferred new confirmed quote is a legitimate append. It must not
        // survive a failed retained-Money check or commit a partial conversion.
        sqlx::raw_sql("CREATE SEQUENCE commerce_conversion_fx_drift_calls; CREATE FUNCTION commerce_conversion_fx_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='commerce_order_revision' AND NEW.source_id='order-convert-fx-drift-key:1' THEN PERFORM nextval('commerce_conversion_fx_drift_calls'); INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('convert-fx-drift-rate','company','THB','2026-11-10',10,'fixture-b',9999,'Synthetic deferred confirmed quote'); END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER commerce_conversion_fx_drift AFTER INSERT ON money_transaction_snapshots DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_conversion_fx_drift()").execute(db.raw()).await.unwrap();
        let drift_before=snapshot(db,"convert-fx-drift").await;let money_before=money_snapshot(db,"company").await;
        let mut drift=transition("convert-fx-drift","convert","convert-fx-drift-key");*drift.body_mut()=Body::from(r#"{"action":"convert","expectedVersion":2}"#);
        let (status,denied)=json(app.clone().oneshot(drift).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"deferred FX drift {denied}");assert_eq!(denied["error"]["code"],"COMMERCE_ORDER_MONEY_CHANGED");assert_eq!(snapshot(db,"convert-fx-drift").await,drift_before);assert_eq!(money_snapshot(db,"company").await,money_before);
        let (calls,called):(i64,bool)=sqlx::query_as("SELECT last_value,is_called FROM commerce_conversion_fx_drift_calls").fetch_one(db.raw()).await.unwrap();assert!(called&&calls>=1,"deferred Finance drift must actually fire");
        let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_fx_rates WHERE id='convert-fx-drift-rate'),(SELECT count(*) FROM commerce_orders WHERE source_id='convert-fx-drift'),(SELECT count(*) FROM command_receipts WHERE command_id='convert-fx-drift-key'),(SELECT count(*) FROM audit_events WHERE command_id='convert-fx-drift-key'),(SELECT count(*) FROM outbox_events WHERE command_id='convert-fx-drift-key')").fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
        sqlx::raw_sql("DROP TRIGGER commerce_conversion_fx_drift ON money_transaction_snapshots; DROP FUNCTION commerce_conversion_fx_drift(); DROP SEQUENCE commerce_conversion_fx_drift_calls").execute(db.raw()).await.unwrap();
        // Over-limit credit and stale version are simultaneously true. The
        // unlimited customer is the control proving the stale CAS path.
        for (order,key,code) in [("credit-stale","credit-stale-denied","CONFLICT"),("unlimited-stale","unlimited-stale-denied","CONFLICT")] {
            let before=snapshot(db,order).await;
            let party_before:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM finance_parties p JOIN commerce_orders o ON o.party_id=p.id AND o.company_id=p.company_id WHERE o.id=$1").bind(order).fetch_one(db.raw()).await.unwrap();
            let (status,denied)=json(app.clone().oneshot(transition(order,"approve",key)).await.unwrap()).await;
            assert_eq!(status,StatusCode::CONFLICT,"credit/stale {denied}");assert_eq!(denied["error"]["code"],code);
            assert_eq!(denied["error"]["message"],"The record changed, a reference is unavailable, or this value already exists. Refresh and review again.");
            assert_eq!(snapshot(db,order).await,before);
            let party_after:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM finance_parties p JOIN commerce_orders o ON o.party_id=p.id AND o.company_id=p.company_id WHERE o.id=$1").bind(order).fetch_one(db.raw()).await.unwrap();assert_eq!(party_after,party_before);
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }
        // The retained TS oracle checks document kind before the stale CAS.
        for (order,key,status,code) in [("credit-stale","convert-sales-stale",StatusCode::UNPROCESSABLE_ENTITY,"QUOTE_REQUIRED"),("other-approve","convert-quote-stale",StatusCode::CONFLICT,"CONFLICT")] {
            let before=snapshot(db,order).await;let money_before=money_snapshot(db,"company").await;
            let (actual,denied)=json(app.clone().oneshot(transition(order,"convert",key)).await.unwrap()).await;assert_eq!(actual,status,"{denied}");assert_eq!(denied["error"]["code"],code);
            assert_eq!(snapshot(db,order).await,before);assert_eq!(money_snapshot(db,"company").await,money_before);
            let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1),(SELECT count(*) FROM commerce_orders WHERE source_id=$2)").bind(key).bind(order).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0));
        }
        // The selected Commerce router calls Inventory within the
        // same owner unit; a replay cannot release or emit effects twice.
        for (order,reservation,key) in [("cancel-quote","cancel-res-q","cancel-draft-quote"),("cancel-sales","cancel-res-s","cancel-draft-sales")] {
            let make=|command:&str,expected:i64|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/commerce/orders/{order}/transition")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={a}")).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision","7").header("x-erpx-context-assignment-version","4").header("content-type","application/json").header("idempotency-key",command).body(Body::from(serde_json::json!({"action":"cancel","expectedVersion":expected}).to_string())).unwrap();
            if order=="cancel-quote" {
                let before=snapshot(db,order).await;
                let reservation_before:(i64,i64,Option<String>)=sqlx::query_as("SELECT consumed_milli,released_milli,release_reason FROM inventory_order_reservations WHERE id=$1").bind(reservation).fetch_one(db.raw()).await.unwrap();
                // Existing Commerce PG suites inject this isolated deferred
                // outbox error after the owner and Inventory writes.
                sqlx::raw_sql("CREATE SEQUENCE commerce_cancel_atomic_fault_calls; CREATE FUNCTION commerce_cancel_atomic_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='cancel-draft-quote' THEN PERFORM nextval('commerce_cancel_atomic_fault_calls'); PERFORM 1/0; END IF; RETURN NEW; END; $$; CREATE CONSTRAINT TRIGGER commerce_cancel_atomic_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_cancel_atomic_fault()").execute(db.raw()).await.unwrap();
                let (status,failed)=json(app.clone().oneshot(make(key,1)).await.unwrap()).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"deferred outbox failure {failed}");
                // Sequence increments survive transaction rollback and establish
                // that the deferred trigger, not an earlier error, fired.
                let (called,attempts):(bool,i64)=sqlx::query_as("SELECT is_called,last_value FROM commerce_cancel_atomic_fault_calls").fetch_one(db.raw()).await.unwrap();assert!(called && attempts>=1,"deferred outbox trigger did not fire");
                assert_eq!(snapshot(db,order).await,before,"order and lines roll back after release");
                let reservation_after:(i64,i64,Option<String>)=sqlx::query_as("SELECT consumed_milli,released_milli,release_reason FROM inventory_order_reservations WHERE id=$1").bind(reservation).fetch_one(db.raw()).await.unwrap();assert_eq!(reservation_after,reservation_before);
                let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
                sqlx::raw_sql("DROP TRIGGER commerce_cancel_atomic_fault ON outbox_events; DROP FUNCTION commerce_cancel_atomic_fault(); DROP SEQUENCE commerce_cancel_atomic_fault_calls").execute(db.raw()).await.unwrap();
            }
            let (status,value)=json(app.clone().oneshot(make(key,1)).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"draft cancellation {value}");assert_eq!(value["data"]["state"],"cancelled");assert_eq!(value["data"]["version"],2);
            let (status,replay)=json(app.clone().oneshot(make(key,1)).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"cancel replay {replay}");assert_eq!(replay,value);
            let row:(String,i64,i64,i64,Option<String>)=sqlx::query_as("SELECT o.state,o.version,r.consumed_milli,r.released_milli,r.release_reason FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id JOIN inventory_order_reservations r ON r.order_line_id=l.id WHERE o.id=$1 AND r.id=$2").bind(order).bind(reservation).fetch_one(db.raw()).await.unwrap();assert_eq!(row,("cancelled".into(),2,200,800,Some("Source order cancelled".into())));
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(1,1,1));
            let stale_key=format!("{key}-stale");let (status,_)=json(app.clone().oneshot(make(&stale_key,1)).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT);
            let unchanged:(i64,i64,Option<String>,i64)=sqlx::query_as("SELECT r.consumed_milli,r.released_milli,r.release_reason,o.version FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id JOIN inventory_order_reservations r ON r.order_line_id=l.id WHERE o.id=$1 AND r.id=$2").bind(order).bind(reservation).fetch_one(db.raw()).await.unwrap();assert_eq!(unchanged,(200,800,Some("Source order cancelled".into()),2));
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&stale_key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }
        // Create with exact Company-owned price-list and persisted references,
        // replay with the same receipt, and verify cross-Company read isolation.
        for (token,company,assignment,revision,version,party,product,list,key) in [(a,"company","finance-fixture-a","7","4","party-a","product-a","price-a","save-company-a"),(b,"foreign","sales-foreign","8","1","party-b","product-b","price-b","save-company-b")] {
            let body=serde_json::json!({"kind":"quote","partyId":party,"orderDate":"2026-11-10","dueDate":"2026-11-15","note":"Reviewed quote","priceListId":list,"lines":[{"productId":product,"quantity":"1","price":"10","taxBps":0}]}).to_string();
            let make=||Request::builder().method("POST").uri("/api/v1/holdings/acme/commerce/orders").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={token}")).header("x-erpx-context-company",company).header("x-erpx-context-assignment",assignment).header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version",version).header("content-type","application/json").header("idempotency-key",key).body(Body::from(body.clone())).unwrap();
            if company=="company" {
                // A real uncommitted FX insertion owns the incompatible table
                // lock. NOWAIT has no waiting backend to sample, so establish
                // the writer PID and prove the exact relation conflict directly.
                let mut writer=db.raw().begin().await.unwrap();
                let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
                sqlx::query("INSERT INTO public.company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('commerce-fx-race','company','THB','2026-11-10',20,'fixture-b',2000,'New confirmed quote')").execute(&mut *writer).await.unwrap();
                let holds:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.company_fx_rates'::regclass AND mode='RowExclusiveLock' AND granted)").bind(writer_pid).fetch_one(db.raw()).await.unwrap();
                assert!(holds,"the identified writer must hold the FX relation lock");
                let mut probe=db.raw().begin().await.unwrap();
                let probe_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *probe).await.unwrap();assert_ne!(writer_pid,probe_pid);
                let error=sqlx::query("LOCK TABLE public.company_fx_rates IN SHARE MODE NOWAIT").execute(&mut *probe).await.unwrap_err();
                assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));probe.rollback().await.unwrap();
                let before_count:i64=sqlx::query_scalar("SELECT count(*) FROM public.commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();
                let before_money=money_snapshot(db,company).await;
                let (status,denied)=json(app.clone().oneshot(make()).await.unwrap()).await;
                assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"held FX lock must exhaust the unit's retry policy: {denied}");
                assert_eq!(denied["error"]["code"],"COMMERCE_UNAVAILABLE");
                let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();
                assert_eq!(effects,(0,0,0));
                let after_count:i64=sqlx::query_scalar("SELECT count(*) FROM public.commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();
                assert_eq!(after_count,before_count);assert_eq!(money_snapshot(db,company).await,before_money);
                writer.commit().await.unwrap();
                // A permanent Money write failure must roll back the entire
                // actual HTTP unit, including the new order and ledger rows.
                let before_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();
                let before_money=money_snapshot(db,company).await;
                sqlx::raw_sql("CREATE SEQUENCE commerce_money_permanent_calls; CREATE FUNCTION commerce_money_permanent_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='company' AND NEW.source_type IN ('commerce_order_revision','commerce_order_line_revision') THEN PERFORM nextval('commerce_money_permanent_calls'); RAISE EXCEPTION 'synthetic Money permanent failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER commerce_money_permanent_fault BEFORE INSERT ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION commerce_money_permanent_fault()").execute(db.raw()).await.unwrap();
                let mut permanent=make();permanent.headers_mut().insert("idempotency-key","money-permanent-failure".parse().unwrap());
                let (status,failed)=json(app.clone().oneshot(permanent).await.unwrap()).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"permanent Money failure: {failed}");assert_eq!(failed["error"]["code"],"COMMERCE_UNAVAILABLE");
                let (calls,called):(i64,bool)=sqlx::query_as("SELECT last_value,is_called FROM commerce_money_permanent_calls").fetch_one(db.raw()).await.unwrap();assert!(called&&calls>=1,"the Money trigger must actually fire");
                let after_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();assert_eq!(after_orders,before_orders);assert_eq!(money_snapshot(db,company).await,before_money);
                let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='money-permanent-failure'),(SELECT count(*) FROM audit_events WHERE command_id='money-permanent-failure'),(SELECT count(*) FROM outbox_events WHERE command_id='money-permanent-failure')").fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
                sqlx::raw_sql("DROP TRIGGER commerce_money_permanent_fault ON money_transaction_snapshots; DROP FUNCTION commerce_money_permanent_fault(); DROP SEQUENCE commerce_money_permanent_calls").execute(db.raw()).await.unwrap();
                // Sequences are nontransactional: the first snapshot insert
                // raises 40001, while retry must execute the complete unit.
                sqlx::raw_sql("CREATE SEQUENCE commerce_money_retry_calls; CREATE FUNCTION commerce_money_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='company' AND NEW.source_type IN ('commerce_order_revision','commerce_order_line_revision') AND nextval('commerce_money_retry_calls')=1 THEN RAISE EXCEPTION 'synthetic Money serialization failure' USING ERRCODE='40001'; END IF; RETURN NEW; END $$; CREATE TRIGGER commerce_money_retry_fault BEFORE INSERT ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION commerce_money_retry_fault()").execute(db.raw()).await.unwrap();
            }
            let (status,value)=json(app.clone().oneshot(make()).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"save {value}");assert_eq!(value["context"]["companyId"],company);let id=value["data"]["id"].as_str().unwrap().to_owned();assert_eq!(value["data"]["version"],1);
            if company=="company" {let calls:i64=sqlx::query_scalar("SELECT last_value FROM commerce_money_retry_calls").fetch_one(db.raw()).await.unwrap();assert_eq!(calls,5,"one failed snapshot attempt plus four committed snapshots");sqlx::raw_sql("DROP TRIGGER commerce_money_retry_fault ON money_transaction_snapshots; DROP FUNCTION commerce_money_retry_fault(); DROP SEQUENCE commerce_money_retry_calls").execute(db.raw()).await.unwrap();}
            let (status,replay)=json(app.clone().oneshot(make()).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"save replay {replay}");assert_eq!(replay,value);
            if company=="company" {
                // Replay must return before any Finance semantic/lock work.
                let mut replay_writer=db.raw().begin().await.unwrap();
                sqlx::query("LOCK TABLE public.money_transaction_snapshots IN ROW EXCLUSIVE MODE").execute(&mut *replay_writer).await.unwrap();
                let (replay_status,replay_again)=json(app.clone().oneshot(make()).await.unwrap()).await;assert_eq!(replay_status,StatusCode::OK,"save replay under Finance contention {replay_again}");assert_eq!(replay_again,value);
                replay_writer.rollback().await.unwrap();
            }
            let (status,detail)=json(app.clone().oneshot(request(Some(token),Some(company),Some(assignment),Some(revision),Some(version),&id,false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"save readback {detail}");assert_eq!(detail["data"]["order"]["company_id"],company);assert_eq!(detail["data"]["order"]["party_id"],party);
            let persisted:(String,String,String,i64,i64)=sqlx::query_as("SELECT company_id,party_id,price_list_id,version,(SELECT count(*) FROM commerce_order_lines l WHERE l.order_id=o.id AND l.product_id=$2) FROM commerce_orders o WHERE id=$1").bind(&id).bind(product).fetch_one(db.raw()).await.unwrap();assert_eq!(persisted,(company.into(),party.into(),list.into(),1,1));
            let (other_token,other_company,other_assignment,other_revision,other_version)=if company=="company"{(b,"foreign","sales-foreign","8","1")}else{(a,"company","finance-fixture-a","7","4")};let (status,_)=json(app.clone().oneshot(request(Some(other_token),Some(other_company),Some(other_assignment),Some(other_revision),Some(other_version),&id,false,"unused")).await.unwrap()).await;assert_eq!(status,StatusCode::NOT_FOUND);
            let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(1,1,1));
            let actor=if company=="company"{"fixture-a"}else{"fixture-b"};assert_saved_effects(db,key,&id,company,actor,assignment,"commerce.order.create",1).await;
            let before_negative=snapshot(db,&id).await;assert_eq!(before_negative["lines"].as_array().unwrap().len(),1);
            let line_v1=before_negative["lines"][0]["id"].as_str().unwrap().to_owned();
            let (rate,usd)=if company=="company"{("commerce-fx-race","50")}else{("commerce-fx-b","20")};
            let converted_row=serde_json::json!({"source_type":"commerce_order_revision","source_id":format!("{sales_id}:1"),"amount_kind":"total","transaction_date":"2026-11-10","currency":"THB","currency_scale":2,"amount_units":"1000","usd_units":"25","fx_rate_id":"commerce-fx-a","used_latest_fallback":false});
            let mut expected_v1=expected_money(&id,&line_v1,1,rate,usd);if company=="company"{expected_v1.push(converted_row.clone());}expected_v1.sort_by(|a,b|(a["source_type"].as_str(),a["source_id"].as_str(),a["amount_kind"].as_str()).cmp(&(b["source_type"].as_str(),b["source_id"].as_str(),b["amount_kind"].as_str())));
            let money_v1=serde_json::Value::Array(expected_v1);
            assert_eq!(money_snapshot(db,company).await,money_v1,"create and replay retain exactly 1+3N Money rows");
            let update_body=serde_json::json!({"kind":"quote","partyId":party,"orderDate":"2026-11-10","dueDate":"2026-11-15","note":"Reviewed update","priceListId":list,"expectedVersion":1,"lines":[{"productId":product,"quantity":"1","price":"10","taxBps":0}]});
            let update=|update_key:&str,payload:&serde_json::Value|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/commerce/orders/{id}")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={token}")).header("x-erpx-context-company",company).header("x-erpx-context-assignment",assignment).header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version",version).header("content-type","application/json").header("idempotency-key",update_key).body(Body::from(payload.to_string())).unwrap();
            let (foreign_party,foreign_product,foreign_list,suffix)=if company=="company"{("party-b","product-b","price-b","a")}else{("party-a","product-a","price-a","b")};
            let mut party_bad=update_body.clone();party_bad["partyId"]=foreign_party.into();party_bad.as_object_mut().unwrap().remove("priceListId");
            let mut product_bad=update_body.clone();product_bad["lines"][0]["productId"]=foreign_product.into();product_bad.as_object_mut().unwrap().remove("priceListId");
            let mut list_bad=update_body.clone();list_bad["priceListId"]=foreign_list.into();
            let bads=[(format!("foreign-party-{suffix}"),party_bad,"CONFLICT"),(format!("foreign-product-{suffix}"),product_bad,"PRICE_LIST_ITEM_REQUIRED"),(format!("foreign-list-{suffix}"),list_bad,"PRICE_LIST_INVALID")];
            for (bad_key,payload,code) in &bads {let (status,value)=json(app.clone().oneshot(update(bad_key,payload)).await.unwrap()).await;assert_eq!(status,if *code=="CONFLICT"{StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY},"foreign reference {value}");assert_eq!(value["error"]["code"],*code);let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(bad_key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));assert_eq!(snapshot(db,&id).await,before_negative,"all order and line fields must survive {bad_key}");}
            assert_eq!(money_snapshot(db,company).await,money_v1,"failed writes leave Money unchanged");
            let before:(i64,String,i64)=sqlx::query_as("SELECT version,note,(SELECT count(*) FROM commerce_order_lines l WHERE l.order_id=o.id AND l.product_id=$2) FROM commerce_orders o WHERE id=$1").bind(&id).bind(product).fetch_one(db.raw()).await.unwrap();assert_eq!(before,(1,"Reviewed quote".into(),1));
            let update_key=format!("update-company-{suffix}");let (status,updated)=json(app.clone().oneshot(update(&update_key,&update_body)).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"update {updated}");assert_eq!(updated["data"]["id"],id);assert_eq!(updated["data"]["version"],2);
            let (status,update_replay)=json(app.clone().oneshot(update(&update_key,&update_body)).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"update replay {update_replay}");assert_eq!(update_replay,updated);assert_saved_effects(db,&update_key,&id,company,actor,assignment,"commerce.order.update",2).await;
            let after_update=snapshot(db,&id).await;assert_eq!(after_update["lines"].as_array().unwrap().len(),1);
            let line_v2=after_update["lines"][0]["id"].as_str().unwrap();
            let mut expected=expected_money(&id,&line_v1,1,rate,usd);
            expected.extend(expected_money(&id,line_v2,2,rate,usd));
            if company=="company"{expected.push(converted_row);}
            expected.sort_by(|a,b| (a["source_type"].as_str(),a["source_id"].as_str(),a["amount_kind"].as_str()).cmp(&(b["source_type"].as_str(),b["source_id"].as_str(),b["amount_kind"].as_str())));
            let money_v2=serde_json::Value::Array(expected);
            assert_eq!(money_snapshot(db,company).await,money_v2,"update and replay retain exactly 1+3N rows per version");
            let mut changed_payload=update_body.clone();changed_payload["note"]="Changed retry intent".into();let (status,conflict)=json(app.clone().oneshot(update(&update_key,&changed_payload)).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"{conflict}");assert_eq!(conflict["error"]["code"],"IDEMPOTENCY_CONFLICT");assert_eq!(snapshot(db,&id).await,after_update);
            let mut stale_replay=update(&update_key,&update_body);stale_replay.headers_mut().insert("x-erpx-context-revision","6".parse().unwrap());let (status,denied)=json(app.clone().oneshot(stale_replay).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"{denied}");assert_eq!(denied["error"]["code"],"WORK_CONTEXT_CHANGED");assert_eq!(snapshot(db,&id).await,after_update);
            assert_saved_effects(db,&update_key,&id,company,actor,assignment,"commerce.order.update",2).await;
            let stale_key=format!("stale-update-{suffix}");let (status,stale)=json(app.clone().oneshot(update(&stale_key,&update_body)).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"{stale}");assert_eq!(stale["error"]["code"],"CONFLICT");assert_eq!(snapshot(db,&id).await,after_update,"stale CAS leaves every order and line field unchanged");
            assert_eq!(money_snapshot(db,company).await,money_v2,"conflicts and stale retries leave Money unchanged");
            let after:(String,String,i64,String,i64)=sqlx::query_as("SELECT company_id,party_id,version,note,(SELECT count(*) FROM commerce_order_lines l WHERE l.order_id=o.id AND l.product_id=$2) FROM commerce_orders o WHERE id=$1").bind(&id).bind(product).fetch_one(db.raw()).await.unwrap();assert_eq!(after,(company.into(),party.into(),2,"Reviewed update".into(),1));
            for (effect_key,n) in [(&update_key,1_i64),(&stale_key,0)]{let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(effect_key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(n,n,n));}
        }
        let rows:Vec<(String,String,String,i64)>=sqlx::query_as("SELECT id,company_id,state,version FROM commerce_orders WHERE id IN ('order-a','order-b') ORDER BY id").fetch_all(db.raw()).await.unwrap();assert_eq!(rows,vec![("order-a".into(),"company".into(),"submitted".into(),2),("order-b".into(),"foreign".into(),"submitted".into(),2)]);
        for (key,expected) in [("submit-company-a",1_i64),("submit-company-b",1),("missing-context",0),("stale-context",0),("forged-context",0),("unauth-duplicate",0),("unauth-slug",0),("unauth-partial",0),("duplicate-context",0),("invalid-slug",0),("partial-context",0),("stale-assignment",0),("wrong-assignment",0)] {let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(expected,expected,expected),"{key}");}
        // The absent-key writer commits after the first blocked request; the
        // same key then observes the newly committed real party and succeeds.
        let before_orders:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();let before_money=money_snapshot(db,"company").await;
        let mut writer=db.raw().begin().await.unwrap();sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-later','company','customer','PLATER','Later customer','THB',30,0,'active',1,0,0)").execute(&mut *writer).await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='public.finance_parties'::regclass AND mode='RowExclusiveLock' AND granted)").bind(pid).fetch_one(db.raw()).await.unwrap();assert!(held);
        let (status,denied)=json(app.clone().oneshot(party_save("party-later","party-source-positive")).await.unwrap()).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"absent-party writer {denied}");assert_eq!(denied["error"]["code"],"COMMERCE_UNAVAILABLE");let count:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='company'").fetch_one(db.raw()).await.unwrap();assert_eq!(count,before_orders);assert_eq!(money_snapshot(db,"company").await,before_money);let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='party-source-positive'),(SELECT count(*) FROM audit_events WHERE command_id='party-source-positive'),(SELECT count(*) FROM outbox_events WHERE command_id='party-source-positive')").fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        writer.commit().await.unwrap();let (status,created)=json(app.clone().oneshot(party_save("party-later","party-source-positive")).await.unwrap()).await;assert_eq!(status,StatusCode::OK,"fresh party after commit {created}");let id=created["data"]["id"].as_str().unwrap();let stored:(String,String,i64)=sqlx::query_as("SELECT company_id,party_id,version FROM commerce_orders WHERE id=$1").bind(id).fetch_one(db.raw()).await.unwrap();assert_eq!(stored,("company".into(),"party-later".into(),1));assert_saved_effects(db,"party-source-positive",id,"company","fixture-a","finance-fixture-a","commerce.order.create",1).await;
        let (status,replay)=json(app.clone().oneshot(party_save("party-later","party-source-positive")).await.unwrap()).await;assert_eq!(status,StatusCode::OK);assert_eq!(replay,created);let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='party-source-positive'),(SELECT count(*) FROM audit_events WHERE command_id='party-source-positive'),(SELECT count(*) FROM outbox_events WHERE command_id='party-source-positive')").fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(1,1,1));
        // Deferred legitimate Sales lifecycle changes test selected-list,
        // preferred-list and genuine no-list observations in the full unit.
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-auto','company','customer','PAUTO','Auto customer','THB',30,0,'active',1,0,0),('party-no-list','foreign','customer','PNONE','No list customer','THB',30,0,'active',1,0,0); INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,status,version,created_by,prepared_by,created_at,updated_at) VALUES('price-generic','company','Generic','THB',NULL,'2026-11-01','2026-11-30',0,'draft',1,'fixture-a','fixture-a',100,100); INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES('price-generic-item','price-generic','product-a',1000,0,0); UPDATE sales_price_lists SET status='active',approved_by='fixture-b',approval_reason='Independent review',activated_at=1000 WHERE id='price-generic'").execute(db.raw()).await.unwrap();
        sqlx::raw_sql("CREATE SEQUENCE commerce_pricing_drift_calls; CREATE FUNCTION commerce_pricing_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='pricing-auto-drift' THEN PERFORM nextval('commerce_pricing_drift_calls'); INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,status,version,created_by,prepared_by,created_at,updated_at) VALUES('price-preferred-late','company','Preferred','THB','party-auto','2026-11-01','2026-11-30',0,'draft',1,'fixture-a','fixture-a',100,100); INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES('price-preferred-late-item','price-preferred-late','product-a',1000,0,0); UPDATE sales_price_lists SET status='active',approved_by='fixture-b',approval_reason='Independent review',activated_at=2000 WHERE id='price-preferred-late'; ELSIF NEW.command_id='pricing-explicit-drift' THEN PERFORM nextval('commerce_pricing_drift_calls'); UPDATE sales_price_lists SET status='superseded',version=version+1 WHERE id='price-generic'; ELSIF NEW.command_id='pricing-absence-drift' THEN PERFORM nextval('commerce_pricing_drift_calls'); INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,status,version,created_by,prepared_by,created_at,updated_at) VALUES('price-foreign-late','foreign','Late generic','THB',NULL,'2026-11-01','2026-11-30',0,'draft',1,'fixture-b','fixture-b',100,100); INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES('price-foreign-late-item','price-foreign-late','product-b',1000,0,0); UPDATE sales_price_lists SET status='active',approved_by='fixture-a',approval_reason='Independent review',activated_at=2000 WHERE id='price-foreign-late'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER commerce_pricing_drift AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_pricing_drift()").execute(db.raw()).await.unwrap();
        let pricing_snapshot=||async {sqlx::query_scalar::<_,serde_json::Value>("SELECT jsonb_build_array((SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM sales_price_lists x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM sales_price_list_items x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM commerce_orders x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.id),'[]') FROM commerce_order_lines x),(SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY x.source_type,x.source_id,x.amount_kind),'[]') FROM money_transaction_snapshots x))").fetch_one(db.raw()).await.unwrap()};
        for (token,company,assignment,revision,version,party,product,selected,key,code) in [(a,"company","finance-fixture-a","7","4","party-auto","product-a",None,"pricing-auto-drift","SALES_PRICING_SOURCE_CHANGED"),(a,"company","finance-fixture-a","7","4","party-auto","product-a",Some("price-generic"),"pricing-explicit-drift","COMMERCE_PRICE_SOURCE_CHANGED"),(b,"foreign","sales-foreign","8","1","party-no-list","product-b",None,"pricing-absence-drift","SALES_PRICING_SOURCE_CHANGED")] {
            let mut body=serde_json::json!({"kind":"quote","partyId":party,"orderDate":"2026-11-10","dueDate":"2026-11-15","note":"Sales witness drift","lines":[{"productId":product,"quantity":"1","price":"10","taxBps":0}]});if let Some(id)=selected{body["priceListId"]=id.into();}
            let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/commerce/orders").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={token}")).header("x-erpx-context-company",company).header("x-erpx-context-assignment",assignment).header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version",version).header("content-type","application/json").header("idempotency-key",key).body(Body::from(body.to_string())).unwrap();
            let before=pricing_snapshot().await;let previous:(i64,bool)=sqlx::query_as("SELECT last_value,is_called FROM commerce_pricing_drift_calls").fetch_one(db.raw()).await.unwrap();let (status,denied)=json(app.clone().oneshot(request).await.unwrap()).await;assert_eq!(status,StatusCode::CONFLICT,"{key}: {denied}");assert_eq!(denied["error"]["code"],code,"{key}");let after:(i64,bool)=sqlx::query_as("SELECT last_value,is_called FROM commerce_pricing_drift_calls").fetch_one(db.raw()).await.unwrap();assert!(after.1&&(!previous.1||after.0>previous.0),"{key} deferred trigger must fire");assert_eq!(pricing_snapshot().await,before,"{key} entire pricing/order/Money rollback");let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(db.raw()).await.unwrap();assert_eq!(effects,(0,0,0));
        }
        sqlx::raw_sql("DROP TRIGGER commerce_pricing_drift ON outbox_events; DROP FUNCTION commerce_pricing_drift(); DROP SEQUENCE commerce_pricing_drift_calls").execute(db.raw()).await.unwrap();
    }).await;
}
