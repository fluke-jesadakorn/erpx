//! Native Commerce quotation/sales-order read and lifecycle on migrated
//! PostgreSQL: current IAM, Company/branch isolation, versions/CAS,
//! idempotency, audit/outbox, rollback and persisted readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commerce::public::order::{
    CommerceFulfillmentRepository, CommerceOrderActor, CommerceOrderReadRepository, OrderAction,
    PgCommerceOrders, SalesOrderDocument, SaveOrderCommand, SaveOrderInput, SaveOrderLineInput,
    SaveOrderReceipt, ServiceCompletionCommand, ServiceCompletionLineInput, TransitionCommand,
    fulfill_service_order, order_detail, save_order, save_order_outcome, transition_order,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use serde_json::json;
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;

#[tokio::test]
async fn quality_masters_prove_active_product_and_scoped_warehouse_in_one_unit() {
    use erpx_server_commerce::public::quality_masters::{
        QualityMasterRequest, inspect_quality_masters, lock_quality_masters,
    };
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let long_product=format!("product-{}","p".repeat(202));let long_warehouse=format!("warehouse-{}","w".repeat(202));
        sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES($1,'company','LONG-P','Long product',100,'none')").bind(&long_product).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES($1,'company','branch','LONG-W','Long warehouse')").bind(&long_warehouse).execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_quality_masters(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();let branch=RecordId::new("branch").unwrap();let product=String::from("product-1");let warehouse=String::from("warehouse-1");
        let request=||QualityMasterRequest::WarehouseProduct {company:company.clone(),branch:Some(branch.clone()),warehouse_id:warehouse.clone(),product_id:product.clone()};
        let witness=inspect_quality_masters(&mut tx,request()).await.unwrap();let facts=witness.facts().unwrap();assert_eq!(facts.product_id,"product-1");assert_eq!(facts.branch_id.as_deref(),Some("branch"));witness.require_current(&mut tx).await.unwrap();
        let other=inspect_quality_masters(&mut tx,QualityMasterRequest::WarehouseProduct {company:company.clone(),branch:Some(RecordId::new("other-branch").unwrap()),warehouse_id:warehouse.clone(),product_id:product.clone()}).await.unwrap();assert!(other.facts().is_none());
        let foreign=inspect_quality_masters(&mut tx,QualityMasterRequest::Product {company:CompanyId::new("foreign").unwrap(),product_id:product.clone()}).await.unwrap();assert!(foreign.facts().is_none());
        let product_witness=inspect_quality_masters(&mut tx,QualityMasterRequest::Product {company:company.clone(),product_id:product.clone()}).await.unwrap();assert_eq!(product_witness.facts().unwrap().product_version,Some(1));
        assert!(long_product.len()>200 && long_warehouse.len()>200);
        let physical=inspect_quality_masters(&mut tx,QualityMasterRequest::WarehouseProduct {company:company.clone(),branch:Some(branch.clone()),warehouse_id:long_warehouse.clone(),product_id:long_product.clone()}).await.unwrap();
        let facts=physical.facts().unwrap();assert_eq!(facts.product_id,long_product);assert_eq!(facts.warehouse_id.as_deref(),Some(long_warehouse.as_str()));physical.require_current(&mut tx).await.unwrap();
        let physical_product=inspect_quality_masters(&mut tx,QualityMasterRequest::Product {company:company.clone(),product_id:long_product.clone()}).await.unwrap();assert!(physical_product.facts().is_some());physical_product.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE commerce_products SET name='Unrelated display rename' WHERE id='product-1'").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();product_witness.require_current(&mut tx).await.unwrap();
        let mut other_tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other_tx).await.unwrap();assert!(witness.require_current(&mut other_tx).await.is_err());other_tx.rollback().await.unwrap();
        sqlx::query("UPDATE commerce_warehouses SET status='archived' WHERE id='warehouse-1'").execute(&mut *tx).await.unwrap();assert!(witness.require_current(&mut tx).await.is_err());assert!(inspect_quality_masters(&mut tx,request()).await.unwrap().facts().is_none());
        sqlx::query("UPDATE commerce_products SET status='archived' WHERE id='product-1'").execute(&mut *tx).await.unwrap();assert!(product_witness.require_current(&mut tx).await.is_err());assert!(inspect_quality_masters(&mut tx,QualityMasterRequest::Product {company,product_id:product}).await.unwrap().facts().is_none());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_master_lock_waits_for_writer_then_reads_fresh_archival() {
    use erpx_server_commerce::public::quality_masters::{
        QualityMasterRequest, inspect_quality_masters, lock_quality_masters,
    };
    use sqlx::Connection;
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE commerce_products SET status='archived' WHERE id='product-1'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *writer)
            .await
            .unwrap();
        let other_pool = pool.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let reader = tokio::spawn(async move {
            let mut connection = other_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
            let mut tx = connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            send.send(pid).unwrap();
            lock_quality_masters(&mut tx).await.unwrap();
            let witness = inspect_quality_masters(
                &mut tx,
                QualityMasterRequest::Product {
                    company: CompanyId::new("company").unwrap(),
                    product_id: "product-1".into(),
                },
            )
            .await
            .unwrap();
            assert!(witness.facts().is_none());
            witness.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
        });
        let reader_pid = receive.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(writer_pid)
                .bind(reader_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            blocked,
            "source lock must wait before acquiring its snapshot"
        );
        writer.commit().await.unwrap();
        reader.await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn quality_customer_return_commerce_uses_actual_scoped_untracked_sources() {
    use erpx_server_commerce::public::quality_return_join::{
        inspect_quality_return_join, lock_quality_return_join,
    };
    use erpx_server_commerce::public::quality_return_origin::{
        QualityReturnSelector, inspect_quality_return_commerce, lock_quality_return_commerce,
    };
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('return-chart','company','Return chart','THB','creator',0,0); INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('debit','return-chart','company','100','Debit','asset',0,0),('credit','return-chart','company','400','Credit','revenue',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES('return-stock','company','RS1','Stock','each',1000,0,'active',1,'none','stock'); UPDATE commerce_order_lines SET item_type='stock',product_id='return-stock' WHERE id='sales-confirmed-line'; INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,created_by,created_at,updated_at) VALUES('return-invoice','company','party-1','receivable','I-1','2026-11-01','2026-11-02','THB',1000,0,1000,1000,'debit','credit','creator',0,0); INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version,invoice_id) VALUES('return-fulfillment','sales-confirmed','RF-1','2026-11-01','creator',0,1,'return-invoice'); INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES('return-fulfillment-line','return-fulfillment','sales-confirmed-line',1000,1000,1000,0); INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,tax_minor,total_minor,version,requested_by,approved_by,created_at,updated_at) VALUES('return-1','company','branch','sales-confirmed','return-fulfillment','return-invoice','customer_return','RET-1','2026-11-01','reason','approved',1000,0,1000,2,'creator','reviewer',0,0); INSERT INTO commerce_return_lines(id,return_id,fulfillment_line_id,product_id,quantity_milli,value_minor,subtotal_minor,tax_minor,ordinal) VALUES('return-line','return-1','return-fulfillment-line','return-stock',1000,1000,1000,0,0)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('return-lot-a','company','return-stock','lot','A','creator',0),('return-lot-b','company','return-stock','lot','B','creator',0); INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition) VALUES('original-a','return-fulfillment-line','return-lot-a',500,500,'available'),('original-b','return-fulfillment-line','return-lot-b',500,500,'available'); ").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_return_commerce(&mut tx).await.unwrap();
        lock_quality_return_join(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let source=inspect_quality_return_commerce(&mut tx,&company,Some(&RecordId::new("branch").unwrap()),QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap();
        let facts=source.facts().unwrap();assert_eq!(facts.return_id,"return-1");assert_eq!(facts.approver,"reviewer");assert_eq!(facts.line_ordinal,0);assert_eq!(facts.lot_id,None);
        source.require_current(&mut tx).await.unwrap();
        assert!(inspect_quality_return_commerce(&mut tx,&CompanyId::new("foreign").unwrap(),None,QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap().facts().is_none());
        assert!(inspect_quality_return_commerce(&mut tx,&company,None,QualityReturnSelector::TrackedAllocation("return-line".into())).await.unwrap().facts().is_none());
        assert!(inspect_quality_return_commerce(&mut tx,&company,Some(&RecordId::new("other-branch").unwrap()),QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap().facts().is_none());
        for (update, restore) in [
            ("UPDATE commerce_returns SET status='requested' WHERE id='return-1'", "UPDATE commerce_returns SET status='approved' WHERE id='return-1'"),
            ("UPDATE commerce_returns SET kind='supplier_return' WHERE id='return-1'", "UPDATE commerce_returns SET kind='customer_return' WHERE id='return-1'"),
            ("UPDATE commerce_fulfillments SET invoice_id=NULL WHERE id='return-fulfillment'", "UPDATE commerce_fulfillments SET invoice_id='return-invoice' WHERE id='return-fulfillment'"),
            ("UPDATE commerce_returns SET fulfillment_id='fulfillment-1' WHERE id='return-1'", "UPDATE commerce_returns SET fulfillment_id='return-fulfillment' WHERE id='return-1'"),
            ("UPDATE commerce_return_lines SET product_id='product-1' WHERE id='return-line'", "UPDATE commerce_return_lines SET product_id='return-stock' WHERE id='return-line'"),
        ] {
            sqlx::query(update).execute(&mut *tx).await.unwrap();
            assert!(inspect_quality_return_commerce(&mut tx,&company,None,QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap().facts().is_none());
            sqlx::query(restore).execute(&mut *tx).await.unwrap();
        }
        let mut other=pool.raw().begin().await.unwrap();sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
        let other_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {
            let mut writer=other_pool.raw().begin().await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();send.send(pid).unwrap();
            sqlx::query("INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('blocked-lot','return-line','original-a','return-lot-a',500,500,0,0,0)").execute(&mut *writer).await.unwrap();
            writer.rollback().await.unwrap();
        });
        let writer_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..200 {
            blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(reader_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked { break; }tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked,"allocation INSERT must wait for the protected absence read");
        tx.rollback().await.unwrap();writer.await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_return_commerce(&mut tx).await.unwrap();
        lock_quality_return_join(&mut tx).await.unwrap();
        let join=inspect_quality_return_join(&mut tx).await.unwrap();
        let source=inspect_quality_return_commerce(&mut tx,&company,None,QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap();
        assert!(source.facts().is_some());
        sqlx::raw_sql("INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('lot-b','return-line','original-b','return-lot-b',500,500,0,0,0),('lot-a','return-line','original-a','return-lot-a',500,500,0,0,0)").execute(&mut *tx).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {commerceQualityOriginRows}=await import('./packages/server-commerce/src/adapters/quality-purchase-sources.ts');const found=commerceQualityOriginRows.match(/SELECT \*,row_number\(\) OVER \(PARTITION BY return_line_id ORDER BY id\)-1 lot_ordinal FROM commerce_return_lot_allocations/);if(!found)throw Error('QUALITY_RETURN_ORDINAL_SOURCE_MISSING');process.stdout.write(found[0]);"#;
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","-e",script]).output().unwrap();assert!(output.status.success(),"actual Commerce Quality TS source unavailable");let original=String::from_utf8(output.stdout).unwrap();
        let sql=format!("SELECT id,lot_ordinal FROM ({original}) actual WHERE return_line_id='return-line' ORDER BY id");let expected:Vec<(String,i64)>=sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_all(&mut *tx).await.unwrap();
        let sql=format!("WITH {} SELECT id,lot_ordinal FROM quality_commerce_return_allocations WHERE return_line_id='return-line' ORDER BY id",join.cte_sql());let actual:Vec<(String,i64)>=sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_all(&mut *tx).await.unwrap();assert_eq!(actual,expected);assert_eq!(actual,vec![("lot-a".into(),0),("lot-b".into(),1)]);join.require_current(&mut tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        assert!(inspect_quality_return_commerce(&mut tx,&company,None,QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap().facts().is_none());
        let tracked=inspect_quality_return_commerce(&mut tx,&company,None,QualityReturnSelector::TrackedAllocation("lot-b".into())).await.unwrap();
        let facts=tracked.facts().unwrap();assert_eq!(facts.lot_ordinal,Some(1));assert_eq!(facts.lot_id.as_deref(),Some("return-lot-b"));assert_eq!(facts.fulfillment_allocation_id.as_deref(),Some("original-b"));
        tracked.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE commerce_returns SET approved_by='creator' WHERE id='return-1'").execute(&mut *tx).await.unwrap();
        assert!(tracked.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_quality_return_join(&mut other).await.unwrap();assert!(join.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut fenced=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fenced).await.unwrap();sqlx::query("SAVEPOINT before_quality_return_locks").execute(&mut *fenced).await.unwrap();lock_quality_return_join(&mut fenced).await.unwrap();let retained=inspect_quality_return_join(&mut fenced).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_quality_return_locks").execute(&mut *fenced).await.unwrap();assert!(retained.require_current(&mut fenced).await.is_err());fenced.rollback().await.unwrap();
        let mut shadow=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *shadow).await.unwrap();sqlx::query("CREATE TEMP TABLE commerce_return_lines(id text)").execute(&mut *shadow).await.unwrap();lock_quality_return_join(&mut shadow).await.unwrap();assert!(inspect_quality_return_join(&mut shadow).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *rls).await.unwrap();let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass);sqlx::raw_sql("ALTER TABLE public.commerce_return_lines ENABLE ROW LEVEL SECURITY; ALTER TABLE public.commerce_return_lines FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();assert!(sqlx::query_scalar::<_,bool>("SELECT row_security_active('public.commerce_return_lines'::regclass)").fetch_one(&mut *rls).await.unwrap());lock_quality_return_join(&mut rls).await.unwrap();assert!(inspect_quality_return_join(&mut rls).await.is_err());rls.rollback().await.unwrap();
        let mut limited=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *limited).await.unwrap();lock_quality_return_join(&mut limited).await.unwrap();sqlx::query("UPDATE commerce_orders SET note=repeat('x',2097153) WHERE id='sales-confirmed'").execute(&mut *limited).await.unwrap();assert!(matches!(inspect_quality_return_join(&mut limited).await,Err(PgTransactionError::Business(ref e)) if e.code=="COMMERCE_QUALITY_RETURN_LIMIT"));limited.rollback().await.unwrap();
        // A matching writer that starts first must finish before our first
        // source snapshot; the fresh read must then refuse untracked origin.
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO commerce_return_lot_allocations(id,return_line_id,fulfillment_allocation_id,lot_id,quantity_milli,value_minor,snapshot_quantity_milli,snapshot_value_minor,snapshot_version) VALUES('writer-first-lot','return-line','original-a','return-lot-a',500,500,0,0,0)").execute(&mut *writer).await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let other_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let reader=tokio::spawn(async move {
            use sqlx::Connection;
            let mut connection=other_pool.raw().acquire().await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx=connection.begin().await.unwrap();
            sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            send.send(pid).unwrap();
            lock_quality_return_commerce(&mut tx).await.unwrap();
            let witness=inspect_quality_return_commerce(&mut tx,&CompanyId::new("company").unwrap(),None,QualityReturnSelector::UntrackedLine("return-line".into())).await.unwrap();
            assert!(witness.facts().is_none());witness.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
        });
        let reader_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..200 {
            blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
            if blocked { break; }tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked,"reader must wait for matching allocation writer before snapshot");
        writer.commit().await.unwrap();reader.await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_receipt_commerce_witness_is_scoped_and_same_unit() {
    use erpx_server_commerce::public::quality_receipt_origin::{
        inspect_quality_receipt_commerce, lock_quality_receipt_commerce,
    };
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES('stock-product','company','STOCK-1','Stock','each',1000,0,'active',1,'none','stock'); UPDATE commerce_order_lines SET item_type='stock',product_id='stock-product' WHERE id='purchase-draft-line'; INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES('receipt-1','purchase-draft','R-1','2026-11-01','creator',0,1); INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES('receipt-line','receipt-1','purchase-draft-line',1000,1000,1000,0)").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_receipt_commerce(&mut tx).await.unwrap();
        let company = CompanyId::new("company").unwrap();
        let branch = RecordId::new("branch").unwrap();
        let witness = inspect_quality_receipt_commerce(&mut tx, &company, Some(&branch), "receipt-line").await.unwrap();
        assert_eq!(witness.facts().unwrap().order_id, "purchase-draft");
        witness.require_current(&mut tx).await.unwrap();
        assert!(inspect_quality_receipt_commerce(&mut tx, &CompanyId::new("foreign").unwrap(), None, "receipt-line").await.unwrap().facts().is_none());
        assert!(inspect_quality_receipt_commerce(&mut tx, &company, Some(&RecordId::new("other-branch").unwrap()), "receipt-line").await.unwrap().facts().is_none());
        assert!(inspect_quality_receipt_commerce(&mut tx, &company, None, "wrong-line").await.unwrap().facts().is_none());
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        sqlx::query("UPDATE commerce_orders SET kind='sales' WHERE id='purchase-draft'").execute(&mut *tx).await.unwrap();
        assert!(inspect_quality_receipt_commerce(&mut tx, &company, None, "receipt-line").await.unwrap().facts().is_none());
        sqlx::query("UPDATE commerce_orders SET kind='purchase' WHERE id='purchase-draft'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE commerce_order_lines SET item_type='service',product_id='product-1' WHERE id='purchase-draft-line'").execute(&mut *tx).await.unwrap();
        assert!(inspect_quality_receipt_commerce(&mut tx, &company, None, "receipt-line").await.unwrap().facts().is_none());
        sqlx::query("UPDATE commerce_order_lines SET item_type='stock',product_id='stock-product' WHERE id='purchase-draft-line'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE commerce_order_lines SET description='changed' WHERE id='purchase-draft-line'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_receipt_source_lock_conflicts_with_real_writers_in_both_orders() {
    use erpx_server_commerce::public::quality_receipt_origin::lock_quality_receipt_commerce;
    use sqlx::Connection;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE commerce_orders SET note='writer-first' WHERE id='purchase-draft'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *writer)
            .await
            .unwrap();
        let other_pool = pool.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let reader = tokio::spawn(async move {
            let mut connection = other_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
            let mut tx = connection.begin().await.unwrap();
            sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            send.send(pid).unwrap();
            lock_quality_receipt_commerce(&mut tx).await.unwrap();
            let note: String =
                sqlx::query_scalar("SELECT note FROM commerce_orders WHERE id='purchase-draft'")
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
            assert_eq!(note, "writer-first");
            tx.rollback().await.unwrap();
        });
        let reader_pid = receive.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(writer_pid)
                .bind(reader_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            blocked,
            "reader must wait for the existing writer before taking its snapshot"
        );
        writer.commit().await.unwrap();
        reader.await.unwrap();
        let mut reader = pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *reader)
            .await
            .unwrap();
        lock_quality_receipt_commerce(&mut reader).await.unwrap();
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *reader)
            .await
            .unwrap();
        let other_pool = pool.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let writer = tokio::spawn(async move {
            let mut tx = other_pool.raw().begin().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            send.send(pid).unwrap();
            sqlx::query(
                "UPDATE commerce_orders SET note='writer-second' WHERE id='purchase-draft'",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            tx.rollback().await.unwrap();
        });
        let writer_pid = receive.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(reader_pid)
                .bind(writer_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked, "writer must wait for the retained source lock");
        reader.rollback().await.unwrap();
        writer.await.unwrap();
    })
    .await;
}

fn actor(
    user: &str,
    role: &str,
    assignment: &str,
    branch: Option<&str>,
    permissions: &[&str],
) -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new(user).unwrap(),
        person_user_ids: vec![],
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: Some(role.into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
        expires_at: NOW + 60_000,
    }
}

fn creator() -> CommerceOrderActor {
    actor(
        "creator",
        "sales_operator",
        "assignment-creator",
        Some("branch"),
        &["commerce.read", "commerce.sales"],
    )
}

fn reviewer() -> CommerceOrderActor {
    actor(
        "reviewer",
        "sales_manager",
        "assignment-reviewer",
        Some("branch"),
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn other_branch() -> CommerceOrderActor {
    actor(
        "other-branch",
        "sales_operator",
        "assignment-other-branch",
        Some("other-branch"),
        &["commerce.read", "commerce.sales"],
    )
}

fn foreign() -> CommerceOrderActor {
    CommerceOrderActor {
        company_id: CompanyId::new("foreign").unwrap(),
        branch_id: Some(RecordId::new("foreign-branch").unwrap()),
        user_id: UserId::new("foreign-user").unwrap(),
        assignment_id: AssignmentId::new("assignment-foreign").unwrap(),
        role: Some("sales_operator".into()),
        ..creator()
    }
}

fn command(order_id: &str, action: OrderAction, expected: i64, key: &str) -> TransitionCommand {
    TransitionCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: RecordId::new(order_id).unwrap(),
        action,
        expected_version: expected,
        reason: None,
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
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
         ('other-branch','branch','other-branch','Other branch',NULL,'company',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','creator','local:creator','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','revoked','local:revoked','active','local',1),\
         ('company','other-branch','local:other-branch','active','local',1),\
         ('foreign','foreign-user','local:foreign-user','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-creator','creator','sales_operator','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-reviewer','reviewer','sales_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual'),\
         ('assignment-revoked','revoked','sales_operator','[]','company','branch',0,9000000000000,1,'branch','branch','manual'),\
         ('assignment-other-branch','other-branch','sales_operator','[]','company','other-branch',0,9000000000000,NULL,'branch','other-branch','manual'),\
         ('assignment-foreign','foreign-user','sales_operator','[]','foreign','foreign-branch',0,9000000000000,NULL,'branch','foreign-branch','manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1),('supplier-1','company','supplier','S1','Supplier','THB',30,0,'active',1,1,1),\
         ('party-f','foreign','customer','CF','Foreign customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) VALUES \
         ('product-1','company','SVC-1','Service','each',10000,700,'active',1,'none','service'),('product-stock','company','STK-1','Stock item','each',10000,700,'active',1,'none','stock'),\
         ('product-f','foreign','SVC-F','Foreign service','each',10000,700,'active',1,'none','service');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO commerce_orders\
         (id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,\
         order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,\
         approved_by,created_at,updated_at,price_list_id,price_list_version,discount_reason,\
         discount_approval_required,discount_approved_by,discount_approved_at) VALUES \
         ('quote-draft','company','branch',NULL,'quote','QT-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','', 'draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('quote-discount','company','branch',NULL,'quote','QT-2','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','submitted',10000,700,NULL,2,'creator',NULL,100,200,NULL,NULL,'Exceptional discount',1,NULL,NULL),\
         ('quote-confirmed','company','branch',NULL,'quote','QT-3','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300,NULL,NULL,NULL,0,NULL,NULL),\
         ('quote-confirmed-fulfilled','company','branch',NULL,'quote','QT-4','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,4,'creator','reviewer',100,400,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-draft','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-submitted','company','branch',NULL,'sales','SO-2','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','submitted',10000,700,NULL,2,'creator',NULL,100,200,NULL,NULL,NULL,0,NULL,NULL),\
         ('sales-confirmed','company','branch',NULL,'sales','SO-3','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,NULL,3,'creator','reviewer',100,300,NULL,NULL,NULL,0,NULL,NULL),\
         ('purchase-draft','company','branch',NULL,'purchase','PO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'creator',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('foreign-quote','foreign','foreign-branch',NULL,'quote','QT-F','party-f',NULL,'THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'foreign-user',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL),\
         ('other-branch-quote','company','other-branch',NULL,'quote','QT-O','party-1',NULL,'THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,NULL,1,'other-branch',NULL,100,100,NULL,NULL,NULL,0,NULL,NULL);\
         INSERT INTO commerce_order_lines\
         (id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,\
         subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) VALUES \
         ('quote-draft-line','quote-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-discount-line','quote-discount','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-confirmed-line','quote-confirmed','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('quote-confirmed-fulfilled-line','quote-confirmed-fulfilled','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-draft-line','sales-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-submitted-line','sales-submitted','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('sales-confirmed-line','sales-confirmed','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('purchase-draft-line','purchase-draft','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('foreign-quote-line','foreign-quote','product-f','Foreign service',1000,0,10000,700,10000,700,0,NULL,0,'service'),\
         ('other-branch-quote-line','other-branch-quote','product-1','Service',1000,0,10000,700,10000,700,0,NULL,0,'service');\
         INSERT INTO commerce_fulfillments\
         (id,order_id,number,fulfillment_date,created_by,created_at,journal_id,invoice_id,version) VALUES \
         ('fulfillment-1','quote-confirmed-fulfilled','F-1','2026-11-02','creator',100,NULL,NULL,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) \
         VALUES('chart-1','company','Company chart','THB','active','creator',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) VALUES \
         ('coa-1','chart-1','company','1000','Receivable','asset','THB','active',0,0),\
         ('coa-2','chart-1','company','2000','Payable','liability','THB','active',0,0),\
         ('coa-3','chart-1','company','4000','Revenue','revenue','THB','active',0,0),\
         ('coa-4','chart-1','company','1200','Inventory','asset','THB','active',0,0),\
         ('coa-5','chart-1','company','2100','Clearing','liability','THB','active',0,0),\
         ('coa-6','chart-1','company','5000','Cost of sales','expense','THB','active',0,0),\
         ('coa-7','chart-1','company','1300','Input tax','asset','THB','active',0,0),\
         ('coa-8','chart-1','company','2200','Output tax','liability','THB','active',0,0),\
         ('coa-9','chart-1','company','1000-C','Cash','asset','THB','active',0,0);\
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,\
         revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,\
         output_tax_account_id,cash_account_id) VALUES('company','chart-1','THB','coa-1','coa-2','coa-3',\
         'coa-4','coa-5','coa-6','coa-7','coa-8','coa-9');\
         INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,\
         max_auto_discount_bps,status,version,created_by,prepared_by,approved_by,approval_reason,activated_at,created_at,updated_at) VALUES \
         ('list-general','company','General prices','THB',NULL,'2026-11-01','2026-11-30',500,'draft',1,\
          'creator','creator',NULL,NULL,NULL,100,100),\
         ('list-customer','company','Customer prices','THB','party-1','2026-11-01','2026-11-30',1000,'draft',1,\
          'creator','creator',NULL,NULL,NULL,101,101);\
         INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES \
         ('list-general-item','list-general','product-1',10000,700,0),\
         ('list-customer-item','list-customer','product-1',10500,700,0);\
         UPDATE sales_price_lists SET status='active',approved_by='reviewer',\
         approval_reason='Independently reviewed',activated_at=1000 WHERE id='list-general';\
         UPDATE sales_price_lists SET status='active',approved_by='reviewer',\
         approval_reason='Independently reviewed',activated_at=1001 WHERE id='list-customer';\
         INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) \
         VALUES('rate-thb','company','THB','2026-01-01',35,'fixture',0,'Commerce test quote');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run_detail(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    id: &str,
) -> Result<SalesOrderDocument, PgTransactionError<BusinessError>> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.clone();
            Box::pin(async move {
                let mut repository = PgCommerceOrders::new(connection);
                order_detail(&actor, &id, &mut repository, TimestampMillis::new(NOW)).await
            })
        })
        .await
}

async fn run_transition(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: TransitionCommand,
) -> Result<
    erpx_server_commerce::public::order::CommerceOrderReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgCommerceOrders::new(connection);
                transition_order(&actor, &command, &mut repository, TimestampMillis::new(NOW)).await
            })
        })
        .await
}

async fn run_error(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: TransitionCommand,
) -> BusinessError {
    run_transition(pool, actor, command)
        .await
        .map_err(business)
        .unwrap_err()
}

async fn run_save(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: SaveOrderCommand,
) -> Result<SaveOrderReceipt, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgCommerceOrders::new(connection);
                save_order(&actor, &command, &mut repository, TimestampMillis::new(NOW)).await
            })
        })
        .await
}

async fn run_save_error(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: SaveOrderCommand,
) -> BusinessError {
    run_save(pool, actor, command)
        .await
        .map_err(business)
        .unwrap_err()
}

/// Exact composition order: Commerce owner save, then the Finance-owned
/// order-revision Money snapshots on the same serializable connection.
async fn run_save_with_money(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: SaveOrderCommand,
) -> Result<SaveOrderReceipt, PgTransactionError<BusinessError>> {
    use erpx_server_finance::public::commerce_order_money::{
        CommerceOrderMoneyInput, CommerceOrderMoneyLine, snapshot_commerce_order_revision,
    };
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let now = TimestampMillis::new(NOW);
                erpx_server_finance::public::commerce_order_money::lock_commerce_order_money_sources(
                    connection,
                )
                .await?;
                let outcome = {
                    let mut repository = PgCommerceOrders::new(connection);
                    save_order_outcome(&actor, &command, &mut repository, now).await?
                };
                if let Some(plan) = outcome.plan.as_ref() {
                    let money = CommerceOrderMoneyInput {
                        order_id: plan.id.clone(),
                        version: plan.version,
                        transaction_date: plan.order_date.clone(),
                        currency: plan.currency.clone(),
                        total: plan.subtotal_minor + plan.tax_minor,
                        lines: plan
                            .lines
                            .iter()
                            .map(|line| CommerceOrderMoneyLine {
                                id: line.id.clone(),
                                unit_price: line.price_minor,
                                subtotal: line.subtotal_minor,
                                tax: line.tax_minor,
                            })
                            .collect(),
                    };
                    snapshot_commerce_order_revision(
                        connection,
                        &actor.company_id,
                        &money,
                        now,
                    )
                    .await?;
                }
                Ok(outcome.receipt)
            })
        })
        .await
}

async fn run_service_completion(
    pool: &DatabasePool,
    actor: CommerceOrderActor,
    command: ServiceCompletionCommand,
) -> Result<
    erpx_server_commerce::public::order::ServiceCompletionOutcome,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                let now = TimestampMillis::new(NOW);
                erpx_server_finance::public::commerce_order_money::lock_commerce_order_money_sources(connection).await?;
                let mut repository = PgCommerceOrders::new(connection);
                let outcome = fulfill_service_order(&actor, &command, &mut repository, now).await?;
                if outcome.created {
                    let lines = repository.fulfillment_money_lines(&outcome.receipt.fulfillment_id).await?;
                    let currency = repository.order_currency(command.order_id.as_str()).await?.ok_or_else(|| BusinessError::new(erpx_server_shared::public::FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
                    drop(repository);
                    let money = lines.into_iter().map(|(id, subtotal, tax)| erpx_server_finance::public::commerce_order_money::CommerceFulfillmentMoneyLine { id, subtotal, tax }).collect::<Vec<_>>();
                    erpx_server_finance::public::commerce_order_money::snapshot_commerce_fulfillment_money(connection, &actor.company_id, &command.date, &currency, &money, now).await?;
                }
                Ok(outcome)
            })
        })
        .await
}

fn service_command(
    key: &str,
    order_id: &str,
    version: i64,
    line_id: &str,
    quantity: &str,
    note: &str,
) -> ServiceCompletionCommand {
    ServiceCompletionCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: RecordId::new(order_id).unwrap(),
        date: "2026-11-12".into(),
        evidence_note: note.into(),
        expected_version: version,
        lines: vec![ServiceCompletionLineInput {
            line_id: line_id.into(),
            quantity: quantity.into(),
        }],
    }
}

#[allow(clippy::too_many_arguments)]
fn save_command(
    key: &str,
    order_id: Option<&str>,
    kind: erpx_server_commerce::public::order::OrderKind,
    party_id: &str,
    warehouse_id: Option<&str>,
    order_date: &str,
    due_date: &str,
    price: &str,
    tax_bps: i64,
    discount_reason: Option<&str>,
) -> SaveOrderCommand {
    SaveOrderCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: order_id.map(|id| RecordId::new(id).unwrap()),
        input: SaveOrderInput {
            kind,
            party_id: party_id.into(),
            warehouse_id: warehouse_id.map(str::to_owned),
            order_date: order_date.into(),
            due_date: due_date.into(),
            note: String::new(),
            price_list_id: None,
            discount_reason: discount_reason.map(str::to_owned),
            lines: vec![SaveOrderLineInput {
                product_id: "product-1".into(),
                quantity: "1".into(),
                price: price.into(),
                tax_bps,
            }],
            expected_version: order_id.map(|_| 1),
        },
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn save_creates_priced_quote_and_cas_updates_in_one_serializable_unit() {
    use erpx_server_commerce::public::order::OrderKind;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let active: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM sales_price_lists WHERE company_id='company' AND status='active' \
             AND currency='THB' AND valid_from<='2026-11-10' AND valid_until>='2026-11-10' \
             AND (customer_id='party-1' OR customer_id IS NULL) \
             ORDER BY CASE WHEN customer_id='party-1' THEN 0 ELSE 1 END, valid_from DESC, activated_at DESC",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(active.first().map(String::as_str), Some("list-customer"));
        let key = "commerce-order-create-0001";
        let error = run_save_error(
            &pool,
            creator(),
            save_command(
                key,
                None,
                OrderKind::Quote,
                "party-1",
                Some("warehouse-1"),
                "2026-11-10",
                "2026-11-15",
                "80",
                700,
                None,
            ),
        )
        .await;
        assert_eq!(error.code, "DISCOUNT_REASON_REQUIRED");
        assert!(!sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM commerce_orders WHERE id=$1)"
        )
        .bind(format!("order-{key}"))
        .fetch_one(pool.raw())
        .await
        .unwrap());

        let receipt = run_save(
            &pool,
            creator(),
            save_command(
                key,
                None,
                OrderKind::Quote,
                "party-1",
                Some("warehouse-1"),
                "2026-11-10",
                "2026-11-15",
                "80",
                700,
                Some("Strategic launch order approved for commercial review"),
            ),
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, format!("order-{key}"));
        assert_eq!(receipt.version, 1);
        let row = sqlx::query(
            "SELECT kind,number,state,version,subtotal_minor,tax_minor,price_list_id,\
             price_list_version,discount_approval_required,discount_reason,created_by \
             FROM commerce_orders WHERE id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("kind"), "quote");
        assert_eq!(row.get::<String, _>("state"), "draft");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<i64, _>("subtotal_minor"), 8_000);
        assert_eq!(row.get::<i64, _>("tax_minor"), 560);
        assert_eq!(row.get::<String, _>("price_list_id"), "list-customer");
        assert_eq!(row.get::<i64, _>("price_list_version"), 1);
        assert_eq!(row.get::<i64, _>("discount_approval_required"), 1);
        assert_eq!(row.get::<String, _>("created_by"), "creator");
        assert!(row
            .get::<String, _>("number")
            .starts_with("QT-2026-"));
        let line = sqlx::query(
            "SELECT id,product_id,description,quantity_milli,price_minor,tax_bps,\
             subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type \
             FROM commerce_order_lines WHERE order_id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line.get::<String, _>("id"), format!("{}-line-0", receipt.id));
        assert_eq!(line.get::<i64, _>("list_price_minor"), 10_500);
        assert_eq!(line.get::<i64, _>("price_minor"), 8_000);
        assert_eq!(line.get::<i64, _>("discount_bps"), 2_380);
        assert_eq!(line.get::<i64, _>("subtotal_minor"), 8_000);
        assert_eq!(line.get::<i64, _>("tax_minor"), 560);
        assert_eq!(line.get::<String, _>("item_type"), "service");
        for (table, count) in [
            (
                "command_receipts",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "audit_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM audit_events WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "outbox_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM outbox_events WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
        ] {
            assert_eq!(count, 1, "{table}");
        }
        assert_eq!(
            run_save(
                &pool,
                creator(),
                save_command(
                    key,
                    None,
                    OrderKind::Quote,
                    "party-1",
                    Some("warehouse-1"),
                    "2026-11-10",
                    "2026-11-15",
                    "80",
                    700,
                    Some("Strategic launch order approved for commercial review"),
                ),
            )
            .await
            .unwrap(),
            receipt
        );

        let mut update = save_command(
            "commerce-order-create-0002",
            Some(&receipt.id),
            OrderKind::Quote,
            "party-1",
            Some("warehouse-1"),
            "2026-11-11",
            "2026-11-20",
            "90",
            700,
            Some("Reviewed margin after the customer negotiation"),
        );
        update.input.note = "Updated quotation".into();
        let updated = run_save(&pool, creator(), update.clone()).await.unwrap();
        assert_eq!(updated.version, 2);
        let row = sqlx::query(
            "SELECT order_date,due_date,note,subtotal_minor,tax_minor,version FROM commerce_orders WHERE id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("order_date"), "2026-11-11");
        assert_eq!(row.get::<String, _>("due_date"), "2026-11-20");
        assert_eq!(row.get::<String, _>("note"), "Updated quotation");
        assert_eq!(row.get::<i64, _>("subtotal_minor"), 9_000);
        assert_eq!(row.get::<i64, _>("tax_minor"), 630);
        assert_eq!(row.get::<i64, _>("version"), 2);
        let line = sqlx::query(
            "SELECT price_minor,discount_bps,subtotal_minor,tax_minor FROM commerce_order_lines WHERE order_id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line.get::<i64, _>("price_minor"), 9_000);
        assert_eq!(line.get::<i64, _>("discount_bps"), 1_428);
        assert_eq!(line.get::<i64, _>("subtotal_minor"), 9_000);
        assert_eq!(line.get::<i64, _>("tax_minor"), 630);

        let mut stale = update.clone();
        stale.key = CommandKey::new("commerce-order-create-stale-0001").unwrap();
        stale.input.note = "Stale overwrite".into();
        let stale = run_save_error(&pool, creator(), stale).await;
        assert_eq!(stale.code, "CONFLICT");
        let mut wrong_kind = save_command(
            "commerce-order-create-0003",
            Some(&receipt.id),
            OrderKind::Sales,
            "party-1",
            Some("warehouse-1"),
            "2026-11-11",
            "2026-11-20",
            "90",
            700,
            Some("Reviewed margin after the customer negotiation"),
        );
        wrong_kind.input.expected_version = Some(2);
        assert_eq!(
            run_save_error(&pool, creator(), wrong_kind).await.code,
            "ORDER_KIND_LOCKED"
        );
        let submitted = save_command(
            "commerce-order-create-0004",
            Some("quote-discount"),
            OrderKind::Quote,
            "party-1",
            Some("warehouse-1"),
            "2026-11-11",
            "2026-11-20",
            "90",
            700,
            Some("Reviewed margin after the customer negotiation"),
        );
        assert_eq!(
            run_save_error(&pool, creator(), submitted).await.code,
            "CONFLICT"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn save_price_list_and_master_guards_fail_closed() {
    use erpx_server_commerce::public::order::OrderKind;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::query(
            "INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) \
             VALUES('stock-1','company','STOCK-1','Stock item','each',5000,700,'active',1,'none','stock')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let base = |key: &str, price: &str, tax: i64| {
            save_command(
                key,
                None,
                OrderKind::Quote,
                "party-1",
                Some("warehouse-1"),
                "2026-11-10",
                "2026-11-15",
                price,
                tax,
                Some("Reviewed commercial discount"),
            )
        };
        let mut selected = base("commerce-order-guard-0001", "80", 700);
        selected.input.price_list_id = Some("missing-list".into());
        assert_eq!(
            run_save_error(&pool, creator(), selected).await.code,
            "PRICE_LIST_INVALID"
        );
        let mut general = base("commerce-order-guard-0002", "100", 700);
        general.input.price_list_id = Some("list-general".into());
        let receipt = run_save(&pool, creator(), general).await.unwrap();
        let row = sqlx::query(
            "SELECT price_list_id,price_list_version,discount_approval_required FROM commerce_orders WHERE id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("price_list_id"), "list-general");
        assert_eq!(row.get::<i64, _>("discount_approval_required"), 0);

        let mut tax = base("commerce-order-guard-0003", "80", 500);
        tax.input.price_list_id = Some("list-general".into());
        assert_eq!(
            run_save_error(&pool, creator(), tax).await.code,
            "PRICE_LIST_TAX_REQUIRED"
        );
        let mut unknown_product = base("commerce-order-guard-0004", "80", 700);
        unknown_product.input.lines[0].product_id = "missing-product".into();
        assert_eq!(
            run_save_error(&pool, creator(), unknown_product).await.code,
            "PRICE_LIST_ITEM_REQUIRED"
        );
        // November automatically selects a list, so list validation precedes
        // both missing-product and warehouse-required checks.
        let mut unknown_without_list = base("commerce-order-guard-no-list-product", "80", 700);
        unknown_without_list.input.order_date = "2026-12-01".into();
        unknown_without_list.input.due_date = "2026-12-10".into();
        unknown_without_list.input.lines[0].product_id = "missing-product".into();
        assert_eq!(
            run_save_error(&pool, creator(), unknown_without_list).await.code,
            "PRODUCT_INVALID"
        );
        let mut stock = base("commerce-order-guard-0005", "50", 700);
        stock.input.warehouse_id = None;
        stock.input.lines[0].product_id = "stock-1".into();
        assert_eq!(
            run_save_error(&pool, creator(), stock).await.code,
            "PRICE_LIST_ITEM_REQUIRED"
        );
        let mut stock_without_list = base("commerce-order-guard-no-list-stock", "50", 700);
        stock_without_list.input.order_date = "2026-12-01".into();
        stock_without_list.input.due_date = "2026-12-10".into();
        stock_without_list.input.warehouse_id = None;
        stock_without_list.input.lines[0].product_id = "stock-1".into();
        assert_eq!(
            run_save_error(&pool, creator(), stock_without_list).await.code,
            "WAREHOUSE_REQUIRED"
        );
        let mut wrong_warehouse = base("commerce-order-guard-0006", "80", 700);
        wrong_warehouse.input.warehouse_id = Some("missing-warehouse".into());
        assert_eq!(
            run_save_error(&pool, creator(), wrong_warehouse).await.code,
            "WAREHOUSE_INVALID"
        );
        let mut wrong_party = base("commerce-order-guard-0007", "80", 700);
        wrong_party.input.party_id = "party-f".into();
        assert_eq!(
            run_save_error(&pool, creator(), wrong_party).await.code,
            "CONFLICT"
        );
        let mut dates = base("commerce-order-guard-0008", "80", 700);
        dates.input.due_date = "2026-11-01".into();
        assert_eq!(
            run_save_error(&pool, creator(), dates).await.code,
            "INVALID_DATES"
        );
        let mut duplicate = base("commerce-order-guard-0009", "80", 700);
        duplicate.input.lines.push(duplicate.input.lines[0].clone());
        assert_eq!(
            run_save_error(&pool, creator(), duplicate).await.code,
            "DUPLICATE_PRODUCT"
        );
        let mut no_sales = base("commerce-order-guard-0010", "80", 700);
        no_sales.input.expected_version = None;
        let read_only = actor(
            "creator",
            "sales_operator",
            "assignment-creator",
            Some("branch"),
            &["commerce.read"],
        );
        assert_eq!(
            run_save_error(&pool, read_only, no_sales).await.code,
            "COMMERCE_PERMISSION_REQUIRED"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn save_outcome_and_finance_money_snapshots_share_one_unit_and_replay_skips_money() {
    use erpx_server_commerce::public::order::OrderKind;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-order-money-0001";
        let mut command = save_command(
            key,
            None,
            OrderKind::Quote,
            "party-1",
            Some("warehouse-1"),
            "2026-11-10",
            "2026-11-15",
            "80",
            700,
            Some("Strategic launch order approved for commercial review"),
        );
        command.input.note = "Money handoff".into();
        let receipt = run_save_with_money(&pool, creator(), command.clone())
            .await
            .unwrap();
        assert_eq!(receipt.version, 1);
        let total = sqlx::query(
            "SELECT amount_units::text amount_units,usd_units::text usd_units,fx_rate_id,\
             currency_scale::int currency_scale FROM money_transaction_snapshots \
             WHERE source_type='commerce_order_revision' AND source_id=$1 AND amount_kind='total'",
        )
        .bind(format!("{}:1", receipt.id))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(total.get::<String, _>("amount_units"), "8560");
        assert_eq!(total.get::<String, _>("usd_units"), "245");
        assert_eq!(total.get::<String, _>("fx_rate_id"), "rate-thb");
        assert_eq!(total.get::<i32, _>("currency_scale"), 2);
        let lines: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT source_id,amount_kind,amount_units::text FROM money_transaction_snapshots \
             WHERE source_type='commerce_order_line_revision' ORDER BY amount_kind",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            [
                (
                    format!("{}-line-0:1", receipt.id),
                    "subtotal".into(),
                    "8000".into()
                ),
                (
                    format!("{}-line-0:1", receipt.id),
                    "tax".into(),
                    "560".into()
                ),
                (
                    format!("{}-line-0:1", receipt.id),
                    "unit_price".into(),
                    "8000".into()
                ),
            ]
        );
        // The exact same command replays the receipt and does not duplicate
        // any Finance evidence.
        assert_eq!(
            run_save_with_money(&pool, creator(), command).await.unwrap(),
            receipt
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots \
             WHERE source_id LIKE $1",
        )
        .bind(format!("{}%", receipt.id))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 4);

        let mut update = save_command(
            "commerce-order-money-0002",
            Some(&receipt.id),
            OrderKind::Quote,
            "party-1",
            Some("warehouse-1"),
            "2026-11-11",
            "2026-11-20",
            "90",
            700,
            Some("Reviewed margin after the customer negotiation"),
        );
        update.input.note = "Money handoff revised".into();
        let updated = run_save_with_money(&pool, creator(), update).await.unwrap();
        assert_eq!(updated.version, 2);
        let next: (String, String) = sqlx::query_as(
            "SELECT amount_units::text,usd_units::text FROM money_transaction_snapshots \
             WHERE source_type='commerce_order_revision' AND source_id=$1 AND amount_kind='total'",
        )
        .bind(format!("{}:2", receipt.id))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(next, ("9630".into(), "275".into()));

        // A late deferred failure still rolls the order, receipt and Finance
        // rows back together.
        let atomic = save_command(
            "commerce-order-money-atomic-0001",
            None,
            OrderKind::Quote,
            "party-1",
            Some("warehouse-1"),
            "2026-11-10",
            "2026-11-15",
            "80",
            700,
            Some("Strategic launch order approved for commercial review"),
        );
        sqlx::raw_sql(
            "CREATE SEQUENCE commerce_money_lock_calls;\
             CREATE FUNCTION commerce_order_money_atomic_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
             PERFORM setval('commerce_money_lock_calls',(SELECT count(DISTINCT l.relation) FROM pg_locks l JOIN pg_class c ON c.oid=l.relation WHERE l.pid=pg_backend_pid() AND l.granted AND l.locktype='relation' AND c.oid IN ('public.company_currency_scales'::regclass,'public.company_fx_rates'::regclass,'public.money_transaction_snapshots'::regclass)),true); \
             PERFORM 1/0; RETURN NEW; END; $$;\
             CREATE CONSTRAINT TRIGGER commerce_order_money_atomic_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN(NEW.command_id='commerce-order-money-atomic-0001') EXECUTE FUNCTION commerce_order_money_atomic_fault();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert!(
            run_save_with_money(&pool, creator(), atomic.clone())
                .await
                .is_err()
        );
        // The deferred trigger still sees all three Finance relation locks,
        // proving the Money sources stay held through deferred effects.
        let (calls, called): (i64, bool) =
            sqlx::query_as("SELECT last_value,is_called FROM commerce_money_lock_calls")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert!(called && calls == 3, "Finance locks were not retained: {calls}");
        sqlx::raw_sql(
            "DROP TRIGGER commerce_order_money_atomic_fault ON outbox_events; DROP FUNCTION commerce_order_money_atomic_fault(); DROP SEQUENCE commerce_money_lock_calls;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let order_id = format!("order-{}", atomic.key.as_str());
        for (label, count) in [
            (
                "commerce_orders",
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM commerce_orders WHERE id=$1")
                    .bind(&order_id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
            ),
            (
                "money_transaction_snapshots",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM money_transaction_snapshots WHERE source_id LIKE $1",
                )
                .bind(format!("{order_id}%"))
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
        ] {
            assert_eq!(count, 0, "{label}");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn service_completion_writes_fulfillment_receipt_money_and_replay() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "service-completion-0001";
        let command = service_command(
            key,
            "sales-confirmed",
            3,
            "sales-confirmed-line",
            "1",
            "Customer accepted the completed service",
        );
        let outcome = run_service_completion(&pool, reviewer(), command.clone())
            .await
            .unwrap();
        let receipt = outcome.receipt.clone();
        assert!(outcome.created);
        assert_eq!(receipt.id, "sales-confirmed");
        assert_eq!(receipt.fulfillment_id, format!("delivery-{key}"));
        assert_eq!(receipt.version, 4);
        assert_eq!(receipt.state, "fulfilled");
        let row = sqlx::query(
            "SELECT o.state,o.version,l.fulfilled_milli,f.completion_kind,f.completion_note \
             FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id \
             JOIN commerce_order_lines l ON l.id='sales-confirmed-line' \
             WHERE o.id='sales-confirmed' AND f.id=$1",
        )
        .bind(&receipt.fulfillment_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("state"), "fulfilled");
        assert_eq!(row.get::<i64, _>("version"), 4);
        assert_eq!(row.get::<i64, _>("fulfilled_milli"), 1_000);
        assert_eq!(row.get::<String, _>("completion_kind"), "service");
        assert_eq!(
            row.get::<String, _>("completion_note"),
            "Customer accepted the completed service"
        );
        let line = sqlx::query(
            "SELECT quantity_milli,value_minor,subtotal_minor,tax_minor \
             FROM commerce_fulfillment_lines WHERE fulfillment_id=$1",
        )
        .bind(&receipt.fulfillment_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(line.get::<i64, _>("quantity_milli"), 1_000);
        assert_eq!(line.get::<i64, _>("value_minor"), 0);
        assert_eq!(line.get::<i64, _>("subtotal_minor"), 10_000);
        assert_eq!(line.get::<i64, _>("tax_minor"), 700);
        assert_eq!(
            run_service_completion(&pool, reviewer(), command).await.unwrap().receipt,
            receipt
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots \
             WHERE source_type='commerce_fulfillment_line' AND source_id=$1",
        )
        .bind(format!("{}-0", receipt.fulfillment_id))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 3);
        for (table, count) in [
            (
                "command_receipts",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "audit_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM audit_events WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "outbox_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM outbox_events WHERE command_id=$1",
                )
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
        ] {
            assert_eq!(count, 1, "{table}");
        }
        let mut changed = service_command(
            "service-completion-0002",
            "sales-confirmed",
            4,
            "sales-confirmed-line",
            "1",
            "Second completion",
        );
        assert_eq!(
            run_service_completion(&pool, reviewer(), changed.clone())
                .await
                .map_err(business)
                .unwrap_err()
                .code,
            "CONFLICT"
        );
        changed.key = CommandKey::new("service-completion-stock-0001").unwrap();
        changed.order_id = RecordId::new("service-stock-order").unwrap();
        changed.expected_version = 1;
        changed.lines[0].line_id = "service-stock-line".into();
        sqlx::raw_sql(
            "INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version,tracking_mode,item_type) \
             VALUES('service-stock-product','company','STOCK-1','Stock item','each',1000,0,'active',1,'none','stock');\
             INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) \
             VALUES('service-stock-order','company','branch','sales','SO-STOCK','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,1,'creator',1,1);\
             INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) \
             VALUES('service-stock-line','service-stock-order','service-stock-product','Stock',1000,0,1000,0,1000,0,0,'stock');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            run_service_completion(&pool, reviewer(), changed)
                .await
                .map_err(business)
                .unwrap_err()
                .code,
            "SERVICE_COMPLETION_REQUIRED"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn save_replay_conflict_and_late_failure_roll_back_exactly() {
    use erpx_server_commerce::public::order::OrderKind;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let command = save_command(
            "commerce-order-atomic-0001",
            None,
            OrderKind::Sales,
            "party-1",
            Some("warehouse-1"),
            "2026-11-10",
            "2026-11-15",
            "90",
            700,
            Some("Reviewed commercial discount"),
        );
        sqlx::raw_sql(
            "CREATE FUNCTION commerce_order_save_atomic_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM 1/0; RETURN NEW; END; $$;\
             CREATE CONSTRAINT TRIGGER commerce_order_save_atomic_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN(NEW.command_id='commerce-order-atomic-0001') EXECUTE FUNCTION commerce_order_save_atomic_fault();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let failure = run_save(&pool, creator(), command.clone()).await;
        assert!(failure.is_err());
        sqlx::raw_sql(
            "DROP TRIGGER commerce_order_save_atomic_fault ON outbox_events; DROP FUNCTION commerce_order_save_atomic_fault();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let order_id = format!("order-{}", command.key.as_str());
        assert!(
            !sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM commerce_orders WHERE id=$1)"
            )
            .bind(&order_id)
            .fetch_one(pool.raw())
            .await
            .unwrap()
        );
        for (table, count) in [
            (
                "command_receipts",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM command_receipts WHERE command_id=$1",
                )
                .bind(command.key.as_str())
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "audit_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM audit_events WHERE command_id=$1",
                )
                .bind(command.key.as_str())
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
            (
                "outbox_events",
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM outbox_events WHERE command_id=$1",
                )
                .bind(command.key.as_str())
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ),
        ] {
            assert_eq!(count, 0, "{table}");
        }
        let receipt = run_save(&pool, creator(), command.clone()).await.unwrap();
        assert_eq!(receipt.version, 1);
        assert!(sqlx::query_scalar::<_, String>(
            "SELECT number FROM commerce_orders WHERE id=$1"
        )
        .bind(&order_id)
        .fetch_one(pool.raw())
        .await
        .unwrap()
        .starts_with("SO-"));
        let mut changed = command.clone();
        changed.input.note = "A different request".into();
        assert_eq!(
            run_save_error(&pool, creator(), changed).await.code,
            "IDEMPOTENCY_CONFLICT"
        );
    })
    .await;
}

#[tokio::test]
async fn purchase_order_save_uses_supplier_guard_and_po_number_while_transition_stays_retained() {
    use erpx_server_commerce::public::order::{OrderKind, SaveOrderInput, SaveOrderLineInput};
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let purchase = SaveOrderCommand {
            key: CommandKey::new("commerce:purchase:save:1").unwrap(),
            order_id: None,
            input: SaveOrderInput {
                kind: OrderKind::Purchase,
                party_id: "supplier-1".into(),
                warehouse_id: Some("warehouse-1".into()),
                order_date: "2026-11-01".into(),
                due_date: "2026-11-08".into(),
                note: "Supplier order".into(),
                price_list_id: None,
                discount_reason: None,
                lines: vec![SaveOrderLineInput {
                    product_id: "product-stock".into(),
                    quantity: "2".into(),
                    price: "5000".into(),
                    tax_bps: 700,
                }],
                expected_version: None,
            },
        };
        let receipt = run_save(&pool, creator(), purchase.clone()).await.unwrap();
        assert_eq!(receipt.id, "order-commerce:purchase:save:1");
        assert_eq!(receipt.version, 1);
        let detail = run_detail(&pool, creator(), &receipt.id).await.unwrap();
        assert_eq!(detail.order.kind, "purchase");
        assert!(detail.order.number.starts_with("PO-"));
        assert_eq!(detail.order.party_id, "supplier-1");
        assert_eq!(detail.lines.len(), 1);
        assert_eq!(detail.lines[0].product_id, "product-stock");
        let replay = run_save(&pool, creator(), purchase).await.unwrap();
        assert_eq!(replay, receipt);
        let service = SaveOrderCommand {
            key: CommandKey::new("commerce:purchase:service:1").unwrap(),
            order_id: None,
            input: SaveOrderInput {
                kind: OrderKind::Purchase,
                party_id: "supplier-1".into(),
                warehouse_id: Some("warehouse-1".into()),
                order_date: "2026-11-01".into(),
                due_date: "2026-11-08".into(),
                note: String::new(),
                price_list_id: None,
                discount_reason: None,
                lines: vec![SaveOrderLineInput {
                    product_id: "product-1".into(),
                    quantity: "1".into(),
                    price: "5000".into(),
                    tax_bps: 700,
                }],
                expected_version: None,
            },
        };
        assert_eq!(
            run_save_error(&pool, creator(), service).await.code,
            "SERVICE_SUPPLIER_BILL_REQUIRED"
        );
        let customer_party = SaveOrderCommand {
            key: CommandKey::new("commerce:purchase:customer:1").unwrap(),
            order_id: None,
            input: SaveOrderInput {
                kind: OrderKind::Purchase,
                party_id: "party-1".into(),
                warehouse_id: Some("warehouse-1".into()),
                order_date: "2026-11-01".into(),
                due_date: "2026-11-08".into(),
                note: String::new(),
                price_list_id: None,
                discount_reason: None,
                lines: vec![SaveOrderLineInput {
                    product_id: "product-stock".into(),
                    quantity: "1".into(),
                    price: "5000".into(),
                    tax_bps: 700,
                }],
                expected_version: None,
            },
        };
        assert_eq!(
            run_save_error(&pool, creator(), customer_party).await.code,
            "CONFLICT"
        );
        let cancel = command(
            &receipt.id,
            OrderAction::Cancel,
            1,
            "commerce:purchase:cancel:1",
        );
        assert_eq!(
            run_transition(&pool, creator(), cancel)
                .await
                .map_err(business)
                .unwrap_err()
                .code,
            "COMMERCE_ORDER_SOURCE_INVALID"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn read_is_company_and_branch_scoped_and_requires_current_iam() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let document = run_detail(&pool, creator(), "quote-draft").await.unwrap();
        assert_eq!(document.order.id, "quote-draft");
        assert_eq!(document.order.party_id, "party-1");
        assert_eq!(document.order.state, "draft");
        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.lines[0].sku, "SVC-1");

        let error = run_detail(&pool, other_branch(), "quote-draft")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let error = run_detail(&pool, creator(), "other-branch-quote")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let error = run_detail(&pool, creator(), "foreign-quote")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "ORDER_NOT_FOUND");
        let purchase = run_detail(&pool, creator(), "purchase-draft")
            .await
            .unwrap();
        assert_eq!(purchase.order.kind, "purchase");
        assert_eq!(purchase.order.number, "PO-1");
        assert_eq!(purchase.lines.len(), 1);

        let revoked = actor(
            "revoked",
            "sales_operator",
            "assignment-revoked",
            Some("branch"),
            &["commerce.read", "commerce.sales"],
        );
        let error = run_detail(&pool, revoked, "quote-draft")
            .await
            .map_err(business)
            .unwrap_err();
        assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");

        let foreign_document = run_detail(&pool, foreign(), "foreign-quote").await.unwrap();
        assert_eq!(foreign_document.order.company_id, "foreign");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_persists_exact_receipt_audit_outbox_and_replays() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-order-submit-0001";
        let receipt = run_transition(
            &pool,
            creator(),
            command("quote-draft", OrderAction::Submit, 1, key),
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, "quote-draft");
        assert_eq!(receipt.state, "submitted");
        assert_eq!(receipt.version, 2);
        assert_eq!(receipt.sales_id, None);

        let row = sqlx::query("SELECT state,version,approved_by FROM commerce_orders WHERE id='quote-draft'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "submitted");
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(row.get::<Option<String>, _>("approved_by"), None);

        let receipt_row = sqlx::query(
            "SELECT operation,actor_id,record_id,result_json FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt_row.get::<String, _>("operation"), "commerce.order.submit");
        assert_eq!(receipt_row.get::<String, _>("actor_id"), "creator");
        assert_eq!(receipt_row.get::<String, _>("record_id"), "quote-draft");
        assert_eq!(
            receipt_row.get::<String, _>("result_json"),
            json!({"id":"quote-draft","state":"submitted","version":2,"salesId":null}).to_string()
        );
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
        let payload: String =
            sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE command_id=$1")
                .bind(key)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "submitted");
        assert_eq!(payload["companyId"], "company");
        assert_eq!(payload["branchId"], "branch");
        assert_eq!(payload["version"], 2);
        assert_eq!(payload["salesId"], serde_json::Value::Null);

        let replay = run_transition(
            &pool,
            creator(),
            command("quote-draft", OrderAction::Submit, 1, key),
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),\
             (SELECT count(*) FROM audit_events WHERE command_id=$1),\
             (SELECT count(*) FROM outbox_events WHERE command_id=$1)",
        )
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));

        let error = run_error(
            &pool,
            creator(),
            TransitionCommand {
                reason: Some("different request".into()),
                ..command("quote-draft", OrderAction::Submit, 1, key)
            },
        )
        .await;
        assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_discount_and_cancel_paths_preserve_independence_and_guards() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let error = run_error(
            &pool,
            creator(),
            command(
                "quote-discount",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0001",
            ),
        )
        .await;
        assert_eq!(error.code, "INDEPENDENT_APPROVER_REQUIRED");

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "quote-discount",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "DISCOUNT_APPROVAL_REASON_REQUIRED");

        let receipt = run_transition(
            &pool,
            reviewer(),
            TransitionCommand {
                reason: Some("Independent discount review".into()),
                ..command(
                    "quote-discount",
                    OrderAction::Approve,
                    2,
                    "commerce-order-approve-0003",
                )
            },
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, "confirmed");
        let row = sqlx::query(
            "SELECT state,version,approved_by,discount_approved_by,discount_approved_at \
             FROM commerce_orders WHERE id='quote-discount'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("state"), "confirmed");
        assert_eq!(row.get::<i64, _>("version"), 3);
        assert_eq!(
            row.get::<Option<String>, _>("approved_by").as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            row.get::<Option<String>, _>("discount_approved_by")
                .as_deref(),
            Some("reviewer")
        );
        assert_eq!(row.get::<Option<i64>, _>("discount_approved_at"), Some(NOW));

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "sales-submitted",
                OrderAction::Approve,
                2,
                "commerce-order-approve-0004",
            ),
        )
        .await;
        assert_eq!(error.code, "COMMERCE_SALES_APPROVAL_FINANCE_GUARD");

        let cancelled = run_transition(
            &pool,
            reviewer(),
            command(
                "quote-confirmed",
                OrderAction::Cancel,
                3,
                "commerce-order-cancel-0001",
            ),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.version, 4);

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "quote-confirmed-fulfilled",
                OrderAction::Cancel,
                4,
                "commerce-order-cancel-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "ORDER_REVERSAL_REQUIRED");

        let error = run_error(
            &pool,
            reviewer(),
            command(
                "sales-confirmed",
                OrderAction::Cancel,
                3,
                "commerce-order-cancel-0003",
            ),
        )
        .await;
        assert_eq!(error.code, "ORDER_REVERSAL_REQUIRED");

        let cancelled = run_transition(
            &pool,
            creator(),
            command(
                "sales-draft",
                OrderAction::Cancel,
                1,
                "commerce-order-cancel-0004",
            ),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "cancelled");
        assert_eq!(cancelled.version, 2);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn convert_copies_the_quote_into_one_draft_sales_order() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-order-convert-0001";
        let receipt = run_transition(
            &pool,
            creator(),
            command("quote-confirmed", OrderAction::Convert, 3, key),
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, "quote-confirmed");
        assert_eq!(receipt.state, "converted");
        assert_eq!(receipt.version, 4);
        assert_eq!(
            receipt.sales_id.as_deref(),
            Some("order-commerce-order-convert-0001")
        );

        let sales_id = receipt.sales_id.clone().unwrap();
        let quote =
            sqlx::query("SELECT state,version FROM commerce_orders WHERE id='quote-confirmed'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(quote.get::<String, _>("state"), "converted");
        assert_eq!(quote.get::<i64, _>("version"), 4);

        let sales = sqlx::query(
            "SELECT company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,\
             due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,\
             created_at,updated_at FROM commerce_orders WHERE id=$1",
        )
        .bind(&sales_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sales.get::<String, _>("company_id"), "company");
        assert_eq!(sales.get::<String, _>("branch_id"), "branch");
        assert_eq!(sales.get::<String, _>("kind"), "sales");
        assert!(sales.get::<String, _>("number").starts_with("SO-"));
        assert_eq!(sales.get::<String, _>("party_id"), "party-1");
        assert_eq!(sales.get::<String, _>("warehouse_id"), "warehouse-1");
        assert_eq!(sales.get::<String, _>("state"), "draft");
        assert_eq!(sales.get::<i64, _>("version"), 1);
        assert_eq!(sales.get::<String, _>("source_id"), "quote-confirmed");
        assert_eq!(sales.get::<String, _>("created_by"), "creator");
        assert_eq!(sales.get::<Option<String>, _>("approved_by"), None);
        assert_eq!(sales.get::<i64, _>("created_at"), NOW);
        assert_eq!(sales.get::<i64, _>("updated_at"), NOW);

        let lines: Vec<(String, i64, i64, String)> = sqlx::query_as(
            "SELECT id,quantity_milli,subtotal_minor,item_type FROM commerce_order_lines \
             WHERE order_id=$1 ORDER BY ordinal",
        )
        .bind(&sales_id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![(format!("{sales_id}-0"), 1000, 10000, "service".into())]
        );

        let replay = run_transition(
            &pool,
            creator(),
            command("quote-confirmed", OrderAction::Convert, 3, key),
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM commerce_orders WHERE source_id='quote-confirmed'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);

        let error = run_error(
            &pool,
            creator(),
            command(
                "quote-confirmed",
                OrderAction::Convert,
                3,
                "commerce-order-convert-0002",
            ),
        )
        .await;
        assert_eq!(error.code, "CONFLICT");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn late_outbox_failure_rolls_back_and_exact_retry_persists_once() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "CREATE FUNCTION fail_commerce_order_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF NEW.command_id='commerce-order-rollback-0001' THEN \
             RAISE EXCEPTION 'blocked outbox'; END IF; RETURN NEW; END $$; \
             CREATE TRIGGER fail_commerce_order_outbox BEFORE INSERT ON outbox_events \
             FOR EACH ROW EXECUTE FUNCTION fail_commerce_order_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let result = run_transition(
            &pool,
            creator(),
            command("sales-draft", OrderAction::Submit, 1, "commerce-order-rollback-0001"),
        )
        .await;
        assert!(result.is_err());
        let row = sqlx::query("SELECT state,version FROM commerce_orders WHERE id='sales-draft'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "draft");
        assert_eq!(row.get::<i64, _>("version"), 1);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='commerce-order-rollback-0001'),\
             (SELECT count(*) FROM audit_events WHERE command_id='commerce-order-rollback-0001'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='commerce-order-rollback-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));

        sqlx::raw_sql(
            "DROP TRIGGER fail_commerce_order_outbox ON outbox_events; \
             DROP FUNCTION fail_commerce_order_outbox();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let receipt = run_transition(
            &pool,
            creator(),
            command("sales-draft", OrderAction::Submit, 1, "commerce-order-rollback-0001"),
        )
        .await
        .unwrap();
        assert_eq!(receipt.state, "submitted");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='commerce-order-rollback-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn transaction_and_serializable_guards_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut repository = PgCommerceOrders::new(&mut connection);
        let error = CommerceOrderReadRepository::order(&mut repository, &creator(), "quote-draft")
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("COMMERCE_ORDER_TRANSACTION_REQUIRED")
            ),
            "{error:?}"
        );

        let mut transaction = pool.raw().begin().await.unwrap();
        let mut repository = PgCommerceOrders::new(&mut transaction);
        let error = CommerceOrderReadRepository::order(&mut repository, &creator(), "quote-draft")
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                PgTransactionError::Persistence(ref value)
                    if value.to_string().contains("COMMERCE_ORDER_SERIALIZABLE_REQUIRED")
            ),
            "{error:?}"
        );
        transaction.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn native_conversion_without_finance_money_port_fails_and_rolls_back() {
    use erpx_server_commerce::public::order::transition_order_guarded;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let key = "commerce-conversion-missing-finance-0001";
        let before: (String, i64) = sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='quote-confirmed'").fetch_one(pool.raw()).await.unwrap();
        let before_lines: Vec<(String,i64,i64)> = sqlx::query_as("SELECT id,subtotal_minor,tax_minor FROM commerce_order_lines WHERE order_id='quote-confirmed' ORDER BY ordinal").fetch_all(pool.raw()).await.unwrap();
        let command = command("quote-confirmed", OrderAction::Convert, 3, key);
        let result = PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let command=command.clone();
            Box::pin(async move {
                let mut repository=PgCommerceOrders::new(connection);
                transition_order_guarded(&creator(),&command,&mut repository,TimestampMillis::new(NOW)).await
            })
        }).await;
        assert_eq!(business(result.unwrap_err()).code,"COMMERCE_CONVERSION_MONEY_REQUIRED");
        let after:(String,i64)=sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='quote-confirmed'").fetch_one(pool.raw()).await.unwrap();assert_eq!(after,before);
        let after_lines:Vec<(String,i64,i64)>=sqlx::query_as("SELECT id,subtotal_minor,tax_minor FROM commerce_order_lines WHERE order_id='quote-confirmed' ORDER BY ordinal").fetch_all(pool.raw()).await.unwrap();assert_eq!(after_lines,before_lines);
        let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_orders WHERE source_id='quote-confirmed'),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id LIKE 'order-commerce-conversion-missing-finance-0001%')").bind(key).fetch_one(pool.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
    }).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn conversion_preflight_preserves_actor_replay_and_quote_precedence() {
    use erpx_server_commerce::public::order::transition_order_guarded;
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        for (actor,order,version,key,code) in [
            ({let mut a=creator();a.permissions.clear();a},"quote-confirmed",99,"conversion-preflight-no-auth-0001","COMMERCE_PERMISSION_REQUIRED"),
            (creator(),"sales-draft",99,"conversion-preflight-not-quote-0001","QUOTE_REQUIRED"),
            (creator(),"quote-confirmed",99,"conversion-preflight-stale-0001","COMMERCE_CONVERSION_MONEY_REQUIRED"),
        ] {
            let input=command(order,OrderAction::Convert,version,key);
            let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
                let input=input.clone();let actor=actor.clone();Box::pin(async move {let mut repo=PgCommerceOrders::new(connection);transition_order_guarded(&actor,&input,&mut repo,TimestampMillis::new(NOW)).await})
            }).await;
            assert_eq!(business(result.unwrap_err()).code,code,"{key}");
            let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_orders WHERE source_id=$2),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(key).bind(order).fetch_one(pool.raw()).await.unwrap();assert_eq!(effects,(0,0,0,0));
        }
        let replay_key="conversion-preflight-existing-receipt-0001";
        let cmd=command("quote-confirmed",OrderAction::Convert,3,replay_key);
        let original=run_transition(&pool,creator(),cmd.clone()).await.unwrap();
        let replay=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
            let cmd=cmd.clone();Box::pin(async move {let mut repo=PgCommerceOrders::new(connection);transition_order_guarded(&creator(),&cmd,&mut repo,TimestampMillis::new(NOW)).await})
        }).await.unwrap();
        assert_eq!(replay,original);
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE source_id='quote-confirmed'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,1);
    }).await;
}

mod composed_goods_receipt {
    //! Composed purchase goods receipt: Commerce fulfillment + functional
    //! valuation + `commerce_goods_valuations`, Inventory stock/lot/Quality hold
    //! and the Finance goods journal on one caller-owned serializable unit.
    use super::*;
    use erpx_server_commerce::public::receipt::{
        CommerceReceipt, CommerceReceiptGoodsValuation, CommerceReceiptRepository,
        GoodsJournalRequest, GoodsValuation, GoodsValuationRequest, PgCommerceReceipt,
        ReceiptAllocationCommand, ReceiptCommand, ReceiptDisposition, ReceiptLineCommand,
        receive_purchase_order_with_goods_valuation,
    };
    use erpx_server_finance::public::FinanceActor;
    use erpx_server_finance::public::commerce_goods_journal::{
        CommerceGoodsJournalPosting, CommerceGoodsKind, CommerceGoodsValuation,
        CommerceGoodsValuationRequest, post_commerce_goods_journal,
        resolve_commerce_goods_valuation,
    };
    use erpx_server_inventory::public::commerce_receipt::InventoryStockReceipt;
    use erpx_server_runtime::public::postgres::{PgWorkFuture, TransactionOptions};
    use erpx_server_shared::public::{Clock, FailureKind};
    use sqlx::PgConnection;
    use std::sync::Arc;

    const RECEIPT_NOW: i64 = 1_800_000_000_000;

    #[derive(Clone, Copy)]
    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(RECEIPT_NOW)
        }
    }

    /// Same binding the composition root mounts: resolve through the Finance
    /// public seam, then post the draft goods journal on the caller's unit.
    #[derive(Default)]
    struct TestGoodsValuation {
        clock: Option<Arc<FixedClock>>,
        resolved: Option<CommerceGoodsValuation>,
    }

    impl TestGoodsValuation {
        fn new() -> Self {
            Self {
                clock: Some(Arc::new(FixedClock)),
                resolved: None,
            }
        }
    }

    fn finance_actor(request: &GoodsValuationRequest) -> FinanceActor {
        FinanceActor {
            user_id: UserId::new(&request.user_id).unwrap(),
            holding_id: HoldingId::new(&request.holding_id).unwrap(),
            company_id: CompanyId::new(&request.company_id).unwrap(),
            assignment_id: AssignmentId::new(&request.assignment_id).unwrap(),
        }
    }

    fn unavailable() -> BusinessError {
        BusinessError::new(
            FailureKind::Unavailable,
            "GOODS_FINANCE_VALUATION_REQUIRED",
            "The Finance goods valuation is unavailable.",
        )
    }

    impl CommerceReceiptGoodsValuation for TestGoodsValuation {
        fn lock<'a>(&'a mut self, _c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
            Box::pin(async move { Ok(()) })
        }

        fn resolve<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            request: &'a GoodsValuationRequest,
        ) -> PgWorkFuture<'a, GoodsValuation, BusinessError> {
            Box::pin(async move {
                let resolved = resolve_commerce_goods_valuation(
                    c,
                    &CommerceGoodsValuationRequest {
                        actor: finance_actor(request),
                        chart_id: request.chart_id.clone(),
                        business_date: request.business_date.clone(),
                        document_currency: request.document_currency.clone(),
                    },
                )
                .await?;
                let valuation = GoodsValuation {
                    stock_currency: resolved.stock_currency().to_owned(),
                    fx_rate_micros: resolved.fx_rate_micros(),
                    source_rate_id: resolved.fx().source_rate_id.clone(),
                    target_rate_id: resolved.fx().target_rate_id.clone(),
                    used_latest_fallback: resolved.fx().used_latest_fallback,
                };
                self.resolved = Some(resolved);
                Ok(valuation)
            })
        }

        fn stock_value(
            &self,
            valuation: &GoodsValuation,
            document_minor: i64,
        ) -> Result<i64, BusinessError> {
            self.resolved
                .as_ref()
                .filter(|resolved| {
                    resolved.stock_currency() == valuation.stock_currency
                        && resolved.fx_rate_micros() == valuation.fx_rate_micros
                })
                .ok_or_else(unavailable)?
                .stock_value_minor(document_minor)
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })
        }

        fn post<'a>(
            &'a mut self,
            c: &'a mut PgConnection,
            request: &'a GoodsJournalRequest,
            valuation: &'a GoodsValuation,
        ) -> PgWorkFuture<'a, String, BusinessError> {
            Box::pin(async move {
                if self.resolved.as_ref().is_none_or(|resolved| {
                    resolved.stock_currency() != valuation.stock_currency
                        || resolved.fx_rate_micros() != valuation.fx_rate_micros
                }) {
                    return Err(unavailable().into());
                }
                let clock = self.clock.clone().unwrap();
                Ok(post_commerce_goods_journal(
                    c,
                    clock.as_ref(),
                    &CommerceGoodsJournalPosting {
                        journal_id: request.journal_id.clone(),
                        actor: finance_actor(&GoodsValuationRequest {
                            company_id: request.company_id.clone(),
                            user_id: request.user_id.clone(),
                            holding_id: request.holding_id.clone(),
                            assignment_id: request.assignment_id.clone(),
                            chart_id: request.chart_id.clone(),
                            business_date: request.business_date.clone(),
                            document_currency: request.document_currency.clone(),
                        }),
                        chart_id: request.chart_id.clone(),
                        business_date: request.business_date.clone(),
                        fulfillment_id: request.fulfillment_id.clone(),
                        memo: request.memo.clone(),
                        inventory_account_id: request.inventory_account_id.clone(),
                        kind: CommerceGoodsKind::Receipt {
                            clearing_account_id: request.clearing_account_id.clone(),
                        },
                        document_currency: request.document_currency.clone(),
                        document_subtotal_minor: request.document_subtotal_minor,
                        stock_value_minor: request.stock_value_minor,
                        fx_rate_micros: Some(request.fx_rate_micros),
                    },
                )
                .await?)
            })
        }
    }

    fn receipt_actor() -> CommerceOrderActor {
        CommerceOrderActor {
            user_id: UserId::new("receiver").unwrap(),
            person_user_ids: vec![],
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
            expires_at: 2_000_000_000_000,
        }
    }

    fn receipt_command(key: &str, order: &str, version: i64, date: &str) -> ReceiptCommand {
        ReceiptCommand {
            key: CommandKey::new(key).unwrap(),
            order_id: RecordId::new(order).unwrap(),
            expected_version: version,
            date: date.into(),
            lines: vec![
                ReceiptLineCommand {
                    line_id: "line-plain".into(),
                    quantity_milli: 1_000,
                    allocations: vec![],
                },
                ReceiptLineCommand {
                    line_id: "line-lot".into(),
                    quantity_milli: 1_000,
                    allocations: vec![ReceiptAllocationCommand {
                        lot_code: Some("L-A".into()),
                        serial_number: None,
                        quantity_milli: 1_000,
                        disposition: ReceiptDisposition::Quarantine,
                        quarantine_reason: Some("Damaged carton".into()),
                        quarantine_due_date: Some("2026-11-20".into()),
                        manufactured_date: None,
                        expiry_date: None,
                    }],
                },
            ],
        }
    }

    fn receipt_run(
        pool: &DatabasePool,
        actor: CommerceOrderActor,
        command: ReceiptCommand,
    ) -> impl std::future::Future<Output = Result<CommerceReceipt, PgTransactionError<BusinessError>>>
    {
        let pool = pool.clone();
        async move {
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let actor = actor.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        let mut repository = PgCommerceReceipt;
                        let mut stock = InventoryStockReceipt::production();
                        let mut finance = TestGoodsValuation::new();
                        receive_purchase_order_with_goods_valuation(
                            &actor,
                            &command,
                            &mut repository,
                            &mut stock,
                            &mut finance,
                            connection,
                            TimestampMillis::new(RECEIPT_NOW),
                        )
                        .await
                    })
                })
                .await
        }
    }

    fn receipt_code(error: PgTransactionError<BusinessError>) -> String {
        match error {
            PgTransactionError::Business(error) => error.code.to_owned(),
            PgTransactionError::Persistence(error) => error.to_string(),
        }
    }

    /// Adds the receipt-specific Finance period, warehouse-manager authority and
    /// purchase orders to the shared Commerce fixture.
    async fn receipt_seed(pool: &DatabasePool) {
        sqlx::raw_sql(
            "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
             ('company','receiver','local:receiver','active','local',1);\
             INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
             ('assignment-receiver','receiver','warehouse_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual');\
             INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,created_at) VALUES \
             ('period-2026-11','company','November 2026','2026-11-01','2026-11-30','open',0,0);\
             INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode,item_type,status,version) VALUES \
             ('product-lot','company','STK-L','Lot product',1000,'lot','stock','active',1);\
             INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,state,version,created_by,created_at,updated_at) VALUES \
             ('po-receipt','company','branch','purchase','PO-R','supplier-1','warehouse-1','THB','2026-11-01','2026-11-10',4500,0,'confirmed',3,'creator',0,0),\
             ('po-receipt-usd','company','branch','purchase','PO-RU','supplier-1','warehouse-1','USD','2026-11-01','2026-11-10',9999,0,'confirmed',1,'creator',0,0),\
             ('po-receipt-foreign','foreign','foreign-branch','purchase','PO-RF','party-f',NULL,'THB','2026-11-01','2026-11-10',1000,0,'confirmed',1,'foreign-user',0,0);\
             INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal,item_type) VALUES \
             ('line-plain','po-receipt','product-stock','Plain product',1000,0,2500,0,2500,0,0,'stock'),\
             ('line-lot','po-receipt','product-lot','Lot product',2000,0,1000,0,2000,0,1,'stock'),\
             ('line-usd','po-receipt-usd','product-stock','USD product',3000,0,3333,0,9999,0,0,'stock'),\
             ('line-foreign','po-receipt-foreign','product-stock','Foreign product',1000,0,1000,0,1000,0,0,'stock');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_receipt_posts_valuation_journal_stock_and_quality_hold() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            receipt_seed(&pool).await;
            let receipt = receipt_run(
                &pool,
                receipt_actor(),
                receipt_command("receipt-goods-one", "po-receipt", 3, "2026-11-05"),
            )
            .await
            .unwrap();
            assert_eq!(receipt.fulfillment_id, "delivery-receipt-goods-one");
            assert_eq!(receipt.state, "partially_fulfilled");
            assert_eq!(receipt.version, 4);
            assert_eq!(receipt.total_value_minor, 3_500);
            assert_eq!(
                receipt
                    .lines
                    .iter()
                    .map(|line| (line.line_id.as_str(), line.value_minor))
                    .collect::<Vec<_>>(),
                vec![("line-plain", 2_500), ("line-lot", 1_000)]
            );

            let journal: (String, Option<i64>, String, String, String) = sqlx::query_as(
                "SELECT currency,fx_rate_micros,status,source_type,source_id \
                 FROM finance_journals WHERE id='journal-receipt-goods-one' AND company_id='company'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                journal,
                (
                    "THB".into(),
                    Some(1_000_000),
                    "draft".into(),
                    "goods_receipt".into(),
                    "delivery-receipt-goods-one".into()
                )
            );
            let lines: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
                "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor \
                 FROM finance_journal_lines WHERE journal_id='journal-receipt-goods-one' ORDER BY ordinal",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                lines,
                vec![
                    ("coa-4".into(), 3_500, 0, 3_500, 0),
                    ("coa-5".into(), 0, 3_500, 0, 3_500),
                ]
            );

            let link: String = sqlx::query_scalar(
                "SELECT journal_id FROM commerce_fulfillments WHERE id='delivery-receipt-goods-one'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(link, "journal-receipt-goods-one");
            let valuation: (String, String, String, String, i64, i64, i64, Option<String>, bool) =
                sqlx::query_as(
                    "SELECT chart_id,document_currency,journal_currency,stock_currency,\
                     fx_rate_micros,document_subtotal_minor,stock_value_minor,source_rate_id,\
                     used_latest_fallback FROM commerce_goods_valuations \
                     WHERE fulfillment_id='delivery-receipt-goods-one' AND company_id='company'",
                )
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                valuation,
                (
                    "chart-1".into(),
                    "THB".into(),
                    "THB".into(),
                    "THB".into(),
                    1_000_000,
                    3_500,
                    3_500,
                    None,
                    false
                )
            );
            let line_currencies: Vec<(String, String, i64, i64)> = sqlx::query_as(
                "SELECT order_line_id,stock_currency,value_minor,subtotal_minor \
                 FROM commerce_fulfillment_lines WHERE fulfillment_id='delivery-receipt-goods-one' \
                 ORDER BY id",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                line_currencies,
                vec![
                    ("line-plain".into(), "THB".into(), 2_500, 2_500),
                    ("line-lot".into(), "THB".into(), 1_000, 1_000),
                ]
            );

            let stock: Vec<(String, i64, i64)> = sqlx::query_as(
                "SELECT product_id,quantity_milli,value_minor FROM commerce_stock \
                 WHERE warehouse_id='warehouse-1' ORDER BY product_id",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                stock,
                vec![
                    ("product-lot".into(), 1_000, 1_000),
                    ("product-stock".into(), 1_000, 2_500),
                ]
            );
            let holds: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT held_quantity_milli,reason,status FROM inventory_quality_holds \
                 WHERE source_type='goods_receipt' AND company_id='company'",
            )
            .fetch_all(pool.raw())
            .await
            .unwrap();
            assert_eq!(holds, vec![(1_000, "Damaged carton".into(), "held".into())]);
            let order: (String, i64) =
                sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='po-receipt'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(order, ("partially_fulfilled".into(), 4));
            let effects: (i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='receipt-goods-one'),\
                 (SELECT count(*) FROM audit_events WHERE command_id='receipt-goods-one'),\
                 (SELECT count(*) FROM outbox_events WHERE command_id='receipt-goods-one')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (1, 1, 1));
            let result_json: String = sqlx::query_scalar(
                "SELECT result_json FROM command_receipts WHERE command_id='receipt-goods-one'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&result_json).unwrap(),
                json!({
                    "orderId": "po-receipt",
                    "fulfillmentId": "delivery-receipt-goods-one",
                    "state": "partially_fulfilled",
                    "version": 4,
                    "totalValueMinor": 3500,
                    "lines": [
                        {"lineId": "line-plain", "quantityMilli": 1000, "valueMinor": 2500},
                        {"lineId": "line-lot", "quantityMilli": 1000, "valueMinor": 1000},
                    ],
                })
            );
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_receipt_replays_without_duplicate_stock_journal_or_hold() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            receipt_seed(&pool).await;
            let input = receipt_command("receipt-goods-replay", "po-receipt", 3, "2026-11-05");
            let first = receipt_run(&pool, receipt_actor(), input.clone())
                .await
                .unwrap();
            let second = receipt_run(&pool, receipt_actor(), input.clone())
                .await
                .unwrap();
            assert_eq!(first, second);
            let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='po-receipt'),\
                 (SELECT count(*) FROM finance_journals WHERE source_type='goods_receipt'),\
                 (SELECT count(*) FROM commerce_goods_valuations),\
                 (SELECT count(*) FROM inventory_quality_holds WHERE source_type='goods_receipt'),\
                 (SELECT count(*) FROM command_receipts WHERE command_id='receipt-goods-replay'),\
                 (SELECT quantity_milli FROM commerce_stock WHERE product_id='product-stock' AND warehouse_id='warehouse-1')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(counts, (1, 1, 1, 1, 1, 1_000));

            let mut changed = input.clone();
            changed.lines[0].quantity_milli = 500;
            let error = receipt_run(&pool, receipt_actor(), changed)
                .await
                .unwrap_err();
            assert_eq!(receipt_code(error), "IDEMPOTENCY_CONFLICT");
            let quantity: i64 = sqlx::query_scalar(
                "SELECT quantity_milli FROM commerce_stock WHERE product_id='product-stock' AND warehouse_id='warehouse-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(quantity, 1_000);
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn foreign_currency_receipt_freezes_functional_value_and_journal() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            receipt_seed(&pool).await;
            let input = ReceiptCommand {
                key: CommandKey::new("receipt-goods-fx").unwrap(),
                order_id: RecordId::new("po-receipt-usd").unwrap(),
                expected_version: 1,
                date: "2026-11-06".into(),
                lines: vec![ReceiptLineCommand {
                    line_id: "line-usd".into(),
                    quantity_milli: 3_000,
                    allocations: vec![],
                }],
            };
            let receipt = receipt_run(&pool, receipt_actor(), input).await.unwrap();
            // The fixture Company quote is 35 THB per USD: 9,999 document minor.
            assert_eq!(receipt.total_value_minor, 349_965);
            let journal: (String, Option<i64>, String, i64, i64) = sqlx::query_as(
                "SELECT currency,fx_rate_micros,status,\
                 (SELECT debit_minor FROM finance_journal_lines l WHERE l.journal_id=j.id ORDER BY ordinal LIMIT 1),\
                 (SELECT base_debit_minor FROM finance_journal_lines l WHERE l.journal_id=j.id ORDER BY ordinal LIMIT 1) \
                 FROM finance_journals j WHERE id='journal-receipt-goods-fx'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(
                journal,
                ("USD".into(), Some(35_000_000), "draft".into(), 9_999, 349_965)
            );
            let valuation: (String, String, String, i64, i64, i64, Option<String>, Option<String>) =
                sqlx::query_as(
                    "SELECT document_currency,journal_currency,stock_currency,fx_rate_micros,\
                     document_subtotal_minor,stock_value_minor,source_rate_id,target_rate_id \
                     FROM commerce_goods_valuations WHERE fulfillment_id='delivery-receipt-goods-fx'",
                )
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(
                valuation,
                (
                    "USD".into(),
                    "USD".into(),
                    "THB".into(),
                    35_000_000,
                    9_999,
                    349_965,
                    None,
                    Some("rate-thb".into())
                )
            );
            let line: (i64, i64, String) = sqlx::query_as(
                "SELECT value_minor,subtotal_minor,stock_currency FROM commerce_fulfillment_lines \
                 WHERE fulfillment_id='delivery-receipt-goods-fx'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(line, (349_965, 9_999, "THB".into()));
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn foreign_and_missing_source_receipts_fail_closed_without_effects() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            receipt_seed(&pool).await;
            let mut foreign = receipt_actor();
            foreign.company_id = CompanyId::new("foreign").unwrap();
            foreign.branch_id = Some(RecordId::new("foreign-branch").unwrap());
            foreign.assignment_id = AssignmentId::new("assignment-foreign").unwrap();
            let error = receipt_run(
                &pool,
                foreign,
                receipt_command("receipt-goods-foreign", "po-receipt-foreign", 1, "2026-11-05"),
            )
            .await
            .unwrap_err();
            assert_eq!(receipt_code(error), "COMMERCE_PERMISSION_REQUIRED");
            // Company isolation is owner-side: the receipt source read never
            // returns another Company's purchase order, whatever the label.
            let mut isolated = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *isolated)
                .await
                .unwrap();
            let mut repository = PgCommerceReceipt;
            assert!(repository
                .order_source(&mut isolated, &receipt_actor(), "po-receipt-foreign")
                .await
                .unwrap()
                .is_none());
            isolated.rollback().await.unwrap();

            let mut stale = receipt_command("receipt-goods-stale", "po-receipt", 99, "2026-11-05");
            stale.lines.truncate(1);
            let error = receipt_run(&pool, receipt_actor(), stale).await.unwrap_err();
            assert_eq!(receipt_code(error), "COMMERCE_ORDER_CHANGED");

            sqlx::query("DELETE FROM commerce_settings WHERE company_id='company'")
                .execute(pool.raw())
                .await
                .unwrap();
            let mut unconfigured =
                receipt_command("receipt-goods-unconfigured", "po-receipt", 3, "2026-11-05");
            unconfigured.lines.truncate(1);
            let error = receipt_run(&pool, receipt_actor(), unconfigured)
                .await
                .unwrap_err();
            assert_eq!(receipt_code(error), "COMMERCE_SETUP_REQUIRED");

            let effects: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='po-receipt'),\
                 (SELECT count(*) FROM commerce_fulfillment_lines WHERE fulfillment_id LIKE 'delivery-receipt-goods-%'),\
                 (SELECT count(*) FROM commerce_stock_moves WHERE fulfillment_id LIKE 'delivery-receipt-goods-%'),\
                 (SELECT count(*) FROM finance_journals),\
                 (SELECT count(*) FROM commerce_goods_valuations),\
                 (SELECT count(*) FROM command_receipts),\
                 (SELECT count(*) FROM outbox_events)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0, 0, 0, 0, 0));
            let order: (String, i64) =
                sqlx::query_as("SELECT state,version FROM commerce_orders WHERE id='po-receipt'")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(order, ("confirmed".into(), 3));
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closed_period_rejects_the_receipt_without_stock_or_receipt_rows() {
        fixture::with_database(4, |pool| async move {
            setup(&pool).await;
            receipt_seed(&pool).await;
            let mut input = receipt_command("receipt-goods-closed", "po-receipt", 3, "2026-12-15");
            input.lines.truncate(1);
            let error = receipt_run(&pool, receipt_actor(), input)
                .await
                .unwrap_err();
            assert_eq!(receipt_code(error), "GOODS_FINANCE_PERIOD_CLOSED");
            let effects: (i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM commerce_fulfillments WHERE order_id='po-receipt'),\
                 (SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse-1' AND product_id='product-stock'),\
                 (SELECT count(*) FROM finance_journals),\
                 (SELECT count(*) FROM commerce_goods_valuations),\
                 (SELECT count(*) FROM command_receipts)",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0, 0, 0));
        })
        .await;
    }
}
