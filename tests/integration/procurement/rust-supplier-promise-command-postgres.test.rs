//! Native Procurement supplier promise command on migrated PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_procurement::public::supplier_promise_command::{
    PgSupplierPromiseCommand, RecordSupplierPromiseCommand, SupplierPromiseCommandActor,
    SupplierPromiseCommandRepository, SupplierPromiseLineCommand, SupplierPromiseRecordReceipt,
    record_supplier_promise, inspect_supplier_promise_history, lock_supplier_promise_history_writes,
};
use erpx_server_procurement::public::supplier_promise_command_http::record_supplier_promise_in_transaction;
use sqlx::Connection;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

#[tokio::test]
async fn promise_history_witness_is_scoped_and_transaction_bound() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let foreign = CompanyId::new("foreign").unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        let error = inspect_supplier_promise_history(&mut tx, &company, "order-owned", "supplier-promise-history-test").await.err().unwrap();
        assert!(matches!(error, PgTransactionError::Persistence(ref e) if e.code == "PROCUREMENT_PROMISE_HISTORY_LOCKS_REQUIRED"));
        tx.rollback().await.unwrap();
        let mut tx = connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_supplier_promise_history_writes(&mut tx).await.unwrap();
        let before = inspect_supplier_promise_history(&mut tx, &company, "order-owned", "supplier-promise-history-test").await.unwrap();
        assert!(before.image().is_none());
        let mut repository = PgSupplierPromiseCommand::new(&mut tx);
        record_supplier_promise(&actor("company", "branch", "owner"), &mut repository, "order-owned", &input(), "history-test", NOW).await.unwrap();
        assert!(before.require_current(&mut tx).await.is_err());
        let current = inspect_supplier_promise_history(&mut tx, &company, "order-owned", "supplier-promise-history-test").await.unwrap();
        let image = current.image().unwrap();
        assert_eq!(image["promise"]["supplier_reference"], "SUP-REF-1");
        assert_eq!(image["lines"][0]["order_line_id"], "order-line-a");
        assert!(inspect_supplier_promise_history(&mut tx, &foreign, "order-owned", "supplier-promise-history-test").await.unwrap().image().is_none());
        assert!(inspect_supplier_promise_history(&mut tx, &company, "wrong-order", "supplier-promise-history-test").await.unwrap().image().is_none());
        assert!(inspect_supplier_promise_history(&mut tx, &company, "order-owned", "wrong-promise").await.unwrap().image().is_none());
        current.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO procurement_delivery_promise_lines(id,promise_id,order_line_id,ordinal,quantity_milli,fulfilled_snapshot_milli,promised_date) VALUES('extra-line','supplier-promise-history-test','order-line-b',1,1000,0,'2026-10-27')").execute(&mut *tx).await.unwrap();
        assert!(current.require_current(&mut tx).await.is_err());
        let current = inspect_supplier_promise_history(&mut tx, &company, "order-owned", "supplier-promise-history-test").await.unwrap();
        assert_eq!(current.image().unwrap()["lines"].as_array().unwrap().len(), 2);
        tx.commit().await.unwrap();
        let mut next = connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_supplier_promise_history_writes(&mut next).await.unwrap();
        assert!(current.require_current(&mut next).await.is_err());
        next.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn promise_history_writer_waits_and_fresh_unit_sees_committed_line() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        run_command(&pool, actor("company", "branch", "owner"), "order-owned", input(), "history-race").await.unwrap();
        let mut reader_connection = pool.raw().acquire().await.unwrap();
        let mut reader = reader_connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_supplier_promise_history_writes(&mut reader).await.unwrap();
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let company = CompanyId::new("company").unwrap();
        let witness = inspect_supplier_promise_history(&mut reader, &company, "order-owned", "supplier-promise-history-race").await.unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move {
            let mut connection = writer_pool.raw().acquire().await.unwrap();
            let mut tx = connection.begin().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
            send.send(pid).unwrap();
            sqlx::query("INSERT INTO procurement_delivery_promise_lines(id,promise_id,order_line_id,ordinal,quantity_milli,fulfilled_snapshot_milli,promised_date) VALUES('race-line','supplier-promise-history-race','order-line-b',1,1000,0,'2026-10-27')").execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        });
        let writer_pid = receive.await.unwrap();
        let blocked = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let blockers: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();
                if blockers.contains(&reader_pid) { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await;
        assert!(blocked.is_ok());
        witness.require_current(&mut reader).await.unwrap();
        reader.commit().await.unwrap();
        writer.await.unwrap();
        let mut fresh = reader_connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_supplier_promise_history_writes(&mut fresh).await.unwrap();
        let current = inspect_supplier_promise_history(&mut fresh, &company, "order-owned", "supplier-promise-history-race").await.unwrap();
        assert_eq!(current.image().unwrap()["lines"].as_array().unwrap().len(), 2);
        fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn promise_history_writer_first_is_visible_after_wait() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        run_command(&pool, actor("company", "branch", "owner"), "order-owned", input(), "history-first").await.unwrap();
        let mut writer_connection = pool.raw().acquire().await.unwrap();
        let mut writer = writer_connection.begin().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("INSERT INTO procurement_delivery_promise_lines(id,promise_id,order_line_id,ordinal,quantity_milli,fulfilled_snapshot_milli,promised_date) VALUES('first-line','supplier-promise-history-first','order-line-b',1,1000,0,'2026-10-27')").execute(&mut *writer).await.unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let reader_pool = pool.clone();
        let reader = tokio::spawn(async move {
            let mut connection = reader_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            send.send(pid).unwrap();
            let mut tx = connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
            lock_supplier_promise_history_writes(&mut tx).await.unwrap();
            let company = CompanyId::new("company").unwrap();
            let witness = inspect_supplier_promise_history(&mut tx, &company, "order-owned", "supplier-promise-history-first").await.unwrap();
            let image = witness.image().unwrap().clone();
            tx.rollback().await.unwrap();
            image
        });
        let reader_pid = receive.await.unwrap();
        let blocked = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let blockers: Vec<i32> = sqlx::query_scalar("SELECT pg_blocking_pids($1)").bind(reader_pid).fetch_one(pool.raw()).await.unwrap();
                if blockers.contains(&writer_pid) { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await;
        assert!(blocked.is_ok());
        writer.commit().await.unwrap();
        let image = reader.await.unwrap();
        assert_eq!(image["lines"].as_array().unwrap().len(), 2);
        assert_eq!(image["lines"][1]["id"], "first-line");
    }).await;
}

fn actor(company: &str, branch: &str, user: &str) -> SupplierPromiseCommandActor {
    SupplierPromiseCommandActor {
        user_id: user.into(),
        assignment_id: format!("assignment-{user}"),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: Some(RecordId::new(branch).unwrap()),
        department_id: None,
        revision: 1,
        assignment_version: 1,
        permissions: vec!["commerce.read".into(), "commerce.purchase".into()],
        modules: vec!["procurement".into()],
        expires_at: NOW.get() + 60_000,
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
         ('company','company','company','Company',NULL,'company','company','active',1),\
         ('branch','branch','branch','Branch','company','company','branch','active',1),\
         ('foreign','company','foreign','Foreign company',NULL,'foreign','foreign','active',1),\
         ('branch-foreign','branch','branch-foreign','Foreign branch','foreign','foreign','branch-foreign','active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('supplier','company','supplier','SUP-1','Bounded supplier','THB',10,10),\
         ('supplier-foreign','foreign','supplier','SUP-F','Foreign supplier','THB',10,10);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES \
         ('warehouse','company','branch','WH-1','Main warehouse'),\
         ('warehouse-foreign','foreign','branch-foreign','WH-F','Foreign warehouse');\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES \
         ('product-a','company','SKU-A','Bounded part','each',100,700),\
         ('product-b','company','SKU-B','Second part','each',200,700),\
         ('product-foreign','foreign','SKU-F','Foreign part','each',100,700);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('order-owned','company','branch',NULL,'purchase','PO-OWNED','supplier','warehouse','THB','2026-10-20','2026-11-10','confirmed',10000,700,'owner',10,10),\
         ('order-foreign','foreign','branch-foreign',NULL,'purchase','PO-FOREIGN','supplier-foreign','warehouse-foreign','THB','2026-10-20','2026-11-10','confirmed',10000,700,'owner',10,10);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES \
         ('order-line-a','order-owned','product-a','Bounded part',10000,4000,100,700,10000,700,0),\
         ('order-line-b','order-owned','product-b','Second part',5000,0,200,700,5000,350,1),\
         ('order-line-f','order-foreign','product-foreign','Foreign part',8000,0,100,700,8000,560,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn input() -> RecordSupplierPromiseCommand {
    RecordSupplierPromiseCommand {
        expected_version: 1,
        kind: "confirmed".into(),
        confirmation_date: "2026-10-25".into(),
        supplier_reference: "SUP-REF-1".into(),
        evidence: "Supplier confirmation evidence".into(),
        attachment_set_ids: Vec::new(),
        lines: vec![SupplierPromiseLineCommand {
            order_line_id: "order-line-a".into(),
            quantity: "4.000".into(),
            promised_date: "2026-10-26".into(),
        }],
    }
}

async fn run_command(
    pool: &DatabasePool,
    actor: SupplierPromiseCommandActor,
    order_id: &str,
    input: RecordSupplierPromiseCommand,
    key: &str,
) -> Result<SupplierPromiseRecordReceipt, BusinessError> {
    PgUnitOfWork::new(pool.clone())
        .run::<SupplierPromiseRecordReceipt, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.to_owned();
            let order_id = order_id.to_owned();
            Box::pin(async move {
                let key = CommandKey::new(key)?;
                record_supplier_promise_in_transaction(connection, &actor, &order_id, &input, &key, NOW).await
            })
        })
        .await
        .map_err(business)
}

#[tokio::test]
async fn records_supplier_promise_and_replays_idempotently() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        let receipt = run_command(&pool, owner.clone(), "order-owned", input(), "command-1").await.unwrap();
        assert_eq!(receipt.id, "order-owned");
        assert_eq!(receipt.version, 2);
        assert_eq!(receipt.promise_id, "supplier-promise-command-1");

        // counts
        let promise_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count, 1);
        let line_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promise_lines")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(line_count, 1);
        let receipt_count: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(receipt_count, 1);
        let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(audit_count, 1);
        let outbox_count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(outbox_count, 1);

        // order version advanced
        let version: i64 = sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(version, 2);

        // replay
        let replay = run_command(&pool, owner.clone(), "order-owned", input(), "command-1").await.unwrap();
        assert_eq!(replay, receipt);

        let promise_count2: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count2, 1);
        let receipt_count2: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='command-1'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(receipt_count2, 1);
    })
    .await;
}

#[tokio::test]
async fn idempotency_conflict_for_different_request() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        run_command(&pool, owner.clone(), "order-owned", input(), "command-2").await.unwrap();

        let mut different = input();
        different.supplier_reference = "DIFFERENT-REF".into();
        let err = run_command(&pool, owner.clone(), "order-owned", different, "command-2").await.unwrap_err();
        assert_eq!(err.code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn cas_conflict_when_order_version_changed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        // advance version manually
        sqlx::query("UPDATE commerce_orders SET version=2 WHERE id='order-owned'")
            .execute(pool.raw()).await.unwrap();

        let err = run_command(&pool, owner.clone(), "order-owned", input(), "command-3").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_CHANGED");

        let promise_count: i64 = sqlx::query_scalar("SELECT count(*) FROM procurement_delivery_promises WHERE order_id='order-owned'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(promise_count, 0);
    })
    .await;
}

#[tokio::test]
async fn company_isolation_blocks_foreign_access() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let foreign = actor("foreign", "branch-foreign", "foreign-user");
        let err = run_command(&pool, foreign.clone(), "order-owned", input(), "command-4").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_NOT_FOUND");

        let owner = actor("company", "branch", "owner");
        let err = run_command(&pool, owner.clone(), "order-foreign", input(), "command-5").await.unwrap_err();
        assert_eq!(err.code, "PURCHASE_ORDER_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn serializable_guard_fails_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let mut outside = pool.raw().acquire().await.unwrap();
        let err = PgSupplierPromiseCommand::new(&mut outside)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED"));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let err = PgSupplierPromiseCommand::new(&mut default_isolation)
            .receipt("key")
            .await
            .unwrap_err();
        assert!(matches!(err, PgTransactionError::Persistence(ref value) if value.code == "PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED"));
        default_isolation.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn readback_and_audit_outbox_persisted() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", "branch", "owner");
        let receipt = run_command(&pool, owner.clone(), "order-owned", input(), "command-6").await.unwrap();

        let promise_id = receipt.promise_id.clone();
        // readback row
        let (id, order_id, version, kind, recorded_by, line_count): (String,String,i64,String,String,i64) =
            sqlx::query_as("SELECT id,order_id,order_version,kind,recorded_by,(SELECT count(*)::bigint FROM procurement_delivery_promise_lines l WHERE l.promise_id=procurement_delivery_promises.id) FROM procurement_delivery_promises WHERE id=$1")
            .bind(&promise_id)
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(id, promise_id);
        assert_eq!(order_id, "order-owned");
        assert_eq!(version, 1);
        assert_eq!(kind, "confirmed");
        assert_eq!(recorded_by, "owner");
        assert_eq!(line_count, 1);

        // audit assignment_id
        let assignment: String = sqlx::query_scalar("SELECT assignment_id FROM audit_events WHERE command_id='command-6'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(assignment, "assignment-owner");

        // outbox payload contains company
        let payload: String = sqlx::query_scalar("SELECT payload_json::text FROM outbox_events WHERE command_id='command-6'")
            .fetch_one(pool.raw()).await.unwrap();
        assert!(payload.contains("company"));
    })
    .await;
}

#[tokio::test]
async fn commerce_request_link_source_fences_absence_and_unit() {
    use erpx_server_procurement::public::commerce_request_link::{inspect_commerce_request_links, lock_commerce_request_links};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO employee_requests(id,company_id,branch_id,kind,category,requester_user_id,created_by,created_at,updated_at) VALUES('link-request','company','branch','purchase','purchase','owner','owner',10,10),('direct-request','foreign','branch-foreign','purchase','purchase','owner','owner',10,10)")
            .execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE employee_requests SET purchase_order_id='order-foreign' WHERE id='direct-request'").execute(pool.raw()).await.unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let script = r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');
const fs=await import('node:fs');
const a=await import('./packages/server-records/src/adapters/commerce-final-owner-projection-sources.ts');
const b=await import('./packages/server-procurement/src/adapters/commerce-final-owner-projection-sources.ts');
const text=fs.readFileSync('./packages/server-commerce/src/adapters/commerce.ts','utf8');
const match=text.match(/readEmployeeRequests1:[\s\S]*?db\.prepare\(`([^`]+)`\)/);
if(!match)throw Error('COMMERCE_LOOKUP_SOURCE_MISSING');
const records=a.commerceFinalOwnerProjectionSources.employee_requests;
const procurement=b.commerceFinalOwnerProjectionSources.procurement_request_orders;
const sql=match[1].replace('${recordsProjectionSources.employee_requests}',records).replace('${procurementProjectionSources.procurement_request_orders}',procurement);
if(sql.includes('${'))throw Error('UNRESOLVED_SOURCE');
process.stdout.write(JSON.stringify({sql,procurement}));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(root).args(["--conditions=react-server","--input-type=module","-e",script]).output().unwrap();
        assert!(output.status.success(), "actual Commerce TS oracle unavailable: {}", String::from_utf8_lossy(&output.stderr));
        let ts: serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        let source_sql=ts["sql"].as_str().unwrap();
        let projection=ts["procurement"].as_str().unwrap();
        assert_eq!(source_sql.matches('?').count(), 3, "Commerce lookup bind shape changed");
        assert_eq!(source_sql.matches(projection).count(), 1, "Procurement projection occurrence changed");
        let parameters = |sql: &str| { let mut n=0; sql.replace('?', "__PARAM__").split("__PARAM__").enumerate().map(|(i, part)| { if i==0 { part.to_owned() } else { n+=1; format!("{}{}", if n==1 {"$2"} else {"$1"}, part) } }).collect::<String>() };
        let original=parameters(source_sql);
        let substituted=parameters(&source_sql.replace(projection, "SELECT request_id,order_id FROM procurement_commerce_request_links"));
        assert_ne!(original, substituted);
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        assert!(inspect_commerce_request_links(&mut tx, "order-owned").await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        sqlx::query("SAVEPOINT source_lock").execute(&mut *tx).await.unwrap();
        lock_commerce_request_links(&mut tx).await.unwrap();
        let savepoint_source = inspect_commerce_request_links(&mut tx, "order-owned").await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT source_lock").execute(&mut *tx).await.unwrap();
        assert!(savepoint_source.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_commerce_request_links(&mut tx).await.unwrap();
        assert!(inspect_commerce_request_links(&mut tx, &"x".repeat(2_097_153)).await.is_err());
        let source = inspect_commerce_request_links(&mut tx, "order-owned").await.unwrap();
        let query = format!("WITH {} SELECT count(*) FROM procurement_commerce_request_links", source.cte_sql());
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str())).bind(source.order_id()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(count, 0);
        source.require_current(&mut tx).await.unwrap();
        // The public source stays authoritative even with an identically named temp table.
        sqlx::query("CREATE TEMP TABLE procurement_request_orders(request_id text, order_id text) ON COMMIT DROP")
            .execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO pg_temp.procurement_request_orders VALUES('shadow','order-owned')").execute(&mut *tx).await.unwrap();
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str())).bind(source.order_id()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(count, 0);
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("DROP TABLE pg_temp.procurement_request_orders").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.procurement_request_orders(request_id,order_id,request_version,request_evidence_version,linked_by,created_at) VALUES('link-request','order-owned',1,1,'owner',10)")
            .execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        let linked = inspect_commerce_request_links(&mut tx, "order-owned").await.unwrap();
        // Dynamic SQL consists solely of repository-owned SQL and the fixed owner CTE;
        // all fixture and request values remain bound parameters.
        let linked_query = format!("WITH {} SELECT request_id,order_id FROM procurement_commerce_request_links", linked.cte_sql());
        let rows: Vec<(String,String)> = sqlx::query_as(sqlx::AssertSqlSafe(linked_query.as_str()))
            .bind(linked.order_id()).fetch_all(&mut *tx).await.unwrap();
        assert_eq!(rows, vec![("link-request".into(), "order-owned".into())]);
        linked.require_current(&mut tx).await.unwrap();
        for (company,order,expected) in [("company","order-owned",Some("link-request")),("foreign","order-foreign",Some("direct-request")),("company","order-foreign",None),("company","missing-order",None)] {
            let witness=inspect_commerce_request_links(&mut tx, order).await.unwrap();
            let actual: Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(original.as_str())).bind(order).bind(company).fetch_all(&mut *tx).await.unwrap();
            let composed=format!("WITH {} {}", witness.cte_sql(), substituted);
            let native: Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(composed.as_str())).bind(witness.order_id()).bind(company).fetch_all(&mut *tx).await.unwrap();
            assert_eq!(actual, expected.into_iter().map(str::to_owned).collect::<Vec<_>>());
            assert_eq!(native, actual);
            witness.require_current(&mut tx).await.unwrap();
        }
        // A second connection cannot obtain the owner lock while this unit holds it.
        let mut other = pool.raw().acquire().await.unwrap();
        let mut competing = other.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        let error = lock_commerce_request_links(&mut competing).await.unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(ref e)
            if e.code == "PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state() == Some("55P03") && e.retryable()));
        competing.rollback().await.unwrap();
        tx.rollback().await.unwrap();
        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_commerce_request_links(&mut tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        let after_rollback = inspect_commerce_request_links(&mut tx, "order-owned").await.unwrap();
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str())).bind(after_rollback.order_id()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(count, 0);
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_request_link_rejects_rls_and_deferred_drift() {
    use erpx_server_procurement::public::commerce_request_link::{inspect_commerce_request_links, lock_commerce_request_links};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO employee_requests(id,company_id,branch_id,kind,category,requester_user_id,created_by,created_at,updated_at) VALUES('deferred-source-request','company','branch','purchase','purchase','owner','owner',10,10)")
            .execute(pool.raw()).await.unwrap();
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        let bypass: bool = sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *tx).await.unwrap();
        assert!(!bypass, "RLS test requires a non-bypass leased database role");
        sqlx::raw_sql("ALTER TABLE public.procurement_request_orders ENABLE ROW LEVEL SECURITY; ALTER TABLE public.procurement_request_orders FORCE ROW LEVEL SECURITY")
            .execute(&mut *tx).await.unwrap();
        let active: bool = sqlx::query_scalar("SELECT row_security_active('public.procurement_request_orders'::regclass)").fetch_one(&mut *tx).await.unwrap();
        assert!(active);
        lock_commerce_request_links(&mut tx).await.unwrap();
        assert!(inspect_commerce_request_links(&mut tx, "order-owned").await.is_err());
        tx.rollback().await.unwrap();

        let mut tx = c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_commerce_request_links(&mut tx).await.unwrap();
        let before = inspect_commerce_request_links(&mut tx, "order-owned").await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION pg_temp.procurement_link_deferred_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO public.procurement_request_orders(request_id,order_id,request_version,request_evidence_version,linked_by,created_at) VALUES('deferred-source-request','order-owned',1,1,'owner',10); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER procurement_link_deferred_fixture AFTER INSERT ON public.employee_requests DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION pg_temp.procurement_link_deferred_fixture()")
            .execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO employee_requests(id,company_id,branch_id,kind,category,requester_user_id,created_by,created_at,updated_at) VALUES('deferred-trigger-request','company','branch','purchase','purchase','owner','owner',10,10)")
            .execute(&mut *tx).await.unwrap();
        before.require_current(&mut tx).await.unwrap();
        sqlx::query("SET CONSTRAINTS procurement_link_deferred_fixture IMMEDIATE").execute(&mut *tx).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.procurement_request_orders WHERE request_id='deferred-source-request' AND order_id='order-owned'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(count, 1, "deferred trigger must have written the link");
        assert!(before.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.procurement_request_orders WHERE order_id='order-owned'").fetch_one(&mut *c).await.unwrap();
        assert_eq!(count, 0, "rollback must undo the deferred link");
    }).await;
}
