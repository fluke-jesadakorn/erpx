//! Finance-owned draft goods journal for a native sales delivery on migrated
//! PostgreSQL: exact chart/period/accounts, balanced two-line draft, source
//! binding, duplicate refusal and rollback with the caller unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::commerce_goods_journal::{
    CommerceGoodsJournalPosting, post_commerce_goods_journal,
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
         ('other','chart','company','501','Other expense','expense','THB','active',0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) \
         VALUES('period','company','Open','2026-01-01','2026-12-31','open',0);",
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

fn posting(journal: &str, value: i64) -> CommerceGoodsJournalPosting {
    CommerceGoodsJournalPosting {
        journal_id: journal.into(),
        actor: actor(),
        chart_id: "chart".into(),
        currency: "THB".into(),
        business_date: "2026-11-10".into(),
        fulfillment_id: "delivery-goods-0001".into(),
        memo: "Delivery delivery-goods-0001".into(),
        inventory_account_id: "inventory".into(),
        cogs_account_id: "cogs".into(),
        value_minor: value,
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
async fn goods_journal_is_balanced_draft_and_source_bound() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let id = post_commerce_goods_journal(&mut tx, &TestClock, &posting("journal-goods-0001", 5_000))
            .await
            .unwrap();
        assert_eq!(id, "journal-goods-0001");
        let journal: (String, String, String, String, i64) = sqlx::query_as(
            "SELECT status,chart_id,source_type,source_id,version FROM finance_journals WHERE id=$1",
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
                "delivery".into(),
                "delivery-goods-0001".into(),
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
                ("cogs".into(), 5_000, 0, 5_000, 0, 0),
                ("inventory".into(), 0, 5_000, 0, 5_000, 1)
            ]
        );
        // The deterministic journal id replays without appending lines.
        assert_eq!(
            post_commerce_goods_journal(&mut tx, &TestClock, &posting("journal-goods-0001", 5_000))
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
async fn goods_journal_rejects_closed_period_and_invalid_accounts() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE finance_periods SET state='closed' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &posting("journal-no-period", 5_000))
                .await,
            "GOODS_FINANCE_PERIOD_CLOSED",
        );
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE finance_periods SET state='open' WHERE id='period'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut invalid = posting("journal-same-account", 5_000);
        invalid.inventory_account_id = "cogs".into();
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &invalid).await,
            "GOODS_FINANCE_POSTING_INVALID",
        );
        let mut zero = posting("journal-zero", 0);
        zero.value_minor = 0;
        denied(
            post_commerce_goods_journal(&mut tx, &TestClock, &zero).await,
            "GOODS_FINANCE_POSTING_INVALID",
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
