//! Synthetic actual migrated Maintenance facts; these seeds are not actor or purchase grants.
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{RecordId, TimestampMillis};
const NOW: i64 = 1_800_000_000_000;
pub async fn seed_work(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('worker','user','company','PRIVATE_WORKER','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('engagement','worker','company','employee','salaried','active','2026-01-01','PRIVATE_JOB','THB','fixture',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES('company','user','local:user','worker','active','local');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',2,'[\"records.write\",\"records.review\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority';UPDATE assignments SET modules_json='[\"maintenance\"]'WHERE id='grant';INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('asset','company','branch','SYNTHETIC','PRIVATE_ASSET','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('work','company','branch','asset',repeat('PRIVATE_TITLE',200000),'normal','in_progress','user','user',0,0)").execute(pool.raw()).await.unwrap();
}

pub fn maintenance_request() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.write".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance/work".into(),
        api: true,
    }
}

pub fn work_id() -> RecordId {
    RecordId::new("work").unwrap()
}

pub fn issue_command() -> erpx_server_maintenance::public::work_commands::MaintenanceWorkCommand {
    erpx_server_maintenance::public::work_wire::normalize_maintenance_change(br#"{"action":"issue-part","expectedVersion":1,"reason":"Synthetic issue evidence","warehouseId":"warehouse","productId":"product","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[]}"#)
        .unwrap().into_command(erpx_server_shared::public::CommandKey::new("synthetic-work-effects").unwrap(),work_id())
}

pub struct PartsWriteClock(pub std::sync::atomic::AtomicI64);

impl erpx_server_shared::public::Clock for PartsWriteClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

pub struct PartsGuardClock;

impl erpx_server_shared::public::Clock for PartsGuardClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

pub async fn seed_parts(pool: &DatabasePool) {
    seed_parts_with_rate(pool, true).await;
}
/// Quote failures begin without a rate; immutable Money history is never
/// deleted or rewritten merely to construct a negative test.
pub async fn seed_parts_with_rate(pool: &DatabasePool, with_rate: bool) {
    sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Synthetic chart','THB','user',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES('inventory','company','chart','100','Synthetic Inventory','asset','THB',0,0),('expense','company','chart','500','Synthetic Expense','expense','THB',0,0);INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES('period','company','Synthetic January','2026-01-01','2026-01-31',0);INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','USD','inventory','inventory','inventory','inventory','inventory','expense','inventory','inventory','inventory');INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','SYNTHETIC','Synthetic warehouse');INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode)VALUES('product','company','SYNTHETIC','Synthetic spare',100,'none');INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('warehouse','product',5000,1000);INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','assets',1,'active',0,1,'Synthetic approved source','public-synthetic-evidence',0),('company','assets-parts-costs',1,'active',0,1,'Synthetic approved source','public-synthetic-evidence',0);INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'synthetic-grant-'||package_id,company_id,package_id,'synthetic-fixture','synthetic-fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),0 FROM company_package_grants WHERE company_id='company'AND package_id IN ('assets','assets-parts-costs')").execute(pool.raw()).await.unwrap();
    if with_rate {
        sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('rate','company','THB','2026-01-01',2,'user',1)").execute(pool.raw()).await.unwrap();
    }
}
