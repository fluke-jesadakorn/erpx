//! Actual Inventory goods-receipt reads and effects over leased PostgreSQL.
//! Commerce fulfillment rows are synthetic source facts; Quality is a test port
//! implementing the owner boundary. These cases do not prove IAM, purchase,
//! complete Commerce command composition or HTTP behavior.
#![allow(dead_code)]
#[path = "../../support/inventory/stock-receipt-facts.rs"]
mod facts;
#[path = "../../support/inventory/reservation-source-fence.rs"]
mod fence;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::stock_receipt::*;
use erpx_server_inventory::public::stock_register::read_stock_register;
use erpx_server_runtime::public::postgres::{
    PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

struct ActualCancellationStockSource {
    facts: erpx_server_inventory::public::quality_cancellation_stock::QualityCancellationStockFacts,
    prepared: erpx_server_quality::public::quality_http::PreparedQualityCancellation,
    after:
        std::sync::OnceLock<erpx_server_quality::public::quality_http::RetainedQualityCancellation>,
}
impl erpx_server_inventory::public::quality_cancellation_stock::PgQualityCancellationStockSource
    for ActualCancellationStockSource
{
    fn facts(
        &self,
    ) -> &erpx_server_inventory::public::quality_cancellation_stock::QualityCancellationStockFacts
    {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.prepared.require_current(c).await
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.after
            .get()
            .ok_or_else(|| {
                BusinessError::invalid(
                    "TEST_SOURCE_REQUIRED",
                    "Actual Quality afterimage required.",
                )
            })?
            .require_current(c)
            .await
    }
}

#[tokio::test]
async fn quality_cancellation_stock_writer_first_retries_after_real_contention() {
    use erpx_server_inventory::public::quality_cancellation_stock::{
        QualityCancellationStockAction, QualityCancellationStockFacts,
        lock_quality_cancellation_stock_sources, prepare_quality_cancellation_stock,
    };
    use erpx_server_quality::public::quality_http::{
        lock_quality_cancellation_writes, lock_quality_read_sources, prepare_quality_cancellation,
    };
    use erpx_server_quality::public::quality_read::{
        CancellationAction, QualityCancellationCommand,
    };
    use erpx_server_shared::public::{CommandKey, CompanyId, RecordId, UserId};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    fixture::with_database(3,|pool|async move {
        facts::setup(&pool,"none",true).await;
        sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at) VALUES('race-hold','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0)").execute(pool.raw()).await.unwrap();
        let baseline:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let owner_pool=pool.clone();let owner_attempts=attempts.clone();
        let (send,mut receive)=tokio::sync::mpsc::unbounded_channel();
        let owner=tokio::spawn(async move {
            PgUnitOfWork::new(owner_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|c| {
                let send=send.clone();let attempts=owner_attempts.clone();
                Box::pin(async move {
                    lock_quality_cancellation_writes(c).await?;lock_quality_read_sources(c).await?;lock_quality_cancellation_stock_sources(c).await?;
                    attempts.fetch_add(1,Ordering::SeqCst);
                    let command=QualityCancellationCommand {command_key:CommandKey::new("race-key").unwrap(),hold_id:RecordId::new("race-hold").unwrap(),expected_version:1,action:CancellationAction::Request,reason:"Review incorrect hold".into()};
                    let quality=prepare_quality_cancellation(c,&command,&CompanyId::new("company").unwrap(),Some(&RecordId::new("branch").unwrap()),&UserId::new("maker").unwrap(),10).await?;
                    let source=ActualCancellationStockSource {facts:QualityCancellationStockFacts {warehouse_id:"warehouse".into(),product_id:"product".into(),action:QualityCancellationStockAction::Request},prepared:quality,after:std::sync::OnceLock::new()};
                    let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await?;send.send(reader_pid).unwrap();
                    let stock=prepare_quality_cancellation_stock(c,&source).await?;
                    stock.require_before(c).await?;
                    let after=source.prepared.apply(c).await?;assert!(source.after.set(after).is_ok());
                    stock.retain(c).await?.require_current(c).await?;
                    Ok(())
                })
            }).await
        });
        let reader_pid=receive.recv().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("cancellation did not wait for stock writer");
        writer.commit().await.unwrap();owner.await.unwrap().unwrap();
        assert!(attempts.load(Ordering::SeqCst)>=2,"stale snapshot must retry");
        let after:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(after,baseline+2);
        let events:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='race-hold'").fetch_one(pool.raw()).await.unwrap();assert_eq!(events,1);
    }).await;
}

#[tokio::test]
async fn quality_cancellation_stock_owner_first_blocks_writer_and_rollback_restores_effect() {
    use erpx_server_inventory::public::quality_cancellation_stock::{
        QualityCancellationStockAction, QualityCancellationStockFacts,
        lock_quality_cancellation_stock_sources, prepare_quality_cancellation_stock,
    };
    use erpx_server_quality::public::quality_http::{
        lock_quality_cancellation_writes, lock_quality_read_sources, prepare_quality_cancellation,
    };
    use erpx_server_quality::public::quality_read::{
        CancellationAction, QualityCancellationCommand,
    };
    use erpx_server_shared::public::{CommandKey, CompanyId, RecordId, UserId};
    fixture::with_database(3,|pool|async move {
        facts::setup(&pool,"none",true).await;
        sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at) VALUES('race-hold','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0)").execute(pool.raw()).await.unwrap();
        let baseline:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        let mut owner=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *owner).await.unwrap();
        lock_quality_cancellation_writes(&mut owner).await.unwrap();lock_quality_read_sources(&mut owner).await.unwrap();lock_quality_cancellation_stock_sources(&mut owner).await.unwrap();
        let command=QualityCancellationCommand {command_key:CommandKey::new("race-key").unwrap(),hold_id:RecordId::new("race-hold").unwrap(),expected_version:1,action:CancellationAction::Request,reason:"Review incorrect hold".into()};
        let prepared=prepare_quality_cancellation(&mut owner,&command,&CompanyId::new("company").unwrap(),Some(&RecordId::new("branch").unwrap()),&UserId::new("maker").unwrap(),10).await.unwrap();
        let source=ActualCancellationStockSource {facts:QualityCancellationStockFacts {warehouse_id:"warehouse".into(),product_id:"product".into(),action:QualityCancellationStockAction::Request},prepared,after:std::sync::OnceLock::new()};
        let stock=prepare_quality_cancellation_stock(&mut owner,&source).await.unwrap();stock.require_before(&mut owner).await.unwrap();
        let effect=source.prepared.apply(&mut owner).await.unwrap();assert!(source.after.set(effect).is_ok());
        let retained=stock.retain(&mut owner).await.unwrap();retained.require_current(&mut owner).await.unwrap();
        let owner_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
        let writer_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {let mut tx=writer_pool.raw().begin().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();send.send(pid).unwrap();sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();});
        let writer_pid=receive.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(owner_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("stock writer did not wait for cancellation");
        retained.require_current(&mut owner).await.unwrap();owner.rollback().await.unwrap();writer.await.unwrap();
        let after:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();assert_eq!(after,baseline+1);
        let state:(String,i64)=sqlx::query_as("SELECT status,(SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='race-hold') FROM inventory_quality_holds WHERE id='race-hold'").fetch_one(pool.raw()).await.unwrap();assert_eq!(state,("held".into(),0));
    }).await;
}

#[tokio::test]
async fn quality_hold_requires_its_stock_row_in_migrated_schema() {
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"none",false).await;
        let (name,definition,validated):(String,String,bool)=sqlx::query_as("SELECT conname,pg_get_constraintdef(oid),convalidated FROM pg_constraint WHERE conrelid='inventory_quality_holds'::regclass AND confrelid='commerce_stock'::regclass AND contype='f'").fetch_one(pool.raw()).await.unwrap();
        assert!(validated);
        assert!(definition.contains("FOREIGN KEY (warehouse_id, product_id) REFERENCES commerce_stock(warehouse_id, product_id)"));
        let error=sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at) VALUES('no-stock','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0)").execute(pool.raw()).await.unwrap_err();
        assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("23503"));
        assert_eq!(error.as_database_error().and_then(|e|e.constraint()),Some(name.as_str()));
    }).await;
}

#[tokio::test]
async fn quality_cancellation_stock_afterimage_tracks_real_owner_triggers_and_rollback() {
    use erpx_server_inventory::public::quality_cancellation_stock::{
        PgQualityCancellationStockSource, QualityCancellationStockAction,
        QualityCancellationStockFacts, lock_quality_cancellation_stock_sources,
        prepare_quality_cancellation_stock,
    };
    use erpx_server_quality::public::quality_http::{
        PreparedQualityCancellation, RetainedQualityCancellation, lock_quality_cancellation_writes,
        lock_quality_read_sources, prepare_quality_cancellation,
    };
    use erpx_server_quality::public::quality_read::{
        CancellationAction, QualityCancellationCommand,
    };
    use erpx_server_shared::public::{CommandKey, CompanyId, RecordId, UserId};
    use std::sync::OnceLock;
    struct ActualSource {
        facts: QualityCancellationStockFacts,
        prepared: PreparedQualityCancellation,
        after: OnceLock<RetainedQualityCancellation>,
    }
    impl PgQualityCancellationStockSource for ActualSource {
        fn facts(&self) -> &QualityCancellationStockFacts {
            &self.facts
        }
        async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
            self.prepared.require_current(c).await
        }
        async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
            let after = self.after.get().ok_or_else(|| {
                BusinessError::invalid(
                    "TEST_SOURCE_REQUIRED",
                    "Actual Quality afterimage required.",
                )
            })?;
            after.require_current(c).await
        }
    }
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"none",true).await;
        sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope) VALUES('cancel-stock-hold','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0,'whole')").execute(pool.raw()).await.unwrap();
        let baseline:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_cancellation_writes(&mut tx).await.unwrap();
        lock_quality_read_sources(&mut tx).await.unwrap();
        lock_quality_cancellation_stock_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();let branch=RecordId::new("branch").unwrap();
        for (index,action,stock_action,actor) in [(1,CancellationAction::Request,QualityCancellationStockAction::Request,"maker"),(2,CancellationAction::Reject,QualityCancellationStockAction::Reject,"reviewer"),(3,CancellationAction::Request,QualityCancellationStockAction::Request,"maker"),(4,CancellationAction::Approve,QualityCancellationStockAction::Approve,"reviewer")] {
            let command=QualityCancellationCommand {command_key:CommandKey::new(format!("cancel-stock-{index}")).unwrap(),hold_id:RecordId::new("cancel-stock-hold").unwrap(),expected_version:index,action,reason:"Review incorrect hold".into()};
            let prepared=prepare_quality_cancellation(&mut tx,&command,&company,Some(&branch),&UserId::new(actor).unwrap(),10+index).await.unwrap();
            let source=ActualSource {facts:QualityCancellationStockFacts {warehouse_id:"warehouse".into(),product_id:"product".into(),action:stock_action},prepared,after:OnceLock::new()};
            let stock=prepare_quality_cancellation_stock(&mut tx,&source).await.unwrap();
            stock.require_before(&mut tx).await.unwrap();
            if index==1 {
                sqlx::query("SAVEPOINT stock_drift").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *tx).await.unwrap();
                assert!(stock.require_before(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT stock_drift").execute(&mut *tx).await.unwrap();
                stock.require_before(&mut tx).await.unwrap();
            }
            let after=source.prepared.apply(&mut tx).await.unwrap();
            assert!(source.after.set(after).is_ok());
            let retained=stock.retain(&mut tx).await.unwrap();
            retained.require_current(&mut tx).await.unwrap();
            if index==1 {
                let mut other=pool.raw().begin().await.unwrap();
                sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
                assert!(retained.require_current(&mut other).await.is_err());
                other.rollback().await.unwrap();
            }
            let image:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *tx).await.unwrap();
            assert_eq!(image,(baseline.0,baseline.1,baseline.2+index));
        }
        tx.rollback().await.unwrap();
        let after:(i64,i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor,version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(after,baseline);
        let status:String=sqlx::query_scalar("SELECT status FROM inventory_quality_holds WHERE id='cancel-stock-hold'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(status,"held");
        let events:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='cancel-stock-hold'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(events,0);
    }).await;
}

#[tokio::test]
async fn quality_stock_witness_reads_version_and_denies_missing_rows() {
    use erpx_server_inventory::public::quality_stock_source::{
        inspect_quality_stock, lock_quality_stock_sources,
    };
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_stock_sources(&mut tx).await.unwrap();
        let stock=inspect_quality_stock(&mut tx,"warehouse","product").await.unwrap().unwrap();
        assert_eq!((&*stock.facts().warehouse_id,&*stock.facts().product_id,stock.facts().quantity_milli,stock.facts().value_minor,stock.facts().version),("warehouse","product",1000,1000,1));
        stock.require_current(&mut tx).await.unwrap();
        assert!(inspect_quality_stock(&mut tx,"wrong","product").await.unwrap().is_none());
        assert!(inspect_quality_stock(&mut tx,"warehouse","wrong").await.unwrap().is_none());
        sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *tx).await.unwrap();
        assert!(stock.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(stock.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_stock_source_waits_for_writer_before_snapshot() {
    use erpx_server_inventory::public::quality_stock_source::{
        inspect_quality_stock, lock_quality_stock_sources,
    };
    fixture::with_database(3,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        let mut writer=pool.raw().begin().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut tx=reader_pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            lock_quality_stock_sources(&mut tx).await.unwrap();
            let source=inspect_quality_stock(&mut tx,"warehouse","product").await.unwrap().unwrap();
            assert_eq!(source.facts().version,2);
            source.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("stock source lock did not wait for writer");
        writer.commit().await.unwrap();reader.await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_receipt_lineage_intersects_actual_inventory_and_commerce_sources() {
    use erpx_server_commerce::public::quality_receipt_origin::{
        inspect_quality_receipt_commerce, lock_quality_receipt_commerce,
    };
    use erpx_server_inventory::public::quality_disposition_count::{
        CurrentQualityDispositionCount, QualityDispositionCountRequest,
        inspect_quality_disposition_count, lock_quality_disposition_count_sources,
    };
    use erpx_server_inventory::public::quality_fulfillment_allocation::{
        QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
        lock_quality_fulfillment_allocations,
    };
    use erpx_server_quality::public::purchase_lineage::{
        CountLineageRequest, QualityLineageCountPort, QualityLineageRequest,
        inspect_quality_purchase_lineage, lock_quality_purchase_lineage,
    };
    use erpx_server_shared::public::{CompanyId, RecordId};
    struct InventoryCounts;
    impl QualityLineageCountPort for InventoryCounts {
        type Witness = CurrentQualityDispositionCount;
        fn inspect<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            r: CountLineageRequest,
        ) -> PgWorkFuture<'a, Option<Self::Witness>, BusinessError> {
            Box::pin(async move {
                inspect_quality_disposition_count(
                    c,
                    QualityDispositionCountRequest {
                        company_id: r.company_id,
                        branch_id: r.branch_id,
                        warehouse_id: r.warehouse_id,
                        count_id: r.count_id,
                        hold_id: r.hold_id,
                    },
                )
                .await
            })
        }
        fn require_current<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            w: &'a Self::Witness,
        ) -> PgWorkFuture<'a, (), BusinessError> {
            Box::pin(async move { w.require_current(c).await })
        }
    }
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id,lot_id) VALUES('receipt-root','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0,'partial',1000,'goods_receipt','receipt-allocation','lot-existing'); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) VALUES('receipt-allocation','delivery-key-0','lot-existing',1000,2500,'quarantine','receipt-root'); INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('receipt-count','company','branch','warehouse','RCNT','2026-11-10','Scrap','posted','receipt-root','maker','maker',0,0); UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id='receipt-count',disposition_quantity_milli=400,disposed_by='approver',disposed_at=1 WHERE id='receipt-root'; INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('receipt-event','receipt-root','receipt-count','post','approver',1,'Scrap',1); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id,lot_id) VALUES('receipt-root-remaining-receipt-count','company','branch','warehouse','product','Remaining','2026-11-10','held','maker',1,1,'partial',600,'goods_receipt','receipt-allocation:remaining:receipt-count','lot-existing')").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_receipt_commerce(&mut tx).await.unwrap();
        lock_quality_fulfillment_allocations(&mut tx).await.unwrap();
        lock_quality_purchase_lineage(&mut tx).await.unwrap();
        lock_quality_disposition_count_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let branch=RecordId::new("branch").unwrap();
        let mut counts=InventoryCounts;
        let lineage=inspect_quality_purchase_lineage(&mut tx,&mut counts,QualityLineageRequest {company_id:company.clone(),branch_id:Some("branch".into()),hold_id:"receipt-root-remaining-receipt-count".into()}).await.unwrap();
        let root=lineage.root();
        assert_eq!(root["id"],"receipt-root");
        assert_eq!(root["source_type"],"goods_receipt");
        assert!(root["spec_id"].is_null());
        let allocations=inspect_quality_fulfillment_allocations(&mut tx,QualityFulfillmentAllocationRequest::QuarantineHold {hold_id:root["id"].as_str().unwrap().into()}).await.unwrap();
        assert_eq!(allocations.rows().len(),1);
        let allocation=&allocations.rows()[0];
        let commerce=inspect_quality_receipt_commerce(&mut tx,&company,Some(&branch),&allocation.fulfillment_line_id).await.unwrap();
        let origin=commerce.facts().unwrap();
        assert_eq!(origin.order_id,"order-1");
        assert_eq!(origin.fulfillment_line_id,allocation.fulfillment_line_id);
        assert_eq!(root["company_id"],origin.company_id);
        assert_eq!(root["branch_id"],origin.branch_id);
        assert_eq!(root["warehouse_id"],origin.warehouse_id);
        assert_eq!(root["product_id"],origin.product_id);
        assert_eq!(root["lot_id"],allocation.lot_id);
        assert_eq!(root["held_quantity_milli"],allocation.quantity_milli);
        assert_eq!(allocation.quality_hold_id.as_deref(),root["id"].as_str());
        assert_eq!(allocation.disposition,"quarantine");
        assert_eq!((allocation.id.as_str(),allocation.value_minor),("receipt-allocation",2500));
        lineage.require_current(&mut tx,&mut counts).await.unwrap();
        allocations.require_current(&mut tx).await.unwrap();
        commerce.require_current(&mut tx).await.unwrap();
        let foreign=CompanyId::new("foreign").unwrap();
        assert!(inspect_quality_receipt_commerce(&mut tx,&foreign,None,&allocation.fulfillment_line_id).await.unwrap().facts().is_none());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_receipt_preparation_waits_for_commerce_writer_before_snapshot() {
    use erpx_server_commerce::public::quality_receipt_origin::{
        inspect_quality_receipt_commerce, lock_quality_receipt_commerce,
    };
    use erpx_server_inventory::public::quality_disposition_count::lock_quality_disposition_count_sources;
    use erpx_server_inventory::public::quality_fulfillment_allocation::lock_quality_fulfillment_allocations;
    use erpx_server_quality::public::purchase_lineage::lock_quality_purchase_lineage;
    use erpx_server_shared::public::{CompanyId, RecordId};
    fixture::with_database(3,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        let mut writer=pool.raw().begin().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET kind='sales' WHERE id='order-1'").execute(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut tx=reader_pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            lock_quality_receipt_commerce(&mut tx).await.unwrap();
            lock_quality_fulfillment_allocations(&mut tx).await.unwrap();
            lock_quality_purchase_lineage(&mut tx).await.unwrap();
            lock_quality_disposition_count_sources(&mut tx).await.unwrap();
            let company=CompanyId::new("company").unwrap();
            let branch=RecordId::new("branch").unwrap();
            let source=inspect_quality_receipt_commerce(&mut tx,&company,Some(&branch),"delivery-key-0").await.unwrap();
            assert!(source.facts().is_none());
            source.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
                if blocked { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("source preparation did not wait for Commerce writer");
        writer.commit().await.unwrap();
        reader.await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_return_origins_join_workflow_and_inventory_witnesses() {
    use erpx_server_commerce::public::quality_return_origin::{
        QualityReturnSelector, inspect_quality_return_commerce, lock_quality_return_commerce,
    };
    use erpx_server_inventory::public::quality_disposition_count::{
        CurrentQualityDispositionCount, QualityDispositionCountRequest,
        inspect_quality_disposition_count, lock_quality_disposition_count_sources,
    };
    use erpx_server_inventory::public::quality_fulfillment_allocation::{
        QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
        lock_quality_fulfillment_allocations,
    };
    use erpx_server_quality::public::purchase_lineage::{
        CountLineageRequest, QualityLineageCountPort, QualityLineageRequest,
        inspect_quality_purchase_lineage, lock_quality_purchase_lineage,
    };
    use erpx_server_shared::public::{CompanyId, RecordId};
    use erpx_server_workflow::public::record_origin::{
        RawRecordOriginRequest, lock_record_origin_sources, read_raw_record_origin,
        verify_raw_record_origin,
    };
    struct InventoryCounts;
    impl QualityLineageCountPort for InventoryCounts {
        type Witness = CurrentQualityDispositionCount;
        fn inspect<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            r: CountLineageRequest,
        ) -> PgWorkFuture<'a, Option<Self::Witness>, BusinessError> {
            Box::pin(async move {
                inspect_quality_disposition_count(
                    c,
                    QualityDispositionCountRequest {
                        company_id: r.company_id,
                        branch_id: r.branch_id,
                        warehouse_id: r.warehouse_id,
                        count_id: r.count_id,
                        hold_id: r.hold_id,
                    },
                )
                .await
            })
        }
        fn require_current<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            w: &'a Self::Witness,
        ) -> PgWorkFuture<'a, (), BusinessError> {
            Box::pin(async move { w.require_current(c).await })
        }
    }
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('return-customer','company','customer','C1','Customer','THB',0,0); INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('return-debit','chart','company','100','Debit','asset',0,0),('return-credit','chart','company','400','Credit','revenue',0,0); INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,created_by,created_at,updated_at) VALUES('return-invoice','company','return-customer','receivable','INV-RETURN','2026-11-01','2026-11-02','THB',2000,0,2000,2000,'return-debit','return-credit','maker',0,0); INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('plain-product','company','P2','Plain',1000,'none'); INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES('warehouse','plain-product',1000,1000,1); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) VALUES('return-order','company','branch','sales','SO-RET','return-customer','warehouse','THB','2026-11-01','2026-11-10',2000,0,'confirmed',3,'maker',0,0); INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES('plain-order-line','return-order','plain-product','Plain',1000,1000,1000,0,1000,0,0,'stock'),('lot-order-line','return-order','product','Lot',1000,1000,1000,0,1000,0,1,'stock'); INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version,invoice_id) VALUES('return-fulfillment','return-order','FUL-RET','2026-11-01','maker',0,1,'return-invoice'); INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES('plain-fulfilled','return-fulfillment','plain-order-line',1000,1000,1000,0),('lot-fulfilled','return-fulfillment','lot-order-line',1000,1000,1000,0); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition) VALUES('sold-allocation','lot-fulfilled','lot-existing',1000,1000,'available'); INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,tax_minor,total_minor,version,requested_by,approved_by,created_at,updated_at) VALUES('return-1','company','branch','return-order','return-fulfillment','return-invoice','customer_return','RET-1','2026-11-01','Return','approved',2000,0,2000,2,'maker','reviewer',0,0); INSERT INTO commerce_return_lines(id,return_id,fulfillment_line_id,product_id,quantity_milli,value_minor,subtotal_minor,tax_minor,ordinal) VALUES('plain-return-line','return-1','plain-fulfilled','plain-product',1000,1000,1000,0,0),('lot-return-line','return-1','lot-fulfilled','product',1000,1000,1000,0,1); INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('return-allocation','lot-return-line','sold-allocation','lot-existing',1000,1000,1000,1000,1); INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('approval-key','commerce.return.approve','reviewer',repeat('a',64),'return-1','{}',1,'{\"companyId\":\"company\"}'); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id,lot_id) VALUES('quality-return-approval-key-0','company','branch','warehouse','plain-product','Returned','2026-11-10','held','reviewer',1,1,'partial',1000,'customer_return','plain-return-line',NULL),('quality-return-approval-key-1-0','company','branch','warehouse','product','Returned','2026-11-10','held','reviewer',1,1,'partial',1000,'customer_return','return-allocation','lot-existing')").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_return_commerce(&mut tx).await.unwrap();
        lock_quality_fulfillment_allocations(&mut tx).await.unwrap();
        lock_quality_purchase_lineage(&mut tx).await.unwrap();
        lock_quality_disposition_count_sources(&mut tx).await.unwrap();
        lock_record_origin_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let branch=RecordId::new("branch").unwrap();
        let mut counts=InventoryCounts;
        for (hold_id,selector) in [("quality-return-approval-key-0",QualityReturnSelector::UntrackedLine("plain-return-line".into())),("quality-return-approval-key-1-0",QualityReturnSelector::TrackedAllocation("return-allocation".into()))] {
            let lineage=inspect_quality_purchase_lineage(&mut tx,&mut counts,QualityLineageRequest {company_id:company.clone(),branch_id:Some("branch".into()),hold_id:hold_id.into()}).await.unwrap();
            let hold=lineage.root();
            let commerce=inspect_quality_return_commerce(&mut tx,&company,Some(&branch),selector).await.unwrap();
            let origin=commerce.facts().unwrap();
            let suffix=match origin.lot_ordinal {Some(lot)=>format!("-{}-{lot}",origin.line_ordinal),None=>format!("-{}",origin.line_ordinal)};
            let key=hold["id"].as_str().unwrap().strip_prefix("quality-return-").unwrap().strip_suffix(&suffix).unwrap();
            assert_eq!(key,"approval-key");
            assert_eq!(hold["source_type"],"customer_return");
            assert!(hold["spec_id"].is_null());
            assert_eq!(hold["company_id"],origin.company_id);
            assert_eq!(hold["branch_id"],origin.branch_id);
            assert_eq!(hold["warehouse_id"],origin.warehouse_id);
            assert_eq!(hold["product_id"],origin.product_id);
            assert_eq!(hold["source_id"],origin.source_id);
            assert_eq!(hold["created_by"],origin.approver);
            assert_eq!(hold["held_quantity_milli"],origin.quantity_milli);
            assert_eq!(hold["lot_id"].as_str(),origin.lot_id.as_deref());
            assert_eq!(origin.value_minor,1000);
            let request=RawRecordOriginRequest {command_key:key.into(),operation:"commerce.return.approve".into(),record_id:origin.return_id.clone(),creator_user_id:origin.approver.clone(),company_id:company.clone()};
            let receipt=read_raw_record_origin(&mut tx,request.clone()).await.unwrap().unwrap();
            assert!(read_raw_record_origin(&mut tx,RawRecordOriginRequest {creator_user_id:"other".into(),..request.clone()}).await.unwrap().is_none());
            assert!(read_raw_record_origin(&mut tx,RawRecordOriginRequest {command_key:"wrong-key".into(),..request.clone()}).await.unwrap().is_none());
            assert!(read_raw_record_origin(&mut tx,RawRecordOriginRequest {operation:"commerce.return.reject".into(),..request.clone()}).await.unwrap().is_none());
            assert!(read_raw_record_origin(&mut tx,RawRecordOriginRequest {company_id:CompanyId::new("foreign").unwrap(),..request.clone()}).await.unwrap().is_none());
            if let (Some(allocation_id),Some(lot_id))=(&origin.fulfillment_allocation_id,&origin.lot_id) {
                let allocation=inspect_quality_fulfillment_allocations(&mut tx,QualityFulfillmentAllocationRequest::OriginalAllocation {allocation_id:allocation_id.clone(),fulfillment_line_id:origin.fulfillment_line_id.clone(),lot_id:lot_id.clone()}).await.unwrap();
                assert_eq!(allocation.rows().len(),1);
                assert_eq!(allocation.rows()[0].id,"sold-allocation");
                allocation.require_current(&mut tx).await.unwrap();
                let wrong_lot=inspect_quality_fulfillment_allocations(&mut tx,QualityFulfillmentAllocationRequest::OriginalAllocation {allocation_id:allocation_id.clone(),fulfillment_line_id:origin.fulfillment_line_id.clone(),lot_id:"wrong-lot".into()}).await.unwrap();
                assert!(wrong_lot.rows().is_empty());
            } else {
                assert_eq!(origin.lot_ordinal,None);
                assert_eq!(origin.fulfillment_allocation_id,None);
            }
            lineage.require_current(&mut tx,&mut counts).await.unwrap();
            commerce.require_current(&mut tx).await.unwrap();
            verify_raw_record_origin(&mut tx,&receipt).await.unwrap();
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_allocation_witness_retains_receipt_and_return_identity() {
    use erpx_server_inventory::public::quality_fulfillment_allocation::{
        QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
        lock_quality_fulfillment_allocations,
    };
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,quantity_scope) VALUES('hold','company','branch','warehouse','product','Review','2026-11-10','maker',0,0,'lot-existing',1000,'goods_receipt','allocation','partial'); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) VALUES('allocation','delivery-key-0','lot-existing',1000,2500,'quarantine','hold')").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_fulfillment_allocations(&mut tx).await.unwrap();
        let hold=|| QualityFulfillmentAllocationRequest::QuarantineHold{hold_id:"hold".into()};
        let witness=inspect_quality_fulfillment_allocations(&mut tx,hold()).await.unwrap();
        let rows=witness.rows();
        assert_eq!(rows.len(),1);
        assert_eq!((&*rows[0].id,&*rows[0].fulfillment_line_id,&*rows[0].lot_id,rows[0].quantity_milli,rows[0].value_minor,&*rows[0].disposition,rows[0].quality_hold_id.as_deref()),("allocation","delivery-key-0","lot-existing",1000,2500,"quarantine",Some("hold")));
        witness.require_current(&mut tx).await.unwrap();
        let original=|line:&str,lot:&str| QualityFulfillmentAllocationRequest::OriginalAllocation{allocation_id:"allocation".into(),fulfillment_line_id:line.into(),lot_id:lot.into()};
        assert_eq!(inspect_quality_fulfillment_allocations(&mut tx,original("delivery-key-0","lot-existing")).await.unwrap().rows().len(),1);
        assert!(inspect_quality_fulfillment_allocations(&mut tx,original("wrong-line","lot-existing")).await.unwrap().rows().is_empty());
        assert!(inspect_quality_fulfillment_allocations(&mut tx,original("delivery-key-0","wrong-lot")).await.unwrap().rows().is_empty());
        assert!(inspect_quality_fulfillment_allocations(&mut tx,QualityFulfillmentAllocationRequest::QuarantineHold{hold_id:"other".into()}).await.unwrap().rows().is_empty());
        sqlx::query("SAVEPOINT immutable_check").execute(&mut *tx).await.unwrap();
        let immutable=sqlx::query("UPDATE inventory_fulfillment_lot_allocations SET value_minor=2499 WHERE id='allocation'").execute(&mut *tx).await.unwrap_err();
        assert_eq!(immutable.as_database_error().and_then(|e| e.code()).as_deref(),Some("23514"));
        sqlx::query("ROLLBACK TO SAVEPOINT immutable_check").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('lot-second','company','product','lot','L-B','maker',0); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) VALUES('allocation-second','delivery-key-0','lot-second',1000,2500,'quarantine','hold')").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_allocation_absence_is_guarded_against_concurrent_insert() {
    use erpx_server_inventory::public::quality_fulfillment_allocation::{
        QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
        lock_quality_fulfillment_allocations,
    };
    fixture::with_database(3,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,quantity_scope) VALUES('hold','company','branch','warehouse','product','Review','2026-11-10','maker',0,0,'lot-existing',1000,'goods_receipt','allocation','partial')").execute(pool.raw()).await.unwrap();
        let mut guard=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *guard).await.unwrap();
        lock_quality_fulfillment_allocations(&mut guard).await.unwrap();
        let absent=inspect_quality_fulfillment_allocations(&mut guard,QualityFulfillmentAllocationRequest::QuarantineHold{hold_id:"hold".into()}).await.unwrap();
        assert!(absent.rows().is_empty());
        let guard_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *guard).await.unwrap();
        let writer_pool=pool.clone();
        let writer=tokio::spawn(async move {
            sqlx::query("INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) VALUES('later','delivery-key-0','lot-existing',1000,2500,'quarantine','hold')").execute(writer_pool.raw()).await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(guard_pid).fetch_one(pool.raw()).await.unwrap();
                if blocked { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("allocation insert did not wait for source lock");
        absent.require_current(&mut guard).await.unwrap();
        guard.commit().await.unwrap();
        writer.await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_fulfillment_lot_allocations WHERE id='later'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count,1);
    }).await;
}

#[tokio::test]
async fn quality_allocation_witness_retains_more_than_one_thousand_matches() {
    use erpx_server_inventory::public::quality_fulfillment_allocation::{
        QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
        lock_quality_fulfillment_allocations,
    };
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"lot",true).await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,quantity_scope) VALUES('many-hold','company','branch','warehouse','product','Review','2026-11-10','maker',0,0,'partial'); INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) SELECT 'bulk-lot-'||n,'company','product','lot','B-'||n,'maker',0 FROM generate_series(1,1001) n; INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) SELECT 'bulk-allocation-'||n,'delivery-key-0','bulk-lot-'||n,1,0,'quarantine','many-hold' FROM generate_series(1,1001) n").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_fulfillment_allocations(&mut tx).await.unwrap();
        let witness=inspect_quality_fulfillment_allocations(&mut tx,QualityFulfillmentAllocationRequest::QuarantineHold {hold_id:"many-hold".into()}).await.unwrap();
        assert_eq!(witness.rows().len(),1001);
        assert!(witness.rows().iter().any(|row|row.id=="bulk-allocation-1001"));
        witness.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

struct TestQuality;

impl StockReceiptQualityPort for TestQuality {
    fn create_quality_hold<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: StockReceiptQualityRequest,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        Box::pin(async move {
            let id = format!("quality-{}", request.allocation_id);
            sqlx::query(
                "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                 product_id,reason,due_date,status,created_by,created_at,updated_at,lot_id,\
                 held_quantity_milli,source_type,source_id,quantity_scope) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'held','inspector',0,0,$8,$9,'goods_receipt',$10,'partial')",
            )
            .bind(&id)
            .bind(&request.company_id)
            .bind(&request.branch_id)
            .bind(&request.warehouse_id)
            .bind(&request.product_id)
            .bind(&request.reason)
            .bind(&request.due_date)
            .bind(&request.lot_id)
            .bind(request.quantity_milli)
            .bind(&request.allocation_id)
            .execute(&mut *c)
            .await?;
            Ok(id)
        })
    }
}

fn allocation(
    lot: Option<&str>,
    serial: Option<&str>,
    quantity: i64,
    disposition: ReceiptDisposition,
) -> StockReceiptAllocationInput {
    StockReceiptAllocationInput {
        lot_code: lot.map(Into::into),
        serial_number: serial.map(Into::into),
        quantity_milli: quantity,
        disposition,
        quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "Packaging inspection".into()),
        quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "2026-11-10".into()),
        manufactured_date: None,
        expiry_date: None,
    }
}

fn input(
    quantity: i64,
    value: i64,
    allocations: Vec<StockReceiptAllocationInput>,
) -> StockReceiptInput {
    StockReceiptInput {
        expected_order_version: 3,
        date: "2026-11-04".into(),
        lines: vec![StockReceiptLineInput {
            line_id: "line-1".into(),
            quantity_milli: quantity,
            stock_value_minor: value,
            allocations,
        }],
    }
}

fn denied<T: std::fmt::Debug>(result: Result<T, Error>, expected: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, expected),
        other => panic!("expected {expected}, got {other:?}"),
    }
}

#[tokio::test]
async fn receipt_reads_current_purchase_source_and_persists_lots_and_quality() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source = read_stock_receipt_source(
                        c,
                        "company",
                        Some("branch"),
                        None,
                        "order-1",
                    )
                    .await?;
                    assert!(source.can_receive());
                    assert_eq!(source.order.number, "PO-1");
                    assert_eq!(source.lines.len(), 1);
                    assert_eq!(source.lines[0].remaining_milli, 5_000);
                    assert!(source.lines[0].stock.is_none());
                    assert!(source.lines[0].lots.is_empty());
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            2_000,
                            5_000,
                            vec![
                                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                            ],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(!applied.replayed);
                    assert_eq!(applied.lines.len(), 1);
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    assert_eq!(applied.lines[0].stock_version, 1);
                    assert!(applied.lines[0].allocations[0].quality_hold_id.is_none());
                    assert!(applied.lines[0].allocations[1].quality_hold_id.is_some());
                    let replay = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(replay.replayed);
                    assert_eq!(replay.lines[0].stock_version, 2);
                    let stock: (i64, i64) = sqlx::query_as(
                        "SELECT quantity_milli,value_minor FROM commerce_stock \
                         WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(stock, (2_000, 5_000));
                    let lots: Vec<(String, i64)> = sqlx::query_as(
                        "SELECT l.lot_code,b.quantity_milli FROM inventory_lots l \
                         JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' ORDER BY l.lot_code",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        lots,
                        vec![("L-A".into(), 1_000), ("L-B".into(), 1_000)]
                    );
                    let evidence: (i64, i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT count(*) FROM inventory_lot_moves WHERE source_type='goods_receipt'),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
                         (SELECT count(*) FROM inventory_quality_holds WHERE source_type='goods_receipt'),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-key-0'),\
                         (SELECT count(*) FROM commerce_stock_moves)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2, 2, 1, 1, 1));
                    let register = read_stock_register(c, "company", None).await?;
                    assert_eq!(register.currency, "THB");
                    let row = register
                        .stock
                        .iter()
                        .find(|row| {
                            row.warehouse_id == "warehouse" && row.product_id == "product"
                        })
                        .unwrap();
                    assert_eq!(row.quantity_milli, 2_000);
                    assert_eq!(row.value_minor, 5_000);
                    assert_eq!(row.quarantined_milli, 1_000);
                    assert_eq!(row.available_milli, 1_000);
                    assert_eq!(register.lots.len(), 2);
                    assert_eq!(register.totals.tracked_quantity_milli, 2_000);
                    assert_eq!(register.totals.tracked_value_minor, 5_000);
                    assert_eq!(register.totals.lot_quantity_milli, 2_000);
                    assert_eq!(register.totals.lot_value_minor, 5_000);
                    assert!(register.totals.consistent);
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_refuses_stale_stock_and_a_quarantined_available_allocation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    assert_eq!(source.lines[0].stock.as_ref().unwrap().version, 1);
                    assert_eq!(source.lines[0].lots.len(), 1);
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            1_000,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 1_000).await;
                    sqlx::query("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'")
                        .execute(&mut *c)
                        .await?;
                    let mut quality = TestQuality;
                    let stale = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await;
                    denied(stale, "STOCK_RECEIPT_SOURCE_CHANGED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rechecks_a_quality_hold_created_after_preparation() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let mut plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            1_000,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    sqlx::query(
                        "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                         product_id,reason,due_date,status,created_by,created_at,updated_at,lot_id,\
                         held_quantity_milli,source_type,source_id,quantity_scope) \
                         VALUES('existing-hold','company','branch','warehouse','product','Hold',\
                         '2026-11-10','held','inspector',0,0,'lot-existing',1000,'manual','hold-1','partial')",
                    )
                    .execute(&mut *c)
                    .await?;
                    let version: i64 = sqlx::query_scalar(
                        "SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    plan.lines[0].expected_stock_version = Some(version);
                    facts::align_fulfillment(c, 1_000, 1_000).await;
                    let mut quality = TestQuality;
                    let refused = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await;
                    refused.map(|_| ())
                })
            })
            .await;
        denied(result, "LOT_UNAVAILABLE");
        let evidence: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
             (SELECT count(*) FROM inventory_quality_holds WHERE id='existing-hold')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1_000, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn receipt_persists_serials_and_rejects_a_duplicate_serial() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "serial", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            2_000,
                            5_000,
                            vec![
                                allocation(None, Some("SN-1"), 1_000, ReceiptDisposition::Available),
                                allocation(None, Some("SN-2"), 1_000, ReceiptDisposition::Available),
                            ],
                        ),
                    )?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert_eq!(applied.lines[0].allocations.len(), 2);
                    let serials: Vec<(String, i64)> = sqlx::query_as(
                        "SELECT l.serial_number,b.quantity_milli FROM inventory_lots l \
                         JOIN inventory_lot_balances b ON b.lot_id=l.id \
                         WHERE l.company_id='company' AND l.product_id='product' ORDER BY l.serial_number",
                    )
                    .fetch_all(&mut *c)
                    .await?;
                    assert_eq!(
                        serials,
                        vec![("SN-1".into(), 1_000), ("SN-2".into(), 1_000)]
                    );
                    let refreshed =
                        read_stock_receipt_source(c, "company", None, None, "order-1")
                            .await?;
                    let duplicate = prepare_stock_receipt(
                        &refreshed,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                None,
                                Some("SN-1"),
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    );
                    match duplicate {
                        Err(error) => assert_eq!(error.code, "SERIAL_ALREADY_RECEIVED"),
                        other => panic!("expected SERIAL_ALREADY_RECEIVED, got {other:?}"),
                    }
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_persists_untracked_stock_without_lot_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan =
                        prepare_stock_receipt(&source, &input(2_000, 5_000, vec![]))?;
                    facts::align_fulfillment(c, 2_000, 5_000).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert_eq!(applied.lines[0].allocations.len(), 0);
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT \
                         (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM inventory_lots WHERE product_id='product'),\
                         (SELECT count(*) FROM inventory_lot_moves),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-key-0')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (2_000, 0, 0, 1));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rolls_back_with_the_enclosing_commerce_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Quarantine,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Roll back after verifying Inventory effects.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(_))
        ));
        let evidence: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
             (SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM inventory_lots WHERE product_id='product'),\
             (SELECT count(*) FROM inventory_lot_balances WHERE warehouse_id='warehouse'),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT count(*) FROM inventory_fulfillment_lot_allocations),\
             (SELECT count(*) FROM commerce_stock_moves)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (0, 0, 0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn receipt_source_excludes_service_lines_and_rejects_tampered_scope() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", Some("branch"), None, "order-1")
                            .await?;
                    assert!(source.can_receive());
                    assert_eq!(source.lines.len(), 1);
                    assert_eq!(source.lines[0].id, "line-1");
                    assert!(source.lines.iter().all(|line| line.id != "line-service"));

                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;

                    let mut branch = plan.clone();
                    branch.branch_id = "branch-two".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &branch,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_SCOPE_CHANGED",
                    );

                    let mut warehouse = plan.clone();
                    warehouse.warehouse_id = "warehouse-two".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &warehouse,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_SCOPE_CHANGED",
                    );

                    let mut company = plan;
                    company.company_id = "other-company".into();
                    denied(
                        apply_stock_receipt(
                            c,
                            &company,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "ORDER_NOT_FOUND",
                    );

                    let evidence: (i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT count(*) FROM commerce_stock),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM inventory_lot_moves)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (0, 0, 0));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rejects_over_receipt_against_the_current_order_line() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;

                    sqlx::query(
                        "UPDATE commerce_order_lines SET fulfilled_milli=4000 WHERE id='line-1'",
                    )
                    .execute(&mut *c)
                    .await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let mut over = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    over.lines[0].quantity_milli = 2_000;
                    over.lines[0].stock_value_minor = 5_000;
                    over.lines[0].allocations[0].quantity_milli = 2_000;
                    over.lines[0].allocations[0].value_minor = 5_000;
                    over.total_value_minor = 5_000;
                    sqlx::raw_sql(
                        "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version)VALUES\
                         ('delivery-over','order-1','GR-OVER','2026-11-04','maker',0,1);\
                         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor)VALUES\
                         ('delivery-over-0','delivery-over','line-1',2000,5000,5000,0);",
                    )
                    .execute(&mut *c)
                    .await?;
                    denied(
                        apply_stock_receipt(
                            c,
                            &over,
                            "delivery-over",
                            "command-key-over",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "OVER_FULFILLMENT",
                    );
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT fulfilled_milli FROM commerce_order_lines WHERE id='line-1'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM commerce_stock_moves WHERE id='delivery-over-0')",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (1_000, 4_000, 1, 0));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_rejects_a_changed_replay_without_duplicating_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    let applied = apply_stock_receipt(
                        c,
                        &plan,
                        "delivery-key",
                        "command-key",
                        "receiver",
                        100,
                        &mut quality,
                    )
                    .await?;
                    assert!(!applied.replayed);

                    let mut changed = plan.clone();
                    changed.lines[0].stock_value_minor = 3_000;
                    changed.lines[0].allocations[0].value_minor = 3_000;
                    changed.total_value_minor = 3_000;
                    denied(
                        apply_stock_receipt(
                            c,
                            &changed,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "STOCK_RECEIPT_REPLAY_MISMATCH",
                    );
                    let evidence: (i64, i64, i64, i64) = sqlx::query_as(
                        "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
                         (SELECT count(*) FROM commerce_stock_moves),\
                         (SELECT count(*) FROM inventory_fulfillment_lot_allocations)",
                    )
                    .fetch_one(&mut *c)
                    .await?;
                    assert_eq!(evidence, (1_000, 2_500, 1, 1));
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_refuses_an_inactive_existing_lot_and_rolls_back_stock() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", true).await;
        sqlx::query("UPDATE inventory_lots SET status='scrapped' WHERE id='lot-existing'")
            .execute(pool.raw())
            .await
            .unwrap();
        let result: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", None, None, "order-1").await?;
                    let plan = prepare_stock_receipt(
                        &source,
                        &input(
                            1_000,
                            2_500,
                            vec![allocation(
                                Some("L-A"),
                                None,
                                1_000,
                                ReceiptDisposition::Available,
                            )],
                        ),
                    )?;
                    facts::align_fulfillment(c, 1_000, 2_500).await;
                    let mut quality = TestQuality;
                    denied(
                        apply_stock_receipt(
                            c,
                            &plan,
                            "delivery-key",
                            "command-key",
                            "receiver",
                            100,
                            &mut quality,
                        )
                        .await,
                        "LOT_UNAVAILABLE",
                    );
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "FIXTURE_ABORT",
                        "Roll back after verifying the inactive lot refusal.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(result, Err(PgTransactionError::Business(_))));
        let evidence: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT (CASE WHEN status='scrapped' THEN 1 ELSE 0 END)::bigint FROM inventory_lots WHERE id='lot-existing')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (1_000, 0, 0, 1));
    })
    .await;
}

#[tokio::test]
async fn receipt_source_includes_field_and_manufacturing_reservations() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none", true).await;
        sqlx::raw_sql(
            "INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,status,version,created_by,created_at,updated_at)\
             VALUES('field-ticket','company','branch','order-1','supplier','Field request','Issue','maker','2026-11-10','open',1,'maker',0,0);\
             INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at)\
             VALUES('field-agreement','company','branch','supplier','Field agreement','2026-01-01','2027-01-01','THB',0,'draft','maker',1,0,0);\
             INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,version,created_by,created_at,updated_at)\
             VALUES('field-order','company','branch','field-ticket',1,'field-agreement',1,'Issue','Site','maker','open',1,'maker',0,0);\
             INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,reason,created_at)\
             VALUES('field-reservation','field-order','warehouse','product',250,0,0,9007199254740991,'maker','Field reserve',0);\
             INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-spec','company','Specification','bom','product',1000,10000,'approved','maker',1,0,0);\
             INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-scenario','company','branch','warehouse','mfg-spec',1,1000,'2026-11-30',0,'[]','calculated','maker',1,0,0);\
             INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-recommendation','mfg-scenario',1,1000,0,'2026-11-30','proposed','maker',1,0,0);\
             INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,version,created_at,updated_at)\
             VALUES('mfg-order','company','branch','warehouse','mfg-recommendation','mfg-spec',1,1000,'reserved','maker',1,0,0);\
             INSERT INTO manufacturing_material_reservations(id,order_id,warehouse_id,product_id,quantity_milli,consumed_milli,released_milli,expires_at,created_by,created_at)\
             VALUES('mfg-reservation','mfg-order','warehouse','product',250,0,0,9007199254740991,'maker',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    let source =
                        read_stock_receipt_source(c, "company", Some("branch"), None, "order-1")
                            .await?;
                    let stock = source.lines[0].stock.as_ref().unwrap();
                    assert_eq!(stock.quantity_milli, 1_000);
                    assert_eq!(stock.reserved_milli, 500);
                    assert_eq!(stock.quarantined_milli, 0);
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_source_fences_every_reservation_source_behind_the_view() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *reader)
            .await
            .unwrap();
        lock_stock_receipt_sources(&mut reader).await.unwrap();
        fence::assert_reservation_source_writes_wait(&pool).await;
        reader.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn receipt_source_is_company_and_branch_scoped() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "lot", false).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_stock_receipt_sources(c).await?;
                    denied(
                        read_stock_receipt_source(c, "other-company", None, None, "order-1").await,
                        "ORDER_NOT_FOUND",
                    );
                    denied(
                        read_stock_receipt_source(
                            c,
                            "company",
                            Some("branch-two"),
                            None,
                            "order-1",
                        )
                        .await,
                        "ORDER_NOT_FOUND",
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

mod commerce_receipt {
    //! Commerce purchase goods receipt through the Inventory stock-receipt seam on
    //! migrated PostgreSQL: same-transaction fulfillment/stock/lot evidence,
    //! current IAM, idempotent replay, source/version guards, rollback and
    //! persisted readback.
    #![allow(dead_code)]
    use super::fixture;

    use erpx_server_commerce::public::order::CommerceOrderActor;
    use erpx_server_commerce::public::receipt::{
        CommerceReceipt, PgCommerceReceipt, ReceiptAllocationCommand, ReceiptCommand,
        ReceiptDisposition, ReceiptLineCommand, receive_purchase_order,
    };
    use erpx_server_inventory::public::commerce_receipt::InventoryStockReceipt;
    use erpx_server_inventory::public::stock_receipt::{
        StockReceiptQualityPort, StockReceiptQualityRequest,
    };
    use erpx_server_runtime::public::postgres::{
        DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PgWorkFuture,
        TransactionOptions, load_migrations, migrate_database,
    };
    use erpx_server_shared::public::{
        AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
        UserId,
    };
    use sqlx::PgConnection;

    const NOW: i64 = 1_800_000_000_000;

    struct TestQuality;

    impl StockReceiptQualityPort for TestQuality {
        fn create_quality_hold<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            request: StockReceiptQualityRequest,
        ) -> PgWorkFuture<'a, String, BusinessError> {
            Box::pin(async move {
                let id = format!("quality-{}", request.allocation_id);
                sqlx::query(
                "INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,\
                 product_id,reason,due_date,status,created_by,created_at,updated_at,lot_id,\
                 held_quantity_milli,source_type,source_id,quantity_scope) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'held','inspector',0,0,$8,$9,'goods_receipt',$10,'partial')",
            )
            .bind(&id)
            .bind(&request.company_id)
            .bind(&request.branch_id)
            .bind(&request.warehouse_id)
            .bind(&request.product_id)
            .bind(&request.reason)
            .bind(&request.due_date)
            .bind(&request.lot_id)
            .bind(request.quantity_milli)
            .bind(&request.allocation_id)
            .execute(&mut *c)
            .await?;
                Ok(id)
            })
        }
    }

    fn actor() -> CommerceOrderActor {
        CommerceOrderActor {
            user_id: UserId::new("receiver").unwrap(),
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: None,
            assignment_id: AssignmentId::new("assignment-receiver").unwrap(),
            role: Some("warehouse_manager".into()),
            authority_revision: Some(1),
            assignment_version: Some(1),
            permissions: vec!["commerce.read".into(), "inventory.prepare".into()],
            modules: vec!["inventory".into()],
            expires_at: NOW + 60_000,
        }
    }

    fn command(
        key: &str,
        expected_version: i64,
        line_id: &str,
        quantity_milli: i64,
        allocations: Vec<ReceiptAllocationCommand>,
    ) -> ReceiptCommand {
        ReceiptCommand {
            key: CommandKey::new(key).unwrap(),
            order_id: RecordId::new("order-1").unwrap(),
            expected_version,
            date: "2026-11-04".into(),
            lines: vec![ReceiptLineCommand {
                line_id: line_id.into(),
                quantity_milli,
                allocations,
            }],
        }
    }

    fn allocation(
        lot: Option<&str>,
        serial: Option<&str>,
        quantity_milli: i64,
        disposition: ReceiptDisposition,
    ) -> ReceiptAllocationCommand {
        ReceiptAllocationCommand {
            lot_code: lot.map(Into::into),
            serial_number: serial.map(Into::into),
            quantity_milli,
            disposition,
            quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
                .then(|| "Packaging inspection".into()),
            quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
                .then(|| "2026-11-10".into()),
            manufactured_date: None,
            expiry_date: None,
        }
    }

    fn lot_allocation() -> ReceiptCommand {
        command(
            "receipt-one",
            3,
            "line-1",
            2_000,
            vec![
                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
            ],
        )
    }

    async fn run(
        pool: &DatabasePool,
        actor: CommerceOrderActor,
        command: ReceiptCommand,
    ) -> Result<CommerceReceipt, PgTransactionError<BusinessError>> {
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let actor = actor.clone();
                let command = command.clone();
                Box::pin(async move {
                    let mut repository = PgCommerceReceipt;
                    let mut stock = InventoryStockReceipt::new(TestQuality);
                    receive_purchase_order(
                        &actor,
                        &command,
                        &mut repository,
                        &mut stock,
                        c,
                        TimestampMillis::new(NOW),
                    )
                    .await
                })
            })
            .await
    }

    fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
        match error {
            PgTransactionError::Business(error) => error,
            other => panic!("expected an owner business failure, got {other:?}"),
        }
    }

    async fn denied(
        pool: &DatabasePool,
        actor: CommerceOrderActor,
        command: ReceiptCommand,
        code: &str,
    ) {
        let error = run(pool, actor, command)
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, code, "{error:?}");
    }

    async fn setup(pool: &DatabasePool) {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign company',NULL,'foreign',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('other-branch','branch','other-branch','Other branch',NULL,'company',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','receiver','local:receiver','active','local',1),\
         ('company','stale','local:stale','active','local',1),\
         ('company','other-branch','local:other-branch','active','local',1),\
         ('foreign','foreign-user','local:foreign-user','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-receiver','receiver','warehouse_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-stale','stale','sales_operator','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-other-branch','other-branch','warehouse_manager','[]','company','other-branch',0,9000000000000,NULL,'branch','other-branch','manual'),\
         ('assignment-foreign','foreign-user','warehouse_manager','[]','foreign',NULL,0,9000000000000,NULL,'holding',NULL,'manual');\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,version,created_by,created_at,updated_at) VALUES \
         ('chart','company','Primary','THB','active',1,'maker',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,status,version,created_at,updated_at) VALUES \
         ('receivable','chart','company','1100','Receivable','asset','active',1,0,0),\
         ('payable','chart','company','2100','Payable','liability','active',1,0,0),\
         ('revenue','chart','company','4100','Revenue','revenue','active',1,0,0),\
         ('inventory','chart','company','1200','Inventory','asset','active',1,0,0),\
         ('clearing','chart','company','2200','Clearing','liability','active',1,0,0),\
         ('cogs','chart','company','5100','Cost of sales','expense','active',1,0,0),\
         ('input-tax','chart','company','1300','Input tax','asset','active',1,0,0),\
         ('output-tax','chart','company','2300','Output tax','liability','active',1,0,0),\
         ('cash','chart','company','1000','Cash','asset','active',1,0,0);\
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES \
         ('company','chart','THB','receivable','payable','revenue','inventory','clearing','cogs','input-tax','output-tax','cash');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('supplier','company','supplier','SUP-1','Supplier','THB',0,0);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','W1','Main warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode,item_type,status,version) VALUES \
         ('product','company','P1','Received product',2500,'lot','stock','active',1),\
         ('serial-product','company','P2','Serial product',1000,'serial','stock','active',1),\
         ('service-product','company','S1','Supplier service',1000,'none','service','active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) VALUES \
         ('order-1','company','branch','purchase','PO-1','supplier','warehouse','THB','2026-11-01','2026-11-10',13500,0,'confirmed',3,'maker',0,0),\
         ('serial-order','company','branch','purchase','PO-2','supplier','warehouse','THB','2026-11-01','2026-11-10',2000,0,'confirmed',1,'maker',0,0);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES \
         ('line-1','order-1','product','Received product',5000,0,2500,0,12500,0,1,'stock'),\
         ('line-service','order-1','service-product','Supplier service',1000,0,1000,0,1000,0,2,'service'),\
         ('line-serial','serial-order','serial-product','Serial product',2000,0,1000,0,2000,0,1,'stock');\
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES \
         ('warehouse','product',1000,1000,1);\
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,created_by,created_at) VALUES \
         ('lot-existing','company','product','lot','L-A',NULL,'maker',0),\
         ('serial-existing','company','serial-product','serial',NULL,'SN-1','maker',0);\
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor,version) VALUES \
         ('lot-existing','warehouse',1000,1000,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn receipt_writes_fulfillment_and_inventory_evidence_in_one_unit() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            let receipt = run(&pool, actor(), lot_allocation()).await.unwrap();
            assert_eq!(receipt.order_id, "order-1");
            assert_eq!(receipt.fulfillment_id, "delivery-receipt-one");
            assert_eq!(receipt.state, "partially_fulfilled");
            assert_eq!(receipt.version, 4);
            assert_eq!(receipt.total_value_minor, 5_000);
            assert_eq!(receipt.lines.len(), 1);
            assert_eq!(receipt.lines[0].line_id, "line-1");
            assert_eq!(receipt.lines[0].value_minor, 5_000);

            let order: (String, i64) =
                sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='order-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(order, ("partially_fulfilled".into(), 4));
            let fulfilled: i64 = sqlx::query_scalar(
                "SELECT fulfilled_milli FROM commerce_order_lines WHERE id='line-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(fulfilled, 2_000);
            let service: i64 = sqlx::query_scalar(
                "SELECT fulfilled_milli FROM commerce_order_lines WHERE id='line-service'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(service, 0);

            let fulfillment: (String, String, String, String) = sqlx::query_as(
                "SELECT order_id,fulfillment_date,completion_kind,created_by \
             FROM commerce_fulfillments WHERE id='delivery-receipt-one'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                fulfillment,
                (
                    "order-1".into(),
                    "2026-11-04".into(),
                    "goods".into(),
                    "receiver".into()
                )
            );
            let line: (i64, i64, i64, Option<String>) = sqlx::query_as(
                "SELECT quantity_milli,value_minor,subtotal_minor,stock_currency \
             FROM commerce_fulfillment_lines WHERE id='delivery-receipt-one-0'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(line, (2_000, 5_000, 5_000, Some("THB".into())));

            let stock: (i64, i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor,version FROM commerce_stock \
             WHERE warehouse_id='warehouse' AND product_id='product'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(stock, (3_000, 6_000, 3));
            let stock_move: (i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor FROM commerce_stock_moves \
             WHERE id='delivery-receipt-one-0' AND fulfillment_id='delivery-receipt-one'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(stock_move, (2_000, 5_000));

            let lots: Vec<(String, i64, i64)> = sqlx::query_as(
                "SELECT l.lot_code,b.quantity_milli,b.value_minor FROM inventory_lots l \
             JOIN inventory_lot_balances b ON b.lot_id=l.id AND b.warehouse_id='warehouse' \
             WHERE l.product_id='product' ORDER BY l.lot_code",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                lots,
                vec![("L-A".into(), 2_000, 3_500), ("L-B".into(), 1_000, 2_500)]
            );
            let allocations: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
                "SELECT l.lot_code,a.disposition,a.quantity_milli,a.quality_hold_id \
             FROM inventory_fulfillment_lot_allocations a \
             JOIN inventory_lots l ON l.id=a.lot_id \
             WHERE a.fulfillment_line_id='delivery-receipt-one-0' ORDER BY a.id",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                allocations,
                vec![
                    ("L-A".into(), "available".into(), 1_000, None),
                    (
                        "L-B".into(),
                        "quarantine".into(),
                        1_000,
                        Some("quality-lot-allocation-delivery-receipt-one-0-1".into())
                    )
                ]
            );
            let hold: (String, i64) = sqlx::query_as(
                "SELECT source_type,held_quantity_milli FROM inventory_quality_holds \
             WHERE id='quality-lot-allocation-delivery-receipt-one-0-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(hold, ("goods_receipt".into(), 1_000));

            let evidence: (i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='receipt-one'),\
             (SELECT count(*) FROM audit_events WHERE command_id='receipt-one'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='receipt-one'),\
             (SELECT count(*) FROM inventory_lot_moves WHERE source_type='goods_receipt'),\
             (SELECT count(*) FROM inventory_fulfillment_lot_allocations)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(evidence, (1, 1, 1, 2, 2));

            let replay = run(&pool, actor(), lot_allocation()).await.unwrap();
            assert_eq!(replay, receipt);
            let after_replay: (i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM command_receipts WHERE command_id='receipt-one')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(after_replay, (1, 1, 1));

            let changed = command(
                "receipt-one",
                3,
                "line-1",
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Available,
                )],
            );
            denied(&pool, actor(), changed, "IDEMPOTENCY_CONFLICT").await;

            let partial = command(
                "receipt-two",
                4,
                "line-1",
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Available,
                )],
            );
            let second = run(&pool, actor(), partial).await.unwrap();
            assert_eq!(second.state, "partially_fulfilled");
            assert_eq!(second.version, 5);
            assert_eq!(second.total_value_minor, 2_500);
            let stock: (i64, i64, i64) = sqlx::query_as(
                "SELECT quantity_milli,value_minor,version FROM commerce_stock \
             WHERE warehouse_id='warehouse' AND product_id='product'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(stock, (4_000, 8_500, 4));
            let allocations: i64 =
                sqlx::query_scalar("SELECT count(*) FROM inventory_fulfillment_lot_allocations")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(allocations, 3);
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn receipt_refuses_stale_scope_service_and_over_receipt() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            denied(
                &pool,
                actor(),
                command(
                    "stale-version",
                    2,
                    "line-1",
                    1_000,
                    vec![allocation(
                        Some("L-A"),
                        None,
                        1_000,
                        ReceiptDisposition::Available,
                    )],
                ),
                "COMMERCE_ORDER_CHANGED",
            )
            .await;
            denied(
                &pool,
                CommerceOrderActor {
                    company_id: CompanyId::new("foreign").unwrap(),
                    branch_id: None,
                    user_id: UserId::new("foreign-user").unwrap(),
                    assignment_id: AssignmentId::new("assignment-foreign").unwrap(),
                    role: Some("warehouse_manager".into()),
                    ..actor()
                },
                lot_allocation(),
                "ORDER_NOT_FOUND",
            )
            .await;
            denied(
                &pool,
                CommerceOrderActor {
                    branch_id: Some(RecordId::new("other-branch").unwrap()),
                    user_id: UserId::new("other-branch").unwrap(),
                    assignment_id: AssignmentId::new("assignment-other-branch").unwrap(),
                    ..actor()
                },
                lot_allocation(),
                "ORDER_NOT_FOUND",
            )
            .await;
            let mut service = lot_allocation();
            service.lines[0].line_id = "line-service".into();
            service.lines[0].allocations.clear();
            service.lines[0].quantity_milli = 1_000;
            denied(&pool, actor(), service, "STOCK_RECEIPT_LINE_INVALID").await;
            denied(
                &pool,
                actor(),
                command(
                    "over-receipt",
                    3,
                    "line-1",
                    6_000,
                    vec![allocation(
                        Some("L-A"),
                        None,
                        6_000,
                        ReceiptDisposition::Available,
                    )],
                ),
                "OVER_FULFILLMENT",
            )
            .await;
            sqlx::query("UPDATE commerce_orders SET currency='USD' WHERE id='order-1'")
                .execute(pool.raw())
                .await
                .unwrap();
            denied(
                &pool,
                actor(),
                lot_allocation(),
                "RECEIPT_FX_GUARD_REQUIRED",
            )
            .await;
            sqlx::query("UPDATE commerce_orders SET currency='THB' WHERE id='order-1'")
                .execute(pool.raw())
                .await
                .unwrap();

            let mut serial = command(
                "duplicate-serial",
                1,
                "line-serial",
                1_000,
                vec![allocation(
                    None,
                    Some("SN-1"),
                    1_000,
                    ReceiptDisposition::Available,
                )],
            );
            serial.order_id = RecordId::new("serial-order").unwrap();
            denied(&pool, actor(), serial, "SERIAL_ALREADY_RECEIVED").await;

            let evidence: (i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM commerce_stock WHERE product_id='serial-product'),\
             (SELECT count(*) FROM command_receipts),\
             (SELECT count(*) FROM inventory_quality_holds)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(evidence, (0, 0, 0, 0, 0));
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn receipt_requires_current_inventory_authority() {
        fixture::with_database(3, |pool| async move {
            setup(&pool).await;
            denied(
                &pool,
                CommerceOrderActor {
                    user_id: UserId::new("stale").unwrap(),
                    assignment_id: AssignmentId::new("assignment-stale").unwrap(),
                    role: Some("sales_operator".into()),
                    ..actor()
                },
                lot_allocation(),
                "COMMERCE_PERMISSION_REQUIRED",
            )
            .await;
            denied(
                &pool,
                CommerceOrderActor {
                    expires_at: NOW - 1,
                    ..actor()
                },
                lot_allocation(),
                "WORK_CONTEXT_CHANGED",
            )
            .await;
            denied(
                &pool,
                CommerceOrderActor {
                    department_id: Some(RecordId::new("department").unwrap()),
                    ..actor()
                },
                lot_allocation(),
                "COMMERCE_RECEIPT_PERMISSION_REQUIRED",
            )
            .await;
            let evidence: (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM command_receipts)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(evidence, (0, 0));
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn late_outbox_failure_rolls_back_every_receipt_rows_and_exact_retry_persists_once() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            sqlx::raw_sql(
                "CREATE FUNCTION fail_receipt_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF NEW.command_id='receipt-rollback' THEN \
             RAISE EXCEPTION 'blocked receipt outbox'; END IF; RETURN NEW; END $$; \
             CREATE TRIGGER fail_receipt_outbox BEFORE INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION fail_receipt_outbox();",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let mut rollback = lot_allocation();
            rollback.key = CommandKey::new("receipt-rollback").unwrap();
            assert!(run(&pool, actor(), rollback).await.is_err());
            let evidence: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT quantity_milli FROM commerce_stock WHERE product_id='product'),\
             (SELECT version FROM commerce_stock WHERE product_id='product'),\
             (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_fulfillment_lines),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM inventory_lot_moves),\
             (SELECT count(*) FROM inventory_quality_holds),\
             (SELECT count(*) FROM command_receipts)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(evidence, (1_000, 1, 0, 0, 0, 0, 0, 0));
            let lots: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM inventory_lots WHERE product_id='product'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(lots, 1);
            sqlx::raw_sql(
                "DROP TRIGGER fail_receipt_outbox ON outbox_events; \
             DROP FUNCTION fail_receipt_outbox();",
            )
            .execute(pool.raw())
            .await
            .unwrap();
            let mut retry = lot_allocation();
            retry.key = CommandKey::new("receipt-rollback").unwrap();
            let receipt = run(&pool, actor(), retry).await.unwrap();
            assert_eq!(receipt.fulfillment_id, "delivery-receipt-rollback");
            let counts: (i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments),\
             (SELECT count(*) FROM commerce_stock_moves),\
             (SELECT count(*) FROM command_receipts WHERE command_id='receipt-rollback'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='receipt-rollback')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(counts, (1, 1, 1, 1));
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn receipt_requires_a_serializable_owner_transaction() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let mut c = pool.raw().begin().await.unwrap();
            let mut repository = PgCommerceReceipt;
            let mut stock = InventoryStockReceipt::new(TestQuality);
            let error = receive_purchase_order(
                &actor(),
                &lot_allocation(),
                &mut repository,
                &mut stock,
                &mut c,
                TimestampMillis::new(NOW),
            )
            .await
            .unwrap_err();
            let PgTransactionError::Persistence(error) = error else {
                panic!("expected a serializable transaction refusal");
            };
            assert_eq!(error.code, "COMMERCE_RECEIPT_SERIALIZABLE_REQUIRED");
            c.rollback().await.unwrap();
            let row: (String, i64) =
                sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='order-1'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(row, ("confirmed".into(), 3));
        })
        .await;
    }
}
