//! Composed native POS command HTTP over the real Account session/IAM and
//! PostgreSQL. This case proves a retail sale create with the real Organization
//! issuer and Finance period source; journal-posting commands remain gated.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_pos::public::commands_http::{
    PosCommandHttpTarget, pos_command_router,
};
use erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders;
use erpx_service::pos::{PosBackend, PosConfiguration, PosService};
use std::sync::Arc;
use tower::ServiceExt;

fn cookie(token: &erpx_server_identity_access::public::OpaqueSecret) -> String {
    format!("__Host-erpx-product={}", token.expose())
}

fn request_with(
    token: &erpx_server_identity_access::public::OpaqueSecret,
    assignment: &str,
    assignment_version: &str,
    path: &str,
    body: &str,
    key: &str,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie(token))
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", assignment)
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", assignment_version)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn request_op(p: &review::Prepared, path: &str, body: &str, key: &str) -> Request<Body> {
    request_with(
        &p.maker_token,
        "finance-fixture-a",
        "4",
        path,
        body,
        key,
    )
}

fn request(p: &review::Prepared, body: &str, key: &str) -> Request<Body> {
    request_op(p, "/api/v1/holdings/acme/pos/commands/sale.create", body, key)
}

fn review_request(p: &review::Prepared, body: &str, key: &str) -> Request<Body> {
    request_with(
        &p.reviewer_token,
        "finance-fixture-b",
        "3",
        "/api/v1/holdings/acme/pos/commands/shift.review",
        body,
        key,
    )
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

async fn grant_pos(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_finance_review',2,'[\"finance.read\",\"records.read\",\"pos.read\",\"pos.sell\",\"pos.approve\",\"pos.manage\",\"module.pos.read\"]','[]','[\"holding\",\"branch\"]','fixture',0);\
         UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_finance_review';\
         UPDATE assignments SET modules_json='[\"finance\",\"pos\"]',version=4 WHERE id='finance-fixture-a';\
         UPDATE assignments SET modules_json='[\"finance\",\"pos\"]' WHERE id='finance-fixture-b';\
         UPDATE active_work_contexts SET assignment_version=4 WHERE assignment_id='finance-fixture-a'",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

async fn seed_pos(p: &review::Prepared) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('branch','branch','branch','Branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) \
         VALUES('warehouse','company','branch','W1','Retail warehouse') ON CONFLICT DO NOTHING;\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) \
         VALUES('chart','company','Chart','THB','fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) \
         VALUES('period-2027','company','2027','2027-01-01','2027-12-31',0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at) \
         VALUES('cash','company','chart','100','Cash','asset',0,0),('difference','company','chart','590','Cash difference','expense',0,0),('advance','company','chart','210','POS customer advances','liability',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_settings(company_id,enabled,currency,accounts_json,accounting_prepared_by,accounting_approved_by,version) \
         VALUES('company',true,'THB','{\"chartId\":\"chart\",\"cash\":\"cash\",\"cashTransfer\":\"transfer\",\"cashDifference\":\"difference\",\"edc\":\"edc\",\"receivable\":\"receivable\",\"advance\":\"advance\",\"bank\":\"bank\"}','fixture-a','fixture-b',1) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) \
         VALUES('register','company','branch','warehouse','Retail','retail',true,1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tax_bps,status,version) \
         VALUES('product','company','SALE-ADD','Sale add product',10000,700,'active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_catalog(product_id,company_id,kind,station,recipe_json,modifiers_json,tax_inclusive,version,enabled) \
         VALUES('product','company','service','Shop','[]','[]',false,1,true) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_register_catalog(register_id,product_id,enabled) \
         VALUES('register','product',true) ON CONFLICT DO NOTHING;\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date) \
         VALUES('shift','register','company','branch','fixture-a',1800000000000,100000,'open',1,'2027-01-15') ON CONFLICT DO NOTHING;\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version,business_date,closed_by,closed_at,counted_minor,expected_minor,variance_minor) \
         VALUES('shift-review','register','company','branch','fixture-a',1800000000000,100000,'closed',1,'2027-01-15','fixture-a',1800003600000,99000,100000,-1000) ON CONFLICT DO NOTHING",
    )
    .execute(p.system.holdings[0].raw())
    .await
    .unwrap();
}

fn router(p: &review::Prepared) -> axum::Router {
    let service = PosService::new(
        p.authorizer.clone(),
        PosConfiguration::new(
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
        ),
        p.system.scenario.clock.clone(),
    );
    let backend = PosBackend::new(
        Arc::new(review::account::fixture::account::policy()),
        Arc::new(FieldServiceReadScopeHeaders),
        Arc::new(service),
    );
    pos_command_router(vec![PosCommandHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(backend),
    }])
    .unwrap()
}

#[tokio::test]
async fn pos_http_creates_a_retail_sale_with_real_organization_and_finance_sources() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_pos(&p).await;
        seed_pos(&p).await;
        let app = router(&p);

        let created = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail","currency":"THB"}"#,
                "pos-http-sale-1",
            ))
            .await
            .unwrap();
        let status = created.status();
        let created = json(created).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let id = created["data"]["id"].as_str().unwrap().to_owned();

        let sale: (String, String, i64, String, String) = sqlx::query_as(
            "SELECT state,currency,version,number,issuer_json FROM pos_sales WHERE id=$1 AND company_id='company'",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(sale.0, "open");
        assert_eq!(sale.1, "THB");
        assert_eq!(sale.2, 1);
        assert!(sale.3.starts_with("POS-20270115-"), "{}", sale.3);
        assert!(sale.4.contains("Synthetic Acme"), "{}", sale.4);

        let replay = app
            .clone()
            .oneshot(request(
                &p,
                r#"{"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail","currency":"THB"}"#,
                "pos-http-sale-1",
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert_eq!(json(replay).await["data"]["id"], id);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_sales WHERE company_id='company'")
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(count, 1);

        let added = app
            .clone()
            .oneshot(request_op(
                &p,
                "/api/v1/holdings/acme/pos/commands/sale.add",
                &format!(
                    r#"{{"id":"{id}","expectedVersion":1,"productId":"product","quantityMilli":1000,"modifierIds":[],"discountMinor":0,"note":"Added line"}}"#
                ),
                "pos-http-sale-add-1",
            ))
            .await
            .unwrap();
        let status = added.status();
        let added = json(added).await;
        assert_eq!(status, StatusCode::OK, "{added}");
        assert_eq!(added["data"]["id"], id);
        assert_eq!(added["data"]["version"], 2);
        let line: (i64, i64, i64) = sqlx::query_as(
            "SELECT quantity_milli,subtotal_minor,tax_minor FROM pos_lines WHERE sale_id=$1",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(line, (1000, 10_000, 700));
        let totals: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT subtotal_minor,tax_minor,total_minor,version FROM pos_sales WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(totals, (10_000, 700, 10_700, 2));

        let replay = app
            .clone()
            .oneshot(request_op(
                &p,
                "/api/v1/holdings/acme/pos/commands/sale.add",
                &format!(
                    r#"{{"id":"{id}","expectedVersion":1,"productId":"product","quantityMilli":1000,"modifierIds":[],"discountMinor":0,"note":"Added line"}}"#
                ),
                "pos-http-sale-add-1",
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let lines: i64 = sqlx::query_scalar("SELECT count(*) FROM pos_lines WHERE sale_id=$1")
            .bind(&id)
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(lines, 1);

        let paid = app
            .clone()
            .oneshot(request_op(
                &p,
                "/api/v1/holdings/acme/pos/commands/payment.create",
                &format!(
                    r#"{{"id":"{id}","expectedVersion":2,"method":"cash","amountMinor":10700,"tenderedMinor":10700,"reference":""}}"#
                ),
                "pos-http-payment-1",
            ))
            .await
            .unwrap();
        let status = paid.status();
        let paid = json(paid).await;
        assert_eq!(status, StatusCode::OK, "{paid}");
        let payment_id = paid["data"]["id"].as_str().unwrap().to_owned();
        let payment: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT state,amount_minor,finance_payment_id FROM pos_payments WHERE id=$1 AND company_id='company'",
        )
        .bind(&payment_id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(payment.0, "succeeded");
        assert_eq!(payment.1, 10_700);
        assert!(payment.2.is_some());
        let receipt: (String, i64, String) = sqlx::query_as(
            "SELECT status,amount_minor,journal_id FROM finance_payments WHERE id=$1",
        )
        .bind(payment.2.clone().unwrap())
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(receipt.0, "settled");
        assert_eq!(receipt.1, 10_700);
        assert_eq!(receipt.2, format!("journal-receipt:{payment_id}"));
        let journal: (String, i64, i64) = sqlx::query_as(
            "SELECT status,COALESCE(SUM(debit_minor),0)::bigint,COALESCE(SUM(credit_minor),0)::bigint FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id=$1 GROUP BY j.status",
        )
        .bind(format!("journal-receipt:{payment_id}"))
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(journal.0, "posted");
        assert_eq!(journal.1, 10_700);
        assert_eq!(journal.2, 10_700);
        let sale_state: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM pos_sales WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(sale_state, ("payment_pending".into(), 3));
    })
    .await;
}

/// A closed shift with a cash variance is reviewed by an independent Account
/// role. The native POS command posts a real Finance journal on the same
/// serializable transaction: balanced lines, deterministic id, POS posting
/// receipt and reviewed shift state.
#[tokio::test]
async fn pos_http_reviews_a_closed_shift_and_posts_a_balanced_native_journal() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        grant_pos(&p).await;
        seed_pos(&p).await;
        let app = router(&p);

        let reviewed = app
            .clone()
            .oneshot(review_request(
                &p,
                r#"{"id":"shift-review","expectedVersion":1,"reason":"Count reviewed"}"#,
                "pos-http-review-1",
            ))
            .await
            .unwrap();
        let status = reviewed.status();
        let reviewed = json(reviewed).await;
        assert_eq!(status, StatusCode::OK, "{reviewed}");
        assert_eq!(reviewed["data"]["id"], "shift-review");
        assert_eq!(reviewed["data"]["version"], 2);

        let shift: (String, Option<String>, i64) = sqlx::query_as(
            "SELECT state,reviewed_by,version FROM pos_shifts WHERE id='shift-review' AND company_id='company'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(shift.0, "reviewed");
        assert_eq!(shift.1.as_deref(), Some("fixture-b"));
        assert_eq!(shift.2, 2);

        let journal: (String, String, String, String) = sqlx::query_as(
            "SELECT id,status,source_type,period_id FROM finance_journals WHERE company_id='company' AND source_type='pos' AND source_id='shift-review'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(journal.0, "journal-shift:shift-review");
        assert_eq!(journal.1, "posted");
        assert_eq!(journal.2, "pos");
        assert_eq!(journal.3, "period-2027");
        let (debit, credit, lines): (i64, i64, i64) = sqlx::query_as(
            "SELECT COALESCE(sum(debit_minor),0)::bigint,COALESCE(sum(credit_minor),0)::bigint,count(*) FROM finance_journal_lines WHERE journal_id='journal-shift:shift-review'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(lines, 2);
        assert_eq!(debit, 1000);
        assert_eq!(credit, 1000);
        let accounts: Vec<String> = sqlx::query_scalar(
            "SELECT account_id FROM finance_journal_lines WHERE journal_id='journal-shift:shift-review' ORDER BY ordinal",
        )
        .fetch_all(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(accounts, ["difference", "cash"]);

        let posting: String = sqlx::query_scalar(
            "SELECT journal_id FROM pos_postings WHERE source_key='shift:shift-review' AND company_id='company'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(posting, "journal-shift:shift-review");

        let replay = app
            .clone()
            .oneshot(review_request(
                &p,
                r#"{"id":"shift-review","expectedVersion":1,"reason":"Count reviewed"}"#,
                "pos-http-review-1",
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let journals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_journals WHERE company_id='company' AND source_id='shift-review'",
        )
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(journals, 1);
    })
    .await;
}
