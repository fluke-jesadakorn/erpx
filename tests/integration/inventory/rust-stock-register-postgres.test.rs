//! Actual Inventory stock-register reads over leased PostgreSQL. These cases
//! prove source scope, lot/count visibility and reconciliation only; they do
//! not prove IAM, Commercial purchase or an HTTP session.
#[path = "../../support/inventory/stock-register-facts.rs"]
mod facts;
#[path = "../../support/inventory/reservation-source-fence.rs"]
mod fence;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_register::{
    lock_stock_register_sources, read_stock_register,
};
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;

#[tokio::test]
async fn register_reads_company_scope_lots_counts_and_reconciles_tracked_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", None).await
                })
            })
            .await
            .unwrap();
        assert_eq!(result.currency, "THB");
        assert_eq!(result.stock.len(), 3);
        assert_eq!(result.lots.len(), 3);
        assert_eq!(result.counts.len(), 3);
        let main = result
            .stock
            .iter()
            .find(|row| row.warehouse_id == "warehouse" && row.product_id == "product")
            .unwrap();
        assert_eq!(main.quantity_milli, 3_000);
        assert_eq!(main.reserved_milli, 500);
        assert_eq!(main.quarantined_milli, 1_000);
        assert_eq!(main.available_milli, 2_000);
        assert_eq!(main.tracking_mode, "lot");
        assert_eq!(main.version, 3);
        let untracked = result
            .stock
            .iter()
            .find(|row| row.product_id == "untracked")
            .unwrap();
        assert_eq!(untracked.available_milli, 7_000);
        let held = result.lots.iter().find(|lot| lot.id == "lot-b").unwrap();
        assert_eq!(held.quality_hold_milli, 1_000);
        assert_eq!(held.quality_status.as_deref(), Some("held"));
        assert_eq!(held.quantity_milli, 1_000);
        assert_eq!(held.version, 1);
        let initial = result
            .counts
            .iter()
            .find(|count| count.id == "count-initial-command")
            .unwrap();
        assert!(initial.initial_physical_source);
        assert_eq!(initial.status, "posted");
        assert_eq!(initial.version, 4);
        let draft = result
            .counts
            .iter()
            .find(|count| count.id == "count-draft")
            .unwrap();
        assert!(!draft.initial_physical_source);
        assert_eq!(draft.version, 2);
        assert_eq!(result.totals.tracked_quantity_milli, 7_000);
        assert_eq!(result.totals.tracked_value_minor, 7_000);
        assert_eq!(result.totals.lot_quantity_milli, 7_000);
        assert_eq!(result.totals.lot_value_minor, 7_000);
        assert!(result.totals.consistent);
    })
    .await;
}

#[tokio::test]
async fn register_preserves_branch_and_company_isolation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let scoped = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", Some("branch-two")).await
                })
            })
            .await
            .unwrap();
        assert_eq!(scoped.stock.len(), 1);
        assert_eq!(scoped.stock[0].warehouse_id, "warehouse-two");
        assert!(
            scoped
                .lots
                .iter()
                .all(|lot| lot.warehouse_id == "warehouse-two")
        );
        assert_eq!(scoped.counts.len(), 1);
        assert_eq!(scoped.counts[0].id, "count-two");
        let foreign = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "another-company", None).await
                })
            })
            .await
            .unwrap();
        assert!(foreign.stock.is_empty());
        assert!(foreign.lots.is_empty());
        assert!(foreign.counts.is_empty());
        assert_eq!(foreign.currency, "THB");
    })
    .await;
}

#[tokio::test]
async fn register_includes_field_and_manufacturing_reservations() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at)\
             VALUES('field-ticket','company','branch','reserve-order','reserve-party','Field request','Issue','maker','2026-11-10','open',1,'maker',0,0);\
             INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at)\
             VALUES('field-agreement','company','branch','reserve-party','Field agreement','2026-01-01','2027-01-01','THB',0,'draft','maker',1,0,0);\
             INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,version,created_by,created_at,updated_at)\
             VALUES('field-order','company','branch','field-ticket',1,'field-agreement',1,'Issue','Site','maker','open',1,'maker',0,0);\
             INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)\
             VALUES('field-reservation','field-order','warehouse','untracked',250,0,0,9007199254740991,'maker','Field reserve',0);\
             INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-spec','company','Specification','bom','untracked',1000,10000,'approved','maker',1,0,0);\
             INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-scenario','company','branch','warehouse','mfg-spec',1,1000,'2026-11-30',0,'[]','calculated','maker',1,0,0);\
             INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-recommendation','mfg-scenario',1,1000,0,'2026-11-30','proposed','maker',1,0,0);\
             INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-order','company','branch','warehouse','mfg-recommendation','mfg-spec',1,1000,'reserved','maker',1,0,0);\
             INSERT INTO manufacturing_material_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,created_at)\
             VALUES('mfg-reservation','mfg-order','warehouse','untracked',250,0,0,9007199254740991,'maker',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let register = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    read_stock_register(c, "company", None).await
                })
            })
            .await
            .unwrap();
        let row = register
            .stock
            .iter()
            .find(|row| row.warehouse_id == "warehouse" && row.product_id == "untracked")
            .unwrap();
        assert_eq!(row.quantity_milli, 7_000);
        assert_eq!(row.reserved_milli, 500);
        assert_eq!(row.available_milli, 6_500);
    })
    .await;
}

#[tokio::test]
async fn register_fences_every_reservation_source_behind_the_view() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *reader)
            .await
            .unwrap();
        lock_stock_register_sources(&mut reader).await.unwrap();
        fence::assert_reservation_source_writes_wait(&pool).await;
        reader.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn register_requires_a_serializable_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let result = read_stock_register(&mut c, "company", None).await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))
        ));
    })
    .await;
}

#[tokio::test]
async fn register_rolls_back_with_the_enclosing_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_register_sources(c).await?;
                    let register = read_stock_register(c, "company", None).await?;
                    assert_eq!(register.stock.len(), 3);
                    sqlx::query("UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse' AND product_id='product'")
                        .execute(&mut *c)
                        .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Rollback after verifying the register read.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            result,
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(_))
        ));
        let quantity: i64 = sqlx::query_scalar(
            "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(quantity, 3_000);
    })
    .await;
}
