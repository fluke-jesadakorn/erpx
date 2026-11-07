//! Newly leased fully migrated databases only. Seed facts are not actor grants.
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId};
pub type Error = PgTransactionError<BusinessError>;
pub fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
pub fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
pub fn denied<T>(result: Result<T, Error>, expected: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, expected),
        _ => panic!("expected {expected}"),
    }
}
pub async fn setup(pool: &DatabasePool, tracking: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    let mut c = pool.raw().begin().await.unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company');INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Branch','company','company','branch');INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES('inventory','company','chart','100','Inventory','asset','THB',0,0),('expense','company','chart','500','Expense','expense','THB',0,0);INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES('period','company','January','2026-01-01','2026-01-31',0);INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('rate','company','THB','2026-01-01',2,'maker',1);INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','USD','inventory','inventory','inventory','inventory','inventory','expense','inventory','inventory','inventory');INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','W1','Warehouse')").execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','P1','Product',100,$1)").bind(tracking).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse','product',5000,1000)").execute(&mut *c).await.unwrap();
    if tracking != "none" {
        if tracking == "lot" {
            sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,expiry_date,created_by,created_at)VALUES('lot-1','company','product','lot','L1','2026-12-31','maker',0),('lot-2','company','product','lot','L2',NULL,'maker',0);INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES('lot-1','warehouse',3000,600),('lot-2','warehouse',2000,400)").execute(&mut *c).await.unwrap();
        } else {
            for index in 0..5 {
                sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,serial_number,created_by,created_at)VALUES($1,'company','product','serial',$2,'maker',0)").bind(format!("serial-{index}")).bind(format!("S{index}")).execute(&mut *c).await.unwrap();
                sqlx::query("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES($1,'warehouse',1000,200)").bind(format!("serial-{index}")).execute(&mut *c).await.unwrap();
            }
        }
    }
    c.commit().await.unwrap();
}
pub async fn hold(
    pool: &DatabasePool,
    id: &str,
    scope: &str,
    lot: Option<&str>,
    quantity: Option<i64>,
) {
    sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,quantity_scope,lot_id,held_quantity_milli)VALUES($1,'company','branch','warehouse','product','Hold','2026-12-31','maker',0,0,$2,$3,$4)").bind(id).bind(scope).bind(lot).bind(quantity).execute(pool.raw()).await.unwrap();
}
