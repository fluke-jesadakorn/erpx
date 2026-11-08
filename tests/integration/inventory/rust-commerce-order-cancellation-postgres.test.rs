//! Inventory-owned release of sales-order reservations on migrated PostgreSQL:
//! exact remaining quantity, retained reason, consumed rows untouched, Company
//! and order isolation.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_inventory::public::commerce_order_cancellation::release_confirmed_sales_order_reservations;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::CompanyId;
use sqlx::PgConnection;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
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
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) \
         VALUES('party','company','customer','C1','Customer','USD','active',1,0,0),\
         ('party-f','foreign','customer','C2','Foreign','USD','active',1,0,0);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) \
         VALUES('product','company','P1','Product','each',100,0,'active',1),\
         ('product-f','foreign','PF','Foreign','each',100,0,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) \
         VALUES('warehouse','company','branch','W1','Warehouse','active',1),\
         ('warehouse-f','foreign','foreign','WF','Foreign warehouse','active',1);\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) \
         VALUES('warehouse','product',10000,100000,1),\
         ('warehouse-f','product-f',10000,100000,1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) \
         VALUES('sales-1','company','branch','sales','SO-1','party','USD','2026-01-01','2026-02-01','','confirmed',3000,0,1,'fixture',0,0),\
         ('sales-2','company','branch','sales','SO-2','party','USD','2026-01-01','2026-02-01','','confirmed',1000,0,1,'fixture',0,0),\
         ('sales-f','foreign','foreign','sales','SO-F','party-f','USD','2026-01-01','2026-02-01','','confirmed',1000,0,1,'fixture',0,0);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) \
         VALUES('line-1','sales-1','product','Product',3,1000,0,3000,0,0),\
         ('line-2','sales-2','product','Product',1,1000,0,1000,0,0),\
         ('line-f','sales-f','product-f','Foreign',1,1000,0,1000,0,0);\
         INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at) \
         VALUES('res-partial','line-1','warehouse','product',1000,200,100,4102444800000,'fixture','sale',0),\
         ('res-consumed','line-1','warehouse','product',500,500,0,4102444800000,'fixture','sale',0),\
         ('res-other','line-2','warehouse','product',400,0,0,4102444800000,'fixture','sale',0),\
         ('res-foreign','line-f','warehouse-f','product-f',300,0,0,4102444800000,'fixture','sale',0);",
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

#[tokio::test]
async fn confirmed_cancel_releases_only_remaining_rows_for_the_company_order() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let released = release_confirmed_sales_order_reservations(&mut tx, &company(), "sales-1")
            .await
            .unwrap();
        assert_eq!(released, 1);
        let rows: Vec<(String, i64, i64, Option<String>)> = sqlx::query_as(
            "SELECT id,released_milli,consumed_milli,release_reason \
             FROM inventory_order_reservations ORDER BY id",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            rows,
            [
                ("res-consumed".into(), 0, 500, None::<String>),
                ("res-foreign".into(), 0, 0, None),
                ("res-other".into(), 0, 0, None),
                (
                    "res-partial".into(),
                    800,
                    200,
                    Some("Source order cancelled".into())
                ),
            ]
        );
        // A second call is idempotent: no row still has remaining quantity.
        assert_eq!(
            release_confirmed_sales_order_reservations(&mut tx, &company(), "sales-1")
                .await
                .unwrap(),
            0
        );
        tx.rollback().await.unwrap();
        let restored: i64 = sqlx::query_scalar(
            "SELECT released_milli FROM inventory_order_reservations WHERE id='res-partial'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(restored, 100);
    })
    .await;
}

#[tokio::test]
async fn release_requires_the_serializable_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        match release_confirmed_sales_order_reservations(&mut connection, &company(), "sales-1")
            .await
        {
            Err(PgTransactionError::Persistence(error)) => {
                assert!(error.to_string().contains("FINANCE_SERIALIZABLE_REQUIRED"))
            }
            _ => panic!("expected serializable requirement"),
        }
    })
    .await;
}
