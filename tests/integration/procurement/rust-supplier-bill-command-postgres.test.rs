//! Native Procurement supplier-bill command on migrated PostgreSQL.
//!
//! The purchase admission (capability foundations, assignment authority and
//! source images) runs in composition ahead of this port. This proof covers
//! the procurement-owned unit only: the exact denial codes, the registered
//! `procurement_stmt_1..10` commit shapes, idempotent replay and the
//! company/branch isolation fences. Cross-owner staging (Commerce
//! receipt-version and invoice links, Finance draft-void, Collaboration
//! notifications) is supplied by an explicit test hook that records its calls;
//! the native crate performs no cross-owner writes itself.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_procurement::public::supplier_bill_command::{
    PgSupplierBillCommand, SupplierBillCommandActor, SupplierBillCommandRepository,
    SupplierBillCommandStaging, SupplierBillCommandFuture, SupplierBillDecidePlan,
    capture_supplier_bill, decide_supplier_bill, save_supplier_bill_policy,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use sqlx::Connection;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

struct TestStaging {
    calls: Vec<String>,
}

impl TestStaging {
    fn new() -> Self {
        Self { calls: Vec::new() }
    }
}

impl SupplierBillCommandStaging for TestStaging {
    type Error = PgTransactionError<BusinessError>;

    fn stage_receipt_version<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("receipt-version:{fulfillment_id}"));
            Ok(())
        })
    }

    fn stage_receipt_links<'a>(
        &'a mut self,
        match_id: &'a str,
        invoice_id: Option<&'a str>,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.calls.push(format!(
                "receipt-links:{match_id}:{}",
                invoice_id.unwrap_or("-")
            ));
            Ok(())
        })
    }

    fn stage_draft_void<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("draft-void:{}", plan.match_id));
            Ok(())
        })
    }

    fn notify_bill<'a>(
        &'a mut self,
        match_id: &'a str,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("notify:{match_id}:{key}"));
            Ok(())
        })
    }
}

fn actor(company: &str, user: &str, permissions: &[&str]) -> SupplierBillCommandActor {
    SupplierBillCommandActor {
        user_id: user.into(),
        assignment_id: format!("assignment-{user}"),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: Some("branch".into()),
        department_id: None,
        revision: 1,
        assignment_version: 1,
        permissions: permissions.iter().map(|value| value.to_string()).collect(),
        expires_at: NOW.get() + 60_000,
    }
}

fn preparer(user: &str) -> SupplierBillCommandActor {
    actor("company", user, &["finance.write", "commerce.read"])
}

fn reviewer(user: &str) -> SupplierBillCommandActor {
    actor(
        "company",
        user,
        &["finance.read", "finance.write", "finance.approve"],
    )
}

fn capture_input(reference: &str, fulfillment_id: &str, version: i64) -> serde_json::Value {
    serde_json::json!({
        "fulfillmentId": fulfillment_id,
        "expectedVersion": version,
        "reference": reference,
        "invoiceDate": "2026-09-03",
        "dueDate": "2026-09-10",
        "evidence": "Actual supplier invoice agrees with the received goods",
        "lines": [
            {"fulfillmentLineId": "fulfill-line-1", "quantity": "2", "price": "10.00", "taxBps": 0}
        ],
    })
}

fn decide_input(action: &str, version: i64) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "expectedVersion": version,
        "reason": "Independent reviewer confirms the supplier invoice and its AP draft",
    })
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company','company','active',1),\
         ('branch','branch','branch','Branch','company','company','branch','active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('supplier','company','supplier','SUP-1','Bounded supplier','THB',10,10),\
         ('supplier-2','company','supplier','SUP-2','Second supplier','THB',10,10);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('chart-1','company','Main chart','THB','fixture',10,10);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) VALUES \
         ('clearing','chart-1','company','1100','Goods receipt clearing','asset','THB','active',10,10),\
         ('inventory-acc','chart-1','company','1200','Inventory','asset','THB','active',10,10),\
         ('payable','chart-1','company','2100','Accounts payable','liability','THB','active',10,10),\
         ('tax-acc','chart-1','company','2200','Input tax','asset','THB','active',10,10),\
         ('receivable','chart-1','company','1101','Accounts receivable','asset','THB','active',10,10),\
         ('revenue','chart-1','company','4000','Revenue','revenue','THB','active',10,10),\
         ('cogs','chart-1','company','5000','COGS','expense','THB','active',10,10),\
         ('output-tax','chart-1','company','2201','Output tax','liability','THB','active',10,10),\
         ('cash','chart-1','company','1000','Cash','asset','THB','active',10,10),\
         ('expense','chart-1','company','5100','Price variance','expense','THB','active',10,10);\
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES \
         ('company','chart-1','THB','receivable','payable','revenue','inventory-acc','clearing','cogs','tax-acc','output-tax','cash');\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES \
         ('warehouse','company','branch','WH-1','Main warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES \
         ('product-a','company','SKU-A','Bounded part','each',1000,0),\
         ('product-b','company','SKU-B','Second part','each',2000,0);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('order-1','company','branch',NULL,'purchase','PO-1','supplier','warehouse','THB','2026-09-01','2026-09-10','confirmed',2000,0,'owner',10,10),\
         ('order-2','company','branch',NULL,'purchase','PO-2','supplier-2','warehouse','THB','2026-09-01','2026-09-10','confirmed',1000,0,'owner',10,10);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES \
         ('order-line-1','order-1','product-a','Bounded part',2000,0,1000,0,2000,0,0),\
         ('order-line-2','order-2','product-b','Second part',1000,0,2000,0,2000,0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES \
         ('period-1','company','Sep 2026','2026-09-01','2026-09-30','open',10); \
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,version,created_by,created_at,updated_at) VALUES \
         ('journal-1','company','chart-1','period-1','J-1','2026-09-02','THB','posted',1,'owner',10,10),\
         ('journal-2','company','chart-1','period-1','J-2','2026-09-02','THB','posted',1,'owner',10,10);\
         INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES \
         ('jl-1a','journal-1','inventory-acc','',2000,0,2000,0,0),\
         ('jl-1b','journal-1','clearing','',0,2000,0,2000,1),\
         ('jl-2a','journal-2','inventory-acc','',2000,0,2000,0,0),\
         ('jl-2b','journal-2','clearing','',0,2000,0,2000,1);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,journal_id,invoice_id,version) VALUES \
         ('fulfillment-1','order-1','R-1','2026-09-02','owner',10,'journal-1',NULL,1),\
         ('fulfillment-2','order-2','R-2','2026-09-02','owner',10,'journal-2',NULL,1);\
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES \
         ('fulfill-line-1','fulfillment-1','order-line-1',2000,2000,2000,0),\
         ('fulfill-line-2','fulfillment-2','order-line-2',1000,2000,2000,0);\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','bill_finance_approver','Bill Finance approver',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','bill_finance_approver',1,'[\"finance.approve\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES \
         ('company','reviewer','principal-reviewer','person-reviewer','active'),\
         ('company','preparer','principal-preparer','person-preparer','active');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id,revoked_at,parent_id) VALUES \
         ('assignment-reviewer','reviewer','bill_finance_approver','[]','company',NULL,0,9007199254740991,1,'holding',NULL,NULL,NULL),\
         ('assignment-preparer','preparer','bill_finance_approver','[]','company','branch',0,9007199254740991,1,'branch','branch',NULL,NULL);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn run_capture(
    pool: &DatabasePool,
    actor: SupplierBillCommandActor,
    input: serde_json::Value,
    key: &str,
) -> Result<(erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, Vec<String>), BusinessError> {
    let calls = PgUnitOfWork::new(pool.clone())
        .run::<(erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, Vec<String>), BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let key = CommandKey::new(key)?;
                let mut repository = PgSupplierBillCommand::new(connection);
                let mut staging = TestStaging::new();
                let receipt =
                    capture_supplier_bill(&actor, &mut repository, &mut staging, &input, key.as_str(), NOW).await?;
                Ok((receipt, staging.calls))
            })
        })
        .await
        .map_err(business)?;
    Ok(calls)
}

async fn run_decide(
    pool: &DatabasePool,
    actor: SupplierBillCommandActor,
    match_id: &str,
    input: serde_json::Value,
    key: &str,
) -> Result<(erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, Vec<String>), BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<(erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, Vec<String>), BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.to_owned();
            let match_id = match_id.to_owned();
            Box::pin(async move {
                let key = CommandKey::new(key)?;
                let mut repository = PgSupplierBillCommand::new(connection);
                let mut staging = TestStaging::new();
                let receipt = decide_supplier_bill(
                    &actor,
                    &mut repository,
                    &mut staging,
                    &match_id,
                    &input,
                    key.as_str(),
                    NOW,
                )
                .await?;
                Ok((receipt, staging.calls))
            })
        })
        .await
        .map_err(business)
}

async fn run_policy(
    pool: &DatabasePool,
    actor: SupplierBillCommandActor,
    input: serde_json::Value,
    key: &str,
) -> Result<erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<erpx_server_procurement::public::supplier_bill_command::SupplierBillRecordReceipt, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let key = CommandKey::new(key)?;
                let mut repository = PgSupplierBillCommand::new(connection);
                save_supplier_bill_policy(&actor, &mut repository, &input, key.as_str(), NOW).await
            })
        })
        .await
        .map_err(business)
}

#[tokio::test]
async fn captures_bill_and_replays_idempotently() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let input = capture_input("REF-1", "fulfillment-1", 1);
        let (receipt, calls) = run_capture(&pool, preparer("preparer"), input.clone(), "capture-1")
            .await
            .unwrap();
        assert_eq!(receipt.id, "bill-match-capture-1");
        assert_eq!(receipt.version, 1);
        assert_eq!(receipt.receipt_count, Some(1));
        assert!(calls.iter().any(|call| call == "receipt-version:fulfillment-1"));
        assert!(calls.iter().any(|call| call.starts_with("notify:bill-match-capture-1")));

        let match_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM procurement_bill_matches WHERE id='bill-match-capture-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(match_count, 1);
        let receipt_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_bill_match_receipts WHERE match_id='bill-match-capture-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt_count, 1);
        let captured: i64 = sqlx::query_scalar(
            "SELECT captured_version FROM procurement_bill_match_receipts WHERE match_id='bill-match-capture-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(captured, 2);
        let line_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_bill_match_lines WHERE match_id='bill-match-capture-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line_count, 1);
        let event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_bill_match_events WHERE match_id='bill-match-capture-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event_count, 1);
        let stored_receipt: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='capture-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(stored_receipt, 1);

        let (replay, _) = run_capture(&pool, preparer("preparer"), input, "capture-1")
            .await
            .unwrap();
        assert_eq!(replay.id, receipt.id);
        assert_eq!(replay.version, receipt.version);
    })
    .await;
}

#[tokio::test]
async fn idempotency_conflict_for_different_capture_request() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        run_capture(&pool, preparer("preparer"), capture_input("REF-1", "fulfillment-1", 1), "capture-2")
            .await
            .unwrap();
        let mut different = capture_input("REF-1", "fulfillment-1", 1);
        different["reference"] = serde_json::json!("REF-CHANGED");
        let err = run_capture(&pool, preparer("preparer"), different, "capture-2")
            .await
            .unwrap_err();
        assert_eq!(err.code, "COMMAND_IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn capture_fails_closed_on_changed_receipt_version() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE commerce_fulfillments SET version=2 WHERE id='fulfillment-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let err = run_capture(
            &pool,
            preparer("preparer"),
            capture_input("REF-1", "fulfillment-1", 1),
            "capture-3",
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "SUPPLIER_BILL_SOURCE_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn capture_rejects_consolidated_scope_mismatch() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let input = serde_json::json!({
            "receipts": [
                {"fulfillmentId": "fulfillment-1", "expectedVersion": 1},
                {"fulfillmentId": "fulfillment-2", "expectedVersion": 1}
            ],
            "reference": "REF-SCOPE",
            "invoiceDate": "2026-09-03",
            "dueDate": "2026-09-10",
            "evidence": "Consolidated supplier invoice across two suppliers",
            "lines": [
                {"fulfillmentLineId": "fulfill-line-1", "quantity": "2", "price": "10.00", "taxBps": 0},
                {"fulfillmentLineId": "fulfill-line-2", "quantity": "1", "price": "20.00", "taxBps": 0}
            ],
        });
        let err = run_capture(&pool, preparer("preparer"), input, "capture-scope")
            .await
            .unwrap_err();
        assert_eq!(err.code, "CONSOLIDATED_BILL_SCOPE_MISMATCH");
    })
    .await;
}

#[tokio::test]
async fn approves_bill_with_independent_reviewer() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (captured, _) = run_capture(
            &pool,
            preparer("preparer"),
            capture_input("REF-1", "fulfillment-1", 1),
            "capture-approve",
        )
        .await
        .unwrap();
        let err = run_decide(
            &pool,
            reviewer("preparer"),
            &captured.id,
            decide_input("approve", 1),
            "decide-self",
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "INDEPENDENT_BILL_REVIEW_REQUIRED");

        let (receipt, calls) = run_decide(
            &pool,
            reviewer("reviewer"),
            &captured.id,
            decide_input("approve", 1),
            "decide-approve",
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, captured.id);
        assert_eq!(receipt.version, 2);
        assert_eq!(receipt.status.as_deref(), Some("approved"));
        assert!(calls.iter().any(|call| call.starts_with("notify:")));

        let status: String = sqlx::query_scalar(
            "SELECT status FROM procurement_bill_matches WHERE id='bill-match-capture-approve'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(status, "approved");
    })
    .await;
}

#[tokio::test]
async fn returns_bill_and_deactivates_receipts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (captured, _) = run_capture(
            &pool,
            preparer("preparer"),
            capture_input("REF-1", "fulfillment-1", 1),
            "capture-return",
        )
        .await
        .unwrap();
        let (receipt, _) = run_decide(
            &pool,
            reviewer("reviewer"),
            &captured.id,
            decide_input("return", 1),
            "decide-return",
        )
        .await
        .unwrap();
        assert_eq!(receipt.status.as_deref(), Some("returned"));
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_bill_match_receipts WHERE match_id='bill-match-capture-return' AND active=TRUE",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(active, 0);
    })
    .await;
}

#[tokio::test]
async fn saves_policy_with_company_authority_and_fences_replay() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let policy_actor = SupplierBillCommandActor {
            user_id: "reviewer".into(),
            assignment_id: "assignment-reviewer".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            revision: 1,
            assignment_version: 1,
            permissions: vec!["finance.approve".into()],
            expires_at: NOW.get() + 60_000,
        };
        let input = serde_json::json!({
            "matchingMode": "required",
            "expectedVersion": 0,
            "reason": "Company requires matched supplier invoices before AP",
        });
        let receipt = run_policy(&pool, policy_actor.clone(), input.clone(), "policy-1")
            .await
            .unwrap();
        assert_eq!(receipt.id, "company");
        assert_eq!(receipt.version, 1);
        assert_eq!(receipt.matching_mode.as_deref(), Some("required"));

        let replay = run_policy(&pool, policy_actor.clone(), input, "policy-1")
            .await
            .unwrap();
        assert_eq!(replay.version, 1);

        let stale = serde_json::json!({
            "matchingMode": "optional",
            "expectedVersion": 0,
            "reason": "Stale policy write with an old version",
        });
        let err = run_policy(&pool, policy_actor.clone(), stale, "policy-2")
            .await
            .unwrap_err();
        assert_eq!(err.code, "STALE_POLICY");

        let unchanged = serde_json::json!({
            "matchingMode": "required",
            "expectedVersion": 1,
            "reason": "No change to the current matching policy",
        });
        let err = run_policy(&pool, policy_actor, unchanged, "policy-3")
            .await
            .unwrap_err();
        assert_eq!(err.code, "POLICY_UNCHANGED");
    })
    .await;
}

#[tokio::test]
async fn policy_requires_company_authority() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // Branch-scoped assignment holds `finance.approve` but is not a
        // Company-wide authority, so the retained denial applies.
        let branch_actor = SupplierBillCommandActor {
            user_id: "preparer".into(),
            assignment_id: "assignment-preparer".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some("branch".into()),
            department_id: None,
            revision: 1,
            assignment_version: 1,
            permissions: vec!["finance.approve".into()],
            expires_at: NOW.get() + 60_000,
        };
        let input = serde_json::json!({
            "matchingMode": "required",
            "expectedVersion": 0,
            "reason": "Branch approver cannot change Company policy",
        });
        let err = run_policy(&pool, branch_actor, input, "policy-auth")
            .await
            .unwrap_err();
        assert_eq!(err.code, "COMPANY_FINANCE_AUTHORITY_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn company_isolation_blocks_foreign_bill() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (captured, _) = run_capture(
            &pool,
            preparer("preparer"),
            capture_input("REF-1", "fulfillment-1", 1),
            "capture-isolation",
        )
        .await
        .unwrap();
        let foreign = actor("foreign", "intruder", &["finance.read", "finance.write", "finance.approve"]);
        let err = run_decide(
            &pool,
            foreign,
            &captured.id,
            decide_input("approve", 1),
            "decide-foreign",
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "SUPPLIER_BILL_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn serializable_guard_fails_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut outside = pool.raw().acquire().await.unwrap();
        let err = PgSupplierBillCommand::new(&mut outside)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "SUPPLIER_BILL_SERIALIZABLE_REQUIRED"));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let err = PgSupplierBillCommand::new(&mut default_isolation)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "SUPPLIER_BILL_SERIALIZABLE_REQUIRED"));
        default_isolation.rollback().await.unwrap();
    })
    .await;
}
