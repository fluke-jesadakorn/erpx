//! Native inventory reservation alert sweep over leased PostgreSQL: Sales and
//! Manufacturing sources, idempotency, expiry phases and the production
//! inventory rollout gate.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::Clock;
use erpx_service::inventory_reservation_alerts::run_once;

async fn seed_masters(pool: &DatabasePool, now: i64) -> i64 {
    let expiring = now + 3_600_000;
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'),('branch','branch','inventory','Inventory branch','company','company','branch','active') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version)VALUES('inv-wh','company','branch','INV-WH','Inventory warehouse','active',1) ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type)VALUES('inv-product','company','INV-P','Inventory product','pcs',1000,0,'active',1,'none','stock') ON CONFLICT DO NOTHING;\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version)VALUES('inv-wh','inv-product',10000,1000,1) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('inv-customer','company','customer','INV-C','Inventory customer','THB',0,0) ON CONFLICT DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    expiring
}

async fn seed_sales(pool: &DatabasePool, suffix: &str, expires: i64, consumed: i64) {
    let order = format!("sales-order-{suffix}");
    let line = format!("sales-line-{suffix}");
    let reservation = format!("sales-res-{suffix}");
    sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES($1,'company','branch','sales',$2,'inv-customer','inv-wh','THB','2026-11-01','2026-11-10',2000,0,'confirmed',1,'fixture-a',0,0) ON CONFLICT DO NOTHING")
        .bind(&order)
        .bind(format!("SO-{suffix}"))
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES($1,$2,'inv-product','Reserved line',2000,0,1000,0,2000,0,1,'stock') ON CONFLICT DO NOTHING")
        .bind(&line)
        .bind(&order)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES($1,$2,'inv-wh','inv-product',2000,$3,0,$4,'fixture-b','Customer dispatch commitment',0) ON CONFLICT DO NOTHING")
        .bind(&reservation)
        .bind(&line)
        .bind(consumed)
        .bind(expires)
        .execute(pool.raw())
        .await
        .unwrap();
}

async fn seed_manufacturing(pool: &DatabasePool, suffix: &str, expires: i64, status: &str) {
    let order = format!("mfg-order-{suffix}");
    let reservation = format!("mfg-res-{suffix}");
    let spec = format!("mfg-spec-{suffix}");
    let scenario = format!("mfg-scenario-{suffix}");
    let recommendation = format!("mfg-rec-{suffix}");
    sqlx::query("INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,version,created_at,updated_at)VALUES($1,'company','Inventory spec','bom','inv-product',1000,10000,'approved','fixture-a',1,0,0) ON CONFLICT DO NOTHING")
        .bind(&spec)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at)VALUES($1,'company','branch','inv-wh',$2,1,1000,'2026-11-10',0,'[]','released','fixture-a',1,0,0) ON CONFLICT DO NOTHING")
        .bind(&scenario)
        .bind(&spec)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,version,created_at,updated_at)VALUES($1,$2,1,1000,0,'2026-11-10','approved','fixture-a',1,0,0) ON CONFLICT DO NOTHING")
        .bind(&recommendation)
        .bind(&scenario)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,material_cost_minor,labor_cost_minor,overhead_cost_minor,reported_output_milli,reported_scrap_milli,created_by,version,created_at,updated_at)VALUES($1,'company','branch','inv-wh',$2,$3,1,1000,$4,0,0,0,0,0,'fixture-c',1,0,0) ON CONFLICT DO NOTHING")
        .bind(&order)
        .bind(&recommendation)
        .bind(&spec)
        .bind(status)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO manufacturing_material_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,created_at)VALUES($1,$2,'inv-wh','inv-product',1000,0,0,$3,'fixture-d',0) ON CONFLICT DO NOTHING")
        .bind(&reservation)
        .bind(&order)
        .bind(expires)
        .execute(pool.raw())
        .await
        .unwrap();
}

#[tokio::test]
async fn inventory_reservation_alerts_cover_sales_and_manufacturing_idempotently() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let expiring = seed_masters(pool, now).await;
        seed_sales(pool, "one", expiring, 0).await;
        seed_manufacturing(pool, "one", expiring, "reserved").await;

        let first = run_once(pool.raw(), now, false).await.unwrap();
        assert_eq!(first, (4, 0));
        let second = run_once(pool.raw(), now, false).await.unwrap();
        assert_eq!(second, (0, 0));
        let expiring_notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type IN ('commerce_order','manufacturing_order') AND message LIKE '%expires within 24 hours%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(expiring_notices, 4);
        let versions: Vec<(String, i64)> = sqlx::query_as(
            "SELECT record_type,record_version FROM staff_notifications WHERE record_type IN ('commerce_order','manufacturing_order') ORDER BY record_type",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert!(versions.iter().all(|(_, version)| *version == 1));

        sqlx::query("UPDATE inventory_order_reservations SET created_at=1,expires_at=$1 WHERE id='sales-res-one'")
            .bind(now - 1)
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE manufacturing_material_reservations SET expires_at=$1 WHERE id='mfg-res-one'")
            .bind(now - 1)
            .execute(pool.raw())
            .await
            .unwrap();
        let expired = run_once(pool.raw(), now, false).await.unwrap();
        assert_eq!(expired, (0, 4));
        let expired_notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type IN ('commerce_order','manufacturing_order') AND message LIKE '%expired and no longer protects%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(expired_notices, 4);

        // Fully consumed reservations and non-reserved manufacturing orders do
        // not produce a second alert.
        seed_sales(pool, "consumed", now + 1_000, 2_000).await;
        seed_manufacturing(pool, "planned", now + 1_000, "planned").await;
        let ignored = run_once(pool.raw(), now, false).await.unwrap();
        assert_eq!(ignored, (0, 0));
    })
    .await;
}

#[tokio::test]
async fn inventory_reservation_alerts_respect_the_production_inventory_rollout() {
    review::account::with_system(|system| async move {
        let p = review::setup_with_finance(system, false).await;
        let pool = &p.system.holdings[0];
        let now = p.system.scenario.clock.now().get();
        let expiring = seed_masters(pool, now).await;
        seed_sales(pool, "gate", expiring, 0).await;
        seed_manufacturing(pool, "gate", expiring, "reserved").await;
        assert_eq!(run_once(pool.raw(), now, true).await.unwrap(), (0, 0));
        sqlx::query("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at)VALUES('company','inventory','pilot',1,'Synthetic inventory rollout','fixture-a',$1) ON CONFLICT DO NOTHING")
            .bind(now)
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(run_once(pool.raw(), now, true).await.unwrap(), (4, 0));
        assert_eq!(run_once(pool.raw(), now, true).await.unwrap(), (0, 0));
    })
    .await;
}
