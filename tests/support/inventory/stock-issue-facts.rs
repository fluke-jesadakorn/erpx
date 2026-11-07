//! Newly leased, fully migrated databases for the Inventory sales-delivery
//! seam. Commerce fulfillment rows are synthetic source facts, not a command
//! receipt, purchase or authorization proof.
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

pub async fn setup(pool: &DatabasePool, tracking: &str) {
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
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES\
           ('branch','branch','branch','Branch','company','company','branch'),\
           ('branch-two','branch','branch-two','Second branch','company','company','branch-two');\
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','W1','Main warehouse');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES\
           ('customer','company','customer','CUS-1','Customer','THB',0,0),\
           ('supplier','company','supplier','SUP-1','Supplier','THB',0,0);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','P1','Delivery product',2500,$1)",
    )
    .bind(tracking)
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode,item_type)VALUES('service-product','company','S1','Customer service',1000,'none','service')",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version)VALUES('warehouse','product',4000,2000,1)",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    match tracking {
        "lot" => {
            sqlx::raw_sql(
                "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at)VALUES\
                   ('lot-a','company','product','lot','L-A','maker',0),\
                   ('lot-b','company','product','lot','L-B','maker',0);\
                 INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES\
                   ('lot-a','warehouse',1000,500),\
                   ('lot-b','warehouse',3000,1500);",
            )
            .execute(&mut *c)
            .await
            .unwrap();
        }
        "serial" => {
            sqlx::raw_sql(
                "INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,serial_number,created_by,created_at)VALUES\
                   ('serial-a','company','product','serial','SN-A','maker',0),\
                   ('serial-b','company','product','serial','SN-B','maker',0),\
                   ('serial-c','company','product','serial','SN-C','maker',0),\
                   ('serial-d','company','product','serial','SN-D','maker',0);\
                 INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES\
                   ('serial-a','warehouse',1000,500),\
                   ('serial-b','warehouse',1000,500),\
                   ('serial-c','warehouse',1000,500),\
                   ('serial-d','warehouse',1000,500);",
            )
            .execute(&mut *c)
            .await
            .unwrap();
        }
        _ => {}
    }
    sqlx::raw_sql(
        "INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES\
         ('order-1','company','branch','sales','SO-1','customer','warehouse','THB','2026-11-01','2026-11-10',11000,0,'confirmed',4,'maker',0,0);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES\
         ('line-1','order-1','product','Delivery product',4000,0,2500,0,10000,0,1,'stock'),\
         ('line-service','order-1','service-product','Customer service',1000,0,1000,0,1000,0,2,'service');\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version)VALUES\
         ('delivery-key','order-1','DN-TEST','2026-11-04','maker',0,1);\
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor)VALUES\
         ('delivery-key-0','delivery-key','line-1',2000,1000,5000,0);\
         INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES\
         ('reservation-1','line-1','warehouse','product',2000,0,0,9007199254740991,'maker','Sales reserve',0);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    c.commit().await.unwrap();
}

/// Align the synthetic Commerce fulfillment line with the delivery plan a test
/// applies. The real Commerce command inserts the fulfillment in the same
/// transaction using the plan values; these tests use one fixed row and must
/// keep its quantity/value equal to the issue evidence.
pub async fn align_fulfillment(c: &mut sqlx::PgConnection, quantity_milli: i64, value_minor: i64) {
    sqlx::query(
        "UPDATE commerce_fulfillment_lines SET quantity_milli=$1,value_minor=$2 \
         WHERE id='delivery-key-0' AND fulfillment_id='delivery-key'",
    )
    .bind(quantity_milli)
    .bind(value_minor)
    .execute(&mut *c)
    .await
    .unwrap();
}
