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
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, UserId,
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
        assert_eq!(
            app.oneshot(missing_key).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    })
    .await;
}
