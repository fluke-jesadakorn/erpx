//! Native Finance bank import and manual payment reconciliation over leased
//! PostgreSQL, current Company scope and the caller-owned transaction.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_finance::public::bank_reconciliation::*;
use erpx_server_finance::public::FinanceActor;
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId, UserId};

fn actor(user: &str, company: &str) -> FinanceActor {
    FinanceActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        assignment_id: AssignmentId::new("bank-fixture").unwrap(),
    }
}

async fn seed(p: &review::Prepared) {
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
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
    .execute(pool.raw())
    .await
    .unwrap();
    let _ = now;
}

fn import(key: &str) -> BankImportInput {
    BankImportInput {
        bank_account: "bank".into(),
        statement_number: format!("ST-{key}"),
        statement_date: "2026-01-31".into(),
        opening: "0.00".into(),
        closing: "10.00".into(),
        closing_fx_rate: Some("1.0".into()),
        source_hash: format!("hash-{key}"),
        rows: vec![
            BankImportRow {
                external_id: format!("EXT-{key}-1"),
                transaction_date: "2026-01-05".into(),
                description: Some("Customer settlement".into()),
                amount: "15.00".into(),
                currency: "THB".into(),
            },
            BankImportRow {
                external_id: format!("EXT-{key}-2"),
                transaction_date: "2026-01-06".into(),
                description: Some("Bank charge".into()),
                amount: "-5.00".into(),
                currency: "THB".into(),
            },
        ],
    }
}

#[tokio::test]
async fn import_statement_replays_and_reconciles_a_posted_payment() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let pool = &p.system.holdings[0];
        let clock = p.system.scenario.clock.clone();
        let maker = actor("fixture-a", "company");
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let plan = prepare_bank_import(&mut tx, &maker, "bank-import-1", &import("1"), clock.as_ref())
            .await
            .unwrap();
        let result = plan.persist(&mut tx).await.unwrap();
        assert_eq!(result["rows"], 2);
        tx.commit().await.unwrap();
        let (statements, rows): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM finance_bank_statements WHERE company_id='company'),\
             (SELECT count(*)::bigint FROM finance_bank_transactions WHERE company_id='company')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((statements, rows), (1, 2));
        let (audit, outbox): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*)::bigint FROM audit_events WHERE command_id='bank-import-1'),\
             (SELECT count(*)::bigint FROM outbox_events WHERE command_id='bank-import-1')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((audit, outbox), (1, 1));

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let replay = prepare_bank_import(&mut tx, &maker, "bank-import-1", &import("1"), clock.as_ref())
            .await
            .unwrap();
        assert_eq!(replay.persist(&mut tx).await.unwrap(), result);
        tx.rollback().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let matched = reconcile_bank_transaction(
            &mut tx,
            &maker,
            "bank-reconcile-1",
            "bankrow-bank-import-1-0",
            &BankReconcileInput {
                expected_version: 1,
                matched_type: "payment".into(),
                matched_id: "bank-payment".into(),
                proof: "Matched the customer settlement to the released receipt.".into(),
            },
            clock.as_ref(),
        )
        .await
        .unwrap();
        assert_eq!(matched["status"], "matched");
        assert_eq!(matched["version"], 2);
        tx.commit().await.unwrap();
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
            "SELECT match_status,matched_type,matched_id,version FROM finance_bank_transactions WHERE id='bankrow-bank-import-1-0'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("matched".into(), Some("payment".into()), Some("bank-payment".into()), 2));
    })
    .await;
}

#[tokio::test]
async fn import_rejects_unbalanced_duplicate_and_foreign_scope() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, true).await;
        seed(&p).await;
        let pool = &p.system.holdings[0];
        let clock = p.system.scenario.clock.clone();
        let maker = actor("fixture-a", "company");
        for (input, code) in [
            ({
                let mut input = import("unbalanced");
                input.closing = "99.00".into();
                input
            }, "STATEMENT_UNBALANCED"),
            ({
                let mut input = import("duplicate");
                input.rows[1].external_id = input.rows[0].external_id.clone();
                input
            }, "DUPLICATE_TRANSACTION"),
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            match prepare_bank_import(&mut tx, &maker, "bank-invalid", &input, clock.as_ref()).await {
                Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                    assert_eq!(error.code, code)
                }
                Err(other) => panic!("expected {code}, got {other:?}"),
                Ok(_) => panic!("expected {code}, got success"),
            }
            tx.rollback().await.unwrap();
        }
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let foreign = actor("fixture-a", "foreign");
        match prepare_bank_import(&mut tx, &foreign, "bank-foreign", &import("foreign"), clock.as_ref()).await {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "BANK_ACCOUNT_INVALID")
            }
            Err(other) => panic!("expected BANK_ACCOUNT_INVALID, got {other:?}"),
            Ok(_) => panic!("expected BANK_ACCOUNT_INVALID, got success"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}
