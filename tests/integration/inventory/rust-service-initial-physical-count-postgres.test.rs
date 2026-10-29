#[path = "../../support/inventory/consumption-facts.rs"]
#[allow(dead_code)]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_inventory::public::service_initial_physical_count::*;
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::BusinessError;

#[tokio::test]
async fn history_observes_any_stock_row_and_rejects_a_changed_source_in_the_same_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        let result = PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            let before = read_initial_count_inventory_history(c, "warehouse", "product", None).await?;
            assert!(before.data().has_history);
            sqlx::query("UPDATE commerce_stock SET quantity_milli=0,value_minor=0 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *c).await?;
            before.require_current(c).await
        })).await;
        facts::denied(result, "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
        let quantity: i64 = sqlx::query_scalar("SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(quantity, 5000);
    }).await;
}

#[tokio::test]
async fn inventory_effects_follow_versions_and_rollback_with_the_enclosing_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
        let result = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            // Deliberately rolled-back local fixture receipts exercise Inventory's
            // effects only; real receipt provenance is tested by composition.
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
            let history = read_initial_count_inventory_history(c,"warehouse","product",None).await?;
            assert!(!history.data().has_history);
            let draft=InitialCountDraft {
                proof:InitialCountSourceProof { count_id:"count-cmd".into(),selection:InitialCountSelection { company_id:"company".into(), branch_id:"branch".into(), warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into() },count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis { chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into() } },
                product:InitialCountProductSnapshot { version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::None },reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::None,0,None,vec![]).unwrap(),created_at:1,
            };
            let draft_effects=insert_initial_count_draft(c,&draft,&history).await.expect("draft effects");
            draft_effects.require_current(c).await?;
            for (index, kind) in [CountAction::Submit, CountAction::Approve, CountAction::Post].into_iter().enumerate() {
                sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,'inventory.count.action','maker','hash','count-cmd','{}',0,'{\"companyId\":\"company\"}')").bind(format!("action-{index}")).execute(&mut *c).await?;
                let before=read_initial_count_record(c,"company",None,"count-cmd").await?.unwrap();
                let transition=InitialCountTransition { count_id:"count-cmd".into(), company_id:"company".into(),tracking:TrackingMode::None,expected_version:index as i64+1,action:kind,actor_id:if index==0 {"maker"} else {"reviewer"}.into(),reason:if index==0 {None} else {Some("verified".into())},command_id:format!("action-{index}"),decision_id:format!("decision-{index}"),journal_id:None,now:index as i64+2 };
                let effects=apply_initial_count_transition(c,&transition,&before).await.expect("transition effects");
                effects.require_current(c).await?;
                let stale=apply_initial_count_transition(c,&transition,&before).await;
                assert!(matches!(stale,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
            }
            let actual:(String,i64,i64,i64) = sqlx::query_as("SELECT c.status,c.version,s.quantity_milli,s.value_minor FROM inventory_counts c JOIN commerce_stock s ON s.warehouse_id=c.warehouse_id WHERE c.id='count-cmd' AND s.product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(actual,("posted".into(),4,0,0));
            Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"FIXTURE_ABORT","Rollback after verifying inventory effects.").into())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "FIXTURE_ABORT"), "unexpected result: {result:?}");
        let counts:(i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT count(*) FROM inventory_adjustment_moves WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(0,0,0));
    }).await;
}

#[tokio::test]
async fn before_insert_tamper_fails_planned_witness_and_rolls_back_receipt_and_source() {
    fixture::with_database(2,|pool|async move{
  facts::setup(&pool,"none").await;
  sqlx::raw_sql("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'; INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0); CREATE FUNCTION fixture_count_line_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.counted_value_minor:=NEW.counted_value_minor+1; RETURN NEW; END $$; CREATE TRIGGER fixture_count_line_tamper BEFORE INSERT ON inventory_count_lines FOR EACH ROW EXECUTE FUNCTION fixture_count_line_tamper()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_initial_count_inventory_sources(c).await?;
   sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
   let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
   let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::None},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::None,0,None,vec![]).unwrap(),created_at:1};
   let effects=insert_initial_count_draft(c,&draft,&history).await?;
   effects.require_current(c).await
  })).await;
  facts::denied(result,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('cmd','setup')),(SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
 }).await;
}

#[tokio::test]
async fn before_stock_update_tamper_fails_planned_witness_and_rolls_back_every_effect() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_stock_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.value_minor:=NEW.value_minor+1; RETURN NEW; END $$; CREATE TRIGGER fixture_stock_tamper BEFORE UPDATE ON commerce_stock FOR EACH ROW EXECUTE FUNCTION fixture_stock_tamper()").execute(pool.raw()).await.unwrap();
        let result = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            // Deliberately rolled-back local fixture receipts exercise Inventory's
            // effects only; real receipt provenance is tested by composition.
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
            let history = read_initial_count_inventory_history(c,"warehouse","product",None).await?;
            assert!(!history.data().has_history);
            let draft=InitialCountDraft {
                proof:InitialCountSourceProof { count_id:"count-cmd".into(),selection:InitialCountSelection { company_id:"company".into(), branch_id:"branch".into(), warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into() },count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis { chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into() } },
                product:InitialCountProductSnapshot { version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::None },reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::None,0,None,vec![]).unwrap(),created_at:1,
            };
            let draft_effects=insert_initial_count_draft(c,&draft,&history).await.expect("draft effects");
            draft_effects.require_current(c).await?;
            for (index, kind) in [CountAction::Submit, CountAction::Approve, CountAction::Post].into_iter().enumerate() {
                sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,'inventory.count.action','maker','hash','count-cmd','{}',0,'{\"companyId\":\"company\"}')").bind(format!("action-{index}")).execute(&mut *c).await?;
                let before=read_initial_count_record(c,"company",None,"count-cmd").await?.unwrap();
                let transition=InitialCountTransition { count_id:"count-cmd".into(), company_id:"company".into(),tracking:TrackingMode::None,expected_version:index as i64+1,action:kind,actor_id:if index==0 {"maker"} else {"reviewer"}.into(),reason:if index==0 {None} else {Some("verified".into())},command_id:format!("action-{index}"),decision_id:format!("decision-{index}"),journal_id:None,now:index as i64+2 };
                let effects=apply_initial_count_transition(c,&transition,&before).await.expect("transition effects");
                effects.require_current(c).await?;
                let stale=apply_initial_count_transition(c,&transition,&before).await;
                assert!(matches!(stale,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
            }
            let actual:(String,i64,i64,i64) = sqlx::query_as("SELECT c.status,c.version,s.quantity_milli,s.value_minor FROM inventory_counts c JOIN commerce_stock s ON s.warehouse_id=c.warehouse_id WHERE c.id='count-cmd' AND s.product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(actual,("posted".into(),4,0,0));
            Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"FIXTURE_ABORT","Rollback after verifying inventory effects.").into())
        })).await;
        facts::denied(result, "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
        let counts:(i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT count(*) FROM inventory_adjustment_moves WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(0,0,0));
    }).await;
}

#[tokio::test]
async fn oversized_history_is_rejected_before_image_transfer() {
    fixture::with_database(2,|pool|async move{
  facts::setup(&pool,"none").await;
  sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at) VALUES('large-count','company','branch','warehouse','LARGE','2026-01-10',repeat('x',2097153),'maker','maker',0,0); INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('large-line','large-count','product',1,'Product','piece',0,0,0,0,0,0)").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{lock_initial_count_inventory_sources(c).await?;let _=read_initial_count_inventory_history(c,"warehouse","product",None).await?;Ok(())})).await;
  assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_))));
 }).await;
}

#[tokio::test]
async fn tracked_positive_stock_effects_match_plan_and_roll_back_with_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, "none").await;
        sqlx::query("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE commerce_products SET tracking_mode='lot' WHERE id='product'").execute(pool.raw()).await.unwrap();
        let result = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            // Deliberately rolled-back local fixture receipts exercise Inventory's
            // effects only; real receipt provenance is tested by composition.
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,created_by,created_at,updated_at) VALUES('fixture-journal','company','chart','period','J-COUNT','2026-01-10','THB','maker',0,0)").execute(&mut *c).await?;
            let history = read_initial_count_inventory_history(c,"warehouse","product",None).await?;
            assert!(!history.data().has_history);
            let draft=InitialCountDraft {
                proof:InitialCountSourceProof { count_id:"count-cmd".into(),selection:InitialCountSelection { company_id:"company".into(), branch_id:"branch".into(), warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into() },count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis { chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into() } },
                product:InitialCountProductSnapshot { version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::Lot },reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::Lot,1500,None,vec![InitialCountLot{lot_code:Some("LOT-A".into()),serial_number:None,manufactured_date:None,expiry_date:None,quantity_milli:1500,unit_cost_minor:100}]).unwrap(),created_at:1,
            };
            let draft_effects=insert_initial_count_draft(c,&draft,&history).await.expect("draft effects");
            draft_effects.require_current(c).await?;
            for (index, kind) in [CountAction::Submit, CountAction::Approve, CountAction::Post].into_iter().enumerate() {
                sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,'inventory.count.action','maker','hash','count-cmd','{}',0,'{\"companyId\":\"company\"}')").bind(format!("action-{index}")).execute(&mut *c).await?;
                let before=read_initial_count_record(c,"company",None,"count-cmd").await?.unwrap();
                let transition=InitialCountTransition { count_id:"count-cmd".into(), company_id:"company".into(),tracking:TrackingMode::Lot,expected_version:index as i64+1,action:kind,actor_id:if index==0 {"maker"} else {"reviewer"}.into(),reason:if index==0 {None} else {Some("verified".into())},command_id:format!("action-{index}"),decision_id:format!("decision-{index}"),journal_id:if index==2 {Some("fixture-journal".into())} else {None},now:index as i64+2 };
                let effects=apply_initial_count_transition(c,&transition,&before).await.expect("transition effects");
                effects.require_current(c).await?;
                let stale=apply_initial_count_transition(c,&transition,&before).await;
                assert!(matches!(stale,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
            }
            let actual:(String,i64,i64,i64) = sqlx::query_as("SELECT c.status,c.version,s.quantity_milli,s.value_minor FROM inventory_counts c JOIN commerce_stock s ON s.warehouse_id=c.warehouse_id WHERE c.id='count-cmd' AND s.product_id='product'").fetch_one(&mut *c).await?;
            assert_eq!(actual,("posted".into(),4,1500,150));
            Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"FIXTURE_ABORT","Rollback after verifying inventory effects.").into())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "FIXTURE_ABORT"), "unexpected result: {result:?}");
        let counts:(i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT count(*) FROM inventory_adjustment_moves WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(0,0,0));
    }).await;
}

#[tokio::test]
async fn late_unplanned_decision_fails_draft_witness_and_rolls_back() {
    fixture::with_database(2,|pool|async move{
  facts::setup(&pool,"none").await;
  sqlx::raw_sql("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'; INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0); CREATE FUNCTION fixture_count_line_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES('extra-decision',NEW.count_id,'cmd','cancel','maker',1,'unexpected',1); RETURN NEW; END $$; CREATE TRIGGER fixture_count_line_tamper AFTER INSERT ON inventory_service_initial_physical_sources FOR EACH ROW EXECUTE FUNCTION fixture_count_line_tamper()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_initial_count_inventory_sources(c).await?;
   sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
   let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
   let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::None},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::None,0,None,vec![]).unwrap(),created_at:1};
   let effects=insert_initial_count_draft(c,&draft,&history).await?;
   effects.require_current(c).await
  })).await;
  facts::denied(result,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('cmd','setup')),(SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
 }).await;
}

#[tokio::test]
async fn late_unplanned_move_fails_draft_witness_and_rolls_back() {
    fixture::with_database(2,|pool|async move{
  facts::setup(&pool,"none").await;
  sqlx::raw_sql("DELETE FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'; INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('variance','company','chart','5400','Variance','expense','THB',0,0); CREATE FUNCTION fixture_count_line_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES('extra-move',NEW.count_id,NEW.warehouse_id,NEW.product_id,0,0,'2026-01-10',1); RETURN NEW; END $$; CREATE TRIGGER fixture_count_line_tamper AFTER INSERT ON inventory_service_initial_physical_sources FOR EACH ROW EXECUTE FUNCTION fixture_count_line_tamper()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_initial_count_inventory_sources(c).await?;
   sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('setup','fixture.setup','maker','hash','company','{}',0,'{\"companyId\":\"company\"}'),('cmd','inventory.count.create','maker','hash','count-cmd','{\"id\":\"count-cmd\",\"status\":\"draft\",\"version\":1,\"_serviceInitialPhysicalStockCount\":{}}',0,'{\"companyId\":\"company\"}')").execute(&mut *c).await?;
   let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
   let draft=InitialCountDraft{proof:InitialCountSourceProof{count_id:"count-cmd".into(),selection:InitialCountSelection{company_id:"company".into(),branch_id:"branch".into(),warehouse_id:"warehouse".into(),product_id:"product".into(),count_date:"2026-01-10".into()},count_command_id:"cmd".into(),service_setup_command_id:"setup".into(),service_setup_request_hash:"hash".into(),finance:InitialCountFinanceBasis{chart_id:"chart".into(),inventory_account_id:"inventory".into(),variance_account_id:"variance".into(),currency:"THB".into()}},product:InitialCountProductSnapshot{version:1,description:"Product".into(),unit:"piece".into(),tracking:TrackingMode::None},reason:"Actual count".into(),actor_id:"maker".into(),value:value_initial_count(TrackingMode::None,0,None,vec![]).unwrap(),created_at:1};
   let effects=insert_initial_count_draft(c,&draft,&history).await?;
   effects.require_current(c).await
  })).await;
  facts::denied(result,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('cmd','setup')),(SELECT count(*) FROM inventory_counts WHERE id='count-cmd'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-cmd')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
 }).await;
}

#[tokio::test]
async fn blocked_source_lock_sees_the_committed_stock_change_before_first_snapshot() {
    fixture::with_database(3,|pool|async move{
  facts::setup(&pool,"none").await;
  let mut writer=pool.raw().begin().await.unwrap();
  sqlx::query("LOCK TABLE commerce_stock IN ACCESS EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
  let (started,received)=tokio::sync::oneshot::channel();let reader_pool=pool.clone();
  let reader=tokio::spawn(async move{PgUnitOfWork::new(reader_pool).run_once::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let signal=std::sync::Mutex::new(Some(started));Box::pin(async move{
   signal.lock().unwrap().take().unwrap().send(()).unwrap();lock_initial_count_inventory_sources(c).await?;
   let history=read_initial_count_inventory_history(c,"warehouse","product",None).await?;
   let value:serde_json::Value=serde_json::from_str(&history.data().image).unwrap();assert_eq!(value["stock"][0]["quantity_milli"],4000);Ok(())
  })}).await});
  received.await.unwrap();let mut blocked=false;
  for _ in 0..200 {blocked=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='commerce_stock'::regclass AND NOT granted)").fetch_one(pool.raw()).await.unwrap();if blocked{break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
  assert!(blocked,"reader must be waiting for the owned stock lock");
  sqlx::query("UPDATE commerce_stock SET quantity_milli=4000 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();writer.commit().await.unwrap();
  reader.await.unwrap().unwrap();
 }).await;
}

#[tokio::test]
async fn ordered_decision_detail_detects_changed_pre_action_values_and_rolls_back() {
    use erpx_server_inventory::public::service_initial_count_decision::read_initial_count_decision_detail;
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool,"none").await;
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_initial_count_inventory_sources(c).await?;
            sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,net_value_minor,created_by,prepared_by,created_at,updated_at) VALUES('detail-count','company','branch','warehouse','D-1','2026-01-10','Fixture',0,'maker','maker',1,1)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) VALUES('detail-line','detail-count','product',1,'Product','piece',0,0,0,1000,500,500,0)").execute(&mut *c).await?;
            let detail=read_initial_count_decision_detail(c,"detail-count","warehouse","SKU","none").await?;
            let line=&detail.detail().lines;
            assert!(line.starts_with("[{\"id\":\"detail-line\",\"count_id\":\"detail-count\""));
            assert!(line.ends_with("\"sku\":\"SKU\",\"tracking_mode\":\"none\",\"current_quantity_milli\":5000,\"current_value_minor\":1000,\"current_version\":1}]"),"{line}");
            assert_eq!(detail.detail().lot_lines,"[]");detail.require_current(c).await?;
            sqlx::query("UPDATE inventory_count_lines SET counted_value_minor=501 WHERE id='detail-line'").execute(&mut *c).await?;
            detail.require_current(c).await
        })).await;
        facts::denied(result,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_counts WHERE id='detail-count'").fetch_one(pool.raw()).await.unwrap();assert_eq!(n,0);
    }).await;
}
