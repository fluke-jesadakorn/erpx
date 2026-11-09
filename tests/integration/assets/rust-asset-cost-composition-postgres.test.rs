//! The composition Finance acquisition-cost handoff on a real serializable
//! Holding transaction: the Finance-owned snapshot is written before the Asset
//! register row and rolls back with the owner unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_assets::public::asset_register::{AcquisitionCostProvider, NewAsset};
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
use erpx_server_shared::public::{Clock, CompanyId, TimestampMillis};
use erpx_service::asset_http::FinanceAssetCost;

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn asset(cost_minor: i64, currency: &str, acquired_date: &str) -> NewAsset {
    NewAsset::parse(&serde_json::json!({
        "branchId": "branch",
        "code": "COMPOSITION-COST-1",
        "name": "Composition cost asset",
        "category": "equipment",
        "currency": currency,
        "acquiredDate": acquired_date,
    }), cost_minor).expect("valid Asset fixture")
}

#[tokio::test]
async fn composition_finance_cost_snapshot_is_exact_and_rolls_back_with_the_asset_unit() {
    fixture::with_database(3, |pool| async move {
        let files = load_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
            MigrationTrack::HoldingWrite,
        )
        .await
        .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
             VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
             ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
             INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at) \
             VALUES('company','THB','fixture',0) ON CONFLICT(company_id) DO UPDATE SET default_currency='THB';\
             INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) \
             VALUES('company','THB',2,'fixture',0);\
             INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) \
             VALUES('composition-rate','company','THB','2026-01-01',36.5,'fixture',0,'reviewed');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let company = CompanyId::new("company").unwrap();
        let provider = FinanceAssetCost::new(company, std::sync::Arc::new(TestClock));

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        provider
            .prepare(&mut tx, "asset-composition-money", &asset(101, "THB", "2026-01-15"))
            .await
            .unwrap();
        let stored: (String, i32, String, String, Option<String>, bool, i64) = sqlx::query_as(
            "SELECT transaction_date::text,currency_scale::int,amount_units::text,\
             usd_units::text,fx_rate_id,used_latest_fallback,captured_at \
             FROM money_transaction_snapshots WHERE company_id='company' \
             AND source_type='operational_asset' AND source_id='asset-composition-money' \
             AND amount_kind='acquisition_cost'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored.0, "2026-01-15");
        assert_eq!(stored.1, 2);
        assert_eq!(stored.2, "101");
        assert_eq!(stored.4.as_deref(), Some("composition-rate"));
        assert!(!stored.5);
        assert_eq!(stored.6, 1_800_000_000_000);
        assert!(stored.3.parse::<i64>().unwrap() > 0);
        tx.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots WHERE source_id='asset-composition-money'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}
