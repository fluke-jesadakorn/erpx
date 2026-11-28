//! Actual People/Payroll Workflow projection source reads over migrated PostgreSQL.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_people_payroll_server::public::workflow_projection_source::{
    PeopleProjectionKind, PeopleProjectionSource, people_projection_source,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::Connection;

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES\
           ('person','user','company','Person','TH','active','fixture',0,0);
         INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,created_at,department_id,person_id) VALUES\
           ('person','user','company','branch','Person','active',0,'department','person');
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES\
           ('engagement','person','company','branch','department','employee','salaried','active','2000-01-01','Worker','THB','fixture',0,0);
         INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,branch_id,department_id,created_by,mode,state,timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,version,created_at,updated_at) VALUES\
           ('overtime-1','company','engagement','person','overtime-branch',NULL,'creator','advance','authorized','Asia/Bangkok','2026-01-01T18:00','2026-01-01T20:00',1767271200000,1767278400000,0,120,'Late delivery',4,0,0);
         INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,state,version,created_by,created_at,updated_at,department_id) VALUES\
           ('leave-1','LV-1','person','company','leave-branch','2026-02-02',1,'pending_approval',3,'creator',0,0,'leave-department');
         INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,created_by,created_at,updated_at) VALUES\
           ('group','company','Monthly','TH','THB','monthly','fixture',0,0);
         INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,revision,version,created_by,created_at,updated_at) VALUES\
           ('payroll-1','company','group','2026-01-01','2026-01-31','2026-01-31','THB','preview','preview',1,6,'creator',0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn people_projection_sources_copy_the_retained_owner_scope_and_state() {
    fixture::with_database(1, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();

        assert_eq!(
            people_projection_source(&mut tx, PeopleProjectionKind::Overtime, "overtime-1")
                .await
                .unwrap(),
            Some(PeopleProjectionSource {
                state: "authorized".into(),
                version: 4,
                company_id: "company".into(),
                branch_id: Some("overtime-branch".into()),
                department_id: None,
            })
        );
        assert_eq!(
            people_projection_source(&mut tx, PeopleProjectionKind::Leave, "leave-1")
                .await
                .unwrap(),
            Some(PeopleProjectionSource {
                state: "pending_approval".into(),
                version: 3,
                company_id: "company".into(),
                branch_id: Some("leave-branch".into()),
                department_id: Some("leave-department".into()),
            })
        );
        // Payroll projects `status` as state and has no branch/department scope.
        assert_eq!(
            people_projection_source(&mut tx, PeopleProjectionKind::Payroll, "payroll-1")
                .await
                .unwrap(),
            Some(PeopleProjectionSource {
                state: "preview".into(),
                version: 6,
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
            })
        );
        for (kind, id) in [
            (PeopleProjectionKind::Overtime, "missing"),
            (PeopleProjectionKind::Leave, "missing"),
            (PeopleProjectionKind::Payroll, "missing"),
        ] {
            assert_eq!(
                people_projection_source(&mut tx, kind, id).await.unwrap(),
                None
            );
        }
        tx.rollback().await.unwrap();

        // The source is a read-only fact: rollback removed no owner row and the
        // port neither fabricated nor committed anything.
        assert_eq!(
            people_projection_source(&mut connection, PeopleProjectionKind::Leave, "leave-1")
                .await
                .unwrap(),
            Some(PeopleProjectionSource {
                state: "pending_approval".into(),
                version: 3,
                company_id: "company".into(),
                branch_id: Some("leave-branch".into()),
                department_id: Some("leave-department".into()),
            })
        );
    })
    .await;
}

#[tokio::test]
async fn workflow_people_member_names_match_legacy_without_synthetic_caps() {
    use erpx_people_payroll_server::public::service_count_flow_people::{lock_journal_flow_people_sources, read_service_count_member_names};
    use erpx_platform_shared_server::public::CompanyId;
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) SELECT 'person-'||n,CASE WHEN n=1 THEN repeat('é',101) ELSE 'user-'||n END,'company',repeat('x',4200)||'ไทย🙂'||n,'TH','active','fixture',0,0 FROM generate_series(1,501) n")
            .execute(pool.raw()).await.unwrap();
        let script = r#"
if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');
const {createFlowRoutingSourceRepository}=await import('./modules/people-payroll/server/src/adapters/workflow-flow-routing-facts.ts');
let sql=[];const db={prepare(q){sql.push(q);return{bind(){return{first(){return null}}}}}};
const repo=createFlowRoutingSourceRepository(db);repo.readPrepareFlowCommandPerson('company','user');repo.readAssignFlowPrimaryAdministrator('company','user');
if(sql.length!==2||sql[0]!==sql[1])throw Error('PEOPLE_FLOW_SOURCE_CHANGED');process.stdout.write(sql[0]);
"#;
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server","-e",script]).output().unwrap();
        assert!(output.status.success(),"actual TS People flow SQL unavailable: {}",String::from_utf8_lossy(&output.stderr));
        let sql=String::from_utf8(output.stdout).unwrap().replacen('?',"$1",1).replacen('?',"$2",1);
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_journal_flow_people_sources(&mut tx).await.unwrap();
        let ids:Vec<String>=sqlx::query_scalar("SELECT user_id FROM hr_people WHERE company_id='company' ORDER BY user_id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(ids.len(),501);assert!(ids.iter().any(|v|v.len()>200 && v.encode_utf16().count()<=200));
        let actual=read_service_count_member_names(&mut tx,&CompanyId::new("company").unwrap(),&ids).await.unwrap();
        assert_eq!(actual.len(),501);assert!(actual.iter().map(|(_,name)|name.len()).sum::<usize>()>2097152);
        for (id,name) in &actual {
            let expected:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind("company").bind(id).fetch_optional(&mut *tx).await.unwrap();
            assert_eq!(expected.as_deref(),Some(name.as_str()));
        }
        sqlx::query("SAVEPOINT people_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE hr_people SET legal_name='changed' WHERE id='person-1'").execute(&mut *tx).await.unwrap();
        assert_ne!(read_service_count_member_names(&mut tx,&CompanyId::new("company").unwrap(),&ids).await.unwrap(),actual);
        sqlx::query("ROLLBACK TO SAVEPOINT people_drift").execute(&mut *tx).await.unwrap();
        assert_eq!(read_service_count_member_names(&mut tx,&CompanyId::new("company").unwrap(),&ids).await.unwrap(),actual);
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn stock_count_people_engagements_retain_exact_same_unit_source() {
    use erpx_people_payroll_server::public::service_count_flow_people::{inspect_stock_count_flow_people,lock_journal_flow_people_sources};
    use erpx_platform_shared_server::public::CompanyId;
    fixture::with_database(2,|pool|async move {
        migrate(&pool).await;
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('p1','person-one','company',repeat('x',4000)||'ไทย','TH','active','fixture',0,0),('p2','without-assignment','company','Second','TH','active','fixture',0,0),('p3','inactive','company','Inactive','TH','inactive','fixture',0,0),('foreign','foreign-user','other','Foreign','TH','active','fixture',0,0);
            INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,created_by,created_at,updated_at) VALUES
              ('e1','p1','company','branch','department-a','employee','salaried','active','2000-01-01',NULL,'Worker','THB','fixture',0,0),('e2','p1','company','branch','department-b','employee','salaried','active','2000-01-01',NULL,'Worker','THB','fixture',0,0),('expired','p1','company',NULL,NULL,'employee','salaried','active','2000-01-01','2001-01-01','Worker','THB','fixture',0,0),('inactive-e','p1','company',NULL,NULL,'employee','salaried','ended','2000-01-01',NULL,'Worker','THB','fixture',0,0),('e3','p2','company',NULL,NULL,'employee','salaried','active','2000-01-01',NULL,'Worker','THB','fixture',0,0),('e4','p3','company',NULL,NULL,'employee','salaried','active','2000-01-01',NULL,'Worker','THB','fixture',0,0),('e5','foreign','other',NULL,NULL,'employee','salaried','active','2000-01-01',NULL,'Worker','THB','fixture',0,0);
            INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('a1','person-one','holding_owner','[]','company',0,8000000000000000,1,'holding')")
            .execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET version=9007199254740992 WHERE id='e2'").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {activeStaffSelect}=await import('./modules/operations/server/src/adapters/staff-policy.ts');process.stdout.write(activeStaffSelect);"#;
        let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","-e",script]).output().unwrap();
        assert!(out.status.success(),"actual TS staff source: {}",String::from_utf8_lossy(&out.stderr));
        let sql=String::from_utf8(out.stdout).unwrap().replacen('?',"$1",1).replacen('?',"$2",1).replacen('?',"$3",1);
        let sql=format!("SELECT user_id,engagement_id FROM ({sql}) s ORDER BY user_id,engagement_id");
        let company=CompanyId::new("company").unwrap();
        let mut missing=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing).await.unwrap();
        assert!(inspect_stock_count_flow_people(&mut missing,&company).await.is_err());missing.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_journal_flow_people_sources(&mut tx).await.unwrap();
        let source=inspect_stock_count_flow_people(&mut tx,&company).await.unwrap();
        let rows:Vec<_>=source.rows().iter().map(|r|(r.user_id.as_str(),r.engagement_id.as_str())).collect();
        assert_eq!(rows,vec![("person-one","e1"),("person-one","e2"),("without-assignment","e3")]);
        assert!(source.rows()[0].name.len()>3000);
        let projection_script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {finalRootOwnerProjectionSources:s}=await import('./modules/people-payroll/server/src/adapters/final-root-owner-projection-sources.ts');process.stdout.write(JSON.stringify({people:s.hr_people,engagements:s.hr_engagements}));"#;
        let projection_out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","-e",projection_script]).output().unwrap();
        assert!(projection_out.status.success(),"actual TS People projections unavailable");
        let ts_projection:serde_json::Value=serde_json::from_slice(&projection_out.stdout).unwrap();
        {
            let mut bound=source.bind_projection(&mut tx).await.unwrap();
            let ts_people=ts_projection["people"].as_str().unwrap();
            let ts_engagements=ts_projection["engagements"].as_str().unwrap();
            assert_eq!(bound.people_sql(),ts_people.replace("FROM hr_people","FROM public.hr_people"));
            assert_eq!(bound.engagements_sql(),ts_engagements.replace("FROM hr_engagements","FROM public.hr_engagements"));
            for (native,legacy) in [(bound.people_sql(),ts_people),(bound.engagements_sql(),ts_engagements)] {
                let n=format!("SELECT row_to_json(t)::text FROM ({native}) t WHERE t.company_id=$1 ORDER BY t.id");
                let t=format!("SELECT row_to_json(t)::text FROM ({legacy}) t WHERE t.company_id=$1 ORDER BY t.id");
                let actual:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(n)).bind("company").fetch_all(bound.connection()).await.unwrap();
                let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(t)).bind("company").fetch_all(bound.connection()).await.unwrap();
                assert_eq!(actual,expected,"owner projection rows and column order");
            }
            bound.finish().await.unwrap();
        }
        let now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *tx).await.unwrap();
        let actual_ts:Vec<(String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind("company").bind(now).bind(now).fetch_all(&mut *tx).await.unwrap();
        assert_eq!(actual_ts,vec![("person-one".into(),"e1".into()),("person-one".into(),"e2".into())]);
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT people_source_drift").execute(&mut *tx).await.unwrap();
        let mut bound=source.bind_projection(&mut tx).await.unwrap();
        sqlx::query("UPDATE hr_people SET legal_name='drift' WHERE id='p1'").execute(bound.connection()).await.unwrap();
        assert!(bound.finish().await.is_err(),"bound projection must detect source drift");
        sqlx::query("ROLLBACK TO SAVEPOINT people_source_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET department_id='changed' WHERE id='e2'").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT people_source_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET version=version+1 WHERE id='e2'").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err(),"adjacent large integers must remain distinct");
        sqlx::query("ROLLBACK TO SAVEPOINT people_source_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_journal_flow_people_sources(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());assert!(source.bind_projection(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();
        sqlx::query("SAVEPOINT before_people_lock").execute(&mut *saved).await.unwrap();lock_journal_flow_people_sources(&mut saved).await.unwrap();
        let retained=inspect_stock_count_flow_people(&mut saved,&company).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_people_lock").execute(&mut *saved).await.unwrap();assert!(retained.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}
