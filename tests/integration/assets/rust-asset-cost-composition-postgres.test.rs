//! The composition Finance acquisition-cost handoff on a real serializable
//! Holding transaction: the Finance-owned snapshot is written before the Asset
//! register row and rolls back with the owner unit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_assets::public::asset_register::{AcquisitionCostProvider, NewAsset};
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
use erpx_server_shared::public::{Clock, CompanyId, TimestampMillis};
use erpx_service::asset_http::FinanceAssetCost;

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn asset(cost_minor: i64, currency: &str, acquired_date: &str) -> NewAsset {
    NewAsset::parse(&serde_json::json!({
        "branchId": "branch",
        "code": "COMPOSITION-COST-1",
        "name": "Composition cost asset",
        "category": "equipment",
        "currency": currency,
        "acquiredDate": acquired_date,
    }), cost_minor).expect("valid Asset fixture")
}

#[tokio::test]
async fn composition_finance_cost_snapshot_is_exact_and_rolls_back_with_the_asset_unit() {
    fixture::with_database(3, |pool| async move {
        let files = load_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
            MigrationTrack::HoldingWrite,
        )
        .await
        .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
             VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
             ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
             INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at) \
             VALUES('company','THB','fixture',0) ON CONFLICT(company_id) DO UPDATE SET default_currency='THB';\
             INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) \
             VALUES('company','THB',2,'fixture',0);\
             INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) \
             VALUES('composition-rate','company','THB','2026-01-01',36.5,'fixture',0,'reviewed');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let company = CompanyId::new("company").unwrap();
        let provider = FinanceAssetCost::new(company, std::sync::Arc::new(TestClock));

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        provider
            .prepare(&mut tx, "asset-composition-money", &asset(101, "THB", "2026-01-15"))
            .await
            .unwrap();
        let stored: (String, i32, String, String, Option<String>, bool, i64) = sqlx::query_as(
            "SELECT transaction_date::text,currency_scale::int,amount_units::text,\
             usd_units::text,fx_rate_id,used_latest_fallback,captured_at \
             FROM money_transaction_snapshots WHERE company_id='company' \
             AND source_type='operational_asset' AND source_id='asset-composition-money' \
             AND amount_kind='acquisition_cost'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(stored.0, "2026-01-15");
        assert_eq!(stored.1, 2);
        assert_eq!(stored.2, "101");
        assert_eq!(stored.4.as_deref(), Some("composition-rate"));
        assert!(!stored.5);
        assert_eq!(stored.6, 1_800_000_000_000);
        assert!(stored.3.parse::<i64>().unwrap() > 0);
        tx.rollback().await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM money_transaction_snapshots WHERE source_id='asset-composition-money'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn maintenance_purchase_image_matches_real_ts_query_for_work_and_plan() {
    use erpx_server_assets::public::asset_purchase_source::{inspect_asset_maintenance_join,lock_asset_purchase_sources};
    use erpx_server_maintenance::public::{asset_purchase_locks::{bind_maintenance_asset_purchase_sources,lock_maintenance_asset_purchase_sources},asset_purchase_record::{observe_maintenance_asset_purchase_record,MaintenanceAssetPurchaseKind}};
    use erpx_server_identity_access::public::{permission_catalog_lock::lock_permission_catalog_sources,sales_recipient::{lock_sales_recipient_sources,inspect_sales_assignment_join,inspect_sales_iam_boundary}};
    use erpx_server_people_payroll::public::{workforce_reference::inspect_sales_workforce_join,identity_facts::inspect_people_sales_authority_boundary};
    use erpx_server_organization::public::inspect_organization_sales_authority_boundary;
    use erpx_server_manage_erpx::public::inspect_beta_policy_authority_boundary;
    use erpx_service::maintenance_asset_purchase_image::MaintenanceAssetPurchaseSources;
    fixture::with_database(2,|pool|async move {
        let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),('branch','branch','branch','Branch','company','company',NULL,'active',1),('department','department','department','Department','branch','company','branch','active',1),('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','maintenance_purchase_test','Maintenance',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','maintenance_purchase_test',1,'[\"records.write\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('maintenance-grant','worker','maintenance_purchase_test','[]','company',0,9007199254740991,1,'holding'); INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','worker','company','Worker','TH','fixture',0,0); INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('engagement','person','company','employee','salaried','active','2000-01-01','Worker','THB','fixture',0,0); INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,created_by,created_at,updated_at) VALUES('asset-work','company','branch',NULL,'A1','Work asset','equipment','THB',0,'2026-01-01','fixture',1,1),('asset-plan','company','branch','department','A2','Plan asset','equipment','THB',0,'2026-01-01','fixture',1,1); INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('work','company','branch',NULL,'asset-work','Work','normal','open','worker','fixture',1,1); INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at) VALUES('plan','company','branch','department','asset-plan','Plan','normal','worker',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',1800000000000,7,'fixture','fixture',1,1); INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) SELECT 'plan-event',id,version,'create','fixture','Fixture',to_jsonb(p),1 FROM maintenance_preventive_plans p WHERE id='plan'").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);let n=0;const sql=(kind,file)=>{n=0;return m.maintenancePurchaseSourceQuery(kind,file).replace(/\?/g,()=>'$'+(++n))};process.stdout.write(JSON.stringify([sql('work',false),sql('work',true),sql('plan',false),sql('plan',true)]));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/server-maintenance/src/adapters/purchase-sources.ts");
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).args(["--conditions=react-server","-e",script]).arg(path).output().unwrap();
        assert!(oracle.status.success(),"actual Maintenance TS source unavailable");
        let sqls:Vec<String>=serde_json::from_slice(&oracle.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_permission_catalog_sources(&mut tx).await.unwrap();
        let mut prep=lock_sales_recipient_sources(&mut tx).await.unwrap();
        lock_asset_purchase_sources(prep.connection()).await.unwrap();
        lock_maintenance_asset_purchase_sources(prep.connection()).await.unwrap();
        let identity_locks=prep.bind().await.unwrap();
        let maintenance_locks=bind_maintenance_asset_purchase_sources(&mut tx).await.unwrap();
        let asset=inspect_asset_maintenance_join(&mut tx).await.unwrap();
        let identity=inspect_sales_assignment_join(&mut tx,&identity_locks).await.unwrap();
        let iam=inspect_sales_iam_boundary(&mut tx,&identity_locks).await.unwrap();
        let people=inspect_sales_workforce_join(&mut tx).await.unwrap();
        let people_authority=inspect_people_sales_authority_boundary(&mut tx).await.unwrap();
        let organization=inspect_organization_sales_authority_boundary(&mut tx).await.unwrap();
        let management=inspect_beta_policy_authority_boundary(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        for (kind,id,index,department) in [(MaintenanceAssetPurchaseKind::Work,"work",0,None),(MaintenanceAssetPurchaseKind::Plan,"plan",2,Some("department"))] {
            for file in [false,true] {
                let maintenance=observe_maintenance_asset_purchase_record(&mut tx,&maintenance_locks,kind,&company,Some("branch"),department,id,file).await.unwrap();
                let sources=MaintenanceAssetPurchaseSources{maintenance:&maintenance,asset:&asset,identity:&identity,iam:&iam,people:&people,people_authority:&people_authority,organization:&organization,management:&management};
                let retained=sources.inspect(&mut tx).await.unwrap();
                let legacy:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(sqls[index+usize::from(file)].as_str())).bind(id).bind("company").bind(Some("branch")).bind(Some("branch")).bind(department).bind(department).fetch_optional(&mut *tx).await.unwrap();
                let actual:serde_json::Value=serde_json::from_str(retained.image().expect("real joined source")).unwrap();
                let expected:serde_json::Value=serde_json::from_str(legacy.as_deref().expect("actual TS query")).unwrap();
                assert_eq!(actual,expected);assert_eq!(actual["ownerEligible"],true);retained.require_current(&mut tx).await.unwrap();
                sqlx::query("SAVEPOINT owner_negative").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE public.hr_engagements SET status='suspended' WHERE id='engagement'").execute(&mut *tx).await.unwrap();
                assert!(retained.require_current(&mut tx).await.is_err());
                let negative=sources.inspect(&mut tx).await.unwrap();let image:serde_json::Value=serde_json::from_str(negative.image().unwrap()).unwrap();
                let legacy_negative:String=sqlx::query_scalar(sqlx::AssertSqlSafe(sqls[index+usize::from(file)].as_str())).bind(id).bind("company").bind(Some("branch")).bind(Some("branch")).bind(department).bind(department).fetch_one(&mut *tx).await.unwrap();
                assert_eq!(image,serde_json::from_str::<serde_json::Value>(&legacy_negative).unwrap());assert_eq!(image["ownerEligible"],false);
                sqlx::query("ROLLBACK TO SAVEPOINT owner_negative").execute(&mut *tx).await.unwrap();retained.require_current(&mut tx).await.unwrap();
                sqlx::query("SAVEPOINT asset_drift").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE public.operational_assets SET version=version+1,updated_at=updated_at+1 WHERE id=$1").bind(if id=="work"{"asset-work"}else{"asset-plan"}).execute(&mut *tx).await.unwrap();
                assert!(retained.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT asset_drift").execute(&mut *tx).await.unwrap();retained.require_current(&mut tx).await.unwrap();
            }
            let absent=observe_maintenance_asset_purchase_record(&mut tx,&maintenance_locks,kind,&company,Some("foreign-branch"),None,id,false).await.unwrap();
            let sources=MaintenanceAssetPurchaseSources{maintenance:&absent,asset:&asset,identity:&identity,iam:&iam,people:&people,people_authority:&people_authority,organization:&organization,management:&management};
            assert!(sources.inspect(&mut tx).await.unwrap().image().is_none());
            let legacy_absent:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(sqls[index].as_str())).bind(id).bind("company").bind(Some("foreign-branch")).bind(Some("foreign-branch")).bind(None::<&str>).bind(None::<&str>).fetch_optional(&mut *tx).await.unwrap();
            assert!(legacy_absent.is_none());
            let foreign=CompanyId::new("foreign").unwrap();
            let absent=observe_maintenance_asset_purchase_record(&mut tx,&maintenance_locks,kind,&foreign,None,None,id,false).await.unwrap();
            let sources=MaintenanceAssetPurchaseSources{maintenance:&absent,asset:&asset,identity:&identity,iam:&iam,people:&people,people_authority:&people_authority,organization:&organization,management:&management};
            assert!(sources.inspect(&mut tx).await.unwrap().image().is_none());
            let legacy_foreign:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(sqls[index].as_str())).bind(id).bind("foreign").bind(None::<&str>).bind(None::<&str>).bind(None::<&str>).bind(None::<&str>).fetch_optional(&mut *tx).await.unwrap();
            assert!(legacy_foreign.is_none());
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn linked_asset_purchase_image_matches_actual_ts_and_revalidates() {
    use erpx_server_assets::public::asset_purchase_source::{lock_asset_purchase_sources,observe_asset_purchase_source};
    use erpx_server_finance::public::asset_purchase_source::{lock_asset_purchase_finance_source,inspect_asset_purchase_finance_join};
    use erpx_service::asset_purchase_image::AssetPurchaseSources;
    fixture::with_database(2,|pool|async move {
        let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES ('company','company','company','Company',NULL,'company',NULL,'active'),('branch','branch','branch','Branch','company','company',NULL,'active'),('department','department','department','Department','branch','company','branch','active'),('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'); INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) VALUES('chart','company','Chart','THB','active','fixture',0,0); INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('account','chart','company','1000','Asset','asset',0,0); INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,cost_minor,useful_life_months,in_service_date,status,currency,version,created_by,created_at,updated_at) VALUES('finance','company','FA1','Finance','account',1000,12,'2026-01-01','in_service','THB',1,'fixture',0,0),('foreign-finance','foreign','FA2','Foreign Finance','account',1000,12,'2026-01-01','in_service','THB',1,'fixture',0,0); INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,finance_asset_id,finance_asset_version,created_by,created_at,updated_at) VALUES('asset','company','branch','department','A1','Asset','equipment','THB',1000,'2026-01-01','finance',1,'fixture',1,1),('foreign-linked','company','branch',NULL,'A2','Foreign linked','equipment','THB',1000,'2026-01-01','foreign-finance',1,'fixture',1,1),('unlinked','company','branch',NULL,'A3','Unlinked','equipment','THB',0,'2026-01-01',NULL,NULL,'fixture',1,1); INSERT INTO operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) VALUES('link-a','asset','finance',1,1,1000,'THB','fixture','fixture',1)").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return {bind(){return {async first(){return sql}}}}}};let out=[];for(const file of [false,true]){let n=0;const sql=await m.assetOwnerSourceImage(db,{companyId:'company',branchId:'branch',departmentId:'department'},{kind:'asset',id:'asset'},file);out.push(sql.replace(/\?/g,()=>'$'+(++n)))}process.stdout.write(JSON.stringify(out));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/server-assets/src/adapters/purchases.ts");
        let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).args(["--conditions=react-server","-e",script]).arg(path).output().unwrap();assert!(out.status.success(),"actual TS query unavailable");let legacy:Vec<String>=serde_json::from_slice(&out.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_asset_purchase_sources(&mut tx).await.unwrap();lock_asset_purchase_finance_source(&mut tx).await.unwrap();
        let finance=inspect_asset_purchase_finance_join(&mut tx).await.unwrap();let company=CompanyId::new("company").unwrap();let foreign=CompanyId::new("foreign").unwrap();
        for (scope,id,branch,department) in [(&company,"asset",Some("branch"),Some("department")),(&company,"foreign-linked",None,None),(&company,"unlinked",None,None),(&company,"asset",Some("wrong"),None),(&foreign,"asset",None,None),(&company,"missing",None,None)] {
            for file in [false,true] {
                let asset=observe_asset_purchase_source(&mut tx,scope,branch,department,id,file).await.unwrap();let sources=AssetPurchaseSources{asset:&asset,finance:&finance};let retained=sources.inspect(&mut tx).await.unwrap();
                let expected:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy[usize::from(file)].as_str())).bind(id).bind(scope.as_str()).bind(branch).bind(branch).bind(department).bind(department).fetch_optional(&mut *tx).await.unwrap();
                assert_eq!(retained.image(),expected.as_deref(),"id={id} file={file}");retained.require_current(&mut tx).await.unwrap();
                if id=="asset" && branch==Some("branch") {let value:serde_json::Value=serde_json::from_str(retained.image().unwrap()).unwrap();assert_eq!(value["finance"]["id"],"finance");assert_eq!(value["links"][0]["id"],"link-a");assert_eq!(value["links"].as_array().unwrap().len(),1);}
                if id=="foreign-linked" || id=="unlinked" {let value:serde_json::Value=serde_json::from_str(retained.image().unwrap()).unwrap();assert!(value["finance"].is_null());}
                let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(retained.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
            }
        }
        let asset=observe_asset_purchase_source(&mut tx,&company,None,None,"asset",false).await.unwrap();let sources=AssetPurchaseSources{asset:&asset,finance:&finance};let retained=sources.inspect(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT finance_drift").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.finance_assets SET name='Changed' WHERE id='finance'").execute(&mut *tx).await.unwrap();assert!(retained.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT finance_drift").execute(&mut *tx).await.unwrap();retained.require_current(&mut tx).await.unwrap();
        let unlinked=observe_asset_purchase_source(&mut tx,&company,None,None,"unlinked",false).await.unwrap();let unlinked_sources=AssetPurchaseSources{asset:&unlinked,finance:&finance};let before_link=unlinked_sources.inspect(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT link_drift").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) VALUES('link-c','unlinked','foreign-finance',1,1,1000,'THB','fixture','fixture',3)").execute(&mut *tx).await.unwrap();assert!(before_link.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT link_drift").execute(&mut *tx).await.unwrap();before_link.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn retirement_work_port_uses_actual_maintenance_witness_and_global_predicate() {
    use erpx_server_assets::public::asset_register::{AssetAction,AssetActor,AssetChange,AssetFuture,AssetRepository,NoCommandLedger,PgAssetRegister,asset_after,change_asset};
    use erpx_server_shared::public::{RecordId,UserId};
    use erpx_server_assets::public::asset_purchase_source::{lock_asset_purchase_sources,observe_asset_purchase_source};
    use erpx_service::asset_retirement_work::inspect_asset_retirement_guard;
    use erpx_server_assets::public::retirement_work::{AssetRetirementWorkEffect,AssetRetirementWorkSource,prepare_asset_retirement_work};
    use erpx_server_maintenance::public::{asset_purchase_locks::{MaintenanceAssetPurchaseLocks,lock_maintenance_asset_purchase_sources,bind_maintenance_asset_purchase_sources},asset_purchase_record::{CurrentAssetRetirementWork,observe_asset_retirement_work}};
    use erpx_server_runtime::public::postgres::PgTransactionError;
    use erpx_server_shared::public::BusinessError;
    type Error=PgTransactionError<BusinessError>;
    struct Source<'a>(&'a MaintenanceAssetPurchaseLocks);
    struct Effect<'a>(CurrentAssetRetirementWork<'a>);
    impl AssetRetirementWorkEffect for Effect<'_>{fn is_clear(&self)->bool{self.0.is_clear()}fn require_current<'a>(&'a self,c:&'a mut sqlx::PgConnection)->AssetFuture<'a,(),Error>{Box::pin(async move{self.0.require_current(c).await})}}
    impl<'s> AssetRetirementWorkSource for Source<'s>{type Effect=Effect<'s>;fn observe<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,id:&'a str)->AssetFuture<'a,Self::Effect,Error>{let locks=self.0;Box::pin(async move{Ok(Effect(observe_asset_retirement_work(c,locks,id).await?))})}}
    struct WriterSource<'s>(&'s MaintenanceAssetPurchaseLocks);
    impl<'s> AssetRetirementWorkSource for WriterSource<'s>{type Effect=Box<dyn AssetRetirementWorkEffect+'s>;fn observe<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,id:&'a str)->AssetFuture<'a,Self::Effect,Error>{let locks=self.0;Box::pin(async move{Ok(Box::new(Effect(observe_asset_retirement_work(c,locks,id).await?)) as Box<dyn AssetRetirementWorkEffect+'s>)})}}
    fixture::with_database(2,|pool|async move{
        let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('branch','branch','branch','Branch','company','company',NULL,'active'),('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'),('foreign-branch','branch','foreign-branch','Foreign branch','foreign','foreign',NULL,'active'); INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,cost_minor,acquired_date,created_by,created_at,updated_at) VALUES('blocked','company','branch','B','Blocked','equipment','THB',0,'2026-01-01','fixture',1,1),('clear','company','branch','C','Clear','equipment','THB',0,'2026-01-01','fixture',1,1); INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('foreign-work','foreign','foreign-branch','blocked','Work','normal','open','worker','fixture',1,1)").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);process.stdout.write(m.finalRootOwnerProjectionSources.maintenance_work_orders)"#;let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/server-maintenance/src/adapters/final-root-owner-projection-sources.ts");let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).args(["--conditions=react-server","-e",script]).arg(path).output().unwrap();assert!(out.status.success(),"actual Maintenance projection unavailable");let projection=String::from_utf8(out.stdout).unwrap();let oracle=format!("SELECT NOT EXISTS(SELECT 1 FROM ({projection}) maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))");
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_asset_purchase_sources(&mut tx).await.unwrap();lock_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let locks=bind_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let mut source=Source(&locks);
        let company=CompanyId::new("company").unwrap();
        let blocked_asset=observe_asset_purchase_source(&mut tx,&company,Some("branch"),None,"blocked",false).await.unwrap();assert!(blocked_asset.exists());
        assert!(matches!(inspect_asset_retirement_guard(&mut tx,&blocked_asset,&locks).await,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"));
        let wrong_scope=observe_asset_purchase_source(&mut tx,&company,Some("wrong"),None,"clear",false).await.unwrap();assert!(!wrong_scope.exists());
        assert!(matches!(inspect_asset_retirement_guard(&mut tx,&wrong_scope,&locks).await,Err(PgTransactionError::Business(ref e)) if e.code=="ASSETS_SOURCE_NOT_FOUND"));
        let selected_asset=observe_asset_purchase_source(&mut tx,&company,Some("branch"),None,"clear",false).await.unwrap();assert!(selected_asset.exists());
        let guard=inspect_asset_retirement_guard(&mut tx,&selected_asset,&locks).await.unwrap();
        for id in ["blocked","clear"] {let selected:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.operational_assets WHERE id=$1 AND company_id='company' AND branch_id='branch')").bind(id).fetch_one(&mut *tx).await.unwrap();assert!(selected);let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(oracle.as_str())).bind(id).fetch_one(&mut *tx).await.unwrap();let result=prepare_asset_retirement_work(&mut source,&mut tx,id).await;if actual{result.unwrap().require_current(&mut tx).await.unwrap();}else{assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"));}}
        let effect=prepare_asset_retirement_work(&mut source,&mut tx,"clear").await.unwrap();
        sqlx::query("SAVEPOINT asset_change").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE public.operational_assets SET lifecycle_status='retirement_pending',retirement_reason='reviewed',version=version+1,updated_at=updated_at+1 WHERE id='clear'").execute(&mut *tx).await.unwrap();
        assert!(selected_asset.require_current(&mut tx).await.is_err());guard.require_current_after_effects(&mut tx).await.unwrap();
        let post_asset=observe_asset_purchase_source(&mut tx,&company,Some("branch"),None,"clear",false).await.unwrap();post_asset.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT asset_change").execute(&mut *tx).await.unwrap();selected_asset.require_current(&mut tx).await.unwrap();guard.require_current_after_effects(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT late_work").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('late','foreign','foreign-branch','clear','Late','normal','open','worker','fixture',1,1)").execute(&mut *tx).await.unwrap();assert!(effect.require_current(&mut tx).await.is_err());assert!(guard.require_current_after_effects(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT late_work").execute(&mut *tx).await.unwrap();effect.require_current(&mut tx).await.unwrap();guard.require_current_after_effects(&mut tx).await.unwrap();
        // Exercise the actual scoped Asset writer; the actor is fixture data,
        // not proof of the production IAM boundary or Workflow receipt.
        let actor=AssetActor{user_id:UserId::new("fixture-user").unwrap(),company_id:company.clone(),branch_id:Some(RecordId::new("branch").unwrap()),department_id:None,permissions:vec!["records.write".into()],modules:vec!["assets".into()]};
        let input=serde_json::json!({"action":"retire-request","expectedVersion":1,"reason":"Reviewed retirement"});
        let missing={let mut repo=PgAssetRegister::new(&mut tx,actor.clone());let mut ledger=NoCommandLedger::<Error>::default();change_asset(&actor,&mut repo,&mut ledger,"clear",&input,"retire-no-source",2).await};
        assert!(matches!(missing,Err(PgTransactionError::Persistence(ref e)) if e.code=="ASSET_RETIREMENT_WORK_SOURCE_REQUIRED"));
        let blocked={let mut repo=PgAssetRegister::new(&mut tx,actor.clone()).with_retirement_work_source(WriterSource(&locks));let mut ledger=NoCommandLedger::<Error>::default();change_asset(&actor,&mut repo,&mut ledger,"blocked",&input,"retire-blocked",2).await};
        assert!(matches!(blocked,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"));
        let unchanged:(i64,i64)=sqlx::query_as("SELECT (SELECT version FROM public.operational_assets WHERE id='blocked'),(SELECT count(*) FROM public.operational_asset_events WHERE asset_id='blocked')").fetch_one(&mut *tx).await.unwrap();assert_eq!(unchanged,(1,0));
        sqlx::query("SAVEPOINT owner_write").execute(&mut *tx).await.unwrap();
        let (outcome,work_effect)={let mut repo=PgAssetRegister::new(&mut tx,actor.clone()).with_retirement_work_source(WriterSource(&locks));let mut ledger=NoCommandLedger::<Error>::default();let result=change_asset(&actor,&mut repo,&mut ledger,"clear",&input,"retire-native",2).await.unwrap();let work=repo.take_retirement_work_effect().expect("retained Maintenance work effect");(result,work)};
        assert_eq!(outcome.result.version,2);assert_eq!(outcome.result.status,"retirement_pending");outcome.receipt.as_ref().unwrap().require_current(&mut tx).await.unwrap();work_effect.require_current(&mut tx).await.unwrap();
        let stored:(i64,i64)=sqlx::query_as("SELECT (SELECT version FROM public.operational_assets WHERE id='clear'),(SELECT count(*) FROM public.operational_asset_events WHERE asset_id='clear' AND action='retire-request')").fetch_one(&mut *tx).await.unwrap();assert_eq!(stored,(2,1));
        sqlx::query("INSERT INTO public.maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('after-write','foreign','foreign-branch','clear','Late','normal','open','worker','fixture',1,1)").execute(&mut *tx).await.unwrap();assert!(work_effect.require_current(&mut tx).await.is_err());
        // Exercise the late writer guard directly: the application normally
        // rejects this requester earlier, but the owner must keep the same
        // precedence even when called through its repository port.
        let mut repo=PgAssetRegister::new(&mut tx,actor.clone()).with_retirement_work_source(WriterSource(&locks));
        let current=repo.asset("clear").await.unwrap().unwrap().state().unwrap();
        let valid=AssetChange{expected_version:2,action:AssetAction::RetireRequest,actor_id:actor.user_id.clone(),command_key:"refresh-valid".into(),reason:"Reviewed again".into(),at:3};
        let transition=asset_after(&current,&valid,Some(&actor.user_id),true).unwrap();
        let mut wrong=valid.clone();wrong.actor_id=UserId::new("other-user").unwrap();wrong.command_key="refresh-wrong".into();
        let result=repo.apply_change("clear",&wrong,&transition).await;assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="ASSET_RETIREMENT_REFRESH_FORBIDDEN" && e.kind==erpx_server_shared::public::FailureKind::Forbidden));drop(repo);
        let no_extra:(i64,i64)=sqlx::query_as("SELECT (SELECT version FROM public.operational_assets WHERE id='clear'),(SELECT count(*) FROM public.operational_asset_events WHERE asset_id='clear')").fetch_one(&mut *tx).await.unwrap();assert_eq!(no_extra,(2,1));
        sqlx::query("ROLLBACK TO SAVEPOINT owner_write").execute(&mut *tx).await.unwrap();let rolled_back:(i64,i64)=sqlx::query_as("SELECT (SELECT version FROM public.operational_assets WHERE id='clear'),(SELECT count(*) FROM public.operational_asset_events WHERE asset_id='clear')").fetch_one(&mut *tx).await.unwrap();assert_eq!(rolled_back,(1,0));
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(effect.require_current(&mut other).await.is_err());assert!(guard.require_current_after_effects(&mut other).await.is_err());other.rollback().await.unwrap();tx.rollback().await.unwrap();
    }).await;
}
