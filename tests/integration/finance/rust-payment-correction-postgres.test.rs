//! Native Finance payment allocation and independent reversal over leased
//! PostgreSQL. The composition receipt/IAM/HTTP ingress is a separate gate.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use erpx_server_finance::public::payment_correction::*;
use erpx_server_shared::public::{AssignmentId, CompanyId, HoldingId, UserId};
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
        "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','fixture-a',0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0),\
         ('ar','company','chart','1200','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('income','company','chart','4000','Income','revenue','THB',0,'none','active',1,0,0),\
         ('bank','company','chart','1000','Bank','asset','THB',0,'bank','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at)VALUES\
         ('supplier','company','supplier','SUP-1','Synthetic Supplier','THB','active',1,0,0),\
         ('customer','company','customer','CUS-1','Synthetic Customer','THB','active',1,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at)VALUES\
         ('period','company','January','2026-01-01','2026-01-31','open',0),\
         ('closed','company','December','2025-12-01','2025-12-31','closed',0)\
         ON CONFLICT (id) DO UPDATE SET state=EXCLUDED.state;\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at)VALUES\
         ('journal-invoice','company','chart','period','J-INV','2026-01-01','THB',1000000,'supplier_bill','bill-open','Invoice','posted','fixture-a','fixture-a',0,0,0),\
         ('journal-alloc','company','chart','period','J-ALLOC','2026-01-10','THB',1000000,'payment','pay-alloc','Payment','posted','fixture-a','fixture-a',0,0,0),\
         ('journal-pay','company','chart','period','J-PAY','2026-01-10','THB',1000000,'payment','pay-settled','Payment','posted','fixture-a','fixture-a',0,0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)VALUES\
         ('jl-inv-1','journal-invoice','bank','Invoice',1000,0,1000,0,'{}',1),\
         ('jl-inv-2','journal-invoice','ap','Invoice',0,1000,0,1000,'{}',2),\
         ('jl-alloc-1','journal-alloc','ap','Payment',1000,0,1000,0,'{}',1),\
         ('jl-alloc-2','journal-alloc','bank','Payment',0,1000,0,1000,'{}',2),\
         ('jl-pay-1','journal-pay','ap','Payment',1000,0,1000,0,'{}',1),\
         ('jl-pay-2','journal-pay','bank','Payment',0,1000,0,1000,'{}',2)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,fx_rate_micros,journal_id,status,version,created_by,created_at,updated_at,source_type)VALUES\
         ('bill-open','company','supplier','payable','BILL-1','2026-01-02','2026-01-31','THB',1000,0,1000,1000,1000,1000,'bank','ap',1000000,'journal-invoice','posted',1,'fixture-a',0,0,'supplier_bill'),\
         ('bill-settled','company','supplier','payable','BILL-2','2026-01-02','2026-01-31','THB',1000,0,1000,0,1000,0,'bank','ap',1000000,'journal-invoice','paid',1,'fixture-a',0,0,'supplier_bill')\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,version,created_by,approved_by,released_by,created_at,updated_at)VALUES\
         ('pay-alloc','company','supplier','payment','PAY-A','2026-01-10','THB',1000,1000,'bank','settled','journal-alloc',1,'fixture-a','fixture-b','fixture-c',0,0),\
         ('pay-settled','company','supplier','payment','PAY-S','2026-01-10','THB',1000,0,'bank','settled','journal-pay',1,'fixture-a','fixture-b','fixture-c',0,0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at)VALUES\
         ('allocation-settled','company','pay-settled','bill-settled',1000,1000,1000,'fixture-c',0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,version,imported_by,created_at)VALUES\
         ('statement-pay','company','bank','ST-P','2026-01-10',0,1000,'imported','hash-st-p',1,'fixture-a',0)\
         ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,matched_type,matched_id,version)VALUES\
         ('bankrow-pay','statement-pay','company','2026-01-10','EXT-P','Settlement',1000,'THB','matched','payment','pay-settled',1)\
         ON CONFLICT (id) DO UPDATE SET match_status='matched',matched_type='payment',matched_id='pay-settled',version=1;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn allocation_updates_payment_and_invoice_with_guards() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = allocate_payment(
            &mut tx,
            &actor("fixture-b"),
            "pay-alloc",
            &PaymentAllocationInput {
                invoice_id: "bill-open".into(),
                amount: "6.00".into(),
                expected_version: 1,
            },
            "alloc-key",
            1_700_000_000_000,
        )
        .await
        .unwrap();
        assert_eq!(result["allocated"], "6.00");
        assert_eq!(result["paymentVersion"], 2);
        assert_eq!(result["invoiceVersion"], 2);
        let paid: i64 =
            sqlx::query_scalar("SELECT outstanding_minor FROM finance_payments WHERE id='pay-alloc'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let open: i64 = sqlx::query_scalar(
            "SELECT outstanding_minor FROM finance_invoices WHERE id='bill-open'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let allocated: i64 = sqlx::query_scalar(
            "SELECT amount_minor FROM finance_allocations WHERE id='allocation-alloc-key'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((paid, open, allocated), (400, 400, 600));
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reversal_restores_invoice_posts_balanced_journal_and_unmatches_bank() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = reverse_payment(
            &mut tx,
            &actor("fixture-b"),
            "pay-settled",
            &PaymentReversalInput {
                expected_version: 1,
                period_id: "period".into(),
                entry_date: "2026-01-15".into(),
                reason: "Independent correction after bank review".into(),
            },
            "reverse-key",
            1_700_000_000_000,
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "voided");
        assert_eq!(result["version"], 2);
        assert_eq!(result["restoredAllocations"], 1);
        let payment: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM finance_payments WHERE id='pay-settled'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(payment, ("voided".into(), 2));
        let invoice: (i64, String) = sqlx::query_as(
            "SELECT outstanding_minor,status FROM finance_invoices WHERE id='bill-settled'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(invoice, (1000, "posted".into()));
        let original: String =
            sqlx::query_scalar("SELECT status FROM finance_journals WHERE id='journal-pay'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(original, "reversed");
        let lines: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT debit_minor,credit_minor FROM finance_journal_lines \
             WHERE journal_id='payment-reversal-reverse-key' ORDER BY ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(lines, vec![(0, 1000), (1000, 0)]);
        let bank: String = sqlx::query_scalar(
            "SELECT match_status FROM finance_bank_transactions WHERE id='bankrow-pay'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bank, "unmatched");
        let correction: String = sqlx::query_scalar(
            "SELECT reversal_journal_id FROM finance_payment_reversals WHERE payment_id='pay-settled'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(correction, "payment-reversal-reverse-key");
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reversal_rejects_creator_closed_period_and_stale_version() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        for (actor_id, period, version, code) in [
            ("fixture-a", "period", 1, "INDEPENDENT_CORRECTION_REQUIRED"),
            ("fixture-b", "closed", 1, "REVERSAL_PERIOD_CLOSED"),
            ("fixture-b", "period", 2, "PAYMENT_SOURCE_CHANGED"),
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            match reverse_payment(
                &mut tx,
                &actor(actor_id),
                "pay-settled",
                &PaymentReversalInput {
                    expected_version: version,
                    period_id: period.into(),
                    entry_date: "2026-01-15".into(),
                    reason: "Independent correction after bank review".into(),
                },
                &format!("reverse-deny-{version}-{period}"),
                1_700_000_000_000,
            )
            .await
            {
                Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) => {
                    assert_eq!(e.code, code)
                }
                Err(other) => panic!("expected business denial {code}, got {other:?}"),
                Ok(_) => panic!("expected business denial {code}, got success"),
            }
            tx.rollback().await.unwrap();
        }
    })
    .await;
}
