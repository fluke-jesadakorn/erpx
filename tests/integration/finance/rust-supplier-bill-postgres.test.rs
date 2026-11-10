//! Native standalone supplier-bill (payable) create/edit/post and payable
//! readback over leased PostgreSQL, current Account/IAM authority, the AP
//! journal and the caller-owned command receipt.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::{
    FinanceActor, JournalAuthority, JournalDraft, JournalError, supplier_bill::*,
};
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_identity_access::public::{
    AuthenticationSecrets, DeviceProof, SecretPurpose, SystemAuthenticationSecrets,
};
use erpx_server_procurement::public::finance_supplier_bill_match::stage_procurement_supplier_bill_match_post;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, HoldingId, UserId,
};
use erpx_service::finance_journal_detail::*;
use erpx_service::finance_journal_detail_http::*;
use erpx_service::finance_journal_review::FinanceJournalEnvironment;
use facts::{Prepared, account, setup};
use std::sync::Arc;
use tower::ServiceExt;

fn service(p: &Prepared) -> FinanceJournalDetailService<facts::TestClock> {
    FinanceJournalDetailService::new(
        p.authorizer.clone(),
        FinanceJournalReadConfiguration::new(
            FinanceJournalEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    )
}

fn owned_maker(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static {
    let request = p.maker_selection.clone();
    move || Ok(request)
}

fn owned_reviewer(
    p: &Prepared,
) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static {
    let request = p.reviewer_selection.clone();
    move || Ok(request)
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('bill-expense','company','chart','5000','Operating expenses','expense','THB',0,'none','active',1,0,0),\
         ('bill-vat-in','company','chart','1300','Input VAT','asset','THB',0,'none','active',1,0,0),\
         ('bill-vat-in-2','company','chart','1350','Input VAT clearing','asset','THB',0,'none','active',1,0,0),\
         ('bill-ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0),\
         ('bill-clearing','company','chart','2150','Goods received not invoiced','liability','THB',0,'none','active',1,0,0),\
         ('bill-bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('bill-supplier','company','supplier','SUP-BILL','Native supplier','THB','active',1,0,0),\
         ('bill-customer','company','customer','CUS-BILL','Native customer','THB','active',1,0,0),\
         ('bill-supplier-other','company-2','supplier','SUP-OTHER','Other Company supplier','THB','active',1,0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn independent_void_actor(p: &Prepared) -> (DeviceProof, HoldingWorkspaceRequest) {
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::query(
        "INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") \
         VALUES('fixture-c','fixture-c','c@example.test',1,$1,$1) ON CONFLICT(id) DO NOTHING",
    )
    .bind(now)
    .execute(p.system.control.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") \
         SELECT 'fixture-c','fixture-c','credential','local:credential','fixture-c',password,$1,$1 \
         FROM account WHERE id='fixture-a' ON CONFLICT(id) DO NOTHING",
    )
    .bind(now)
    .execute(p.system.control.raw())
    .await
    .unwrap();
    let third_central = account::login(&p.system, "c@example.test", None).await;
    let third = account::child(&p.system, &third_central, account::ACME).await;
    sqlx::query(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('finance-fixture-c','fixture-c','custom_finance_review','[\"finance\"]','company',$1,$2,3,'holding')",
    )
    .bind(now - 1000)
    .bind(expires)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) \
         VALUES($1,'fixture-c','company','finance-fixture-c',3,7,$2,$3)",
    )
    .bind(&third.result.session_id)
    .bind(expires)
    .bind(now)
    .execute(pool.raw())
    .await
    .unwrap();
    let third_proof = DeviceProof {
        active: Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session, &third.token)),
        ..Default::default()
    };
    let third_selection = HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new("finance-fixture-c").unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    };
    (third_proof, third_selection)
}

async fn seed_procurement_match(
    pool: &DatabasePool,
    match_id: &str,
    order_id: &str,
    fulfillment_id: &str,
    invoice_id: &str,
    expected_subtotal: i64,
    subtotal: i64,
    tax: i64,
    variance_account: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,\
         fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,\
         tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,\
         subtotal_minor,tax_minor,quantity_matches,variance_account_id,status,invoice_id,version,\
         created_by,created_at,updated_at) \
         VALUES($1,'company','branch',NULL,$2,$3,'bill-supplier','THB','chart','bill-clearing',\
         'bill-ap','bill-vat-in',$4,'2026-01-02','2026-02-01','Synthetic reviewed match',\
         $5,$6,$7,TRUE,$8,'invoiced',$9,1,'fixture-a',0,0)",
    )
    .bind(match_id)
    .bind(order_id)
    .bind(fulfillment_id)
    .bind(format!("MATCH-{match_id}"))
    .bind(expected_subtotal)
    .bind(subtotal)
    .bind(tax)
    .bind(variance_account)
    .bind(invoice_id)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) \
         VALUES($1,$2,$3,1)",
    )
    .bind(format!("{match_id}-receipt"))
    .bind(match_id)
    .bind(fulfillment_id)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn line(
    description: &str,
    quantity: &str,
    price: &str,
    tax_bps: i64,
    account: &str,
) -> SupplierBillLine {
    SupplierBillLine {
        description: description.into(),
        quantity: quantity.into(),
        unit_price: price.into(),
        tax_bps,
        account_id: account.into(),
        tax_account_id: None,
    }
}

fn create_input(number: &str) -> SupplierBillCreateInput {
    SupplierBillCreateInput {
        kind: "payable".into(),
        party_id: "bill-supplier".into(),
        invoice_number: number.into(),
        invoice_date: "2026-01-02".into(),
        due_date: "2026-02-01".into(),
        currency: "THB".into(),
        tax_code: Some("VAT7".into()),
        debit_account_id: "bill-expense".into(),
        credit_account_id: "bill-ap".into(),
        tax_account_id: Some("bill-vat-in".into()),
        subtotal: "250.00".into(),
        tax: "17.50".into(),
        lines: Some(vec![
            line("Consulting", "2", "100.00", 700, "bill-expense"),
            line("Materials", "1", "50.00", 700, "bill-expense"),
        ]),
    }
}

fn edit_input(expected_version: i64) -> SupplierBillEditInput {
    SupplierBillEditInput {
        kind: "payable".into(),
        party_id: "bill-supplier".into(),
        invoice_number: "BILL-2026-0001".into(),
        invoice_date: "2026-01-03".into(),
        due_date: "2026-02-02".into(),
        currency: "THB".into(),
        tax_code: Some("VAT7".into()),
        debit_account_id: "bill-expense".into(),
        credit_account_id: "bill-ap".into(),
        tax_account_id: Some("bill-vat-in".into()),
        subtotal: "300.00".into(),
        tax: "21.00".into(),
        expected_version,
        lines: Some(vec![line(
            "Revised services",
            "3",
            "100.00",
            700,
            "bill-expense",
        )]),
    }
}

fn post_input() -> SupplierBillPostInput {
    SupplierBillPostInput {
        period_id: "period".into(),
        entry_date: "2026-01-10".into(),
        expected_version: Some(2),
        fx_rate: None,
        attachment_set_ids: Vec::new(),
        fx_gain_account_id: None,
        fx_loss_account_id: None,
    }
}

fn command_actor(
    user: &str,
    assignment: &str,
) -> erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor {
    erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor {
        user_id: user.into(),
        assignment_id: assignment.into(),
        role: "custom_finance_review".into(),
        session_id: format!("session-{user}"),
        revision: 7,
        assignment_version: 3,
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        scope_kind: "holding".into(),
        scope_node_id: "company".into(),
        iam_json: "{}".into(),
    }
}

fn journal_actor(user: &str, assignment: &str) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
    }
}

struct AllowJournal;
impl JournalAuthority for AllowJournal {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(&mut self, _: &FinanceActor, _: &JournalDraft) -> Result<(), Self::Error> {
        Ok(())
    }
}

async fn adapter_post(
    pool: &sqlx::PgPool,
    key: &str,
    id: &str,
    input: &SupplierBillPostInput,
    clock: &facts::TestClock,
) -> Result<SupplierBillPostResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = erpx_server_finance::public::supplier_bill::post_supplier_bill(
        &mut tx,
        &command_actor("fixture-b", "finance-fixture-b"),
        &journal_actor("fixture-b", "finance-fixture-b"),
        &CommandKey::new(key).unwrap(),
        id,
        false,
        input,
        clock,
        &mut AllowJournal,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

fn assert_business<T: std::fmt::Debug>(
    result: Result<T, PgTransactionError<BusinessError>>,
    code: &str,
) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[tokio::test]
async fn supplier_bill_create_edit_post_and_readback_retain_ap_evidence() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let create_key = "supplier-bill-create-0001";
        let create = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(create_key).unwrap(),
                create_input("BILL-2026-0001"),
            )
            .await
            .unwrap();
        assert_eq!(create.data.id, format!("invoice-{create_key}-bill"));
        assert_eq!(create.data.number, "BILL-2026-0001");
        assert_eq!(create.data.total, "267.50");
        assert_eq!(create.data.outstanding, "267.50");
        assert_eq!(create.data.status, "draft");
        assert_eq!(create.data.version, 1);

        let invoice = sqlx::query(
            "SELECT kind,status,source_type,invoice_number,tax_code,subtotal_minor,tax_minor,\
             total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,\
             line_mode,line_revision,version,created_by,approved_by,journal_id \
             FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&create.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(invoice.try_get::<String, _>("kind").unwrap(), "payable");
        assert_eq!(invoice.try_get::<String, _>("status").unwrap(), "draft");
        assert_eq!(
            invoice.try_get::<String, _>("source_type").unwrap(),
            "standalone"
        );
        assert_eq!(invoice.try_get::<String, _>("tax_code").unwrap(), "VAT7");
        assert_eq!(invoice.try_get::<i64, _>("subtotal_minor").unwrap(), 25_000);
        assert_eq!(invoice.try_get::<i64, _>("tax_minor").unwrap(), 1_750);
        assert_eq!(invoice.try_get::<i64, _>("total_minor").unwrap(), 26_750);
        assert_eq!(
            invoice.try_get::<String, _>("debit_account_id").unwrap(),
            "bill-expense"
        );
        assert_eq!(
            invoice.try_get::<String, _>("credit_account_id").unwrap(),
            "bill-ap"
        );
        assert_eq!(
            invoice.try_get::<String, _>("tax_account_id").unwrap(),
            "bill-vat-in"
        );
        assert_eq!(
            invoice.try_get::<String, _>("line_mode").unwrap(),
            "native_lines"
        );
        assert_eq!(invoice.try_get::<i64, _>("line_revision").unwrap(), 1);
        assert_eq!(invoice.try_get::<i64, _>("version").unwrap(), 1);
        assert_eq!(
            invoice.try_get::<String, _>("created_by").unwrap(),
            "fixture-a"
        );
        assert!(
            invoice
                .try_get::<Option<String>, _>("approved_by")
                .unwrap()
                .is_none()
        );

        let edit_key = "supplier-bill-edit-0001";
        let edit = service(&p)
            .edit_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(edit_key).unwrap(),
                create.data.id.clone(),
                edit_input(1),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(edit.data.total, "321.00");
        assert_eq!(edit.data.version, 2);

        let post_key = "supplier-bill-post-0001";
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new(post_key).unwrap(),
                create.data.id.clone(),
                post_input(),
            )
            .await
            .unwrap();
        assert_eq!(posted.data.id, create.data.id);
        assert_eq!(posted.data.status, "posted");
        assert_eq!(posted.data.journal_id, format!("journal-{post_key}"));

        let posted_invoice = sqlx::query(
            "SELECT status,version,approved_by,posted_at,journal_id,base_total_minor,\
             base_outstanding_minor,fx_rate_micros,line_revision \
             FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&create.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            posted_invoice.try_get::<String, _>("status").unwrap(),
            "posted"
        );
        assert_eq!(posted_invoice.try_get::<i64, _>("version").unwrap(), 3);
        assert_eq!(
            posted_invoice.try_get::<String, _>("approved_by").unwrap(),
            "fixture-b"
        );
        assert_eq!(
            posted_invoice.try_get::<String, _>("journal_id").unwrap(),
            posted.data.journal_id
        );
        assert_eq!(
            posted_invoice
                .try_get::<i64, _>("base_total_minor")
                .unwrap(),
            32_100
        );
        assert_eq!(
            posted_invoice
                .try_get::<i64, _>("base_outstanding_minor")
                .unwrap(),
            32_100
        );
        assert_eq!(
            posted_invoice.try_get::<i64, _>("fx_rate_micros").unwrap(),
            1_000_000
        );
        assert_eq!(
            posted_invoice.try_get::<i64, _>("line_revision").unwrap(),
            2
        );

        let journal = sqlx::query(
            "SELECT company_id,chart_id,period_id,entry_date,currency,source_type,source_id,\
             memo,status,created_by,posted_by,version FROM finance_journals WHERE id=$1",
        )
        .bind(&posted.data.journal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            journal.try_get::<String, _>("company_id").unwrap(),
            "company"
        );
        assert_eq!(journal.try_get::<String, _>("chart_id").unwrap(), "chart");
        assert_eq!(journal.try_get::<String, _>("period_id").unwrap(), "period");
        assert_eq!(
            journal.try_get::<String, _>("entry_date").unwrap(),
            "2026-01-10"
        );
        assert_eq!(
            journal.try_get::<String, _>("source_type").unwrap(),
            "invoice"
        );
        assert_eq!(
            journal.try_get::<String, _>("source_id").unwrap(),
            create.data.id
        );
        assert_eq!(journal.try_get::<String, _>("status").unwrap(), "posted");
        assert_eq!(
            journal.try_get::<String, _>("posted_by").unwrap(),
            "fixture-b"
        );

        let lines = sqlx::query(
            "SELECT account_id,debit_minor,credit_minor,ordinal FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&posted.data.journal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0].try_get::<String, _>("account_id").unwrap(),
            "bill-expense"
        );
        assert_eq!(lines[0].try_get::<i64, _>("debit_minor").unwrap(), 30_000);
        assert_eq!(lines[0].try_get::<i64, _>("credit_minor").unwrap(), 0);
        assert_eq!(
            lines[1].try_get::<String, _>("account_id").unwrap(),
            "bill-ap"
        );
        assert_eq!(lines[1].try_get::<i64, _>("debit_minor").unwrap(), 0);
        assert_eq!(lines[1].try_get::<i64, _>("credit_minor").unwrap(), 32_100);
        assert_eq!(
            lines[2].try_get::<String, _>("account_id").unwrap(),
            "bill-vat-in"
        );
        assert_eq!(lines[2].try_get::<i64, _>("debit_minor").unwrap(), 2_100);
        assert_eq!(lines[2].try_get::<i64, _>("credit_minor").unwrap(), 0);

        let line_versions: Vec<(i64, String, i64)> = sqlx::query_as(
            "SELECT source_version,origin_kind,COUNT(*) FROM finance_invoice_line_versions \
             WHERE invoice_id=$1 AND company_id='company' GROUP BY source_version,origin_kind \
             ORDER BY source_version",
        )
        .bind(&create.data.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            line_versions,
            vec![(1, "native_line".into(), 2), (2, "native_line".into(), 1)]
        );

        let money: Vec<(String, i64, i64, Option<String>)> = sqlx::query_as(
            "SELECT amount_kind,amount_units::bigint,usd_units::bigint,carried_source_id \
             FROM money_transaction_snapshots WHERE company_id='company' \
             AND source_type='finance_invoice' AND source_id=$1 ORDER BY amount_kind",
        )
        .bind(&create.data.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            money,
            vec![
                ("draft:v1".into(), 26_750, 13_375, None),
                ("draft:v2".into(), 32_100, 16_050, None),
                ("total".into(), 32_100, 16_050, Some(create.data.id.clone())),
            ]
        );

        let receipts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT operation,COUNT(*) FROM command_receipts WHERE command_id IN ($1,$2,$3) \
             GROUP BY operation ORDER BY operation",
        )
        .bind(create_key)
        .bind(edit_key)
        .bind(post_key)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            receipts,
            vec![
                ("finance.supplier-bill.create".into(), 1),
                ("finance.supplier-bill.post".into(), 1),
                ("finance.supplier-bill.update".into(), 1),
            ]
        );
        let effects: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM audit_events WHERE command_id IN ($1,$2,$3)),\
             (SELECT COUNT(*) FROM outbox_events WHERE command_id IN ($1,$2,$3))",
        )
        .bind(create_key)
        .bind(edit_key)
        .bind(post_key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (3, 3));

        let detail = service(&p)
            .supplier_bill_detail(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                create.data.id.clone(),
            )
            .await
            .unwrap();
        assert_eq!(detail.data.invoice["invoice_number"], "BILL-2026-0001");
        assert_eq!(detail.data.invoice["status"], "posted");
        assert_eq!(detail.data.invoice["party_name"], "Native supplier");
        assert_eq!(detail.data.lines.len(), 1);
        assert_eq!(detail.data.line_history.len(), 2);
        assert_eq!(
            detail.data.journal.as_ref().unwrap()["id"],
            posted.data.journal_id
        );
        assert_eq!(detail.context.company_id, "company");
        assert!(
            detail
                .context
                .permissions
                .iter()
                .any(|p| p == "finance.read")
        );

        let page = service(&p)
            .finance_invoices_page(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                erpx_server_finance::public::finance_invoice_read::InvoicePageQuery {
                    q: "BILL-2026".into(),
                    kind: "payable".into(),
                    status: String::new(),
                    queue: "open".into(),
                    chart_id: Some("chart".into()),
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.data.items.len(), 1);
        assert_eq!(page.data.items[0]["id"], create.data.id);
        assert_eq!(page.data.items[0]["kind"], "payable");
        assert_eq!(page.data.items[0]["outstanding_minor"], 32_100);
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_replay_versions_and_concurrent_posting_stay_single() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let key = "supplier-bill-replay-0001";
        let first = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                create_input("BILL-2026-0001"),
            )
            .await
            .unwrap();
        let replay = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(key).unwrap(),
                create_input("BILL-2026-0001"),
            )
            .await
            .unwrap();
        assert_eq!(replay.data.id, first.data.id);
        assert_eq!(replay.data.version, 1);
        let invoice_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM finance_invoices WHERE company_id='company' AND kind='payable'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(invoice_count, 1);
        let receipt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM command_receipts WHERE command_id=$1")
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(receipt_count, 1);

        service(&p)
            .edit_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-edit-0001").unwrap(),
                first.data.id.clone(),
                edit_input(1),
            )
            .await
            .unwrap()
            .unwrap();
        assert_business(
            service(&p)
                .edit_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-stale-edit").unwrap(),
                    first.data.id.clone(),
                    edit_input(1),
                )
                .await,
            "VERSION_CONFLICT",
        );
        let mut stale = post_input();
        stale.expected_version = Some(1);
        assert_business(
            service(&p)
                .post_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-stale-post").unwrap(),
                    first.data.id.clone(),
                    stale,
                )
                .await,
            "VERSION_CONFLICT",
        );
        service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-post-0001").unwrap(),
                first.data.id.clone(),
                post_input(),
            )
            .await
            .unwrap();
        assert_business(
            service(&p)
                .post_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-post-0002").unwrap(),
                    first.data.id.clone(),
                    post_input(),
                )
                .await,
            "INVOICE_NOT_FOUND",
        );

        let second = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-race-create").unwrap(),
                create_input("BILL-2026-0002"),
            )
            .await
            .unwrap();
        let clock = p.system.scenario.clock.as_ref();
        let mut race = post_input();
        race.expected_version = Some(1);
        let (left, right) = tokio::join!(
            adapter_post(
                pool.raw(),
                "supplier-bill-race-1",
                &second.data.id,
                &race,
                clock
            ),
            adapter_post(
                pool.raw(),
                "supplier-bill-race-2",
                &second.data.id,
                &race,
                clock
            ),
        );
        let successes = usize::from(left.is_ok()) + usize::from(right.is_ok());
        assert_eq!(successes, 1, "{left:?} / {right:?}");
        let (status, version, journals): (String, i64, i64) = sqlx::query_as(
            "SELECT i.status,i.version,(SELECT COUNT(*) FROM finance_journals j \
             WHERE j.company_id=i.company_id AND j.source_type='invoice' AND j.source_id=i.id) \
             FROM finance_invoices i WHERE i.id=$1 AND i.company_id='company'",
        )
        .bind(&second.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((status.as_str(), version, journals), ("posted", 2, 1));
        let posted_receipts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM command_receipts WHERE command_id IN ($1,$2) \
             AND operation='finance.supplier-bill.post'",
        )
        .bind("supplier-bill-race-1")
        .bind("supplier-bill-race-2")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(posted_receipts, 1);
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_rejects_wrong_party_account_currency_company_and_closed_period() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let mut wrong_party = create_input("BILL-WRONG-PARTY");
        wrong_party.party_id = "bill-customer".into();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-wrong-party").unwrap(),
                    wrong_party,
                )
                .await,
            "PARTY_INVALID",
        );
        let mut wrong_control = create_input("BILL-WRONG-CONTROL");
        wrong_control.credit_account_id = "bill-bank".into();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-wrong-control").unwrap(),
                    wrong_control,
                )
                .await,
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
        );
        let mut wrong_debit = create_input("BILL-WRONG-DEBIT");
        wrong_debit.debit_account_id = "bill-bank".into();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-wrong-debit").unwrap(),
                    wrong_debit,
                )
                .await,
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
        );
        let mut wrong_line_tax = create_input("BILL-WRONG-LINE-TAX");
        wrong_line_tax.lines.as_mut().unwrap()[0].tax_account_id = Some("bill-vat-in-2".into());
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-wrong-line-tax").unwrap(),
                    wrong_line_tax,
                )
                .await,
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
        );
        let mut wrong_currency = create_input("BILL-WRONG-CURRENCY");
        wrong_currency.currency = "USD".into();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-wrong-currency").unwrap(),
                    wrong_currency,
                )
                .await,
            "SUPPLIER_BILL_ACCOUNTS_INVALID",
        );
        let mut other_company = create_input("BILL-OTHER-COMPANY");
        other_company.party_id = "bill-supplier-other".into();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-other-company").unwrap(),
                    other_company,
                )
                .await,
            "PARTY_INVALID",
        );

        let bill = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-closed-period").unwrap(),
                create_input("BILL-2026-CLOSED"),
            )
            .await
            .unwrap();
        assert_business(
            service(&p)
                .create_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-duplicate-number").unwrap(),
                    create_input("BILL-2026-CLOSED"),
                )
                .await,
            "INVOICE_NUMBER_CONFLICT",
        );
        let mut missing_lines = edit_input(1);
        missing_lines.invoice_number = "BILL-2026-CLOSED".into();
        missing_lines.lines = None;
        assert_business(
            service(&p)
                .edit_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-missing-lines").unwrap(),
                    bill.data.id.clone(),
                    missing_lines,
                )
                .await,
            "INVOICE_LINES_REQUIRED",
        );
        sqlx::query("UPDATE finance_periods SET state='closed' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut closed = post_input();
        closed.expected_version = Some(1);
        assert_business(
            service(&p)
                .post_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-closed-post").unwrap(),
                    bill.data.id.clone(),
                    closed,
                )
                .await,
            "PERIOD_CLOSED",
        );
        sqlx::query("UPDATE finance_periods SET state='open' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at) \
             VALUES('other-company-bill','company-2','bill-supplier-other','payable',\
             'OTHER-1','2026-01-02','2026-02-01','THB',100,0,100,100,'bill-expense','bill-ap',\
             'draft',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_business(
            service(&p)
                .supplier_bill_detail(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    "other-company-bill".into(),
                )
                .await,
            "INVOICE_NOT_FOUND",
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_whole_document_create_posts() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let mut input = create_input("BILL-2026-WHOLE");
        input.subtotal = "100.00".into();
        input.tax = "0.00".into();
        input.tax_code = None;
        input.tax_account_id = None;
        input.lines = None;
        let create_key = "supplier-bill-whole-create";
        let bill = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new(create_key).unwrap(),
                input,
            )
            .await
            .unwrap();
        assert_eq!(bill.data.total, "100.00");
        let line: (String, String, i64, i64, Option<i64>) = sqlx::query_as(
            "SELECT origin_kind,description,subtotal_minor,tax_minor,quantity_millis \
             FROM finance_invoice_line_versions WHERE invoice_id=$1 AND company_id='company' \
             AND source_version=1",
        )
        .bind(&bill.data.id)
        .fetch_one(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(line.0, "whole_document");
        assert_eq!(line.1, "Whole document BILL-2026-WHOLE");
        assert_eq!((line.2, line.3, line.4), (10_000, 0, None));
        let post_key = "supplier-bill-whole-post";
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new(post_key).unwrap(),
                bill.data.id.clone(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        let lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&posted.data.journal_id)
        .fetch_all(p.system.holdings[0].raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("bill-expense".into(), 10_000, 0),
                ("bill-ap".into(), 0, 10_000)
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_http_route_creates_posts_and_reads_payable() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        seed(&p.system.holdings[0]).await;
        let app = finance_journal_read_router(
            Arc::new(account::fixture::account::policy()),
            account::fixture::account::holding_registry(),
            Arc::new(FinanceJournalReadScopeHeaders),
            vec![FinanceJournalReadTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                service: Arc::new(service(&p)),
            }],
        )
        .unwrap();
        let create_key = "supplier-bill-http-0001";
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/payables")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", create_key)
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(
                serde_json::to_vec(&create_input("BILL-2026-HTTP")).unwrap(),
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::CREATED, "{value}");
        let id = format!("invoice-{create_key}-bill");
        assert_eq!(value["data"]["id"], id);
        assert_eq!(value["data"]["status"], "draft");

        let post_key = "supplier-bill-http-post-0001";
        let post_request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/holdings/acme/finance/payables/{id}/post"))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", post_key)
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "periodId": "period",
                    "entryDate": "2026-01-10",
                    "expectedVersion": 1
                }))
                .unwrap(),
            ))
            .unwrap();
        let response = app.clone().oneshot(post_request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["data"]["id"], id);
        assert_eq!(value["data"]["status"], "posted");
        assert_eq!(value["data"]["journalId"], format!("journal-{post_key}"));

        let read_request = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/holdings/acme/finance/payables/{id}"))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(read_request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["invoice"]["invoice_number"], "BILL-2026-HTTP");
        assert_eq!(value["data"]["invoice"]["status"], "posted");
        assert_eq!(value["context"]["companyId"], "company");

        let missing_key = Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/finance/payables")
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.maker_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-a")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(
                serde_json::to_vec(&create_input("BILL-NO-KEY")).unwrap(),
            ))
            .unwrap();
        let missing = app.clone().oneshot(missing_key).await.unwrap();
        assert_eq!(missing.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            missing
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        let repeat_post = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/holdings/acme/finance/payables/{id}/post"))
            .header("host", "acme.example.test")
            .header(
                "cookie",
                format!("__Host-erpx-product={}", p.reviewer_token.expose()),
            )
            .header("origin", "https://acme.example.test")
            .header("content-type", "application/json")
            .header("idempotency-key", "supplier-bill-http-post-repeat")
            .header("x-erpx-context-company", "company")
            .header("x-erpx-context-assignment", "finance-fixture-b")
            .header("x-erpx-context-revision", "7")
            .header("x-erpx-context-assignment-version", "3")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({
                    "periodId": "period",
                    "entryDate": "2026-01-10",
                    "expectedVersion": 1
                }))
                .unwrap(),
            ))
            .unwrap();
        let repeat = app.oneshot(repeat_post).await.unwrap();
        assert_eq!(repeat.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            repeat
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    })
    .await;
}

async fn seed_supplier_bill_evidence(pool: &DatabasePool, bill_id: &str) -> String {
    let set = "bill-evidence-set";
    sqlx::query(
        "INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,\
         component_ref,purpose_key,expected_target_version,policy_version,classification,\
         audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES($1,'company','supplier_bill',$2,'action','invoice-review','bill_match_support',\
         1,1,'confidential','{}',1,10,'draft','fixture-a',0,0)",
    )
    .bind(set)
    .bind(bill_id)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,\
         declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,\
         created_by,expires_at,created_at,updated_at) \
         VALUES('bill-evidence-upload','supplier_bill',$1,'bill_match_support','bill.pdf',\
         'application/pdf',123,123,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',\
         'bill-evidence-staging','bill-evidence-final',1,'ready','fixture-a',9999999999999,0,0)",
    )
    .bind(bill_id)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,\
         original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,\
         detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('bill-evidence-attachment','bill-evidence-upload','supplier_bill',$1,\
         'bill_match_support','bill-evidence-object',1,'bill.pdf','application/pdf',123,\
         'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','fixture-a',0,\
         'company','application/pdf','available',0,0)",
    )
    .bind(bill_id)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('bill-evidence-link',$1,'bill-evidence-attachment','draft','fixture-a',0,9999999999999)",
    )
    .bind(set)
    .execute(pool.raw())
    .await
    .unwrap();
    set.to_owned()
}

#[tokio::test]
async fn supplier_bill_post_binds_review_evidence_sets_in_the_posting_unit() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-evidence-create").unwrap(),
                create_input("BILL-EVIDENCE-0001"),
            )
            .await
            .unwrap();
        let set = seed_supplier_bill_evidence(pool, &created.data.id).await;
        let post_key = "supplier-bill-evidence-post";
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new(post_key).unwrap(),
                created.data.id.clone(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: vec![set.clone()],
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        use sqlx::Row;
        let set_row =
            sqlx::query("SELECT state,bound_command_id,bound_at FROM attachment_sets WHERE id=$1")
                .bind(&set)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(set_row.try_get::<String, _>("state").unwrap(), "bound");
        assert_eq!(
            set_row
                .try_get::<Option<String>, _>("bound_command_id")
                .unwrap(),
            Some(post_key.to_owned())
        );
        assert!(
            set_row
                .try_get::<Option<i64>, _>("bound_at")
                .unwrap()
                .is_some()
        );
        let link = sqlx::query(
            "SELECT state,binding_type,binding_id FROM attachment_links WHERE set_id=$1",
        )
        .bind(&set)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(link.try_get::<String, _>("state").unwrap(), "bound");
        assert_eq!(
            link.try_get::<String, _>("binding_type").unwrap(),
            "finance_supplier_bill_post"
        );
        assert_eq!(
            link.try_get::<String, _>("binding_id").unwrap(),
            format!("{}:2", created.data.id)
        );
        let replay = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new(post_key).unwrap(),
                created.data.id.clone(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: vec![set],
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.journal_id, posted.data.journal_id);
        assert_eq!(replay.data.status, "posted");
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_posts_a_commerce_fulfillment_source_with_its_owner_witness() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let invoice = "invoice-commerce-source";
        let delivery = "delivery-commerce-source";
        p.create("commerce-goods", false).await.unwrap();
        p.review(
            "journal-commerce-goods",
            "post-commerce-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let goods_journal = "journal-commerce-goods";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-source','company','purchase','PO-COMMERCE',\
             'bill-supplier','THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,\
             'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-source','FUL-COMMERCE','2026-01-05','fixture-a',0,$2,NULL,1,\
             'goods','')",
        )
        .bind(delivery)
        .bind(goods_journal)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-COMMERCE','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-commerce-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        let row: (String, String) = sqlx::query_as(
            "SELECT status,source_type FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(invoice)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("posted".into(), "commerce_fulfillment".into()));
        let journal_lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&posted.data.journal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            journal_lines,
            vec![
                ("bill-clearing".into(), 25_000, 0),
                ("bill-ap".into(), 0, 26_750),
                ("bill-vat-in".into(), 1_750, 0),
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_rejects_a_non_purchase_commerce_source() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let invoice = "invoice-sales-source";
        let delivery = "delivery-sales-source";
        p.create("commerce-sales-goods", false).await.unwrap();
        p.review(
            "journal-commerce-sales-goods",
            "post-commerce-sales-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let goods_journal = "journal-commerce-sales-goods";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-sales-source','company','sales','SO-COMMERCE',\
             'bill-supplier','THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,\
             'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-sales-source','FUL-SALES','2026-01-05','fixture-a',0,$2,NULL,1,\
             'goods','')",
        )
        .bind(delivery)
        .bind(goods_journal)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-SALES','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        assert_business(
            service(&p)
                .post_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-sales-source-post").unwrap(),
                    invoice.into(),
                    SupplierBillPostInput {
                        period_id: "period".into(),
                        entry_date: "2026-01-10".into(),
                        expected_version: Some(1),
                        fx_rate: None,
                        attachment_set_ids: Vec::new(),
                        fx_gain_account_id: None,
                        fx_loss_account_id: None,
                    },
                )
                .await,
            "SUPPLIER_BILL_SOURCE_CHANGED",
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_rejects_a_stale_or_unposted_commerce_source() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        p.create("commerce-stale-goods", false).await.unwrap();
        p.review(
            "journal-commerce-stale-goods",
            "post-commerce-stale-goods",
            facts::post(),
        )
        .await
        .unwrap();
        p.create("commerce-draft-goods", false).await.unwrap();
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-stale','company','purchase','PO-STALE','bill-supplier',\
             'THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        for (delivery, number, journal) in [
            (
                "delivery-commerce-stale",
                "FUL-STALE",
                "journal-commerce-stale-goods",
            ),
            (
                "delivery-commerce-draft",
                "FUL-DRAFT",
                "journal-commerce-draft-goods",
            ),
        ] {
            sqlx::query(
                "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
                 created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
                 VALUES($1,'order-commerce-stale',$2,'2026-01-05','fixture-a',0,$3,NULL,1,\
                 'goods','')",
            )
            .bind(delivery)
            .bind(number)
            .bind(journal)
            .execute(pool.raw())
            .await
            .unwrap();
        }
        for (invoice, delivery, number) in [
            (
                "invoice-commerce-stale",
                "delivery-commerce-stale",
                "BILL-STALE",
            ),
            (
                "invoice-commerce-draft",
                "delivery-commerce-draft",
                "BILL-DRAFT",
            ),
        ] {
            sqlx::query(
                "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
                 due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
                 debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
                 source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
                 VALUES($1,'company','bill-supplier','payable',$3,'2026-01-02',\
                 '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
                 'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
            )
            .bind(invoice)
            .bind(delivery)
            .bind(number)
            .execute(pool.raw())
            .await
            .unwrap();
            sqlx::query(
                "UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2",
            )
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
                 transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
                 VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
            )
            .bind(invoice)
            .execute(pool.raw())
            .await
            .unwrap();
        }
        sqlx::query(
            "UPDATE commerce_fulfillments SET version=version+1 WHERE id='delivery-commerce-stale'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        for (invoice, key) in [
            ("invoice-commerce-stale", "supplier-bill-stale-source"),
            ("invoice-commerce-draft", "supplier-bill-unposted-source"),
        ] {
            assert_business(
                service(&p)
                    .post_supplier_bill(
                        "acme.example.test",
                        p.reviewer.clone(),
                        owned_reviewer(&p),
                        CommandKey::new(key).unwrap(),
                        invoice.into(),
                        SupplierBillPostInput {
                            period_id: "period".into(),
                            entry_date: "2026-01-10".into(),
                            expected_version: Some(1),
                            fx_rate: None,
                            attachment_set_ids: Vec::new(),
                            fx_gain_account_id: None,
                            fx_loss_account_id: None,
                        },
                    )
                    .await,
                "SUPPLIER_BILL_SOURCE_CHANGED",
            );
        }
    })
    .await;
}

#[tokio::test]
async fn procurement_match_repair_migration_repairs_stranded_rows() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        p.create("commerce-repair-goods", false).await.unwrap();
        p.review(
            "journal-commerce-repair-goods",
            "post-commerce-repair-goods",
            facts::post(),
        )
        .await
        .unwrap();
        for (suffix, invoice_status) in [("posted", "posted"), ("voided", "voided")] {
            let order = format!("order-repair-{suffix}");
            let delivery = format!("delivery-repair-{suffix}");
            let invoice = format!("invoice-repair-{suffix}");
            sqlx::query(
                "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
                 due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
                 VALUES($1,'company','purchase',$2,'bill-supplier','THB','2026-01-02',\
                 '2026-02-01',25000,1750,'fulfilled',1,'fixture-a',0,0)",
            )
            .bind(&order)
            .bind(format!("PO-REPAIR-{suffix}"))
            .execute(pool.raw())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
                 created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
                 VALUES($1,$2,$3,'2026-01-05','fixture-a',0,\
                 'journal-commerce-repair-goods',NULL,1,'goods','')",
            )
            .bind(&delivery)
            .bind(&order)
            .bind(format!("FUL-REPAIR-{suffix}"))
            .execute(pool.raw())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
                 due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
                 debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
                 source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
                 VALUES($1,'company','bill-supplier','payable',$2,'2026-01-02',\
                 '2026-02-01','THB',25000,1750,26750,0,'bill-clearing','bill-ap','bill-vat-in',\
                 'commerce_fulfillment',$3,2,$4,1,1,'whole_document','fixture-a',0,0)",
            )
            .bind(&invoice)
            .bind(format!("BILL-REPAIR-{suffix}"))
            .bind(&delivery)
            .bind(invoice_status)
            .execute(pool.raw())
            .await
            .unwrap();
            sqlx::query(
                "UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2",
            )
            .bind(&invoice)
            .bind(&delivery)
            .execute(pool.raw())
            .await
            .unwrap();
            seed_procurement_match(
                pool,
                &format!("bill-match-repair-{suffix}"),
                &order,
                &delivery,
                &invoice,
                25000,
                25000,
                1750,
                None,
            )
            .await;
        }
        sqlx::raw_sql(include_str!(
            "../../../migrations/postgres/holding-write/1101_procurement_bill_match_repair.sql"
        ))
        .execute(pool.raw())
        .await
        .unwrap();
        let posted: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,invoice_id FROM procurement_bill_matches \
             WHERE id='bill-match-repair-posted'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            posted,
            (
                "posted".into(),
                2,
                Some("invoice-repair-posted".into())
            )
        );
        let returned: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,invoice_id FROM procurement_bill_matches \
             WHERE id='bill-match-repair-voided'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(returned, ("returned".into(), 2, None));
        let receipt_active: bool = sqlx::query_scalar(
            "SELECT active FROM procurement_bill_match_receipts \
             WHERE id='bill-match-repair-voided-receipt'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(!receipt_active);
        let events: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT match_id,action,version FROM procurement_bill_match_events \
             WHERE match_id LIKE 'bill-match-repair-%' ORDER BY match_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                (
                    "bill-match-repair-posted".into(),
                    "posted".into(),
                    2
                ),
                (
                    "bill-match-repair-voided".into(),
                    "returned".into(),
                    2
                ),
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_rejects_a_stale_secondary_match_receipt() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        p.create("commerce-consolidated-goods", false).await.unwrap();
        p.review(
            "journal-commerce-consolidated-goods",
            "post-commerce-consolidated-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-consolidated";
        let primary = "delivery-commerce-consolidated-primary";
        let secondary = "delivery-commerce-consolidated-secondary";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-consolidated','company','purchase','PO-CONSOLIDATED',\
             'bill-supplier','THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,\
             'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        for (id, number) in [
            (primary, "FUL-CONSOLIDATED-1"),
            (secondary, "FUL-CONSOLIDATED-2"),
        ] {
            sqlx::query(
                "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
                 created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
                 VALUES($1,'order-commerce-consolidated',$2,'2026-01-05','fixture-a',0,\
                 'journal-commerce-consolidated-goods',NULL,1,'goods','')",
            )
            .bind(id)
            .bind(number)
            .execute(pool.raw())
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-CONSOLIDATED','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(primary)
        .execute(pool.raw())
        .await
        .unwrap();
        for id in [primary, secondary] {
            sqlx::query(
                "UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2",
            )
            .bind(invoice)
            .bind(id)
            .execute(pool.raw())
            .await
            .unwrap();
        }
        seed_procurement_match(
            pool,
            "bill-match-consolidated",
            "order-commerce-consolidated",
            primary,
            invoice,
            25000,
            25000,
            1750,
            None,
        )
        .await;
        sqlx::query(
            "INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) \
             VALUES('bill-match-consolidated-secondary','bill-match-consolidated',$1,1)",
        )
        .bind(secondary)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET version=version+1 WHERE id=$1")
            .bind(secondary)
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        assert_business(
            service(&p)
                .post_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-consolidated-post").unwrap(),
                    invoice.into(),
                    SupplierBillPostInput {
                        period_id: "period".into(),
                        entry_date: "2026-01-10".into(),
                        expected_version: Some(1),
                        fx_rate: None,
                        attachment_set_ids: Vec::new(),
                        fx_gain_account_id: None,
                        fx_loss_account_id: None,
                    },
                )
                .await,
            "SUPPLIER_BILL_SOURCE_CHANGED",
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_posts_a_matched_commerce_bill_with_negative_variance() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::query(
            "UPDATE assignments SET valid_from=0,valid_until=9999999999999 \
             WHERE company_id='company'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        p.create("commerce-negative-variance-goods", false).await.unwrap();
        p.review(
            "journal-commerce-negative-variance-goods",
            "post-commerce-negative-variance-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-negative-variance";
        let delivery = "delivery-commerce-negative-variance";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-negative-variance','company','purchase','PO-NEGATIVE-VARIANCE','bill-supplier',\
             'THB','2026-01-02','2026-02-01',26000,1750,'fulfilled',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-negative-variance','FUL-NEGATIVE-VARIANCE','2026-01-05','fixture-a',0,\
             'journal-commerce-negative-variance-goods',NULL,1,'goods','')",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-NEGATIVE-VARIANCE','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        seed_procurement_match(
            pool,
            "bill-match-negative-variance",
            "order-commerce-negative-variance",
            delivery,
            invoice,
            26000,
            25000,
            1750,
            Some("bill-expense"),
        )
        .await;
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-negative-variance-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        let lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&posted.data.journal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("bill-clearing".into(), 26_000, 0),
                ("bill-expense".into(), 0, 1_000),
                ("bill-vat-in".into(), 1_750, 0),
                ("bill-ap".into(), 0, 26_750),
            ]
        );
        let matched: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM procurement_bill_matches WHERE id='bill-match-negative-variance'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(matched, ("posted".into(), 2));
        let event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM procurement_bill_match_events \
             WHERE match_id='bill-match-negative-variance' ORDER BY version DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event, ("posted".into(), 2));
        let notifications: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications \
             WHERE record_type='supplier_bill' AND record_id='bill-match-negative-variance' \
             AND record_version=2",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(notifications > 0);
        let replay = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-negative-variance-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.status, "posted");
        assert_eq!(replay.data.journal_id, posted.data.journal_id);
        let match_after_replay: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM procurement_bill_matches WHERE id='bill-match-negative-variance'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(match_after_replay, ("posted".into(), 2));
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_posts_a_matched_commerce_bill_with_price_variance() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::query(
            "UPDATE assignments SET valid_from=0,valid_until=9999999999999 \
             WHERE company_id='company'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        p.create("commerce-variance-goods", false).await.unwrap();
        p.review(
            "journal-commerce-variance-goods",
            "post-commerce-variance-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-variance";
        let delivery = "delivery-commerce-variance";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-variance','company','purchase','PO-VARIANCE','bill-supplier',\
             'THB','2026-01-02','2026-02-01',24000,1750,'fulfilled',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-variance','FUL-VARIANCE','2026-01-05','fixture-a',0,\
             'journal-commerce-variance-goods',NULL,1,'goods','')",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-VARIANCE','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        seed_procurement_match(
            pool,
            "bill-match-variance",
            "order-commerce-variance",
            delivery,
            invoice,
            24000,
            25000,
            1750,
            Some("bill-expense"),
        )
        .await;
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-variance-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        let lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&posted.data.journal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("bill-clearing".into(), 24_000, 0),
                ("bill-expense".into(), 1_000, 0),
                ("bill-vat-in".into(), 1_750, 0),
                ("bill-ap".into(), 0, 26_750),
            ]
        );
        let matched: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM procurement_bill_matches WHERE id='bill-match-variance'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(matched, ("posted".into(), 2));
        let event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM procurement_bill_match_events \
             WHERE match_id='bill-match-variance' ORDER BY version DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event, ("posted".into(), 2));
        let notifications: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications \
             WHERE record_type='supplier_bill' AND record_id='bill-match-variance' \
             AND record_version=2",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(notifications > 0);
        let replay = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-variance-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.status, "posted");
        assert_eq!(replay.data.journal_id, posted.data.journal_id);
        let match_after_replay: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM procurement_bill_matches WHERE id='bill-match-variance'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(match_after_replay, ("posted".into(), 2));
    })
    .await;
}

#[tokio::test]
async fn procurement_match_post_rejects_a_stale_version() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        p.create("commerce-cas-goods", false).await.unwrap();
        p.review(
            "journal-commerce-cas-goods",
            "post-commerce-cas-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-cas";
        let delivery = "delivery-commerce-cas";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-cas','company','purchase','PO-CAS','bill-supplier',\
             'THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-cas','FUL-CAS','2026-01-05','fixture-a',0,\
             'journal-commerce-cas-goods',NULL,1,'goods','')",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-CAS','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        seed_procurement_match(
            pool,
            "bill-match-cas",
            "order-commerce-cas",
            delivery,
            invoice,
            25000,
            25000,
            1750,
            None,
        )
        .await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let stale = stage_procurement_supplier_bill_match_post(
            &mut connection,
            "company",
            "bill-match-cas",
            2,
            invoice,
            "fixture-a",
            "cas-stale",
            0,
        )
        .await
        .unwrap_err();
        assert_eq!(stale.code, "PROCUREMENT_SUPPLIER_BILL_MATCH_CHANGED");
        stage_procurement_supplier_bill_match_post(
            &mut connection,
            "company",
            "bill-match-cas",
            1,
            invoice,
            "fixture-a",
            "cas-ok",
            0,
        )
        .await
        .unwrap();
        let replay = stage_procurement_supplier_bill_match_post(
            &mut connection,
            "company",
            "bill-match-cas",
            1,
            invoice,
            "fixture-a",
            "cas-replay",
            0,
        )
        .await
        .unwrap_err();
        assert_eq!(replay.code, "PROCUREMENT_SUPPLIER_BILL_MATCH_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_draft_void_releases_a_matched_commerce_fulfillment() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        p.create("commerce-void-goods", false).await.unwrap();
        p.review(
            "journal-commerce-void-goods",
            "post-commerce-void-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-void";
        let delivery = "delivery-commerce-void";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-void','company','purchase','PO-VOID','bill-supplier',\
             'THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-void','FUL-VOID','2026-01-05','fixture-a',0,\
             'journal-commerce-void-goods',NULL,1,'goods','')",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-VOID-MATCHED','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        seed_procurement_match(
            pool,
            "bill-match-void",
            "order-commerce-void",
            delivery,
            invoice,
            25000,
            25000,
            1750,
            None,
        )
        .await;
        let voided = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-matched-draft-void").unwrap(),
                invoice.into(),
                SupplierBillVoidInput {
                    expected_version: 1,
                    period_id: None,
                    entry_date: None,
                    memo: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(voided.data.status, "voided");
        assert_eq!(voided.data.version, 2);
        let fulfillment: (Option<String>, i64) = sqlx::query_as(
            "SELECT invoice_id,version FROM commerce_fulfillments WHERE id=$1",
        )
        .bind(delivery)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(fulfillment, (None, 3));
        let matched: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,invoice_id FROM procurement_bill_matches WHERE id='bill-match-void'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(matched, ("returned".into(), 2, None));
        let receipt_active: bool = sqlx::query_scalar(
            "SELECT active FROM procurement_bill_match_receipts WHERE id='bill-match-void-receipt'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(!receipt_active);
        sqlx::query(
            "INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,\
             fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,\
             tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,\
             subtotal_minor,tax_minor,quantity_matches,variance_account_id,status,invoice_id,version,\
             created_by,created_at,updated_at) \
             VALUES('bill-match-rematch','company','branch',NULL,'order-commerce-void',$1,\
             'bill-supplier','THB','chart','bill-clearing','bill-ap','bill-vat-in','MATCH-REMARK',\
             '2026-01-02','2026-02-01','Synthetic re-match after void',25000,25000,1750,TRUE,NULL,\
             'review',NULL,1,'fixture-a',0,0)",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) \
             VALUES('bill-match-rematch-receipt','bill-match-rematch',$1,3)",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        let active_receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM procurement_bill_match_receipts \
             WHERE fulfillment_id=$1 AND active=TRUE",
        )
        .bind(delivery)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(active_receipts, 1);
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_posted_void_reverses_a_matched_commerce_bill_and_releases_the_fulfillment() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::query(
            "UPDATE assignments SET valid_from=0,valid_until=9999999999999 \
             WHERE company_id='company'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        p.create("commerce-posted-void-goods", false).await.unwrap();
        p.review(
            "journal-commerce-posted-void-goods",
            "post-commerce-posted-void-goods",
            facts::post(),
        )
        .await
        .unwrap();
        let invoice = "invoice-commerce-posted-void";
        let delivery = "delivery-commerce-posted-void";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,kind,number,party_id,currency,order_date,\
             due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) \
             VALUES('order-commerce-posted-void','company','purchase','PO-POSTED-VOID',\
             'bill-supplier','THB','2026-01-02','2026-02-01',25000,1750,'fulfilled',1,\
             'fixture-a',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,\
             created_at,journal_id,invoice_id,version,completion_kind,completion_note) \
             VALUES($1,'order-commerce-posted-void','FUL-POSTED-VOID','2026-01-05','fixture-a',0,\
             'journal-commerce-posted-void-goods',NULL,1,'goods','')",
        )
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-POSTED-VOID-MATCHED','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,2,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        seed_procurement_match(
            pool,
            "bill-match-posted-void",
            "order-commerce-posted-void",
            delivery,
            invoice,
            25000,
            25000,
            1750,
            None,
        )
        .await;
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-matched-posted-void-post").unwrap(),
                invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        let matched_after_post: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM procurement_bill_matches WHERE id='bill-match-posted-void'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(matched_after_post, ("posted".into(), 2));
        let (third_proof, third_selection) = independent_void_actor(&p).await;
        let selection = third_selection.clone();
        let voided = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                third_proof,
                move || Ok(selection),
                CommandKey::new("supplier-bill-matched-posted-void").unwrap(),
                invoice.into(),
                SupplierBillVoidInput {
                    expected_version: 2,
                    period_id: Some("period".into()),
                    entry_date: Some("2026-01-20".into()),
                    memo: Some("Matched supplier invoice reversed".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(voided.data.status, "voided");
        assert_eq!(voided.data.version, 3);
        let reversal_id = voided
            .data
            .reversal_journal_id
            .clone()
            .expect("posted matched void returns its reversing journal");
        let original: String = sqlx::query_scalar(
            "SELECT status FROM finance_journals WHERE id=$1 AND company_id='company'",
        )
        .bind(&posted.data.journal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(original, "reversed");
        let reversal: (String, String, String) = sqlx::query_as(
            "SELECT source_type,source_id,status FROM finance_journals WHERE id=$1 AND company_id='company'",
        )
        .bind(&reversal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            reversal,
            (
                "invoice_reversal".into(),
                invoice.to_string(),
                "posted".into()
            )
        );
        let fulfillment: (Option<String>, i64) = sqlx::query_as(
            "SELECT invoice_id,version FROM commerce_fulfillments WHERE id=$1",
        )
        .bind(delivery)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(fulfillment, (None, 3));
        let matched_after_void: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,invoice_id FROM procurement_bill_matches WHERE id='bill-match-posted-void'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(matched_after_void, ("returned".into(), 3, None));
        let returned_event: (String, i64) = sqlx::query_as(
            "SELECT action,version FROM procurement_bill_match_events \
             WHERE match_id='bill-match-posted-void' ORDER BY version DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(returned_event, ("returned".into(), 3));
        let notifications: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications \
             WHERE record_type='supplier_bill' AND record_id='bill-match-posted-void' \
             AND record_version=3",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(notifications > 0);
        let receipt_active: bool = sqlx::query_scalar(
            "SELECT active FROM procurement_bill_match_receipts WHERE id='bill-match-posted-void-receipt'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(!receipt_active);
        let rematch_invoice = "invoice-commerce-posted-rematch";
        let rematch_match = "bill-match-posted-rematch";
        sqlx::query(
            "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
             due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
             debit_account_id,credit_account_id,tax_account_id,source_type,source_id,\
             source_version,status,version,line_revision,line_mode,created_by,created_at,updated_at) \
             VALUES($1,'company','bill-supplier','payable','BILL-POSTED-REMATCH','2026-01-02',\
             '2026-02-01','THB',25000,1750,26750,26750,'bill-clearing','bill-ap','bill-vat-in',\
             'commerce_fulfillment',$2,4,'draft',1,1,'whole_document','fixture-a',0,0)",
        )
        .bind(rematch_invoice)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("UPDATE commerce_fulfillments SET invoice_id=$1,version=version+1 WHERE id=$2")
            .bind(rematch_invoice)
            .bind(delivery)
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,\
             fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,\
             tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,\
             subtotal_minor,tax_minor,quantity_matches,variance_account_id,status,invoice_id,version,\
             created_by,created_at,updated_at) \
             VALUES($1,'company','branch',NULL,'order-commerce-posted-void',$2,'bill-supplier',\
             'THB','chart','bill-clearing','bill-ap','bill-vat-in','MATCH-POSTED-REMATCH',\
             '2026-01-02','2026-02-01','Synthetic posted re-match',25000,25000,1750,TRUE,NULL,\
             'invoiced',$3,1,'fixture-a',0,0)",
        )
        .bind(rematch_match)
        .bind(delivery)
        .bind(rematch_invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) \
             VALUES($1,$2,$3,3)",
        )
        .bind(format!("{rematch_match}-receipt"))
        .bind(rematch_match)
        .bind(delivery)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,\
             transaction_date,currency,currency_scale,amount_units,usd_units,captured_at) \
             VALUES('company','finance_invoice',$1,'draft:v1','2026-01-02','THB',2,26750,26750,0)",
        )
        .bind(rematch_invoice)
        .execute(pool.raw())
        .await
        .unwrap();
        let rematched = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-posted-rematch-post").unwrap(),
                rematch_invoice.into(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(rematched.data.status, "posted");
        let rematch_row: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status,version,invoice_id FROM procurement_bill_matches WHERE id=$1",
        )
        .bind(rematch_match)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rematch_row,
            ("posted".into(), 2, Some(rematch_invoice.into()))
        );
        let rematch_fulfillment: (Option<String>, i64) = sqlx::query_as(
            "SELECT invoice_id,version FROM commerce_fulfillments WHERE id=$1",
        )
        .bind(delivery)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rematch_fulfillment, (Some(rematch_invoice.into()), 4));
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_posted_void_posts_an_exact_reversal_with_an_independent_approver() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-posted-void-create").unwrap(),
                create_input("BILL-POSTED-VOID"),
            )
            .await
            .unwrap();
        let posted = service(&p)
            .post_supplier_bill(
                "acme.example.test",
                p.reviewer.clone(),
                owned_reviewer(&p),
                CommandKey::new("supplier-bill-posted-void-post").unwrap(),
                created.data.id.clone(),
                SupplierBillPostInput {
                    period_id: "period".into(),
                    entry_date: "2026-01-10".into(),
                    expected_version: Some(1),
                    fx_rate: None,
                    attachment_set_ids: Vec::new(),
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(posted.data.status, "posted");
        assert_business(
            service(&p)
                .void_supplier_bill(
                    "acme.example.test",
                    p.reviewer.clone(),
                    owned_reviewer(&p),
                    CommandKey::new("supplier-bill-posted-void-same").unwrap(),
                    created.data.id.clone(),
                    SupplierBillVoidInput {
                        expected_version: 2,
                        period_id: Some("period".into()),
                        entry_date: Some("2026-01-20".into()),
                        memo: None,
                    },
                )
                .await,
            "INDEPENDENT_APPROVER_REQUIRED",
        );
        let (third_proof, third_selection) = independent_void_actor(&p).await;
        let void_key = CommandKey::new("supplier-bill-posted-void-0001").unwrap();
        let input = SupplierBillVoidInput {
            expected_version: 2,
            period_id: Some("period".into()),
            entry_date: Some("2026-01-20".into()),
            memo: Some("Duplicate supplier invoice".into()),
        };
        let selection = third_selection.clone();
        let voided = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                third_proof.clone(),
                move || Ok(selection),
                void_key.clone(),
                created.data.id.clone(),
                input.clone(),
            )
            .await
            .unwrap();
        assert_eq!(voided.data.status, "voided");
        assert_eq!(voided.data.outstanding, "0.00");
        assert_eq!(voided.data.version, 3);
        let reversal_id = voided
            .data
            .reversal_journal_id
            .clone()
            .expect("posted void returns its reversing journal");
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT status,outstanding_minor,version FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "voided");
        assert_eq!(row.try_get::<i64, _>("outstanding_minor").unwrap(), 0);
        assert_eq!(row.try_get::<i64, _>("version").unwrap(), 3);
        let original: String = sqlx::query_scalar(
            "SELECT status FROM finance_journals WHERE id=$1 AND company_id='company'",
        )
        .bind(&posted.data.journal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(original, "reversed");
        let reversal: (String, String, String) = sqlx::query_as(
            "SELECT source_type,source_id,status FROM finance_journals WHERE id=$1 AND company_id='company'",
        )
        .bind(&reversal_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            reversal,
            (
                "invoice_reversal".into(),
                created.data.id.clone(),
                "posted".into()
            )
        );
        let lines: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&reversal_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                ("bill-expense".into(), 0, 25_000),
                ("bill-ap".into(), 26_750, 0),
                ("bill-vat-in".into(), 0, 1_750),
            ]
        );
        let selection = third_selection;
        let replay = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                third_proof,
                move || Ok(selection),
                void_key,
                created.data.id.clone(),
                input,
            )
            .await
            .unwrap();
        assert_eq!(replay.data.status, "voided");
        assert_eq!(replay.data.version, 3);
        assert_eq!(
            replay.data.reversal_journal_id.as_deref(),
            Some(reversal_id.as_str())
        );
    })
    .await;
}

#[tokio::test]
async fn supplier_bill_void_is_draft_only_and_replays() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let created = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-void-create").unwrap(),
                create_input("BILL-VOID-0001"),
            )
            .await
            .unwrap();
        assert_eq!(created.data.status, "draft");
        assert_eq!(created.data.version, 1);
        let voided = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-void-0001").unwrap(),
                created.data.id.clone(),
                SupplierBillVoidInput {
                    expected_version: 1,
                    period_id: None,
                    entry_date: None,
                    memo: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(voided.data.id, created.data.id);
        assert_eq!(voided.data.status, "voided");
        assert_eq!(voided.data.outstanding, "0.00");
        assert_eq!(voided.data.version, 2);
        let row = sqlx::query(
            "SELECT status,outstanding_minor,version FROM finance_invoices WHERE id=$1 AND company_id='company'",
        )
        .bind(&created.data.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(row.try_get::<String, _>("status").unwrap(), "voided");
        assert_eq!(row.try_get::<i64, _>("outstanding_minor").unwrap(), 0);
        assert_eq!(row.try_get::<i64, _>("version").unwrap(), 2);
        let replay = service(&p)
            .void_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-void-0001").unwrap(),
                created.data.id.clone(),
                SupplierBillVoidInput {
                    expected_version: 1,
                    period_id: None,
                    entry_date: None,
                    memo: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(replay.data.status, "voided");
        assert_eq!(replay.data.version, 2);
        assert_business(
            service(&p)
                .void_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-void-again").unwrap(),
                    created.data.id.clone(),
                    SupplierBillVoidInput {
                        expected_version: 2,
                        period_id: None,
                        entry_date: None,
                        memo: None,
                    },
                )
                .await,
            "INVOICE_NOT_DRAFT",
        );
        let fresh = service(&p)
            .create_supplier_bill(
                "acme.example.test",
                p.maker.clone(),
                owned_maker(&p),
                CommandKey::new("supplier-bill-void-create-2").unwrap(),
                create_input("BILL-VOID-0002"),
            )
            .await
            .unwrap();
        assert_business(
            service(&p)
                .void_supplier_bill(
                    "acme.example.test",
                    p.maker.clone(),
                    owned_maker(&p),
                    CommandKey::new("supplier-bill-void-stale").unwrap(),
                    fresh.data.id,
                    SupplierBillVoidInput {
                        expected_version: 99,
                        period_id: None,
                        entry_date: None,
                        memo: None,
                    },
                )
                .await,
            "VERSION_CONFLICT",
        );
    })
    .await;
}
