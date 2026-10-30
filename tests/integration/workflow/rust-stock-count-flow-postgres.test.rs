#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::stock_count_flow::*;
use std::path::Path;
fn person(id: &str) -> StockCountParticipant {
    StockCountParticipant {
        user_id: id.into(),
        name: id.into(),
        team_name: "Company".into(),
    }
}
fn view(status: &str, record_version: i64, flow_version: i64, revision: i64) -> StockCountFlowView {
    let record = StockCountFlowRecord {
        company_id: "company".into(),
        count_id: "count-example".into(),
        branch_id: "branch".into(),
        status: status.into(),
        version: record_version,
        creator: "maker".into(),
        preparer: "maker".into(),
        approved_by: None,
    };
    let fact = |step, who: &str| StockCountStepFacts {
        step,
        candidates: vec![person(who)],
        selected: Some(who.into()),
        actor_role_allowed: true,
    };
    resolve_stock_count_flow(
        record,
        flow_version,
        revision,
        [
            fact(StockCountStep::Draft, "maker"),
            fact(StockCountStep::Submitted, "reviewer"),
            fact(StockCountStep::Approved, "reviewer"),
        ],
        "maker",
        true,
        true,
    )
}
#[tokio::test]
async fn route_assignment_and_action_commit_with_exact_versions_and_event_images() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_stock_count_flow_sources(c).await?;
   let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
   let v=view("draft",1,0,0);
   let assign=StockCountFlowAssignmentWitness::prepare(basis,&v,StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command:"assign-command",event:"assign-event",now:1})?;
   assert_eq!(assign.result_version(),2);assert_eq!(assign.notification().recipient,"reviewer");assign.apply(c).await?;assign.retain(c).await?;Ok(())
  })).await.unwrap();
  let row:(i64,i64,String)=sqlx::query_as("SELECT version,defaults_revision,steps_json FROM flow_routes WHERE company_id='company' AND kind='stock_count' AND record_id='count-example'").fetch_one(pool.raw()).await.unwrap();assert_eq!((row.0,row.1),(2,0));assert_eq!(serde_json::from_str::<serde_json::Value>(&row.2).unwrap()["submitted"]["primaryUserId"],"reviewer");
  PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_stock_count_flow_sources(c).await?;let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;let v=view("draft",1,2,0);
   let action=StockCountFlowActionWitness::prepare(basis,&v,StockCountFlowActionRequest{actor:"maker",actor_name:"Maker",action:"submit",reason:"",command:"submit-command",event:"submit-event",now:2})?;
   action.apply_before(c).await?;let notification=action.apply_after(c,"submitted").await?.unwrap();assert_eq!(notification.recipient,"reviewer");assert_eq!(notification.record_version,2);action.retain(c,"submitted").await?;Ok(())
  })).await.unwrap();
  let event:(String,String,i64,Option<String>)=sqlx::query_as("SELECT step_key,action,record_version,next_user_id FROM flow_events WHERE id='submit-event'").fetch_one(pool.raw()).await.unwrap();assert_eq!(event,("draft".into(),"submit".into(),2,Some("reviewer".into())));
 }).await;
}

#[path = "../../support/workflow/command-actor.rs"]
mod actor_fixture;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{Clock, CompanyId, IdentifierSource, RecordId, TimestampMillis};
use erpx_server_workflow::public::commands::*;
use erpx_server_workflow::public::service_initial_count_receipt::*;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
struct ClockFixture;
impl Clock for ClockFixture {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1)
    }
}
struct IdFixture;
impl IdentifierSource for IdFixture {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn fixture_actor() -> CommandActor {
    actor_fixture::parse_frozen_actor(
        r#"{"userId":"maker","assignmentId":"assignment","role":"holding_owner","companyId":"company","branchId":null,"departmentId":null,"authority":{"sessionId":"session","revision":1,"assignmentVersion":1}}"#,
    )
}
struct FixtureAuthority;
impl PgCommandAuthority for FixtureAuthority {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(fixture_actor())
    }
    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        Ok(())
    }
}
struct CountFixtureEffects;
impl PgOwnerCommandEffects for CountFixtureEffects {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_service_initial_count_receipts(c).await
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let result = CommandJson::parse(
            r#"{"id":"count-count-key","status":"draft","version":1,"_serviceInitialPhysicalStockCount":{"kind":"service-initial-physical-stock-count","countId":"count-count-key","companyId":"company","warehouseId":"warehouse","branchId":"branch","productId":"product","countCommandId":"count-key","serviceSetupCommandId":"setup-key","serviceSetupRequestHash":"setuphash","chartId":"chart","inventoryAccountId":"inv","varianceAccountId":"variance","currency":"THB"}}"#,
        )?;
        Ok(WorkflowOutput {
            version: 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at) VALUES('count-count-key','company','branch','warehouse','COUNT-TEST','2026-01-10','Actual count','maker','maker',1,1); INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES('line','count-count-key','product',1,'Product','piece',0,0,0,0,0,0); INSERT INTO inventory_service_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,service_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) SELECT 'count-count-key','company','branch','warehouse','product','count-key','setup-key','chart','inv','variance','THB',result_json::jsonb->'_serviceInitialPhysicalStockCount','maker',1 FROM command_receipts WHERE command_id='count-key'").execute(c).await?;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let receipt =
            read_service_initial_count_receipt(c, "count-key", "company", "count-count-key")
                .await?;
        if receipt.facts().is_none() {
            return Err(BusinessError::invalid(
                "MISSING_FIXTURE_RECEIPT",
                "Expected fixture receipt.",
            )
            .into());
        }
        receipt.require_current(c).await
    }
}
async fn seed_committed_service_mapping(pool: &DatabasePool) {
    let mut t = pool.raw().begin().await.unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('company','company','company','Company','company'); INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES('branch','branch','branch','Branch','company','company','branch'); INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','company','Chart','THB','maker',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES('ar','company','chart','110','AR','asset','THB','receivable',0,0),('ap','company','chart','210','AP','liability','THB','payable',0,0),('revenue','company','chart','410','Revenue','revenue','THB','none',0,0),('inv','company','chart','120','Inventory','asset','THB','none',0,0),('clearing','company','chart','220','Clearing','liability','THB','none',0,0),('cogs','company','chart','510','COGS','expense','THB','none',0,0),('input','company','chart','130','Input','asset','THB','none',0,0),('output','company','chart','230','Output','liability','THB','none',0,0),('cash','company','chart','140','Cash','asset','THB','bank',0,0),('variance','company','chart','5400','Variance','expense','THB','none',0,0); INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','ar','ap','revenue','inv','clearing','cogs','input','output','cash'); INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','company','branch','W','Warehouse'); INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES('product','company','P','Product',0)").execute(&mut *t).await.unwrap();
    sqlx::raw_sql("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'setup-key','commerce.service-parts-posting-setup.initialize','maker','setuphash','company',jsonb_build_object('id','company','status','configured','version',1,'_servicePartsPostingSetup',jsonb_build_object('kind','service-parts-posting-setup','companyId','company','commandId','setup-key','mode','initialize','mapping',to_jsonb(s)))::text,0,'{\"companyId\":\"company\"}' FROM commerce_settings s WHERE s.company_id='company'; INSERT INTO commerce_service_parts_posting_sources(company_id,command_id,mode,mapping_json,proof_json,created_by,created_at) SELECT 'company','setup-key','initialize',to_jsonb(s),jsonb_build_object('kind','service-parts-posting-setup','companyId','company','commandId','setup-key','mode','initialize','mapping',to_jsonb(s)),'maker',0 FROM commerce_settings s WHERE s.company_id='company'").execute(&mut *t).await.unwrap();
    t.commit().await.unwrap();
}
#[tokio::test]
async fn committed_1062_source_receipt_and_exact_ledger_replay_are_reciprocal() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();seed_committed_service_mapping(&pool).await;
  for expected_replay in [false,true] {PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{
   let command=WorkflowRequest{key:"count-key".into(),operation:"inventory.count.create".into(),record_id:"count-count-key".into(),input:CommandJson::parse(r#"{"expectedVersion":0,"contextKey":"fixture","input":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Actual count","lines":[{"productId":"product","countedQuantity":"0","allocations":[]}]}}"#)?};
   let mut authority=FixtureAuthority;let mut ids=IdFixture;let mut effects=CountFixtureEffects;let mut ledger=PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut authority,&ClockFixture,&mut ids);
   let outcome=ledger.execute(&command,&mut effects).await?;assert_eq!(outcome.replayed,expected_replay);assert!(outcome.result.as_str().contains("_serviceInitialPhysicalStockCount"));Ok(())
  })).await.unwrap();}
  let conflict=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   let command=WorkflowRequest{key:"count-key".into(),operation:"inventory.count.create".into(),record_id:"count-count-key".into(),input:CommandJson::parse(r#"{"expectedVersion":0,"contextKey":"different","input":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Actual count","lines":[{"productId":"product","countedQuantity":"0","allocations":[]}]}}"#)?};
   let mut authority=FixtureAuthority;let mut ids=IdFixture;let mut effects=CountFixtureEffects;let mut ledger=PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut authority,&ClockFixture,&mut ids);
   let _=ledger.execute(&command,&mut effects).await?;Ok(())
  })).await;
  assert!(matches!(conflict,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
  let tamper=sqlx::query("UPDATE command_receipts SET result_json='{}' WHERE command_id='count-key'").execute(pool.raw()).await;
  assert!(tamper.is_err());
  let count:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='count-key'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-count-key'),(SELECT count(*) FROM inventory_count_lines WHERE count_id='count-count-key')").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,(1,1,1));
 }).await;
}

#[tokio::test]
async fn scoped_defaults_and_late_drift_are_fenced_on_the_same_transaction() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::raw_sql("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES('company','stock_count','submitted',NULL,NULL,'company-reviewer','maker','default',1),('company','stock_count','submitted','branch',NULL,'branch-reviewer','maker','default',2)").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_stock_count_flow_sources(c).await?;let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
   assert_eq!(basis.selected_user(StockCountStep::Submitted,None)?.as_deref(),Some("branch-reviewer"));
   assert_eq!(basis.selected_user(StockCountStep::Submitted,Some("preferred"))?.as_deref(),Some("preferred"));
   sqlx::query("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES('company','stock_count','approved','other','maker','late',3)").execute(&mut *c).await?;
   basis.require_current(c).await
  })).await;
  assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="FLOW_PRIMARY_REQUIRED"));
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM flow_defaults WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,2);
 }).await;
}

struct AssignmentFixtureEffects {
    witness: Option<StockCountFlowAssignmentWitness>,
}
impl PgOwnerCommandEffects for AssignmentFixtureEffects {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_stock_count_flow_sources(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let basis = read_stock_count_flow_basis(c, "company", "count-example", "branch").await?;
        let v = view("draft", 1, 0, 0);
        let witness = StockCountFlowAssignmentWitness::prepare(
            basis,
            &v,
            StockCountFlowAssignmentRequest {
                step: StockCountStep::Submitted,
                user: "reviewer",
                expected_flow: 0,
                expected_record: 1,
                company_admin: true,
                actor: "maker",
                actor_name: "Maker",
                reason: "Assign review",
                command: "assign-key",
                event: "assign-event",
                now: 1,
            },
        )?;
        self.witness = Some(witness);
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::parse(r#"{"id":"count-example","version":2}"#)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.witness.as_ref().unwrap().apply(c).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.witness.as_ref().unwrap().retain(c).await
    }
}
#[tokio::test]
async fn assignment_ledger_replay_does_not_duplicate_route_or_flow_event() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  for expected in [false,true] {PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{
   let input=normalize_stock_count_assignment(r#"{"stepKey":"submitted","userId":"reviewer","expectedVersion":0,"recordVersion":1,"reason":"Assign review","saveDefault":false}"#)?;
   let command=WorkflowRequest{key:"assign-key".into(),operation:"flow.assign".into(),record_id:"count-example".into(),input};let mut authority=FixtureAuthority;let mut ids=IdFixture;let mut effects=AssignmentFixtureEffects{witness:None};let mut ledger=PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut authority,&ClockFixture,&mut ids);
   let result=ledger.execute(&command,&mut effects).await?;assert_eq!(result.replayed,expected);assert_eq!(result.result.as_str(),r#"{"id":"count-example","version":2}"#);Ok(())
  })).await.unwrap();}
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM flow_routes WHERE record_id='count-example' AND kind='stock_count'),(SELECT count(*) FROM flow_events WHERE command_id='assign-key'),(SELECT count(*) FROM command_receipts WHERE command_id='assign-key')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(2,1,1));
 }).await;
}

#[tokio::test]
async fn blocked_flow_lock_reads_the_new_committed_default_revision() {
    fixture::with_database(3,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE flow_defaults IN ACCESS EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
  let (started,received)=tokio::sync::oneshot::channel();let reader_pool=pool.clone();
  let reader=tokio::spawn(async move{PgUnitOfWork::new(reader_pool).run_once::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let signal=std::sync::Mutex::new(Some(started));Box::pin(async move{
   signal.lock().unwrap().take().unwrap().send(()).unwrap();lock_stock_count_flow_sources(c).await?;let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
   assert_eq!(basis.defaults_revision(),1);assert_eq!(basis.selected_user(StockCountStep::Submitted,None)?.as_deref(),Some("reviewer"));Ok(())
  })}).await});
  received.await.unwrap();let mut blocked=false;
  for _ in 0..200 {blocked=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='flow_defaults'::regclass AND NOT granted)").fetch_one(pool.raw()).await.unwrap();if blocked{break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
  assert!(blocked,"reader must be waiting for the owned Flow defaults lock");
  sqlx::query("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES('company','stock_count','submitted','reviewer','maker','default',1)").execute(&mut *writer).await.unwrap();writer.commit().await.unwrap();reader.await.unwrap().unwrap();
 }).await;
}

#[tokio::test]
async fn assignment_unplanned_handoff_trigger_rejects_and_rolls_back_receipt_route_and_event() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION fixture_flow_false_handoff() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.next_step_key:='submitted'; NEW.next_user_id:='unexpected'; RETURN NEW; END $$; CREATE TRIGGER fixture_flow_false_handoff BEFORE INSERT ON flow_events FOR EACH ROW EXECUTE FUNCTION fixture_flow_false_handoff()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   let input=normalize_stock_count_assignment(r#"{"stepKey":"submitted","userId":"reviewer","expectedVersion":0,"recordVersion":1,"reason":"Assign review","saveDefault":false}"#)?;
   let command=WorkflowRequest{key:"assign-key".into(),operation:"flow.assign".into(),record_id:"count-example".into(),input};let mut authority=FixtureAuthority;let mut ids=IdFixture;let mut effects=AssignmentFixtureEffects{witness:None};let mut ledger=PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut authority,&ClockFixture,&mut ids);
   let _=ledger.execute(&command,&mut effects).await?;Ok(())
  })).await;
  assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="FLOW_PRIMARY_REQUIRED"));
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='assign-key'),(SELECT count(*) FROM flow_routes WHERE record_id='count-example'),(SELECT count(*) FROM flow_events WHERE record_id='count-example')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
 }).await;
}
#[tokio::test]
async fn oversized_operation_and_result_id_are_rejected_without_variable_projection() {
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::raw_sql("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('oversize-op',repeat('x',7000),'maker','hash','count-oversize-op',jsonb_build_object('padding',repeat('x',2097153))::text,0,'{\"companyId\":\"company\"}'),('oversize-result','inventory.count.create','maker','hash','count-oversize-result',jsonb_build_object('id',repeat('x',2097153),'status','draft','version',1)::text,0,'{\"companyId\":\"company\"}')").execute(pool.raw()).await.unwrap();
  for key in ["oversize-op","oversize-result"] {let outcome=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move{
   lock_service_initial_count_receipts(c).await?;let count=format!("count-{key}");let _=read_service_initial_count_receipt(c,key,"company",&count).await?;Ok(())
  })).await;assert!(matches!(outcome,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));}
 }).await;
}

#[tokio::test]
async fn original_service_setup_receipt_exposes_exact_request_hash_for_matching_mapping() {
    use erpx_server_workflow::public::service_count_setup_receipt::{
        read_service_count_setup_history, read_service_count_setup_receipt,
    };
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();seed_committed_service_mapping(&pool).await;
  PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   lock_service_initial_count_receipts(c).await?;
   let mapping:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id='company'").fetch_one(&mut *c).await?;
   let source=read_service_count_setup_receipt(c,"company","setup-key",&mapping).await?;assert_eq!(source.facts().unwrap().request_hash,"setuphash");source.require_current(c).await?;
   let historical=read_service_count_setup_history(c,"company","setup-key").await?;assert_eq!(historical.facts(),source.facts());historical.require_current(c).await?;
   assert!(read_service_count_setup_receipt(c,"company","setup-key","{}").await.is_err());Ok(())
  })).await.unwrap();
 }).await;
}

#[tokio::test]
async fn count_decision_evidence_rejects_trigger_tamper_and_rolls_back_receipt() {
    use erpx_server_workflow::public::service_count_decision_evidence::*;
    fixture::with_database(2, |pool| async move {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_count_decision_evidence(c).await?;
            let actor=fixture_actor();
            let facts=ServiceCountDecisionFacts{id:"decision-ok".into(),command_id:"decision-command-ok".into(),record_id:"count-example".into(),action:"inventory.count.approve".into(),reviewed_version:2,input_digest:"1".repeat(64),created_at:1};
            let plan=PlannedServiceCountDecision::prepare(c,&actor,facts).await?;
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('decision-command-ok','inventory.count.approve','maker','hash','count-example','{}',1,$1)").bind(actor.receipt_authority()?.as_str()).execute(&mut *c).await?;
            plan.apply(c).await?;
            plan.require_current(c).await
        })).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_decision_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.input_digest:=repeat('0',64); RETURN NEW; END $$; CREATE TRIGGER fixture_decision_tamper BEFORE INSERT ON decision_evidence FOR EACH ROW EXECUTE FUNCTION fixture_decision_tamper()").execute(pool.raw()).await.unwrap();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_count_decision_evidence(c).await?;
            let actor=fixture_actor();
            let facts=ServiceCountDecisionFacts{id:"decision".into(),command_id:"decision-command".into(),record_id:"count-example".into(),action:"inventory.count.approve".into(),reviewed_version:2,input_digest:"1".repeat(64),created_at:1};
            let plan=PlannedServiceCountDecision::prepare(c,&actor,facts).await?;
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('decision-command','inventory.count.approve','maker','hash','count-example','{}',1,$1)").bind(actor.receipt_authority()?.as_str()).execute(&mut *c).await?;
            plan.apply(c).await?;
            plan.require_current(c).await
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
        let actual:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='decision-command'),(SELECT count(*) FROM decision_evidence WHERE id='decision')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(actual,(0,0));
    }).await;
}

// Isolated fixture actor exercises Workflow's planned ledger evidence only.
struct PlannedLedgerFixture<'a> {
    generated:
        &'a erpx_server_workflow::public::service_count_command_evidence::CountCommandGeneratedIds,
    clock: erpx_server_workflow::public::service_count_command_evidence::CountCommandClock,
    plan: Option<
        erpx_server_workflow::public::service_count_command_evidence::PlannedCountCommandEvidence,
    >,
}
impl PgOwnerCommandEffects for PlannedLedgerFixture<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_service_initial_count_receipts(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let output = WorkflowOutput {
            version: 2,
            result: CommandJson::parse(if request.operation == "inventory.count.update" {
                r#"{"id":"count-fixture","status":"draft","version":2}"#
            } else {
                r#"{"id":"count-fixture","status":"approved","version":2}"#
            })?,
            event_scope: None,
        };
        self.plan=Some(erpx_server_workflow::public::service_count_command_evidence::plan_count_command_evidence(c,actor,request,&output,self.clock).await?);
        Ok(output)
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.plan
            .as_ref()
            .unwrap()
            .retain(c, self.generated)
            .await?
            .require_current(c)
            .await
    }
}
#[tokio::test]
async fn planned_count_command_rejects_corruption_before_retaining_ledger_rows() {
    use erpx_server_workflow::public::service_count_command_evidence::{
        CountCommandClock, CountCommandGeneratedIds,
    };
    fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION fixture_corrupt_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.actor_id:='tampered'; RETURN NEW; END $$; CREATE TRIGGER fixture_corrupt_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION fixture_corrupt_audit()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   let request=WorkflowRequest{key:"count-evidence-fixture".into(),operation:"inventory.count.approve".into(),record_id:"count-fixture".into(),input:CommandJson::parse(r#"{"expectedVersion":1}"#)?};let generated=CountCommandGeneratedIds::default();let clock=CountCommandClock::sample(&ClockFixture)?;let mut raw_ids=IdFixture;let mut ids=generated.wrap(&mut raw_ids);let mut authority=FixtureAuthority;let mut effects=PlannedLedgerFixture{generated:&generated,clock,plan:None};PgCommandLedger::new(CompanyId::new("company")?,c,&mut authority,&clock,&mut ids).execute(&request,&mut effects).await?;Ok(())
  })).await;assert!(matches!(result,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='count-evidence-fixture'),(SELECT count(*) FROM audit_events WHERE command_id='count-evidence-fixture'),(SELECT count(*) FROM outbox_events WHERE command_id='count-evidence-fixture')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
 }).await;
}

#[tokio::test]
async fn count_revision_evidence_checks_ordinary_ledger_without_flow_or_decision() {
    use erpx_server_workflow::public::service_count_command_evidence::{
        CountCommandClock, CountCommandGeneratedIds, verify_historical_count_command,
    };
    fixture::with_database(2, |pool| async move {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        let result = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(
            TransactionOptions::default(), |c| Box::pin(async move {
                let request = WorkflowRequest {
                    key: "count-revision-evidence".into(),
                    operation: "inventory.count.update".into(),
                    record_id: "count-fixture".into(),
                    input: CommandJson::parse(r#"{"expectedVersion":1,"reason":"Count revision"}"#)?,
                };
                let generated = CountCommandGeneratedIds::default();
                let clock = CountCommandClock::sample(&ClockFixture)?;
                let mut raw_ids = IdFixture;
                let mut ids = generated.wrap(&mut raw_ids);
                let mut authority = FixtureAuthority;
                let mut effects = PlannedLedgerFixture { generated: &generated, clock, plan: None };
                PgCommandLedger::new(CompanyId::new("company")?, c, &mut authority, &clock, &mut ids)
                    .execute(&request, &mut effects).await?;
                verify_historical_count_command(c, &fixture_actor(), &request, "branch", None).await?;
                let wrong = WorkflowRequest { input: CommandJson::parse(r#"{"expectedVersion":2,"reason":"Count revision"}"#)?, key:request.key.clone(), operation:request.operation.clone(), record_id:request.record_id.clone() };
                assert!(verify_historical_count_command(c, &fixture_actor(), &wrong, "branch", None).await.is_err());
                let omitted = WorkflowRequest { input: CommandJson::parse(r#"{"reason":"Count revision"}"#)?, key:request.key.clone(), operation:request.operation.clone(), record_id:request.record_id.clone() };
                assert!(verify_historical_count_command(c, &fixture_actor(), &omitted, "branch", None).await.is_err());
                let mut wrong_actor = fixture_actor(); wrong_actor.role = "company_owner".into();
                assert!(verify_historical_count_command(c, &wrong_actor, &request, "branch", None).await.is_err());
                let events: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM flow_events WHERE command_id=$1),(SELECT count(*) FROM decision_evidence WHERE command_id=$1)")
                    .bind(&request.key).fetch_one(&mut *c).await?;
                assert_eq!(events, (0, 0));
                Err(BusinessError::invalid("ROLLBACK_EVIDENCE_FIXTURE", "Do not retain synthetic Count evidence").into())
            })).await;
        assert!(matches!(result, Err(PgTransactionError::Business(e)) if e.code == "ROLLBACK_EVIDENCE_FIXTURE"));
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='count-revision-evidence'),(SELECT count(*) FROM audit_events WHERE command_id='count-revision-evidence'),(SELECT count(*) FROM outbox_events WHERE command_id='count-revision-evidence')")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (0, 0, 0));
    }).await;
}

#[tokio::test]
async fn assignment_default_is_scoped_retained_and_rolls_back_on_tampering() {
    fixture::with_database(2, |pool| async move {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        let unit=PgUnitOfWork::new(pool.clone());
        let result=unit.run_once::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_stock_count_flow_sources(c).await?;
            let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
            let plan=StockCountFlowAssignmentWitness::prepare_with_default(basis,&view("draft",1,0,0),StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command:"assign-default",event:"assign-default-event",now:1},true)?;
            plan.apply(c).await?;plan.retain(c).await?;
            sqlx::query("UPDATE flow_defaults SET primary_user_id='tamper'").execute(&mut *c).await?;
            plan.retain(c).await?;Ok(())
        })).await;
        assert!(result.is_err());
        let empty:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults),(SELECT count(*) FROM flow_routes),(SELECT count(*) FROM flow_events)").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(empty,(0,0,0));
        sqlx::raw_sql("CREATE FUNCTION fixture_extra_default() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO flow_defaults(revision,company_id,kind,step_key,branch_id,primary_user_id,created_by,reason,created_at) OVERRIDING SYSTEM VALUE VALUES(-1,'company','other_kind','other','another-branch','other','maker','tampered',1); RETURN NULL; END $$; CREATE TRIGGER fixture_extra_default AFTER INSERT ON flow_events FOR EACH STATEMENT EXECUTE FUNCTION fixture_extra_default()").execute(pool.raw()).await.unwrap();
        let extra=unit.run_once::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_stock_count_flow_sources(c).await?;
            let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
            let plan=StockCountFlowAssignmentWitness::prepare_with_default(basis,&view("draft",1,0,0),StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command:"assign-default",event:"assign-default-event",now:1},true)?;
            plan.apply(c).await?;plan.retain(c).await?;Ok(())
        })).await;assert!(extra.is_err());
        let none:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults),(SELECT count(*) FROM flow_routes),(SELECT count(*) FROM flow_events)").fetch_one(pool.raw()).await.unwrap();assert_eq!(none,(0,0,0));
        sqlx::raw_sql("DROP TRIGGER fixture_extra_default ON flow_events; DROP FUNCTION fixture_extra_default()").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_extra_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('foreign-event',NEW.command_id,'other-company','stock_count','other-count','draft','assign','maker','Maker','foreign',1,1); RETURN NULL; END $$; CREATE TRIGGER fixture_extra_event AFTER INSERT ON flow_events FOR EACH ROW WHEN (NEW.id='assign-default-event') EXECUTE FUNCTION fixture_extra_event()").execute(pool.raw()).await.unwrap();
        let extra_event=unit.run_once::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_stock_count_flow_sources(c).await?;
            let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
            let plan=StockCountFlowAssignmentWitness::prepare_with_default(basis,&view("draft",1,0,0),StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command:"assign-default",event:"assign-default-event",now:1},true)?;
            plan.apply(c).await?;plan.retain(c).await?;Ok(())
        })).await;assert!(extra_event.is_err());
        let none:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults),(SELECT count(*) FROM flow_routes),(SELECT count(*) FROM flow_events)").fetch_one(pool.raw()).await.unwrap();assert_eq!(none,(0,0,0));
        sqlx::raw_sql("DROP TRIGGER fixture_extra_event ON flow_events; DROP FUNCTION fixture_extra_event()").execute(pool.raw()).await.unwrap();
        unit.run_once::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_stock_count_flow_sources(c).await?;
            let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
            let plan=StockCountFlowAssignmentWitness::prepare_with_default(basis,&view("draft",1,0,0),StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command:"assign-default",event:"assign-default-event",now:1},true)?;
            plan.apply(c).await?;plan.retain(c).await?;Ok(())
        })).await.unwrap();
        let row:(String,String,Option<String>,String,String)=sqlx::query_as("SELECT company_id,branch_id,department_id,step_key,primary_user_id FROM flow_defaults").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row,("company".into(),"branch".into(),None,"submitted".into(),"reviewer".into()));
        let route:(i64,i64)=sqlx::query_as("SELECT version,defaults_revision FROM flow_routes").fetch_one(pool.raw()).await.unwrap();assert_eq!(route,(2,0));
    }).await;
}

#[tokio::test]
async fn concurrent_default_assignments_serialize_against_the_same_route() {
    fixture::with_database(3, |pool| async move {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        let run=|command: &'static str, event: &'static str| {
            let pool=pool.clone();async move {
                PgUnitOfWork::new(pool).run_once::<_,BusinessError,_>(TransactionOptions::default(),move|c|Box::pin(async move {
                    lock_stock_count_flow_sources(c).await?;
                    let basis=read_stock_count_flow_basis(c,"company","count-example","branch").await?;
                    let plan=StockCountFlowAssignmentWitness::prepare_with_default(basis,&view("draft",1,0,0),StockCountFlowAssignmentRequest{step:StockCountStep::Submitted,user:"reviewer",expected_flow:0,expected_record:1,company_admin:true,actor:"maker",actor_name:"Maker",reason:"Assign review",command,event,now:1},true)?;
                    plan.apply(c).await?;plan.retain(c).await?;Ok(())
                })).await
            }
        };
        let (a,b)=tokio::join!(run("one","event-one"),run("two","event-two"));
        assert_ne!(a.is_ok(),b.is_ok());
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults),(SELECT count(*) FROM flow_routes),(SELECT count(*) FROM flow_events)").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(1,1,1));
    }).await;
}
