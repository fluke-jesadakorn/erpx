#[path = "../../support/rust-postgres.rs"]
mod fixture;
mod domain {
    pub(crate) mod rfq_transition {
        pub use erpx_procurement_server::public::rfq_transition::*;
    }
}
#[path = "../../../modules/procurement/server/src/adapters/rfq_transition_stage.rs"]
mod rfq_transition_stage;
#[path = "../../../platform/composition/src/procurement_rfq_notice.rs"]
mod rfq_notice;
#[path = "../../../platform/composition/src/procurement_rfq_assertions.rs"]
mod rfq_assertions;
#[path = "../../../platform/composition/src/procurement_rfq_quantity.rs"]
mod rfq_quantity;
use erpx_procurement_server::public::rfq_transition::{RfqTransitionAction, RfqTransitionDecision};
use rfq_transition_stage::*;
use sqlx::{Connection, Row};

#[tokio::test]
async fn node_postgres_promise_all_first_error_and_aborted_unit() {
    fixture::with_database(2, |pool| async move {
        let db_name: String=sqlx::query_scalar("SELECT current_database()::text").fetch_one(pool.raw()).await.unwrap();
        let script=r#"const {Pool}=require('pg');const {transactionDatabase,PostgresDatabase}=require('./platform/runtime/server/src/adapters/runtime/postgres.ts');
const url=new URL(process.env.ERPX_TEST_ADMIN_URL);url.pathname='/'+process.env.ERPX_CASE_DATABASE;
(async()=>{const pool=new Pool({connectionString:url.toString()});const c=await pool.connect();try{await c.query('BEGIN');const db=transactionDatabase(c);const work=[db.prepare('SELECT 1').all(),db.prepare('SELECT 1/0 AS value').all(),db.prepare('SELECT 1 FROM no_such_rfq_table').all(),db.prepare('SELECT 2').all()];const settled=await Promise.allSettled(work);let later;try{await db.prepare('SELECT 3').all()}catch(e){later=e.code}await c.query('ROLLBACK');await c.query('BEGIN');const decoded=await Promise.allSettled([db.prepare('SELECT 1').all(),db.prepare('SELECT 9007199254740992::bigint AS value').all(),db.prepare('SELECT 3').all(),db.prepare('SELECT 4').all()]);const afterDecode=await db.prepare('SELECT 5 AS value').first();const mixed=[];for(const statements of [["SELECT 9007199254740992::bigint AS value","SELECT 1/0 AS value","SELECT 1","SELECT 2"],["SELECT 1/0 AS value","SELECT 9007199254740992::bigint AS value","SELECT 1","SELECT 2"],["SELECT 9007199254740992::bigint AS value","SELECT 9007199254740992::bigint AS value","SELECT 1","SELECT 2"]]){await c.query('ROLLBACK');await c.query('BEGIN');const results=await Promise.allSettled(statements.map(sql=>db.prepare(sql).all()));let after='ok';try{await db.prepare('SELECT 5').all()}catch(e){after=e.code}mixed.push({codes:results.map(x=>x.status==='rejected'?(x.reason.code||x.reason.name):'ok'),after})}const direct=[];for(const statements of [["SELECT 9007199254740992::bigint AS value","SELECT 1/0 AS value","SELECT 1","SELECT 2"],["SELECT 1/0 AS value","SELECT 9007199254740992::bigint AS value","SELECT 1","SELECT 2"]]){await c.query('ROLLBACK');await c.query('BEGIN');const pending=statements.map(sql=>db.prepare(sql).all());let first='ok';try{await Promise.all(pending)}catch(e){first=e.code||e.name}const rollback=c.query('ROLLBACK');const outcomes=await Promise.allSettled(pending);await rollback;const after=await db.prepare('SELECT 9 AS value').first();direct.push({first,codes:outcomes.map(x=>x.status==='rejected'?(x.reason.code||x.reason.name):'ok'),after:after.value})}const plain=new PostgresDatabase({connectionString:url.toString(),max:1});const disabled=[];for(const statements of [["SELECT 1/0 AS value","SELECT 9007199254740992::bigint AS value","SELECT 1","SELECT 2"],["SELECT 9007199254740992::bigint AS value","SELECT 1/0 AS value","SELECT 1","SELECT 2"]]){const results=await Promise.allSettled(statements.map(sql=>plain.prepare(sql).all()));disabled.push(results.map(x=>x.status==='rejected'?(x.reason.code||x.reason.name):'ok'))}await plain.pool.end();process.stdout.write(JSON.stringify({codes:settled.map(x=>x.status==='rejected'?x.reason.code:'ok'),later,decode:decoded.map(x=>x.status==='rejected'?x.reason.name+':'+x.reason.message:'ok'),afterDecode:afterDecode.value,mixed,disabled,direct}));}finally{await c.query('ROLLBACK');c.release();await pool.end()}})().catch(e=>{process.stderr.write(e.code||e.name);process.exitCode=1});"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .env("ERPX_CASE_DATABASE",db_name).args(["--conditions=react-server","--import=tsx","-e",script]).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let result:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["codes"],serde_json::json!(["ok","22012","25P02","25P02"]));
        assert_eq!(result["later"],"25P02");
        assert_eq!(result["decode"],serde_json::json!(["ok","RangeError:DATABASE_NUMBER_OUT_OF_RANGE","ok","ok"]));
        assert_eq!(result["afterDecode"],5);
        assert_eq!(result["mixed"],serde_json::json!([
            {"codes":["RangeError","22012","25P02","25P02"],"after":"25P02"},
            {"codes":["22012","25P02","25P02","25P02"],"after":"25P02"},
            {"codes":["RangeError","RangeError","ok","ok"],"after":"ok"}
        ]));
        assert_eq!(result["disabled"],serde_json::json!([["22012","RangeError","ok","ok"],["RangeError","22012","ok","ok"]]));
        assert_eq!(result["direct"],serde_json::json!([
            {"first":"RangeError","codes":["RangeError","22012","25P02","25P02"],"after":9},
            {"first":"22012","codes":["22012","25P02","25P02","25P02"],"after":9}
        ]));
    }).await;
}

#[tokio::test]
async fn allocation_source_preserves_grouped_numeric_without_i64_truncation() {
    use erpx_procurement_server::public::rfq_request_allocations::read_rfq_request_allocations;
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE procurement_rfqs(id text PRIMARY KEY,request_id text,status text); CREATE TABLE procurement_rfq_lines(rfq_id text,request_item_ordinal integer,request_quantity_milli bigint); INSERT INTO procurement_rfqs VALUES('a','request','approved'),('b','request','cancelled'),('c','other','approved'); INSERT INTO procurement_rfq_lines VALUES('a',0,9000000000000000000),('a',0,9000000000000000000),('a',NULL,NULL),('b',0,10),('c',0,20)").execute(&mut *c).await.unwrap();
        let rows=read_rfq_request_allocations(&mut c,"request").await.unwrap();
        assert_eq!(rows.len(),2);
        assert_eq!(rows.iter().find(|r|r.ordinal==Some(0)).unwrap().allocated_milli_decimal,"18000000000000000000");
        assert_eq!(rows.iter().find(|r|r.ordinal.is_none()).unwrap().allocated_milli_decimal,"0");
        assert!(matches!(rfq_quantity::rfq_quotation_items_json("[]",&rows),Err(rfq_quantity::RfqItemMapError::DatabaseNumberOutOfRange)));
        assert!(matches!(rfq_quantity::rfq_quotation_items_json(r#"[{"quantity":"bad"}]"#,&rows),Err(rfq_quantity::RfqItemMapError::DatabaseNumberOutOfRange)));
        assert!(read_rfq_request_allocations(&mut c,"missing").await.unwrap().is_empty());
    }).await;
}

#[tokio::test]
async fn migrated_cancel_releases_commitment_and_cache_changes_roll_back() {
    use erpx_platform_runtime_server::public::postgres::{MigrationTrack, load_migrations, migrate_database};
    fixture::with_database(3, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql(r#"INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES('company','company','company','Company',NULL,'company','active'),('branch','branch','branch','Branch','company','company','active'); INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('warehouse','company','branch','W','Warehouse','active',1); INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES('product','company','P','Product','each',100,0); INSERT INTO employee_requests(id,company_id,kind,category,requester_user_id,created_by,state,items_json,approval_json,evidence_version,created_at,updated_at) VALUES('request','company','purchase','purchasing','requester','creator','in_progress','[{"quantity":"2","unit":"each"}]','{"decision":"approved"}',1,0,0); INSERT INTO procurement_rfqs(id,company_id,branch_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,version,created_at,updated_at,request_id,request_version,request_evidence_version,request_items_json) VALUES('rfq','company','branch','warehouse','THB','RFQ','demand','2026-11-20','collecting','creator',1,0,0,'request',1,1,'[{"quantity":"2","unit":"each"}]'); INSERT INTO procurement_rfq_lines(id,rfq_id,product_id,description,unit,quantity_milli,ordinal,request_item_ordinal,request_quantity_milli,request_unit,request_units_per_product_unit_milli) VALUES('line','rfq','product','Product','each',1000,0,0,1000,'each',1000)"#).execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','buyer','Buyer',true,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','buyer',1,'[\"commerce.purchase\"]','[]','[\"holding\"]','system',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) VALUES('buyer-assignment','buyer-user','buyer','[]','company',NULL,0,9000000000000,NULL,1,'holding',NULL,NULL,'manual',NULL)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('supplier','company','supplier','SUP','Supplier','THB',0,0); INSERT INTO procurement_rfq_offers VALUES('offer','rfq','supplier',1,'ref','evidence','2026-11-19','2026-11-20','THB',100,0,'seller',0)").execute(pool.raw()).await.unwrap();
        let before:i64=sqlx::query_scalar("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(pool.raw()).await.unwrap();
        let allocated:i64=sqlx::query_scalar("SELECT allocated_quantity_milli FROM procurement_request_item_commitments WHERE request_id='request' AND request_item_ordinal=0").fetch_one(pool.raw()).await.unwrap();assert_eq!(allocated,1000);
        let mut c=pool.raw().acquire().await.unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        lock_transition_sources(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.procurement_request_item_commitments IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        erpx_collaboration_server::public::rfq_notifications::lock_rfq_notifications(&mut tx).await.unwrap();
        use rfq_assertions::{RfqAssertionInput, RfqRequestAssertion, stage_rfq_assertions};
        let assertion = RfqAssertionInput{key:"assert-key",id:"rfq",version:1,status:"collecting",company:"company",branch:Some("branch"),department:None,request:Some(RfqRequestAssertion{id:"request",evidence_version:Some(1),items_json:Some("[{\"quantity\":\"2\",\"unit\":\"each\"}]")})};
        stage_rfq_assertions(&mut tx,&assertion).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM command_assertions WHERE command_id='assert-key'").fetch_one(&mut *tx).await.unwrap(),2);
        sqlx::query("SAVEPOINT local_duplicate").execute(&mut *tx).await.unwrap();
        let error=stage_rfq_assertions(&mut tx,&assertion).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23505"));
        assert_eq!(error.as_database_error().unwrap().constraint(),Some("command_assertions_pkey"));
        sqlx::query("ROLLBACK TO SAVEPOINT local_duplicate").execute(&mut *tx).await.unwrap();
        sqlx::query("DELETE FROM command_assertions WHERE command_id='assert-key'").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO command_assertion_baselines VALUES('assert-key','rfq-version',1)").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT baseline_duplicate").execute(&mut *tx).await.unwrap();
        let error=stage_rfq_assertions(&mut tx,&assertion).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23505"));
        sqlx::query("ROLLBACK TO SAVEPOINT baseline_duplicate").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT false_before_duplicate").execute(&mut *tx).await.unwrap();
        let stale=RfqAssertionInput{version:2,request:None,..assertion};
        let error=stage_rfq_assertions(&mut tx,&stale).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
        assert_eq!(error.as_database_error().unwrap().constraint(),Some("command_assertions_ok_check"));
        sqlx::query("ROLLBACK TO SAVEPOINT false_before_duplicate").execute(&mut *tx).await.unwrap();
        sqlx::query("DELETE FROM command_assertions WHERE command_id='assert-key'").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT stale_request").execute(&mut *tx).await.unwrap();
        let stale_request=RfqAssertionInput{request:Some(RfqRequestAssertion{id:"request",evidence_version:Some(2),items_json:Some("[]")}),..stale};
        let stale_request=RfqAssertionInput{version:1,..stale_request};
        let error=stage_rfq_assertions(&mut tx,&stale_request).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
        sqlx::query("ROLLBACK TO SAVEPOINT stale_request").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT invalid_approval").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE employee_requests SET approval_json='{' WHERE id='request'").execute(&mut *tx).await.unwrap();
        let linked=RfqAssertionInput{request:Some(RfqRequestAssertion{id:"request",evidence_version:Some(1),items_json:Some("[{\"quantity\":\"2\",\"unit\":\"each\"}]")}),..stale_request};
        let error=stage_rfq_assertions(&mut tx,&linked).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("22P02"));
        sqlx::query("ROLLBACK TO SAVEPOINT invalid_approval").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT submit_revision").execute(&mut *tx).await.unwrap();
        let submit_decision=RfqTransitionDecision{status:"review",selected_offer_id:Some("offer".into())};
        let submit=TransitionWrite{id:"rfq",company_id:"company",branch_id:Some("branch"),department_id:None,expected_status:"collecting",expected_version:1,actor_id:"buyer",command_key:"submit-key",evidence:"submit reason",now:100,action:RfqTransitionAction::Submit,decision:&submit_decision};
        assert!(stage_transition(&mut tx,&submit).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(&mut *tx).await.unwrap(),before+2);
        sqlx::query("ROLLBACK TO SAVEPOINT submit_revision").execute(&mut *tx).await.unwrap();
        for action in [RfqTransitionAction::Approve,RfqTransitionAction::Return] {
            sqlx::query("SAVEPOINT action_revision").execute(&mut *tx).await.unwrap();
            sqlx::query("UPDATE procurement_rfqs SET status='review',selected_offer_id='offer',submitted_by='maker' WHERE id='rfq'").execute(&mut *tx).await.unwrap();
            let action_before:i64=sqlx::query_scalar("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(&mut *tx).await.unwrap();
            let action_decision=if action==RfqTransitionAction::Approve {RfqTransitionDecision{status:"approved",selected_offer_id:Some("offer".into())}} else {RfqTransitionDecision{status:"collecting",selected_offer_id:None}};
            let action_write=TransitionWrite{expected_status:"review",command_key:"action-key",action,decision:&action_decision,..submit};
            assert!(stage_transition(&mut tx,&action_write).await.unwrap());
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(&mut *tx).await.unwrap(),action_before+2);
            sqlx::query("ROLLBACK TO SAVEPOINT action_revision").execute(&mut *tx).await.unwrap();
        }
        let decision=RfqTransitionDecision{status:"cancelled",selected_offer_id:None};
        let input=TransitionWrite{id:"rfq",company_id:"company",branch_id:Some("branch"),department_id:None,expected_status:"collecting",expected_version:1,actor_id:"buyer",command_key:"cancel-key",evidence:"cancel reason",now:100,action:RfqTransitionAction::Cancel,decision:&decision};
        assert!(stage_transition(&mut tx,&input).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT allocated_quantity_milli FROM procurement_request_item_commitments WHERE request_id='request' AND request_item_ordinal=0").fetch_one(&mut *tx).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(&mut *tx).await.unwrap(),before+1);
        let notice=erpx_procurement_server::public::rfq_notification_source::bind_rfq_notification_source(&mut tx,"company","rfq",2).await.unwrap();
        rfq_notice::notify_rfq(&mut tx,"cancel-key",&notice).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM staff_notifications WHERE record_type='procurement_rfq' AND record_id='rfq'").fetch_one(&mut *tx).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM inbox_events WHERE source_type='procurement_rfq' AND source_id='rfq'").fetch_one(&mut *tx).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM inbox_deliveries WHERE id='cancel-key-buyer-user'").fetch_one(&mut *tx).await.unwrap(),1);
        sqlx::raw_sql("SET CONSTRAINTS finance_book_projection IMMEDIATE; SET CONSTRAINTS finance_book_projection DEFERRED; SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT allocated_quantity_milli FROM procurement_request_item_commitments WHERE request_id='request' AND request_item_ordinal=0").fetch_one(&mut *c).await.unwrap(),1000);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM work_cache_revision_baselines WHERE table_name='procurement_rfqs'").fetch_one(&mut *c).await.unwrap(),before);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM procurement_rfqs WHERE id='rfq'").fetch_one(&mut *c).await.unwrap(),"collecting");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM procurement_rfq_events WHERE rfq_id='rfq'").fetch_one(&mut *c).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM staff_notifications WHERE record_type='procurement_rfq' AND record_id='rfq'").fetch_one(&mut *c).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM inbox_events WHERE source_type='procurement_rfq' AND source_id='rfq'").fetch_one(&mut *c).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM inbox_deliveries WHERE id='cancel-key-buyer-user'").fetch_one(&mut *c).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn notification_source_is_current_and_bound_to_its_transaction() {
    use erpx_procurement_server::public::rfq_notification_source::bind_rfq_notification_source;
    fixture::with_database(2, |pool| async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE procurement_rfqs(id text primary key,company_id text,branch_id text,department_id text,version bigint,updated_at bigint); CREATE TABLE procurement_rfq_events(id text primary key); INSERT INTO procurement_rfqs VALUES('r','c','b',NULL,2,20);").execute(&mut *c).await.unwrap();
        let mut tx=c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(bind_rfq_notification_source(&mut tx,"c","r",2).await.is_err());
        lock_transition_sources(&mut tx).await.unwrap();
        assert!(bind_rfq_notification_source(&mut tx,"other","r",2).await.is_err());
        assert!(bind_rfq_notification_source(&mut tx,"c","r",3).await.is_err());
        let witness=bind_rfq_notification_source(&mut tx,"c","r",2).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE procurement_rfqs SET updated_at=21 WHERE id='r'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut other=c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_transition_sources(&mut other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn stage_is_scoped_atomic_and_rolled_back_with_the_owner_unit() {
    fixture::with_database(2, |pool| async move {
        let mut c = pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE procurement_rfqs(id text primary key,company_id text,branch_id text,department_id text,status text,version bigint,updated_at bigint,selected_offer_id text,selection_reason text,submitted_by text,approved_by text); CREATE TABLE procurement_rfq_events(id text primary key,rfq_id text,version bigint,action text,actor_id text,evidence text,selected_offer_id text,created_at bigint,unique(rfq_id,version)); INSERT INTO procurement_rfqs VALUES('r','c','b',NULL,'collecting',1,1,NULL,NULL,NULL,NULL);").execute(&mut *c).await.unwrap();
        let decision = RfqTransitionDecision { status: "review", selected_offer_id: Some("offer".into()) };
        let mut input = TransitionWrite { id:"r", company_id:"other", branch_id:Some("b"), department_id:None, expected_status:"collecting", expected_version:1, actor_id:"actor", command_key:"k", evidence:"reason", now:20, action:RfqTransitionAction::Submit, decision:&decision };
        assert!(lock_transition_sources(&mut c).await.is_err());
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(stage_transition(&mut tx, &input).await.is_err());
        lock_transition_sources(&mut tx).await.unwrap();
        assert!(!stage_transition(&mut tx, &input).await.unwrap());
        input.company_id = "c";
        assert!(stage_transition(&mut tx, &input).await.unwrap());
        assert!(!stage_transition(&mut tx, &input).await.unwrap());
        let row = sqlx::query("SELECT status,version,selected_offer_id,selection_reason,submitted_by FROM procurement_rfqs WHERE id='r'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(row.get::<String,_>(0), "review");
        assert_eq!(row.get::<i64,_>(1), 2);
        assert_eq!(row.get::<String,_>(2), "offer");
        assert_eq!(row.get::<String,_>(3), "reason");
        assert_eq!(row.get::<String,_>(4), "actor");
        let event: (String,i64,String,String) = sqlx::query_as("SELECT id,version,action,evidence FROM procurement_rfq_events").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(event, ("rfq-event-k".into(),2,"submit".into(),"reason".into()));
        let approved = RfqTransitionDecision { status: "approved", selected_offer_id: Some("offer".into()) };
        let approve = TransitionWrite { expected_status:"review", expected_version:2, command_key:"approve", action:RfqTransitionAction::Approve, decision:&approved, ..input };
        assert!(stage_transition(&mut tx, &approve).await.unwrap());
        let returned = RfqTransitionDecision { status: "collecting", selected_offer_id: None };
        let return_write = TransitionWrite { expected_status:"approved", expected_version:3, command_key:"return", action:RfqTransitionAction::Return, decision:&returned, ..approve };
        assert!(stage_transition(&mut tx, &return_write).await.unwrap());
        let cleared: (Option<String>, Option<String>, Option<String>, Option<String>) = sqlx::query_as("SELECT selected_offer_id,selection_reason,submitted_by,approved_by FROM procurement_rfqs WHERE id='r'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(cleared, (None,None,None,None));
        let cancelled = RfqTransitionDecision { status: "cancelled", selected_offer_id: None };
        let cancel = TransitionWrite { expected_status:"collecting", expected_version:4, command_key:"cancel", action:RfqTransitionAction::Cancel, decision:&cancelled, ..return_write };
        assert!(stage_transition(&mut tx, &cancel).await.unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM procurement_rfq_events").fetch_one(&mut *tx).await.unwrap(), 4);
        tx.rollback().await.unwrap();
        let row: (String,i64) = sqlx::query_as("SELECT status,version FROM procurement_rfqs WHERE id='r'").fetch_one(&mut *c).await.unwrap();
        assert_eq!(row, ("collecting".into(),1));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM procurement_rfq_events").fetch_one(&mut *c).await.unwrap(), 0);
        // The retained application treats an empty selected_offer_id as null
        // for the event, but cancellation does not clear the stored column.
        sqlx::query("UPDATE procurement_rfqs SET selected_offer_id='' WHERE id='r'").execute(&mut *c).await.unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_transition_sources(&mut tx).await.unwrap();
        let empty_cancel = TransitionWrite { expected_status:"collecting", expected_version:1, command_key:"empty-cancel", action:RfqTransitionAction::Cancel, decision:&cancelled, ..input };
        assert!(stage_transition(&mut tx, &empty_cancel).await.unwrap());
        let row: (String, Option<String>) = sqlx::query_as("SELECT r.selected_offer_id,e.selected_offer_id FROM procurement_rfqs r JOIN procurement_rfq_events e ON e.rfq_id=r.id WHERE r.id='r'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(row, (String::new(), None));
        tx.rollback().await.unwrap();
    }).await;
}
