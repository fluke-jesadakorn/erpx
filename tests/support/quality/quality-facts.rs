//! Newly leased fully migrated Quality test databases only.
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
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company');\
         INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES\
         ('branch','branch','branch','Branch','company','company','branch'),\
         ('branch-other','branch','branch-other','Other Branch','company','company','branch-other');\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES\
         ('warehouse','company','branch','W1','Quality warehouse'),\
         ('warehouse-other','company','branch-other','W2','Other warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES\
         ('product','company','P1','Quality product',1000,'none');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES\
         ('warehouse','product',5000,1000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

pub async fn hold(pool: &DatabasePool, id: &str) {
    sqlx::query(
        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope)VALUES($1,'company','branch','warehouse','product','Damaged packaging requires independent review','2026-11-10','held','maker',0,0,'whole')",
    )
    .bind(id)
    .execute(pool.raw())
    .await
    .unwrap();
}

pub async fn hold_in_branch(pool: &DatabasePool, id: &str, branch: &str) {
    sqlx::query(
        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope)VALUES($1,'company',$2,'warehouse','product','Scoped quality hold','2026-11-10','held','maker',0,0,'whole')",
    )
    .bind(id)
    .bind(branch)
    .execute(pool.raw())
    .await
    .unwrap();
}

pub async fn specification(pool: &DatabasePool, id: &str) {
    sqlx::query(
        "INSERT INTO inventory_quality_specs(id,company_id,product_id,name,status,version,created_by,created_at,updated_at)VALUES($1,'company','product','Receiving limits','draft',1,'maker',0,0)",
    )
    .bind(id)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO inventory_quality_spec_metrics(id,spec_id,code,label,unit,minimum_value,maximum_value,ordinal)VALUES\
         ('metric-weight','spec','weight','Weight','kg',9.9,10.1,0),\
         ('metric-temperature','spec','temperature','Temperature','C',NULL,30,1);\
         UPDATE inventory_quality_specs SET status='active',version=2,approved_by='manager',approval_reason='Approved limits',activated_at=0 WHERE id='spec';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

pub async fn inspection(pool: &DatabasePool, hold: &str, id: &str, result: &str) {
    sqlx::query(
        "INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at,spec_id,spec_version)VALUES($1,$2,1,$3,'Independent inspection evidence','inspector',1,'spec',2)",
    )
    .bind(id)
    .bind(hold)
    .bind(result)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO inventory_quality_measurements(id,inspection_id,metric_id,code,label,unit,minimum_value,maximum_value,measured_value,passed,ordinal)VALUES\
         ('measurement-weight','inspection','metric-weight','weight','Weight','kg',9.9,10.1,10.0,1,0),\
         ('measurement-temperature','inspection','metric-temperature','temperature','Temperature','C',NULL,30,25,1,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

pub async fn count(pool: &DatabasePool, hold: &str, id: &str) {
    sqlx::query(
        "INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,net_value_minor,created_by,prepared_by,created_at,updated_at,quality_hold_id)VALUES($1,'company','branch','warehouse',$2,'2026-11-10','Controlled scrap','submitted',1,-1000,'manager','manager',0,0,$3)",
    )
    .bind(id)
    .bind(format!("Q-{id}"))
    .bind(hold)
    .execute(pool.raw())
    .await
    .unwrap();
}
