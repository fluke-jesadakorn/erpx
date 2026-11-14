//! Finance-owned functional-FX goods journal for native Commerce receipt and
//! delivery on migrated PostgreSQL: exact chart/period/accounts, functional
//! stock valuation, balanced draft lines, source binding, duplicate refusal and
//! rollback with the caller unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::commerce_goods_journal::{
    CommerceGoodsJournalPosting, CommerceGoodsKind, CommerceGoodsValuationRequest,
    post_commerce_goods_journal, resolve_commerce_goods_valuation,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

fn actor() -> FinanceActor {
    FinanceActor {
        user_id: UserId::new("fixture").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) \
         VALUES('chart','company','Primary','THB','active','fixture',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) \
         VALUES('inventory','chart','company','100','Inventory','asset','THB','active',0,0),\
         ('cogs','chart','company','500','Cost of sales','expense','THB','active',0,0),\
         ('clearing','chart','company','200','Goods clearing','liability','THB','active',0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) \
         VALUES('period','company','Open','2026-01-01','2026-12-31','open',0);\
         INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) \
         VALUES('thb-rate','company','THB','2026-01-01',35.5,'fixture',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(c: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(c)
        .await
        .unwrap();
}

fn valuation_request(currency: &str) -> CommerceGoodsValuationRequest {
    CommerceGoodsValuationRequest {
        actor: actor(),
        chart_id: "chart".into(),
        business_date: "2026-11-10".into(),
        document_currency: currency.into(),
    }
}

fn receipt(journal: &str, document: i64, stock: i64) -> CommerceGoodsJournalPosting {
    CommerceGoodsJournalPosting {
        journal_id: journal.into(),
        actor: actor(),
        chart_id: "chart".into(),
        business_date: "2026-11-10".into(),
        fulfillment_id: "delivery-goods-receipt".into(),
        memo: "Goods receipt delivery-goods-receipt".into(),
        inventory_account_id: "inventory".into(),
        kind: CommerceGoodsKind::Receipt {
            clearing_account_id: "clearing".into(),
        },
        document_currency: "USD".into(),
        document_subtotal_minor: document,
        stock_value_minor: stock,
        fx_rate_micros: Some(35_500_000),
    }
}

fn delivery(journal: &str, value: i64) -> CommerceGoodsJournalPosting {
    CommerceGoodsJournalPosting {
        journal_id: journal.into(),
        actor: actor(),
        chart_id: "chart".into(),
        business_date: "2026-11-10".into(),
        fulfillment_id: "delivery-goods-0001".into(),
        memo: "Delivery delivery-goods-0001".into(),
        inventory_account_id: "inventory".into(),
        kind: CommerceGoodsKind::Delivery {
            cogs_account_id: "cogs".into(),
        },
        document_currency: "THB".into(),
        document_subtotal_minor: value,
        stock_value_minor: value,
        fx_rate_micros: Some(1_000_000),
    }
}

fn denied<T>(
    result: Result<T, erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>>,
    code: &str,
) {
    match result {
        Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
            assert_eq!(error.code, code)
        }
        _ => panic!("expected {code}"),
    }
}

#[tokio::test]
async fn goods_receipt_uses_functional_fx_and_is_source_bound() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let valuation = resolve_commerce_goods_valuation(&mut tx, &valuation_request("USD"))
            .await
            .unwrap();
        assert_eq!(valuation.stock_currency(), "THB");
        assert_eq!(valuation.fx_rate_micros(), 35_500_000);
        assert_eq!(valuation.stock_value_minor(10_000).unwrap(), 355_000);
        // 333 USD cents at 35.5 is 11,821.5 THB cents; round half away once.
        assert_eq!(valuation.stock_value_minor(333).unwrap(), 11_822);

        let id = post_commerce_goods_journal(
            &mut tx,
            &TestClock,
            &receipt("journal-goods-0001", 10_000, 355_000),
        )
        .await
        .unwrap();
        assert_eq!(id, "journal-goods-0001");
        let journal: (String, String, String, String, String, i64, i64) = sqlx::query_as(
            "SELECT status,chart_id,source_type,source_id,currency,fx_rate_micros,version \
             FROM finance_journals WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            journal,
            (
                "draft".into(),
                "chart".into(),
                "goods_receipt".into(),
                "delivery-goods-receipt".into(),
                "USD".into(),
                35_500_000,
                1
            )
        );
        let lines: Vec<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal \
             FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            lines,
            [
                ("inventory".into(), 10_000, 0, 355_000, 0, 0),
                ("clearing".into(), 0, 10_000, 0, 355_000, 1)
            ]
        );
        // The deterministic journal id replays without appending lines.
        assert_eq!(
            post_commerce_goods_journal(
                &mut tx,
                &TestClock,
                &receipt("journal-goods-0001", 10_000, 355_000)
            )
            .await
            .unwrap(),
            "journal-goods-0001"
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM finance_journal_lines WHERE journal_id=$1")
                .bind(&id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(count, 2);
        // Different evidence under the same identity is refused, never rewritten.
        let mut other_source = receipt("journal-goods-0001", 10_000, 355_000);
        other_source.fulfillment_id = "delivery-other-receipt".into();
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &other_source).await,
            "GOODS_FINANCE_JOURNAL_CHANGED",
        );
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM finance_journals")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn goods_delivery_posts_base_value_and_rejects_foreign_or_wrong_valuation() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let id = post_commerce_goods_journal(
            &mut tx,
            &TestClock,
            &delivery("journal-goods-0002", 5_000),
        )
        .await
        .unwrap();
        let journal: (String, String, i64) = sqlx::query_as(
            "SELECT source_type,currency,fx_rate_micros FROM finance_journals WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(journal, ("delivery".into(), "THB".into(), 1_000_000));
        let lines: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor \
             FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            lines,
            [
                ("cogs".into(), 5_000, 0, 5_000, 0),
                ("inventory".into(), 0, 5_000, 0, 5_000)
            ]
        );
        // A caller valuation that disagrees with the current quote is refused.
        denied(
            post_commerce_goods_journal(
                &mut tx,
                &TestClock,
                &receipt("journal-goods-wrong-value", 10_000, 354_999),
            )
            .await,
            "GOODS_FINANCE_VALUATION_MISMATCH",
        );
        // Deliveries are always valued in the Company base currency.
        let mut foreign = delivery("journal-goods-foreign-delivery", 5_000);
        foreign.document_currency = "USD".into();
        foreign.document_subtotal_minor = 5_000;
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &foreign).await,
            "GOODS_FINANCE_POSTING_INVALID",
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn goods_journal_rejects_missing_rate_closed_period_and_foreign_chart() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        // EUR has no Company quote, so a foreign receipt cannot be valued.
        denied(
            resolve_commerce_goods_valuation(&mut tx, &valuation_request("EUR")).await,
            "GOODS_FINANCE_RATE_REQUIRED",
        );
        let mut euro = receipt("journal-goods-euro", 10_000, 355_000);
        euro.document_currency = "EUR".into();
        euro.fx_rate_micros = None;
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &euro).await,
            "GOODS_FINANCE_RATE_REQUIRED",
        );
        // A chart from another Company is never accepted by a FinanceActor.
        let mut foreign_chart = receipt("journal-goods-foreign-chart", 10_000, 355_000);
        foreign_chart.chart_id = "other-chart".into();
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &foreign_chart).await,
            "GOODS_FINANCE_CHART_UNAVAILABLE",
        );
        // Inventory and clearing may not collapse into one account.
        let mut same_account = receipt("journal-goods-same-account", 10_000, 355_000);
        same_account.kind = CommerceGoodsKind::Receipt {
            clearing_account_id: "inventory".into(),
        };
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &same_account).await,
            "GOODS_FINANCE_POSTING_INVALID",
        );
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE finance_periods SET state='closed' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        denied(
            post_commerce_goods_journal(
                &mut tx,
                &TestClock,
                &receipt("journal-no-period", 10_000, 355_000),
            )
            .await,
            "GOODS_FINANCE_PERIOD_CLOSED",
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn goods_journal_requires_the_caller_transaction() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        match post_commerce_goods_journal(
            &mut connection,
            &TestClock,
            &delivery("journal-no-unit", 5_000),
        )
        .await
        {
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "GOODS_FINANCE_TRANSACTION_REQUIRED")
            }
            _ => panic!("expected GOODS_FINANCE_TRANSACTION_REQUIRED"),
        }
    })
    .await;
}
