//! Native generic controlled payment preparation/review over leased PostgreSQL.
//! This covers the Finance-owned source facts and writer transitions; the
//! composition command receipt, IAM purchase and HTTP ingress are separate.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use erpx_server_finance::public::generic_payment::*;
use erpx_server_finance::public::service_receipt::*;
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId, UserId};
use facts::{account, setup};

fn actor(user: &str) -> erpx_server_finance::public::FinanceActor {
    erpx_server_finance::public::FinanceActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment-a").unwrap(),
    }
}

async fn seed(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0),\
         ('ar','company','chart','1200','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('income','company','chart','4000','Income','revenue','THB',0,'none','active',1,0,0),\
         ('bank','company','chart','1000','Bank','asset','THB',0,'bank','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('supplier','company','supplier','SUP-1','Synthetic Supplier','THB','active',1,0,0),\
         ('customer','company','customer','CUS-1','Synthetic Customer','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,status,version,created_by,created_at,updated_at,source_type)VALUES\
         ('bill-open','company','supplier','payable','BILL-1','2026-01-02','2026-01-31','THB',1000,0,1000,1000,1000,1000,'bank','ap','posted',1,'fixture-a',0,0,'supplier_bill'),\
         ('invoice-open','company','customer','receivable','INV-1','2026-01-02','2026-01-31','THB',1000,0,1000,1000,1000,1000,'ar','income','posted',1,'fixture-a',0,0,'finance_invoice')\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,version,imported_by,created_at)VALUES\
         ('statement-open','company','bank','ST-1','2026-01-10',0,1000,'imported','hash-st-1',1,'fixture-a',0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version)VALUES\
         ('bankrow-open','statement-open','company','2026-01-10','EXT-1','Incoming settlement',1000,'THB','unmatched',1)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','fixture-a',0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at)VALUES('period','company','January','2026-01-01','2026-01-31','open',0)\
         ON CONFLICT (id) DO UPDATE SET state='open';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn payment(kind: &str, invoice: &str, party: &str) -> GenericPaymentInput {
    GenericPaymentInput {
        kind: kind.into(),
        party_id: party.into(),
        payment_number: if kind == "receipt" { "RC-1" } else { "PAY-1" }.into(),
        payment_date: "2026-01-10".into(),
        currency: "THB".into(),
        amount: "10.00".into(),
        bank_account: "bank".into(),
        settlement_invoice_id: invoice.into(),
    }
}

#[tokio::test]
async fn generic_supplier_payment_and_customer_receipt_create_approve_release() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let clock = p.system.scenario.clock.clone();
        for (kind, invoice, party, id) in [
            ("payment", "bill-open", "supplier", "pay-generic"),
            ("receipt", "invoice-open", "customer", "rcpt-generic"),
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let maker = actor("fixture-a");
            let plan = prepare_generic_payment(
                &mut tx,
                &maker,
                id,
                &payment(kind, invoice, party),
                clock.as_ref(),
            )
            .await
            .unwrap();
            assert_eq!(plan.amount(), 1000);
            let retained = plan.persist(&mut tx).await.unwrap();
            retained.require_current(&mut tx).await.unwrap();
            tx.commit().await.unwrap();

            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let reviewer = actor("fixture-b");
            let source = read_service_receipt_review(&mut tx, &reviewer, id)
                .await
                .unwrap();
            assert_eq!(source.row()["kind"], kind);
            assert_eq!(source.row()["status"], "draft");
            source
                .apply(&mut tx, &reviewer, true, 1, false, false, clock.now().get())
                .await
                .unwrap();
            tx.commit().await.unwrap();

            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let releaser = actor("fixture-c");
            let source = read_service_receipt_review(&mut tx, &releaser, id)
                .await
                .unwrap();
            assert_eq!(source.row()["status"], "approved");
            source
                .apply(
                    &mut tx,
                    &releaser,
                    false,
                    2,
                    false,
                    false,
                    clock.now().get(),
                )
                .await
                .unwrap();
            tx.commit().await.unwrap();
            let status: String = sqlx::query_scalar(
                "SELECT status FROM finance_payments WHERE id=$1 AND company_id='company'",
            )
            .bind(id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(status, "released");
        }
    })
    .await;
}

#[tokio::test]
async fn generic_payment_rejects_expense_claim_and_wrong_invoice_kind() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let clock = p.system.scenario.clock.clone();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        match prepare_generic_payment(
            &mut tx,
            &actor("fixture-a"),
            "pay-invalid",
            &payment("payment", "invoice-open", "customer"),
            clock.as_ref(),
        )
        .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) => {
                assert_eq!(e.code, "PAYMENT_SETTLEMENT_SOURCE_INVALID")
            }
            Err(other) => panic!("expected business denial, got {other:?}"),
            Ok(_) => panic!("expected business denial, got success"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn generic_payment_confirmation_posts_payable_and_receivable_directions() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let clock = p.system.scenario.clock.clone();
        for (kind, invoice, party, id, control, control_debit) in [
            ("payment", "bill-open", "supplier", "pay-confirm", "ap", true),
            ("receipt", "invoice-open", "customer", "rcpt-confirm", "ar", false),
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let plan = prepare_generic_payment(
                &mut tx,
                &actor("fixture-a"),
                id,
                &payment(kind, invoice, party),
                clock.as_ref(),
            )
            .await
            .unwrap();
            plan.persist(&mut tx).await.unwrap();
            let reviewer = actor("fixture-b");
            let source = read_service_receipt_review(&mut tx, &reviewer, id)
                .await
                .unwrap();
            source
                .apply(&mut tx, &reviewer, true, 1, false, false, clock.now().get())
                .await
                .unwrap();
            let releaser = actor("fixture-c");
            let source = read_service_receipt_review(&mut tx, &releaser, id)
                .await
                .unwrap();
            source
                .apply(
                    &mut tx,
                    &releaser,
                    false,
                    2,
                    false,
                    false,
                    clock.now().get(),
                )
                .await
                .unwrap();
            let confirm = prepare_service_receipt_confirm(
                &mut tx,
                &releaser,
                id,
                invoice,
                "confirm-key-1",
                &ServiceReceiptConfirmInput {
                    expected_version: 3,
                    period_id: "period".into(),
                    bank_transaction_id: "bankrow-open".into(),
                    bank_version: 1,
                    bank_reference: "REF-1".into(),
                    proof: "Reviewed the bank result".into(),
                    fx_rate: None,
                    fx_gain_account_id: None,
                    fx_loss_account_id: None,
                    allocations: vec![ServiceReceiptAllocationInput {
                        invoice_id: invoice.into(),
                        amount: "10.00".into(),
                    }],
                    attachment_set_ids: Vec::new(),
                },
                clock.as_ref(),
            )
            .await
            .unwrap();
            let lines = &confirm.primary_draft().lines;
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].account_id, control);
            assert_eq!(lines[0].debit, if control_debit { "10.00" } else { "0.00" });
            assert_eq!(lines[0].credit, if control_debit { "0.00" } else { "10.00" });
            assert_eq!(lines[1].account_id, "bank");
            assert_eq!(lines[1].debit, if control_debit { "0.00" } else { "10.00" });
            assert_eq!(lines[1].credit, if control_debit { "10.00" } else { "0.00" });
            tx.rollback().await.unwrap();
        }
    })
    .await;
}
