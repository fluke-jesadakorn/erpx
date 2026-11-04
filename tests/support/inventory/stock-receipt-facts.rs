//! Newly leased, fully migrated databases for the Inventory goods-receipt
//! seam. Commerce fulfillment rows are synthetic source facts, not a command
//! receipt, purchase or authorization proof.
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

pub async fn setup(pool: &DatabasePool, tracking: &str, with_stock: bool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    let mut c = pool.raw().begin().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company');\
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Branch','company','company','branch');\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','W1','Main warehouse');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('supplier','company','supplier','SUP-1','Supplier','THB',0,0);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','P1','Receipt product',2500,$1)",
    )
    .bind(tracking)
    .execute(&mut *c)
    .await
    .unwrap();
    if with_stock {
        sqlx::query(
            "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version)VALUES('warehouse','product',1000,1000,1)",
        )
        .execute(&mut *c)
        .await
        .unwrap();
        if tracking == "lot" {
            sqlx::raw_sql(
                "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at)VALUES('lot-existing','company','product','lot','L-A','maker',0);\
                 INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('lot-existing','warehouse',1000,1000);",
            )
            .execute(&mut *c)
            .await
            .unwrap();
        }
    }
    sqlx::raw_sql(
        "INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES\
         ('order-1','company','branch','purchase','PO-1','supplier','warehouse','THB','2026-11-01','2026-11-10',12500,0,'confirmed',3,'maker',0,0);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES\
         ('line-1','order-1','product','Receipt product',5000,0,2500,0,12500,0,1,'stock');\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version)VALUES\
         ('delivery-key','order-1','GR-TEST','2026-11-04','maker',0,1);\
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor)VALUES\
         ('delivery-key-0','delivery-key','line-1',5000,12500,12500,0);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    c.commit().await.unwrap();
}
