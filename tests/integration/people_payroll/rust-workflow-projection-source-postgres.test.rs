//! Actual People/Payroll Workflow projection source reads over migrated PostgreSQL.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::workflow_projection_source::{
    PeopleProjectionKind, PeopleProjectionSource, people_projection_source,
};
use erpx_server_runtime::public::postgres::{
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
