//! Native Quality alert sweep over a real serializable PostgreSQL unit.
#![allow(dead_code)]
#[path = "../../support/quality/quality-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::quality_alerts::drain_quality_alerts;

/// 2026-09-21T12:00:00Z.
const NOW: i64 = 1_789_992_000_000;
/// 2026-09-23T12:00:00Z.
const TWO_DAYS_LATER: i64 = 1_790_164_800_000;
const WINDOW: i64 = 86_400_000;

/// `id, phase, source_version, due_date, detected_at, version, acknowledged_by`.
type AlertRow = (String, String, i64, String, i64, i64, Option<String>);

fn holds() -> &'static str {
    "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company-other','company','company-other','Other Company','company-other');\
     INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse-company-other','company-other','branch','W9','Other company warehouse');\
     INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product-company-other','company-other','P9','Other company product',1000,'none');\
     INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse-company-other','product-company-other',1000,100);\
     INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope)VALUES\
     ('hold-today','company','branch','warehouse','product','Inspect before the promised day','2026-09-21','held','maker',0,0,'whole'),\
     ('hold-horizon','company','branch','warehouse','product','Inspect on the window horizon','2026-09-22','held','maker',0,0,'whole'),\
     ('hold-overdue','company','branch','warehouse','product','Recover the missed inspection','2026-09-20','held','maker',0,0,'whole'),\
     ('hold-later','company','branch','warehouse','product','Inspect beyond the window','2026-09-23','held','maker',0,0,'whole'),\
     ('hold-terminal','company','branch','warehouse','product','Cancelled hold keeps its history','2026-09-20','cancelled','maker',0,0,'whole'),\
     ('hold-other-company','company-other','branch','warehouse-company-other','product-company-other','Other Company inspection','2026-09-19','held','maker',0,0,'whole');"
}

#[tokio::test]
async fn the_scheduled_sweep_is_idempotent_and_covers_each_company_once() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(holds()).execute(pool.raw()).await.unwrap();

        assert_eq!(drain_quality_alerts(pool.clone(), NOW, WINDOW).await.unwrap(), (2, 2));
        let seeded: Vec<AlertRow> = sqlx::query_as(
            "SELECT id,phase,source_version,due_date::text,detected_at,version,acknowledged_by \
             FROM inventory_quality_alerts ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            seeded,
            vec![
                (
                    "quality-alert-due-soon-hold-horizon".into(),
                    "due_soon".into(),
                    1,
                    "2026-09-22".into(),
                    NOW,
                    1,
                    None
                ),
                (
                    "quality-alert-due-soon-hold-today".into(),
                    "due_soon".into(),
                    1,
                    "2026-09-21".into(),
                    NOW,
                    1,
                    None
                ),
                (
                    "quality-alert-overdue-hold-other-company".into(),
                    "overdue".into(),
                    1,
                    "2026-09-19".into(),
                    NOW,
                    1,
                    None
                ),
                (
                    "quality-alert-overdue-hold-overdue".into(),
                    "overdue".into(),
                    1,
                    "2026-09-20".into(),
                    NOW,
                    1,
                    None
                ),
            ]
        );

        // A restart or replay never duplicates an escalation identity.
        assert_eq!(drain_quality_alerts(pool.clone(), NOW, WINDOW).await.unwrap(), (0, 0));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_alerts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 4);

        // Later clocks advance the same holds across the window: the horizon
        // hold becomes overdue and the later hold enters the due-soon window.
        assert_eq!(
            drain_quality_alerts(pool.clone(), TWO_DAYS_LATER, WINDOW)
                .await
                .unwrap(),
            (1, 2)
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_alerts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 7);
        assert_eq!(
            drain_quality_alerts(pool.clone(), TWO_DAYS_LATER, WINDOW)
                .await
                .unwrap(),
            (0, 0)
        );
    })
    .await;
}

#[tokio::test]
async fn an_unusable_window_is_refused_before_the_sweep_writes() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(holds()).execute(pool.raw()).await.unwrap();
        let error = drain_quality_alerts(pool.clone(), NOW, -1).await.unwrap_err();
        assert_eq!(error.code, "QUALITY_ALERT_CLOCK_INVALID");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_alerts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}
