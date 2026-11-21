//! Real Account/SSO → Maintenance/Inventory/Finance/Workflow fixture. Only
//! source data is synthetic; no actor or owner-policy port is substituted.
#[path = "review-composition.rs"]
pub mod review;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_identity_access::public::{
    effective_authority::WorkAuthorizationRequest,
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{AssignmentId, Clock, CommandKey, IdentifierSource, RecordId};
use erpx_service::maintenance_parts::{
    MaintenancePartsSourcePreparation, execute_maintenance_issue_part,
};
pub use review::*;
use sqlx::PgConnection;

pub struct Parts {
    pub reviewer: Prepared,
    pub issuer_selection: HoldingWorkspaceRequest,
}
pub async fn setup_parts(system: account::System) -> Parts {
    let reviewer = review::setup(system).await;
    let pool = &reviewer.system.holdings[0];
    let now = reviewer.system.scenario.clock.now().get();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch','branch','north','Synthetic North','company','company','branch','active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_parts_issuer','Synthetic actual parts issuer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_parts_issuer',1,'[\"records.write\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('parts-worker','fixture-a','company','PRIVATE_WORKER','TH','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('parts-engagement','parts-worker','company','employee','salaried','active','2026-01-01','PRIVATE_JOB','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','parts-worker','active','local',1);INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('asset','company','branch','SYNTHETIC','PRIVATE_ASSET','synthetic','THB','fixture-a',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('work','company','branch','asset',repeat('PRIVATE_TITLE',200000),'normal','in_progress','fixture-a','fixture-a',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES('inventory','company','chart','110','Inventory','asset','THB',0,0),('expense','company','chart','510','Expense','expense','THB',0,0);INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','USD','inventory','inventory','sales','inventory','inventory','expense','inventory','inventory','cash');INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','SYNTHETIC','Synthetic warehouse');INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','SYNTHETIC','Synthetic spare',100,'none');INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse','product',5000,1000)").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('parts-fixture-a','fixture-a','custom_parts_issuer','[\"maintenance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE active_work_contexts SET assignment_id='parts-fixture-a',revision=8,updated_at=$1 WHERE user_id='fixture-a'").bind(now).execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','assets',1,'active',0,1,'Synthetic approved source','public synthetic evidence',$1),('company','assets-parts-costs',1,'active',0,1,'Synthetic approved source','public synthetic evidence',$1)").bind(now).execute(&mut*tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'parts-origin-'||package_id,company_id,package_id,'fixture-a','synthetic-source-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id IN ('assets','assets-parts-costs')").bind(now).execute(&mut*tx).await.unwrap();
    tx.commit().await.unwrap();
    let issuer_selection = HoldingWorkspaceRequest {
        expected: Some(ExpectedWorkSelection {
            revision: 8,
            assignment_id: AssignmentId::new("parts-fixture-a").unwrap(),
            assignment_version: 3,
        }),
        ..reviewer.maker_selection.clone()
    };
    Parts {
        reviewer,
        issuer_selection,
    }
}
pub fn issue_request() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.write".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance/work".into(),
        api: true,
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
impl Parts {
    pub fn pool(&self) -> &DatabasePool {
        &self.reviewer.system.holdings[0]
    }
    pub async fn issue(&self, key: &str, version: i64) -> String {
        let key = key.to_owned();
        let clock = self.reviewer.system.scenario.clock.clone();
        self.reviewer.authorizer.execute_with_sources("acme.example.test",self.reviewer.maker.clone(),self.issuer_selection.clone(),issue_request(),MaintenancePartsSourcePreparation,move|c,current|{
            let key=key.clone();let clock=clock.clone();Box::pin(async move {
                assert!(!current.has_permission("finance.write"));
                let raw=format!(r#"{{"action":"issue-part","expectedVersion":{version},"reason":"Synthetic genuine issue","warehouseId":"warehouse","productId":"product","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]}}"#);
                let command=erpx_server_maintenance::public::work_wire::normalize_maintenance_change(raw.as_bytes()).unwrap().into_command(CommandKey::new(key)?,RecordId::new("work")?);
                execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,clock.as_ref(),clock.as_ref(),&mut Ids).await?;Ok(())
            })
        }).await.unwrap();
        sqlx::query_scalar("SELECT journal_id FROM operational_inventory_consumptions WHERE source_version=$1 AND source_id='work'").bind(version+1).fetch_one(self.pool().raw()).await.unwrap()
    }
    pub async fn post(
        &self,
        id: &str,
        key: &str,
    ) -> Result<erpx_server_finance::public::journal_review::JournalReviewResult, Error> {
        self.reviewer.review(id, key, post()).await
    }
}
pub async fn inactive(c: &mut PgConnection, package: &str, now: i64) {
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company'AND package_id=$1").bind(package).fetch_one(&mut*c).await.unwrap();
    sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id=$1").bind(package).execute(&mut*c).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT $1,'company',package_id,'fixture-a','synthetic-withdrawal',$2,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$3 FROM company_package_grants WHERE company_id='company'AND package_id=$4").bind(uuid::Uuid::new_v4().to_string()).bind(before).bind(now).bind(package).execute(c).await.unwrap();
}
pub async fn withdraw(p: &Parts, packages: &[&str]) {
    let mut tx = p.pool().raw().begin().await.unwrap();
    for package in packages {
        inactive(
            &mut tx,
            package,
            p.reviewer.system.scenario.clock.now().get(),
        )
        .await;
    }
    tx.commit().await.unwrap();
}
pub async fn counts(p: &Parts, key: &str) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1),(SELECT count(*)FROM money_transaction_snapshots WHERE source_type='finance_journal')").bind(key).fetch_one(p.pool().raw()).await.unwrap()
}
pub async fn state(p: &Parts, id: &str) -> (String, i64) {
    sqlx::query_as("SELECT status,version::bigint FROM finance_journals WHERE id=$1")
        .bind(id)
        .fetch_one(p.pool().raw())
        .await
        .unwrap()
}
pub fn spawn_post(
    p: &Parts,
    id: String,
    key: &str,
) -> tokio::task::JoinHandle<
    Result<erpx_server_finance::public::journal_review::JournalReviewResult, Error>,
> {
    let administration = p.reviewer.administration();
    let proof = p.reviewer.reviewer.clone();
    let selection = p.reviewer.reviewer_selection.clone();
    let command = command(&id, key, post());
    tokio::spawn(async move {
        administration
            .review("acme.example.test", proof, selection, command)
            .await
    })
}
pub async fn blocked_by(p: &Parts, database: &str, pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{let yes:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND $2=ANY(pg_blocking_pids(pid)))").bind(database).bind(pid).fetch_one(p.reviewer.system.control.raw()).await.unwrap();if yes{return;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("actual retained-source lock wait")
}
pub async fn legacy_source_exists(p: &Parts, id: &str) -> bool {
    let original =
        include_str!("../../../packages/server-maintenance/src/adapters/finance-record-sources.ts");
    let sql = original
        .split("journal:`")
        .nth(1)
        .unwrap()
        .split('`')
        .next()
        .unwrap();
    // Exact source-owned B0 SELECT, not a second hand-written predicate.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS(SELECT 1 FROM({sql})b0 WHERE b0.company_id=$1 AND b0.id=$2)"
    )))
    .bind("company")
    .bind(id)
    .fetch_one(p.pool().raw())
    .await
    .unwrap()
}
pub fn spawn_issue(
    p: &Parts,
    key: &str,
    version: i64,
) -> tokio::task::JoinHandle<Result<(), Error>> {
    let authorizer = p.reviewer.authorizer.clone();
    let proof = p.reviewer.maker.clone();
    let selection = p.issuer_selection.clone();
    let clock = p.reviewer.system.scenario.clock.clone();
    let key = key.to_owned();
    tokio::spawn(async move {
        authorizer.execute_with_sources("acme.example.test",proof,selection,issue_request(),MaintenancePartsSourcePreparation,move|c,current|{let key=key.clone();let clock=clock.clone();Box::pin(async move{
        let raw=format!(r#"{{"action":"issue-part","expectedVersion":{version},"reason":"Synthetic concurrent genuine issue","warehouseId":"warehouse","productId":"product","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]}}"#);
        let command=erpx_server_maintenance::public::work_wire::normalize_maintenance_change(raw.as_bytes()).unwrap().into_command(CommandKey::new(key)?,RecordId::new("work")?);
        execute_maintenance_issue_part(c,current,&command,PackageEnforcementMode::Enforced,clock.as_ref(),clock.as_ref(),&mut Ids).await?;Ok(())
    })}).await
    })
}
