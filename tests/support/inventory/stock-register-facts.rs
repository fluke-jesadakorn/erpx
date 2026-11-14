//! Newly leased, fully migrated databases for the Inventory stock-register
//! read. These rows are owner facts, never actor grants.
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

pub async fn setup(pool: &DatabasePool) {
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
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0),('chart-foreign','another-company','Foreign','THB','maker',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES\
           ('warehouse','company','branch','W1','Main warehouse'),\
           ('warehouse-two','company','branch-two','W2','Second warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES\
           ('product','company','P1','Tracked product',2500,'lot'),\
           ('untracked','company','U1','Untracked product',1000,'none');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES\
           ('warehouse','product',3000,3000),\
           ('warehouse-two','product',4000,4000),\
           ('warehouse','untracked',7000,7000);\
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,expiry_date,created_by,created_at)VALUES\
           ('lot-a','company','product','lot','L-A','2027-01-01','maker',0),\
           ('lot-b','company','product','lot','L-B',NULL,'maker',0);\
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor)VALUES\
           ('lot-a','warehouse',2000,2000),\
           ('lot-b','warehouse',1000,1000),\
           ('lot-a','warehouse-two',4000,4000);\
         INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,lot_id,held_quantity_milli,version,created_at,updated_at,quantity_scope)VALUES\
           ('hold-b','company','branch','warehouse','product','Packaging review','2026-11-10','held','inspector','lot-b',1000,1,0,0,'partial');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('reserve-party','company','customer','RES-1','Reservation customer','THB',0,0);\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at)VALUES('reserve-order','company','branch','sales','SO-RES','reserve-party','warehouse','THB','2026-11-01','2026-11-10',2500,0,'confirmed',1,'maker',0,0);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type)VALUES('reserve-line','reserve-order','product','Reserved product',1000,0,2500,0,2500,0,1,'stock');\
         INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)VALUES('reserve-1','reserve-line','warehouse','product',500,0,0,9007199254740991,'maker','Active dispatch reservation',0);\
         INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,created_at,updated_at)VALUES\
           ('count-draft','company','branch','warehouse','CNT-1','2026-11-04','Cycle count','draft',2,0,'maker','maker',0,0),\
           ('count-initial-command','company','branch','warehouse','CNT-2','2026-11-03','Opening physical count','posted',4,0,'maker','maker',0,0),\
           ('count-two','company','branch-two','warehouse-two','CNT-3','2026-11-02','Second branch count','draft',1,0,'maker','maker',0,0);",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    // Mark the posted count as the retained initial physical source. The
    // receipts only satisfy immutable reciprocal provenance; they are synthetic
    // test rows, not grants or product workflow evidence.
    sqlx::query(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES\
         ('inventory-account','company','chart','100','Inventory','asset','THB',0,0),\
         ('variance-account','company','chart','5400','Variance','expense','THB',0,0)",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','THB','inventory-account','inventory-account','inventory-account','inventory-account','inventory-account','inventory-account','inventory-account','inventory-account','inventory-account')",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal)VALUES\
         ('initial-line','count-initial-command','product',1,'Opening physical count','pcs',0,0,0,0,0,NULL,1)",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES\
         ('goods-setup','commerce.sales-goods-posting-setup.initialize','maker','hash','company',\
           jsonb_build_object('id','company','status','configured','version','1')::text,0,'{\"companyId\":\"company\"}'),\
         ('initial-command','inventory.count.create','maker','hash','count-initial-command',\
           jsonb_build_object('id','count-initial-command','status','draft','version',1,'_initialPhysicalStockCount',\
             jsonb_build_object('kind','initial-physical-stock-count','countId','count-initial-command',\
               'companyId','company','warehouseId','warehouse','branchId','branch','productId','product',\
               'countCommandId','initial-command','goodsSetupCommandId','goods-setup',\
               'goodsSetupRequestHash','hash','chartId','chart','inventoryAccountId','inventory-account',\
               'varianceAccountId','variance-account','currency','THB'))::text,0,'{\"companyId\":\"company\"}')",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_initial_physical_stock_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,goods_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at)VALUES\
         ('count-initial-command','company','branch','warehouse','product','initial-command','goods-setup','chart','inventory-account','variance-account','THB',\
          jsonb_build_object('kind','initial-physical-stock-count','countId','count-initial-command',\
            'companyId','company','warehouseId','warehouse','branchId','branch','productId','product',\
            'countCommandId','initial-command','goodsSetupCommandId','goods-setup',\
            'goodsSetupRequestHash','hash','chartId','chart','inventoryAccountId','inventory-account',\
            'varianceAccountId','variance-account','currency','THB'),'maker',0)",
    )
    .execute(&mut *c)
    .await
    .unwrap();
    c.commit().await.unwrap();
}
