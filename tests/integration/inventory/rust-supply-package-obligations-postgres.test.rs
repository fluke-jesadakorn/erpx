//! Native Supply inventory package-obligation read over leased PostgreSQL.
//! Covers the owner scope gate, the empty-image report and the invalid-stock
//! pending signal that the retired TS read family owned.
#[path = "../../support/rust-postgres.rs"]
#[allow(dead_code)]
mod fixture;
use erpx_server_inventory::public::supply_package_obligations::{
    lock_supply_inventory_package_sources, read_supply_inventory_package_obligations,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

async fn migrated(pool: &DatabasePool) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

#[tokio::test]
async fn rejects_any_scope_mismatch_before_any_read() {
    fixture::with_database(2, |pool| async move {
        migrated(&pool).await;
        let mut tx = serializable(&pool).await;
        for (company, package, version, module, now) in [
            ("", "supply", 1, "inventory", 0),
            ("company", "sales", 1, "inventory", 0),
            ("company", "supply", 2, "inventory", 0),
            ("company", "supply", 1, "quality", 0),
            ("company", "supply", 1, "inventory", -1),
            ("company", "supply-advanced", 1, "finance", 0),
        ] {
            assert!(
                read_supply_inventory_package_obligations(
                    &mut tx, company, package, version, module, now
                )
                .await
                .is_err(),
                "scope must fail for {company}/{package}/{version}/{module}/{now}"
            );
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn empty_company_reports_zero_outstanding_with_stable_revision() {
    fixture::with_database(2, |pool| async move {
        migrated(&pool).await;
        let mut tx = serializable(&pool).await;
        lock_supply_inventory_package_sources(&mut tx).await.unwrap();
        let first =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 7)
                .await
                .unwrap();
        assert_eq!(first.company_id, "company");
        assert_eq!(first.package_id, "supply");
        assert_eq!(first.module_id, "inventory");
        assert_eq!(first.observed_at, 7);
        assert_eq!(first.outstanding, 0);
        assert!(first.facts.is_empty());
        assert_eq!(first.revision.len(), 64);
        let second =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 7)
                .await
                .unwrap();
        assert_eq!(first.revision, second.revision);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn transit_branch_pending_until_completed_match() {
    fixture::with_database(2, |pool| async move {
        migrated(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company') ON CONFLICT DO NOTHING;\
             INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Branch','company','company','branch') ON CONFLICT DO NOTHING;\
             INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('wh','company','branch','WH','Main'),('wh2','company','branch','WH2','Second') ON CONFLICT DO NOTHING;\
             INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','P1','Product',100,'none') ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,status,total_value_minor,delivery_mode,created_by,prepared_by,created_at,updated_at)VALUES('transfer-1','company','wh','wh2','branch','branch',1,1,'T-1','2026-11-10','Replenishment','draft',0,'transit','maker','maker',0,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO inventory_transfer_lines(id,transfer_id,product_id,product_version,description,unit,quantity_milli,value_minor,source_quantity_milli,source_value_minor,source_version,ordinal)VALUES('tline-1','transfer-1','product',1,'Widget','pcs',3000,1000,3000,1000,1,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO inventory_transit(id,transfer_id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,status,due_date,currency,chart_id,inventory_account_id,created_by,approved_by,created_at,updated_at)VALUES('transit-1','transfer-1','company','wh','wh2','branch','branch','in_transit','2026-11-10','THB','chart','inventory','maker','manager',0,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO inventory_transit_lines(id,transit_id,source_line_id,product_id,quantity_milli,value_minor,received_milli,lost_milli,settled_value_minor)VALUES('line-1','transit-1','tline-1','product',3000,1000,0,0,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = serializable(&pool).await;
        lock_supply_inventory_package_sources(&mut tx).await.unwrap();
        let moving =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 9)
                .await
                .unwrap();
        let transit = moving
            .facts
            .iter()
            .find(|fact| fact.kind == "transit")
            .expect("transit fact");
        assert!(transit.pending);
        tx.rollback().await.unwrap();
        sqlx::query("INSERT INTO inventory_transit_events(id,transit_id,line_id,kind,quantity_milli,value_minor,movement_date,evidence,actor_id,reviewed_version,created_at)VALUES('event-1','transit-1','line-1','receipt',3000,1000,'2026-11-11','Received', 'receiver',2,1)")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE inventory_transit_lines SET received_milli=3000,settled_value_minor=1000 WHERE id='line-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE inventory_transit SET status='completed' WHERE id='transit-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = serializable(&pool).await;
        lock_supply_inventory_package_sources(&mut tx).await.unwrap();
        let done =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 9)
                .await
                .unwrap();
        let transit = done
            .facts
            .iter()
            .find(|fact| fact.kind == "transit")
            .expect("transit fact");
        assert!(!transit.pending);
        assert_ne!(moving.revision, done.revision);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn draft_count_is_pending_and_removal_settles() {
    fixture::with_database(2, |pool| async move {
        migrated(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Company','company') ON CONFLICT DO NOTHING;\
             INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Branch','company','company','branch') ON CONFLICT DO NOTHING;\
             INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('wh','company','branch','WH','Main') ON CONFLICT DO NOTHING;\
             INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('wh2','company','branch','WH2','Second') ON CONFLICT DO NOTHING;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,created_by,prepared_by,created_at,updated_at)VALUES('count-1','company','branch','wh','COUNT-1','2026-11-10','Cycle count','draft','maker','maker',0,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,status,total_value_minor,created_by,prepared_by,created_at,updated_at)VALUES('transfer-1','company','wh','wh2','branch','branch',1,1,'T-1','2026-11-10','Replenishment','draft',0,'maker','maker',0,0)")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = serializable(&pool).await;
        lock_supply_inventory_package_sources(&mut tx).await.unwrap();
        let pending =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 9)
                .await
                .unwrap();
        assert_eq!(pending.outstanding, 2);
        assert_eq!(pending.facts.len(), 2);
        assert_eq!(pending.facts[0].kind, "count");
        assert_eq!(pending.facts[0].id, "count-1");
        assert!(pending.facts[0].pending);
        assert_eq!(pending.facts[1].kind, "transfer");
        assert_eq!(pending.facts[1].id, "transfer-1");
        assert!(pending.facts[1].pending);
        tx.rollback().await.unwrap();
        sqlx::query("DELETE FROM inventory_counts WHERE id='count-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("DELETE FROM inventory_transfers WHERE id='transfer-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = serializable(&pool).await;
        lock_supply_inventory_package_sources(&mut tx).await.unwrap();
        let settled =
            read_supply_inventory_package_obligations(&mut tx, "company", "supply", 1, "inventory", 9)
                .await
                .unwrap();
        assert_eq!(settled.outstanding, 0);
        assert_ne!(pending.revision, settled.revision);
        tx.rollback().await.unwrap();
    })
    .await;
}
