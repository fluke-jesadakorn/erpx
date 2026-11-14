//! Actual bounded Maintenance reads on retained Account/Holding authorization.
//! These are owner read projections, not command, purchase or browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/maintenance/parts-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_identity_access::public::effective_authority::{
    HoldingWorkSourcePreparation, WorkAuthorizationRequest,
};
use erpx_server_maintenance::public::read::{
    read_maintenance_detail, read_maintenance_plan_detail, read_maintenance_plans,
    read_maintenance_workspace,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest;
use facts::*;
use sqlx::PgConnection;

#[derive(Clone, Copy)]
struct ReadSources;
impl HoldingWorkSourcePreparation for ReadSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn scoped_input() -> HoldingWorkspaceRequest {
    let mut request = input();
    request.branch_slug = Some("north".into());
    request
}

fn read_request() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.read".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance".into(),
        api: false,
    }
}
async fn seed(pool: &DatabasePool) {
    seed_work(pool).await;
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"records.read\",\"records.write\",\"records.review\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority';INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at)VALUES('work-event-1','work',1,'create','user','Synthetic work opened','in_progress','user',NULL,NULL,NULL,0)").execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,created_by,edited_by,created_at,updated_at)VALUES('plan','company','branch',NULL,'asset','Synthetic preventive plan','normal','user',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',$1,7,'user','user',0,0)").bind(NOW).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql("INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at)SELECT 'plan-event-1',id,version,'create','user','Synthetic plan created',to_jsonb(p),0 FROM maintenance_preventive_plans p WHERE id='plan'").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn workspace_detail_and_preventive_reads_are_company_scope_bound_and_preserve_source_facts() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            seed(&holding).await;
            let now = clock.now();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            let workspace =
                                read_maintenance_workspace(connection, current, now).await?;
                            assert_eq!(workspace.items.len(), 1);
                            let work = &workspace.items[0];
                            assert_eq!(work.id, "work");
                            assert_eq!(work.asset_code, "SYNTHETIC");
                            assert_eq!(work.asset_name, "PRIVATE_ASSET");
                            assert_eq!(work.owner_name.as_deref(), Some("PRIVATE_WORKER"));
                            assert_eq!(work.owner_eligible, 1);
                            assert_eq!(work.can_review, 1);
                            assert!(workspace.can_create);
                            assert_eq!(workspace.assets.len(), 1);
                            assert_eq!(workspace.people.len(), 1);
                            assert!(workspace.nodes.iter().any(|node| node.id == "branch"));

                            let detail = read_maintenance_detail(
                                connection, current, "work", now,
                            )
                            .await?;
                            assert_eq!(detail.history.len(), 1);
                            assert_eq!(detail.history[0].action, "create");
                            assert!(detail.consumptions.is_empty());
                            assert_eq!(detail.value()["work"]["status"], "in_progress");
                            assert!(
                                read_maintenance_detail(connection, current, "missing", now)
                                    .await
                                    .is_err()
                            );

                            let plans = read_maintenance_plans(connection, current, now).await?;
                            assert_eq!(plans.len(), 1);
                            assert_eq!(plans[0].asset_code, "SYNTHETIC");
                            assert_eq!(plans[0].owner_eligible, 1);
                            let plan = read_maintenance_plan_detail(
                                connection, current, "plan", now,
                            )
                            .await?;
                            assert_eq!(plan.asset_version, Some(1));
                            assert_eq!(plan.people.len(), 1);
                            assert_eq!(plan.history.len(), 1);
                            assert!(plan.occurrences.is_empty());
                            assert_eq!(plan.value()["plan"]["status"], "draft");
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn reads_refuse_missing_read_permission_expired_context_and_foreign_record_ids() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            seed(&holding).await;
            // Scope a foreign work row to another branch and company.
            sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('branch-b','branch','south','South','company','company',NULL,'active');INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('foreign-asset','company','branch-b','FOREIGN','Foreign asset','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('foreign-work','company','branch-b','foreign-asset','Foreign work','normal','open','user','user',0,0);UPDATE assignments SET scope_kind='branch',scope_node_id='branch' WHERE id='grant'").execute(holding.raw()).await.unwrap();
            let now = clock.now();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            let workspace =
                                read_maintenance_workspace(connection, current, now).await?;
                            assert!(workspace.items.iter().all(|item| item.id != "foreign-work"));
                            let error = read_maintenance_detail(
                                connection,
                                current,
                                "foreign-work",
                                now,
                            )
                            .await
                            .unwrap_err();
                            assert!(
                                matches!(
                                    error,
                                    erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)
                                        if e.kind == FailureKind::NotFound
                                ),
                                "foreign work must be a scoped not-found"
                            );
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();

            // Remove the read permission while retaining an otherwise valid
            // session and selected assignment.
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',4,'[\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=4 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
            let denied = authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move { read_maintenance_workspace(connection, current, now).await })
                    },
                )
                .await
                .unwrap_err();
            assert!(
                matches!(
                    denied,
                    erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)
                        if e.kind == FailureKind::Forbidden
                ),
                "missing records.read must fail closed"
            );

            // Restore read, then observe beyond the selected context expiry.
            sqlx::raw_sql("UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
            let expired_now = TimestampMillis::new(NOW + 3_600_001);
            let expired = authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    scoped_input(),
                    read_request(),
                    ReadSources,
                    move |connection, current| {
                        Box::pin(async move {
                            read_maintenance_workspace(connection, current, expired_now).await
                        })
                    },
                )
                .await;
            assert!(expired.is_err(), "expired selected context must fail closed");
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn creation_vacancy_retains_global_absence_and_same_unit_locks() {
    use erpx_server_maintenance::public::work_source::inspect_maintenance_asset_vacancy;
    use erpx_server_shared::public::RecordId;
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let _ = setup(&control, &holding).await;
            seed_work(&holding).await;
            sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('vacant-asset','company','branch','VACANT','Synthetic','synthetic','THB','fixture',0,0)").execute(holding.raw()).await.unwrap();
            let asset=RecordId::new("vacant-asset").unwrap();
            let occupied=RecordId::new("asset").unwrap();
            let mut no_lock=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *no_lock).await.unwrap();
            assert!(inspect_maintenance_asset_vacancy(&mut no_lock,&asset).await.is_err());
            no_lock.rollback().await.unwrap();
            let mut tx=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
            assert!(inspect_maintenance_asset_vacancy(&mut tx,&occupied).await.unwrap().is_none());
            let witness=inspect_maintenance_asset_vacancy(&mut tx,&asset).await.unwrap().unwrap();
            assert_eq!(witness.asset_id(),&asset);witness.require_current(&mut tx).await.unwrap();
            sqlx::query("INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('cancelled-work','company','branch','vacant-asset','Synthetic','normal','cancelled','user','user',0,0)").execute(&mut *tx).await.unwrap();
            witness.require_current(&mut tx).await.unwrap();
            // Legacy checks by globally keyed asset_id, not work company.
            sqlx::query("INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('foreign-active-work','foreign','branch','vacant-asset','Synthetic','normal','open','user','user',0,0)").execute(&mut *tx).await.unwrap();
            assert!(witness.require_current(&mut tx).await.is_err());
            assert!(inspect_maintenance_asset_vacancy(&mut tx,&asset).await.unwrap().is_none());
            tx.rollback().await.unwrap();
            let mut other=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
            sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE").execute(&mut *other).await.unwrap();
            assert!(witness.require_current(&mut other).await.is_err());
            other.rollback().await.unwrap();
            let mut savepoint=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *savepoint).await.unwrap();
            sqlx::query("SELECT pg_current_xact_id()").execute(&mut *savepoint).await.unwrap();
            sqlx::query("SAVEPOINT before_lock").execute(&mut *savepoint).await.unwrap();
            sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE").execute(&mut *savepoint).await.unwrap();
            let proof=inspect_maintenance_asset_vacancy(&mut savepoint,&asset).await.unwrap().unwrap();
            sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *savepoint).await.unwrap();
            assert!(proof.require_current(&mut savepoint).await.is_err());
            savepoint.rollback().await.unwrap();
        }).await;
    }).await;
}

#[tokio::test]
async fn maintenance_asset_purchase_locks_require_same_unit_and_report_contention() {
    use erpx_server_maintenance::public::asset_purchase_locks::*;
    use erpx_server_runtime::public::postgres::{MigrationTrack,PgTransactionError,load_migrations,migrate_database};
    fixture::with_database(2,|pool|async move {
        let migrations=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(bind_maintenance_asset_purchase_sources(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let source=bind_maintenance_asset_purchase_sources(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        let held:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('public.maintenance_work_orders'::regclass,'public.maintenance_preventive_plans'::regclass,'public.maintenance_preventive_occurrences'::regclass)").fetch_one(&mut *tx).await.unwrap();assert_eq!(held,3);tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_maintenance_asset_purchase_sources(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_maintenance_sources").execute(&mut *saved).await.unwrap();lock_maintenance_asset_purchase_sources(&mut saved).await.unwrap();let retained=bind_maintenance_asset_purchase_sources(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_maintenance_sources").execute(&mut *saved).await.unwrap();assert!(retained.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.maintenance_preventive_plans IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let conflict=lock_maintenance_asset_purchase_sources(&mut blocked).await;assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn maintenance_asset_purchase_record_matches_actual_ts_owner_image() {
    use erpx_server_maintenance::public::{asset_purchase_locks::*,asset_purchase_record::*};
    use erpx_server_shared::public::CompanyId;
    fixture::with_database(2,|control|async move {fixture::with_database(2,move|holding|async move {
        let (_authorizer,_clock)=setup(&control,&holding).await;seed(&holding).await;
        // This isolated fixture keeps the existing role and permissions, but
        // the TS predicate uses DB wall time rather than the auth fixture clock.
        sqlx::query("UPDATE assignments SET valid_from=floor(extract(epoch FROM clock_timestamp())*1000)::bigint-60000,valid_until=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+3600000 WHERE id='grant'").execute(holding.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);let n=0;const sql=(kind,file)=>{n=0;return m.maintenancePurchaseSourceQuery(kind,file).replace(/\?/g,()=>'$'+(++n))};process.stdout.write(JSON.stringify([sql('work',false),sql('work',true),sql('plan',false),sql('plan',true)]));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/purchase-sources.ts");let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").unwrap_or_else(|_|"bun".into())).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();assert!(output.status.success(),"actual TS source: {}",String::from_utf8_lossy(&output.stderr));let sqls:Vec<String>=serde_json::from_slice(&output.stdout).unwrap();
        let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let locks=bind_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let company=CompanyId::new("company").unwrap();
        for (kind,id,index) in [(MaintenanceAssetPurchaseKind::Work,"work",0),(MaintenanceAssetPurchaseKind::Plan,"plan",2)] {
            for file in [false,true]{let observed=observe_maintenance_asset_purchase_record(&mut tx,&locks,kind,&company,Some("branch"),None,id,file).await.unwrap();assert_eq!(observed.file_mode(),file);assert!(observed.exists());assert_eq!(observed.asset_id(),Some("asset"));assert_eq!(observed.owner_id(),Some("user"));assert_eq!(observed.branch_id(),Some("branch"));assert_eq!(observed.department_id(),None);
                let legacy=format!("SELECT encode(sha256(convert_to((source.image::jsonb->'record')::text,'UTF8')),'hex') FROM ({}) source",sqls[index+usize::from(file)]);let expected:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy.as_str())).bind(id).bind("company").bind(Some("branch")).bind(Some("branch")).bind(None::<&str>).bind(None::<&str>).fetch_optional(&mut *tx).await.unwrap();assert_eq!(observed.record_digest(),expected.as_deref());observed.require_current(&mut tx).await.unwrap();
                // The only rewrite to the actual TS full image SQL is replacing
                // its owner relation with this retained CTE. Keep all joins and
                // column-to-column predicates inside PostgreSQL.
                let direct=sqls[index+usize::from(file)].replace("$6","@department").replace("$5","@department").replace("$4","@branch").replace("@department","$4").replace("@branch","$3");
                let relation=match kind {MaintenanceAssetPurchaseKind::Work=>"maintenance_work_orders",MaintenanceAssetPurchaseKind::Plan=>"maintenance_preventive_plans"};
                let projected=direct.replace(&format!("FROM {relation} r JOIN"),"FROM maintenance_asset_purchase_source r JOIN");
                assert_ne!(projected,direct,"actual TS source shape changed");
                let projected=format!("WITH {} {}",observed.cte_sql(),projected);
                let legacy=observed.bind_source(sqlx::query(sqlx::AssertSqlSafe(direct.as_str()))).fetch_one(&mut *tx).await.unwrap();
                let joined=observed.bind_source(sqlx::query(sqlx::AssertSqlSafe(projected.as_str()))).fetch_one(&mut *tx).await.unwrap();
                let expected:serde_json::Value=serde_json::from_str(sqlx::Row::try_get::<&str,_>(&legacy,"image").unwrap()).unwrap();
                let actual:serde_json::Value=serde_json::from_str(sqlx::Row::try_get::<&str,_>(&joined,"image").unwrap()).unwrap();
                assert_eq!(actual,expected);assert_eq!(expected["ownerEligible"],true,"fixture must establish genuine current owner eligibility");observed.require_current(&mut tx).await.unwrap();
                sqlx::query("SAVEPOINT owner_inactive").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE public.hr_engagements SET status='suspended' WHERE id='engagement'").execute(&mut *tx).await.unwrap();
                let legacy=observed.bind_source(sqlx::query(sqlx::AssertSqlSafe(direct.as_str()))).fetch_one(&mut *tx).await.unwrap();
                let joined=observed.bind_source(sqlx::query(sqlx::AssertSqlSafe(projected.as_str()))).fetch_one(&mut *tx).await.unwrap();
                let negative:serde_json::Value=serde_json::from_str(sqlx::Row::try_get::<&str,_>(&legacy,"image").unwrap()).unwrap();
                let projected_negative:serde_json::Value=serde_json::from_str(sqlx::Row::try_get::<&str,_>(&joined,"image").unwrap()).unwrap();
                assert_eq!(negative["ownerEligible"],false);assert_eq!(projected_negative,negative);
                observed.require_current(&mut tx).await.unwrap();
                sqlx::query("ROLLBACK TO SAVEPOINT owner_inactive").execute(&mut *tx).await.unwrap();
                let restored=observed.bind_source(sqlx::query(sqlx::AssertSqlSafe(projected.as_str()))).fetch_one(&mut *tx).await.unwrap();
                let restored:serde_json::Value=serde_json::from_str(sqlx::Row::try_get::<&str,_>(&restored,"image").unwrap()).unwrap();
                assert_eq!(restored["ownerEligible"],true);observed.require_current(&mut tx).await.unwrap();}
            let absent=observe_maintenance_asset_purchase_record(&mut tx,&locks,kind,&company,Some("foreign"),None,id,false).await.unwrap();assert!(!absent.exists());absent.require_current(&mut tx).await.unwrap();
        }
        let command=observe_maintenance_asset_purchase_record(&mut tx,&locks,MaintenanceAssetPurchaseKind::Work,&company,None,None,"work",false).await.unwrap();let file=observe_maintenance_asset_purchase_record(&mut tx,&locks,MaintenanceAssetPurchaseKind::Work,&company,None,None,"work",true).await.unwrap();
        sqlx::query("SAVEPOINT record_version").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.maintenance_work_orders SET version=version+1,updated_at=updated_at+1 WHERE id='work'").execute(&mut *tx).await.unwrap();assert!(command.require_current(&mut tx).await.is_err());file.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT record_version").execute(&mut *tx).await.unwrap();command.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT record_content").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.maintenance_work_orders SET title='changed' WHERE id='work'").execute(&mut *tx).await.unwrap();assert!(command.require_current(&mut tx).await.is_err());assert!(file.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT record_content").execute(&mut *tx).await.unwrap();command.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn retirement_work_matches_real_assets_typescript_guard() {
    use erpx_server_maintenance::public::{asset_purchase_locks::*,asset_purchase_record::*};
    use erpx_server_runtime::public::postgres::{MigrationTrack,load_migrations,migrate_database};
    fixture::with_database(2,|pool|async move {
        let migrations=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
        // Extract the exact guard from the Assets adapter and the actual
        // Maintenance-owned projection; neither selector is caller SQL.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const fs=await import('node:fs');const m=await import(process.argv[2]);const s=fs.readFileSync(process.argv[1],'utf8');const match=s.match(/assertion\(write,key,'no-active-work',`([^`]+)`/);if(!match)throw Error('GUARD_MISSING');const sql=match[1].replace('${maintenanceProjectionSources.maintenance_work_orders}',m.finalRootOwnerProjectionSources.maintenance_work_orders).replace('?', '$1');process.stdout.write('SELECT '+sql);"#;
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(&root).arg("--conditions=react-server").arg("-e").arg(script)
            .arg(root.join("packages/server-assets/src/adapters/assets.ts")).arg(root.join("packages/server-maintenance/src/adapters/final-root-owner-projection-sources.ts")).output().unwrap();
        assert!(output.status.success(),"actual Assets guard oracle unavailable");let sql=String::from_utf8(output.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_maintenance_asset_purchase_sources(&mut tx).await.unwrap();let locks=bind_maintenance_asset_purchase_sources(&mut tx).await.unwrap();
        let long_id="x".repeat(240);
        sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES($1,'company','branch','RETIRE-LONG','Synthetic','synthetic','THB','user',0,0)").bind(&long_id).execute(&mut *tx).await.unwrap();
        let vacant=observe_asset_retirement_work(&mut tx,&locks,&long_id).await.unwrap();assert!(vacant.is_clear());
        let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(&long_id).fetch_one(&mut *tx).await.unwrap();assert_eq!(actual,vacant.is_clear());
        for (id,company,status) in [("done","company","completed"),("cancelled","company","cancelled"),("foreign-active","foreign","open")] {
            sqlx::query("INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,completion_evidence,resulting_condition,created_by,created_at,updated_at) VALUES($1,$2,'branch',$3,'Synthetic','normal',$4,'user',CASE WHEN $4='completed' THEN 'synthetic completion' ELSE NULL END,CASE WHEN $4='completed' THEN 'good' ELSE NULL END,'user',0,0)").bind(id).bind(company).bind(&long_id).bind(status).execute(&mut *tx).await.unwrap();
            let observed=observe_asset_retirement_work(&mut tx,&locks,&long_id).await.unwrap();
            let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(&long_id).fetch_one(&mut *tx).await.unwrap();assert_eq!(observed.is_clear(),actual);
            if status=="open" {assert!(!actual);assert!(vacant.require_current(&mut tx).await.is_err());} else {assert!(actual);vacant.require_current(&mut tx).await.unwrap();}
        }
        let occupied=observe_asset_retirement_work(&mut tx,&locks,&long_id).await.unwrap();assert!(!occupied.is_clear());occupied.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}
