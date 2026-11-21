//! Inventory-only revision proof; synthetic source receipts are rolled back.
#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::{
    service_initial_count_revision::*, service_initial_physical_count::*,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

#[tokio::test]
async fn revision_replaces_only_planned_inventory_and_rolls_back_with_owner_unit() {
    fixture::with_database(2,|pool|async move {
  facts::setup(&pool,"none").await;
  sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
  sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
  for (tracking,existing_lot) in [(TrackingMode::None,false),(TrackingMode::Lot,false),(TrackingMode::Lot,true)] {
   let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
    lock_initial_count_inventory_sources(c).await?;
    sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
    let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
    let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(tracking,0,None,vec![]).unwrap(),created_at:1};
    insert_initial_count_draft(c,&draft,&history).await?;
    if existing_lot { sqlx::raw_sql("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES('existing-lot','company','product','lot','Lot-A','maker',1); INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('existing-lot','warehouse',2000,200)").execute(&mut *c).await?; }
    let before=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();
    let pair=read_initial_count_inventory_history(c,"warehouse","product",Some("count-cmd")).await?;
    let allocations=if tracking==TrackingMode::Lot {vec![InitialCountRevisionAllocation{lot_id:if existing_lot {Some("existing-lot".into())} else {None},lot_code:Some("Lot-A".into()),serial_number:None,manufactured_date:None,expiry_date:None,quantity_milli:2000,unit_cost_minor:if existing_lot {None} else {Some(100)}}]}else{vec![]};
    let input=InitialCountRevision{company_id:"company".into(),branch_id:"branch".into(),count_id:"count-cmd".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),expected_version:1,count_date:"2026-01-11".into(),reason:"Revised actual count".into(),actor_id:"maker".into(),now:2,product:draft.product.clone(),quantity_milli:2000,unit_cost_minor:if tracking==TrackingMode::None{Some(100)}else{None},allocations};
    let plan=prepare_initial_count_revision(c,&before,&pair,input).await?;plan.apply(c,&before,&pair).await?;plan.require_current(c).await?;
    let revised=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();let image:serde_json::Value=serde_json::from_str(&revised.data().image).unwrap();assert_eq!(image["count"]["version"],2);assert_eq!(image["count"]["status"],"draft");assert_eq!(image["lines"][0]["counted_value_minor"],200);
    sqlx::query("UPDATE inventory_counts SET reason='tampered' WHERE id='count-cmd'").execute(&mut *c).await?;assert!(plan.require_current(c).await.is_err());
    Err(BusinessError::invalid("ROLLBACK_FIXTURE","Rollback synthetic receipt").into())
   })).await;
   assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="ROLLBACK_FIXTURE"));
   let count:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_counts WHERE id='count-cmd'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
  }
 }).await;
}

#[tokio::test]
async fn revision_uses_positive_balance_source_identity_and_rejects_missing_or_duplicate() {
    // Inventory owner fixture only; the synthetic receipts are rolled back and
    // do not establish selected Identity, Finance or Workflow authority.
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool,"none").await;
        sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
            let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
            let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::Lot},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::Lot,0,None,vec![]).unwrap(),created_at:1};
            insert_initial_count_draft(c,&draft,&history).await?;
            let long="A".repeat(140);
            sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,status,created_by,created_at) VALUES('historic','company','product','lot',$1,'depleted','maker',1),('zero','company','product','lot','ZERO','active','maker',1),('case-a','company','product','lot','CASE-A','active','maker',1),('case-b','company','product','lot','CASE-B','active','maker',1)").bind(&long).execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,serial_number,status,created_by,created_at) VALUES('serial-source','company','product','serial','SERIAL-1','active','maker',1)").execute(&mut *c).await?;
            sqlx::raw_sql("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES('historic','warehouse',1000,100),('zero','warehouse',0,0),('case-a','warehouse',1000,100),('case-b','warehouse',1000,100),('serial-source','warehouse',1000,100)").execute(&mut *c).await?;
            let before=read_initial_count_record(c,"company",Some("branch"),"count-cmd").await?.unwrap();let pair=read_initial_count_inventory_history(c,"warehouse","product",Some("count-cmd")).await?;
            let allocation=|id:&str,requested:Option<&str>|InitialCountRevisionAllocation{lot_id:Some(id.into()),lot_code:requested.map(str::to_owned),serial_number:None,manufactured_date:None,expiry_date:None,quantity_milli:1000,unit_cost_minor:None};
            let revision=|allocations:Vec<InitialCountRevisionAllocation>|InitialCountRevision{company_id:"company".into(),branch_id:"branch".into(),count_id:"count-cmd".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),expected_version:1,count_date:"2026-01-11".into(),reason:"😀".repeat(2000),actor_id:"maker".into(),now:2,product:draft.product.clone(),quantity_milli:allocations.iter().map(|v|v.quantity_milli).sum(),unit_cost_minor:None,allocations};
            for id in ["missing","zero"] {let e=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation(id,Some("request"))])).await.err().expect("missing or zero-balance source rejected");assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="COUNT_LOT_INVALID"));}
            let e=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation("historic",None),allocation("historic",None)])).await.err().unwrap();assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="DUPLICATE_COUNT_LOT"));
            let found_duplicate=InitialCountRevisionAllocation{lot_id:None,lot_code:Some("case-a".into()),serial_number:None,manufactured_date:None,expiry_date:None,quantity_milli:1000,unit_cost_minor:Some(100)};
            let e=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation("case-a",None),found_duplicate])).await.err().unwrap();assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="DUPLICATE_TRACKING_IDENTITY"));
            let e=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation("historic",None)])).await.err().unwrap();assert!(matches!(e,PgTransactionError::Business(ref e) if e.code=="COUNT_LOT_MISSING"));
            // A serial-mode historical source has NULL lot_code; TS's nullish
            // coalescing uses the supplied lot code for the current lot mode.
            let fallback=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation("serial-source",Some("Fallback")),allocation("historic",Some("ignored")),allocation("case-a",None),allocation("case-b",None)])).await?;
            sqlx::query("SAVEPOINT fallback_revision").execute(&mut *c).await?;fallback.apply(c,&before,&pair).await?;fallback.require_current(c).await?;
            let fallback_lot:String=sqlx::query_scalar("SELECT lot_code FROM inventory_count_lot_lines WHERE count_line_id='count-cmd-0' AND source_lot_id='serial-source'").fetch_one(&mut *c).await?;assert_eq!(fallback_lot,"Fallback");sqlx::query("ROLLBACK TO SAVEPOINT fallback_revision").execute(&mut *c).await?;
            // Existing positive-balance source wins over a differing supplied
            // value, even if its persisted identity predates the 100-char limit.
            let plan=prepare_initial_count_revision(c,&before,&pair,revision(vec![allocation("historic",Some("ignored")),allocation("case-a",None),allocation("case-b",None),allocation("serial-source",Some("Fallback"))])).await?;plan.apply(c,&before,&pair).await?;plan.require_current(c).await?;
            let actual:(String,String,String,i64)=sqlx::query_as("SELECT l.lot_code,l.source_lot_id,c.reason,l.counted_value_minor FROM inventory_count_lot_lines l JOIN inventory_count_lines cl ON cl.id=l.count_line_id JOIN inventory_counts c ON c.id=cl.count_id WHERE c.id='count-cmd' AND l.source_lot_id='historic'").fetch_one(&mut *c).await?;assert_eq!(actual,(long,"historic".into(),"😀".repeat(2000),100));
            Err(BusinessError::invalid("ROLLBACK_FIXTURE","Roll back synthetic sources.").into())
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="ROLLBACK_FIXTURE"));let rows:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_counts WHERE id='count-cmd'").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,0);
    }).await;
}

#[tokio::test]
async fn initial_count_terminal_and_marker_sources_match_actual_ts_and_isolate_unrelated_json() {
    // One rollback-only unit exercises the actual imported TypeScript Count
    // terminal branch and Service marker journal SQL. It is source-predicate
    // evidence only, not Count authority, valid Service lineage or purchase.
    use erpx_server_inventory::public::{
        count_terminal_history_source::*, initial_count_history_source::*,
    };
    use erpx_server_workflow::public::record_origin::*;
    use sqlx::AssertSqlSafe;

    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters");
        let script = r#"import fs from 'node:fs';if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');
const terminal=await import(process.argv[1]);const terminalSql=terminal.supplyInventoryTerminalReceipts;
const text=fs.readFileSync(process.argv[2],'utf8');
const line=text.split('\n').find(x=>x.startsWith('const serviceInitialPhysicalCountMarkedJournal='));
if(!line)throw Error('MARKER_SOURCE_MISSING');
const rows=await import(process.argv[3]);
const marker=new Function('serviceInitialPhysicalStockMarkedRows',line+';return serviceInitialPhysicalCountMarkedJournal')(rows.serviceInitialPhysicalStockMarkedRows);
process.stdout.write(JSON.stringify({terminal:terminalSql,marker}));"#;
        let output = std::process::Command::new(
            std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_| "node".into()),
        )
        .arg("--conditions=react-server")
        .arg("--input-type=module")
        .arg("-e")
        .arg(script)
        .arg(dir.join("supply-terminal-receipts.ts"))
        .arg(dir.join("finance-record-sources.ts"))
        .arg(dir.join("service-initial-physical-stock-count-source-sql.ts"))
        .output()
        .expect("pinned Node TS source oracle");
        assert!(output.status.success(), "actual TS source: {}", String::from_utf8_lossy(&output.stderr));
        let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual_terminal = oracle["terminal"].as_str().unwrap();
        let actual_marker = oracle["marker"].as_str().unwrap();
        let projected_terminal = actual_terminal
            .replace("FROM inventory_counts count", "FROM initial_count_history_counts count")
            .replace("JOIN inventory_count_decisions event", "JOIN count_history_decisions event")
            .replace("JOIN command_receipts command", "JOIN initial_count_history_support_receipts command");
        let projected_marker = actual_marker
            .replace("inventory_counts c", "initial_count_history_counts c")
            .replace("command_receipts", "initial_count_history_receipts");
        assert_ne!(actual_terminal, projected_terminal);
        assert_ne!(actual_marker, projected_marker);

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_sources(&mut tx).await.unwrap();
        lock_count_terminal_history_source(&mut tx).await.unwrap();
        lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();
        let counts = inspect_initial_count_history_source(&mut tx).await.unwrap();
        let decisions = inspect_count_terminal_history_source(&mut tx).await.unwrap();
        let receipts = inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();
        let terminal_sql = format!(
            "WITH {}, {}, {} SELECT EXISTS(SELECT 1 FROM ({}) t WHERE t.kind='count' AND t.company_id=$1 AND t.id=$2)",
            counts.cte_sql(), decisions.cte_sql(), receipts.cte_sql(), projected_terminal
        );
        let terminal_raw = format!("SELECT EXISTS(SELECT 1 FROM ({actual_terminal}) t WHERE t.kind='count' AND t.company_id=$1 AND t.id=$2)");
        let marker_sql = format!(
            "WITH {}, {} SELECT EXISTS(SELECT 1 FROM ({}) m WHERE m.company_id=$1 AND m.id=$2)",
            counts.cte_sql(), receipts.cte_sql(), projected_marker
        );
        let marker_raw = format!("SELECT EXISTS(SELECT 1 FROM ({actual_marker}) m WHERE m.company_id=$1 AND m.id=$2)");
        async fn eval(c: &mut sqlx::PgConnection, sql: &str, company: &str, id: &str) -> bool {
            sqlx::query_scalar::<_, bool>(AssertSqlSafe(sql)).bind(company).bind(id).fetch_one(c).await.unwrap()
        }
        async fn compare(c: &mut sqlx::PgConnection, raw: &str, projected: &str, company: &str, id: &str) -> bool {
            let actual = eval(c, raw, company, id).await;
            let candidate = eval(c, projected, company, id).await;
            assert_eq!(actual, candidate, "{company} {id}");
            actual
        }

        // Synthetic rows stay in this rollback-only unit. Deferred owner guards
        // remain enabled; no marker/source is committed or treated as lineage.
        sqlx::raw_sql(r#"INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES
('terminal-history-journal-null','company','chart','period','THJ-N','2026-01-10','THB','stock_count','terminal-history-marker-null','fixture','draft','maker',0,0),
('terminal-history-journal-missing','company','chart','period','THJ-M','2026-01-10','THB','stock_count','terminal-history-marker-missing','fixture','draft','maker',0,0),
('terminal-history-journal-foreign','foreign','chart','period','THJ-F','2026-01-10','THB','stock_count','terminal-history-marker-foreign','fixture','draft','maker',0,0),
('terminal-history-journal-authority','company','chart','period','THJ-A','2026-01-10','THB','stock_count','terminal-history-marker-authority','fixture','draft','maker',0,0);
INSERT INTO public.inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,version,journal_id,created_by,prepared_by,created_at,updated_at) VALUES
('terminal-history-good','company','branch','warehouse','TH-C1','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-chain','company','branch','warehouse','TH-C2','2026-01-10','fixture','posted',3,NULL,'maker','maker',0,0),
('terminal-history-stale','company','branch','warehouse','TH-C3','2026-01-10','fixture','posted',3,NULL,'maker','maker',0,0),
('terminal-history-too-early','company','branch','warehouse','TH-C4','2026-01-10','fixture','posted',1,NULL,'maker','maker',0,0),
('terminal-history-version-mismatch','company','branch','warehouse','TH-C5','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-foreign','foreign','branch','warehouse','TH-C6','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-null','company','branch','warehouse','TH-C7','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-missing','company','branch','warehouse','TH-C8','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-payment-only','company','branch','warehouse','TH-C9','2026-01-10','fixture','posted',2,NULL,'maker','maker',0,0),
('terminal-history-marker-null','company','branch','warehouse','TH-M1','2026-01-10','fixture','draft',1,'terminal-history-journal-null','maker','maker',0,0),
('terminal-history-marker-missing','company','branch','warehouse','TH-M2','2026-01-10','fixture','draft',1,'terminal-history-journal-missing','maker','maker',0,0),
('terminal-history-marker-foreign','foreign','branch','warehouse','TH-M3','2026-01-10','fixture','draft',1,'terminal-history-journal-foreign','maker','maker',0,0),
('terminal-history-marker-authority','company','branch','warehouse','TH-M4','2026-01-10','fixture','draft',1,'terminal-history-journal-authority','maker','maker',0,0);
INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES
('terminal-history-good-post','inventory.count.post','user','fixture','terminal-history-good','{"status":"posted","version":2}','{"companyId":"company"}',0),
('terminal-history-chain-post','inventory.count.post','user','fixture','terminal-history-chain','{"status":"posted","version":2}','{"companyId":"company"}',0),
('terminal-history-stale-post','inventory.count.post','user','fixture','terminal-history-stale','{"status":"posted","version":2}','{"companyId":"company"}',0),
('terminal-history-too-early-post','inventory.count.post','user','fixture','terminal-history-too-early','{"status":"posted","version":2}','{"companyId":"company"}',0),
('terminal-history-version-mismatch-post','inventory.count.post','user','fixture','terminal-history-version-mismatch','{"status":"posted","version":3}','{"companyId":"company"}',0),
('terminal-history-foreign-post','inventory.count.post','user','fixture','terminal-history-foreign','{"status":"posted","version":2}','{"companyId":"foreign"}',0),
('terminal-history-null-post','inventory.count.post','user','fixture','terminal-history-null','null','{"companyId":"company"}',0),
('terminal-history-missing-post','inventory.count.post','user','fixture','terminal-history-missing','{}','{"companyId":"company"}',0),
('terminal-history-payment-only-pay','finance.payment.create','user','fixture','terminal-history-payment-only','{"status":"posted","version":2}','{"companyId":"company"}',0),
('terminal-history-unrelated-payment','finance.payment.create','user','fixture','terminal-history-good','{"status":"posted","version":2}','[]',0),
('terminal-history-marker-null-create','inventory.count.create','maker','fixture','terminal-history-marker-null','{"id":"terminal-history-marker-null","status":"draft","version":1,"_serviceInitialPhysicalStockCount":null}','{"companyId":"company"}',0),
('terminal-history-marker-missing-create','inventory.count.create','maker','fixture','terminal-history-marker-missing','{"id":"terminal-history-marker-missing","status":"draft","version":1}','{"companyId":"company"}',0),
('terminal-history-marker-foreign-create','inventory.count.create','maker','fixture','terminal-history-marker-foreign','{"id":"terminal-history-marker-foreign","status":"draft","version":1,"_serviceInitialPhysicalStockCount":null}','{"companyId":"foreign"}',0),
('terminal-history-marker-authority-create','inventory.count.create','maker','fixture','terminal-history-marker-authority','{"id":"terminal-history-marker-authority","status":"draft","version":1,"_serviceInitialPhysicalStockCount":null}','{"companyId":"foreign"}',0),
('terminal-history-chain-bind','attachment.record.bind','user','fixture','terminal-history-chain','{"setId":"terminal-history-chain-set","version":3}','{"companyId":"company"}',1);
INSERT INTO public.inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES
('terminal-history-good-decision','terminal-history-good','terminal-history-good-post','post','user',1,'fixture',0),
('terminal-history-chain-decision','terminal-history-chain','terminal-history-chain-post','post','user',1,'fixture',0),
('terminal-history-stale-decision','terminal-history-stale','terminal-history-stale-post','post','user',1,'fixture',0),
('terminal-history-too-early-decision','terminal-history-too-early','terminal-history-too-early-post','post','user',1,'fixture',0),
('terminal-history-version-mismatch-decision','terminal-history-version-mismatch','terminal-history-version-mismatch-post','post','user',1,'fixture',0),
('terminal-history-foreign-decision','terminal-history-foreign','terminal-history-foreign-post','post','user',1,'fixture',0),
('terminal-history-null-decision','terminal-history-null','terminal-history-null-post','post','user',1,'fixture',0),
('terminal-history-missing-decision','terminal-history-missing','terminal-history-missing-post','post','user',1,'fixture',0),
('terminal-history-payment-only-decision','terminal-history-payment-only','terminal-history-payment-only-pay','post','user',1,'fixture',0);
INSERT INTO public.attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES
('terminal-history-chain-set','company','stock_count','terminal-history-chain','record','count_sheet',2,1,'internal','{}','bound','user',0,0,1,'terminal-history-chain-bind');
INSERT INTO public.attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES
('terminal-history-chain-event','company','terminal-history-chain-set','user','set.bound','{"bindingId":"terminal-history-chain","bindingType":"record_document"}',1);"#).execute(&mut *tx).await.unwrap();
        // The installed receipt triggers and authority expression index reject
        // invalid JSON, so no guard is weakened to fabricate a malformed row.
        // The unrelated payment receipt is valid but structurally outside this
        // Count branch and must not be selected or evaluated by either SQL.
        assert!(compare(&mut tx, &terminal_raw, &terminal_sql, "foreign", "terminal-history-foreign").await);
        for (id, expected) in [
            ("terminal-history-good", true),
            ("terminal-history-chain", true),
            ("terminal-history-stale", false),
            ("terminal-history-too-early", false),
            ("terminal-history-version-mismatch", false),
            ("terminal-history-foreign", false),
            ("terminal-history-null", false),
            ("terminal-history-missing", false),
            ("terminal-history-payment-only", false),
        ] {
            assert_eq!(compare(&mut tx, &terminal_raw, &terminal_sql, "company", id).await, expected, "{id}");
        }
        // JSON null (key present) and missing keys are different for the
        // Service marker: jsonb_exists is true for null, false for missing.
        assert!(compare(&mut tx, &marker_raw, &marker_sql, "company", "terminal-history-journal-null").await);
        assert!(!compare(&mut tx, &marker_raw, &marker_sql, "company", "terminal-history-journal-missing").await);
        // A foreign Count is not visible under the selected Company, while the
        // receipt authority Company is deliberately ignored by the legacy join.
        assert!(!compare(&mut tx, &marker_raw, &marker_sql, "company", "terminal-history-journal-foreign").await);
        assert!(compare(&mut tx, &marker_raw, &marker_sql, "foreign", "terminal-history-journal-foreign").await);
        assert!(compare(&mut tx, &marker_raw, &marker_sql, "company", "terminal-history-journal-authority").await);

        // A stale terminal version must be re-evaluated by the consumer; the
        // retained source fence is an identity/lock fence, not row contents.
        sqlx::query("SAVEPOINT terminal_history_stale_version").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE public.command_receipts SET result_json='{\"status\":\"posted\",\"version\":3}' WHERE command_id='terminal-history-good-post'").execute(&mut *tx).await.unwrap();
        assert!(!compare(&mut tx, &terminal_raw, &terminal_sql, "company", "terminal-history-good").await);
        counts.require_current(&mut tx).await.unwrap();
        decisions.require_current(&mut tx).await.unwrap();
        receipts.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT terminal_history_stale_version").execute(&mut *tx).await.unwrap();
        assert!(compare(&mut tx, &terminal_raw, &terminal_sql, "company", "terminal-history-good").await);

        // No row, receipt, decision or attachment survives the enclosing rollback.
        tx.rollback().await.unwrap();
        let rows: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM public.inventory_counts WHERE id LIKE 'terminal-history-%'),(SELECT count(*) FROM public.command_receipts WHERE command_id LIKE 'terminal-history-%'),(SELECT count(*) FROM public.inventory_count_decisions WHERE id LIKE 'terminal-history-%'),(SELECT count(*) FROM public.attachment_sets WHERE id LIKE 'terminal-history-%')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(rows, (0, 0, 0, 0));

        // Lock loss invalidates the retained same-unit source witnesses.
        let mut saved = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();
        sqlx::query("SAVEPOINT before_terminal_history_locks").execute(&mut *saved).await.unwrap();
        lock_initial_count_history_sources(&mut saved).await.unwrap();
        lock_count_terminal_history_source(&mut saved).await.unwrap();
        lock_initial_count_history_receipt_sources(&mut saved).await.unwrap();
        let saved_counts = inspect_initial_count_history_source(&mut saved).await.unwrap();
        let saved_decisions = inspect_count_terminal_history_source(&mut saved).await.unwrap();
        let saved_receipts = inspect_initial_count_history_receipt_join(&mut saved).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_terminal_history_locks").execute(&mut *saved).await.unwrap();
        assert!(saved_counts.require_current(&mut saved).await.is_err());
        assert!(saved_decisions.require_current(&mut saved).await.is_err());
        assert!(saved_receipts.require_current(&mut saved).await.is_err());
        saved.rollback().await.unwrap();
    }).await;
}
