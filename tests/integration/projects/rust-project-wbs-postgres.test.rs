//! Native Projects detail and WBS reads on migrated PostgreSQL.
//! These tests prove owner SQL, Company/membership scope, bounded dependency
//! facts, persisted-source readback and rollback; they do not prove a command,
//! purchase, task or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_projects::public::project_wbs::{PgProjectWbsRead, ProjectReadActor, project_wbs};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(user: &str, company: &str, permissions: &[&str], modules: &[&str]) -> ProjectReadActor {
    ProjectReadActor {
        user_id: user.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW.get() + 60_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES \
         ('company','owner','principal-owner','person-owner','active'),\
         ('company','alias-user','principal-alias','person-owner','active'),\
         ('company','member-user','principal-member','person-member','active'),\
         ('company','other','principal-other','person-other','active'),\
         ('foreign','owner','principal-foreign','person-foreign','active');\
         INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,version,created_at,updated_at,lifecycle_status) VALUES \
         ('project-owned','company','branch','department','Owned project','Bounded project source','owner',4,10,10,'active'),\
         ('project-member','company',NULL,NULL,'Member project','Bounded member source','owner',1,10,10,'active'),\
         ('project-hidden','company',NULL,NULL,'Hidden project','Bounded hidden source','other',1,10,10,'active'),\
         ('project-foreign','foreign',NULL,NULL,'Foreign project','Bounded foreign source','owner',1,10,10,'active');\
         INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES \
         ('project-member','member-user','owner',10);\
         INSERT INTO staff_project_milestones(id,company_id,project_id,title,description,due_at,owner_user_id,state,version,created_by,completed_at,created_at,updated_at,parent_milestone_id,ordinal_position) VALUES \
         ('m-root','company','project-owned','Programme','Root package',2000,'owner','planned',1,'owner',NULL,10,10,NULL,0),\
         ('m-child','company','project-owned','Delivery','Child package',1000,'owner','planned',1,'owner',NULL,10,10,'m-root',1),\
         ('m-second','company','project-owned','Acceptance','Successor package',3000,'owner','planned',1,'owner',NULL,10,10,'m-root',2),\
         ('m-done','company','project-owned','Completed predecessor','Completed package',500,'owner','completed',1,'owner',600,10,10,NULL,3),\
         ('m-next','company','project-owned','Next package','Dependent package',4000,'owner','planned',1,'owner',NULL,10,10,NULL,4),\
         ('m-member','company','project-member','Member package','Member-only package',1000,'owner','planned',1,'owner',NULL,10,10,NULL,0),\
         ('m-hidden','company','project-hidden','Hidden package','Hidden-only package',1000,'other','planned',1,'other',NULL,10,10,NULL,0),\
         ('m-foreign','foreign','project-foreign','Foreign package','Foreign-only package',1000,'owner','planned',1,'owner',NULL,10,10,NULL,0);\
         INSERT INTO staff_project_milestone_dependencies(company_id,project_id,predecessor_milestone_id,successor_milestone_id,created_by,created_at) VALUES \
         ('company','project-owned','m-child','m-second','owner',10),\
         ('company','project-owned','m-done','m-next','owner',10);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn owner_and_member_read_company_scoped_wbs_with_dependencies() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let owner = actor("owner", "company", &["records.read"], &["projects"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgProjectWbsRead::new(connection);
                    let result = project_wbs(&current, &mut reader, "project-owned", NOW).await?;
                    assert_eq!(result.project.id, "project-owned");
                    assert_eq!(result.project.lifecycle_status, "active");
                    assert_eq!(result.project.version, 4);
                    assert_eq!(result.milestones.len(), 5);
                    assert_eq!(result.milestones[0].id, "m-done");
                    assert!(!result.milestones[0].is_overdue);
                    let child = result
                        .milestones
                        .iter()
                        .find(|row| row.id == "m-child")
                        .unwrap();
                    assert!(child.is_overdue);
                    let second = result
                        .milestones
                        .iter()
                        .find(|row| row.id == "m-second")
                        .unwrap();
                    assert_eq!(second.depends_on_ids, vec!["m-child"]);
                    assert_eq!(second.depends_on_titles, vec!["Delivery"]);
                    assert_eq!(second.blocked_by_titles, vec!["Delivery"]);
                    assert_eq!(second.blocked_by_count, 1);
                    let next = result
                        .milestones
                        .iter()
                        .find(|row| row.id == "m-next")
                        .unwrap();
                    assert_eq!(next.depends_on_ids, vec!["m-done"]);
                    assert!(next.blocked_by_titles.is_empty());
                    assert_eq!(next.blocked_by_count, 0);

                    let alias = actor("alias-user", "company", &["records.read"], &["projects"]);
                    let alias_result =
                        project_wbs(&alias, &mut reader, "project-owned", NOW).await?;
                    assert_eq!(alias_result.project.owner_user_id, "owner");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let member = actor("member-user", "company", &["records.read"], &["projects"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = member.clone();
                Box::pin(async move {
                    let mut reader = PgProjectWbsRead::new(connection);
                    let result = project_wbs(&current, &mut reader, "project-member", NOW).await?;
                    assert_eq!(result.milestones.len(), 1);
                    let hidden = business(
                        project_wbs(&current, &mut reader, "project-hidden", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(hidden.kind, FailureKind::NotFound);
                    assert_eq!(hidden.code, "PROJECT_NOT_FOUND");
                    let foreign = business(
                        project_wbs(&current, &mut reader, "project-foreign", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(foreign.code, "PROJECT_NOT_FOUND");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor("owner", "foreign", &["records.read"], &["projects"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgProjectWbsRead::new(connection);
                    let result = project_wbs(&current, &mut reader, "project-foreign", NOW).await?;
                    assert_eq!(result.project.company_id, "foreign");
                    assert_eq!(result.milestones.len(), 1);
                    assert_eq!(result.milestones[0].id, "m-foreign");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let persisted: i64 = sqlx::query_scalar("SELECT count(*) FROM staff_project_milestones")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(persisted, 8, "reads must not create or delete project rows");
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_and_rollback_preserve_the_source() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("owner", "company", &["records.read"], &["projects"]);

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgProjectWbsRead::new(&mut outside)
            .project(
                &CompanyId::new("company").unwrap(),
                "project-owned",
                &owner.user_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PROJECT_WBS_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgProjectWbsRead::new(&mut default_isolation)
            .project(
                &CompanyId::new("company").unwrap(),
                "project-owned",
                &owner.user_id,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PROJECT_WBS_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM staff_project_milestones")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgProjectWbsRead::new(connection);
                    let result = project_wbs(&current, &mut reader, "project-owned", NOW).await?;
                    assert_eq!(result.milestones.len(), 5);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM staff_project_milestones")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(before, after);

        let rollback = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO staff_project_milestones(id,company_id,project_id,title,description,due_at,owner_user_id,state,version,created_by,completed_at,created_at,updated_at,parent_milestone_id,ordinal_position) VALUES \
                         ('m-rollback','company','project-owned','Rollback package','Rollback source',9000,'owner','planned',1,'owner',NULL,10,10,NULL,9);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgProjectWbsRead::new(connection);
                    let result = project_wbs(&current, &mut reader, "project-owned", NOW).await?;
                    assert_eq!(result.milestones.len(), 6);
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await;
        assert!(rollback.is_err());
        let disappeared: i64 =
            sqlx::query_scalar("SELECT count(*) FROM staff_project_milestones WHERE id='m-rollback'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(disappeared, 0);
    })
    .await;
}

#[tokio::test]
async fn policy_and_selected_context_fail_closed_before_source_reads() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut reader = PgProjectWbsRead::new(connection);
                    let denied = actor("owner", "company", &[], &["projects"]);
                    let error = business(
                        project_wbs(&denied, &mut reader, "project-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Forbidden);
                    assert_eq!(error.code, "PERMISSION_REQUIRED");

                    let module = actor("owner", "company", &["records.read"], &["finance"]);
                    let error = business(
                        project_wbs(&module, &mut reader, "project-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.code, "MODULE_FORBIDDEN");

                    let mut expired = actor("owner", "company", &["records.read"], &["projects"]);
                    expired.expires_at = NOW.get();
                    let error = business(
                        project_wbs(&expired, &mut reader, "project-owned", NOW)
                            .await
                            .unwrap_err(),
                    );
                    assert_eq!(error.kind, FailureKind::Conflict);
                    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn project_graph_reads_captured_minutes_and_rejects_unsafe_raw_actual_fields() {
    use erpx_server_projects::public::project_graph::PgProjectGraphRead;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,reviewed_by,reviewed_at,created_at,updated_at) VALUES('graph-source','company','project-owned','other','2026-01-01',10,'approved',1,'owner',1,1,1); INSERT INTO staff_project_package_actuals(id,company_id,project_id,milestone_id,source_kind,source_id,source_version,minutes,amount_minor,currency,linked_by,linked_at) VALUES('graph-actual','company','project-owned','m-child','work_log','graph-source',1,10,NULL,NULL,'owner',1); UPDATE staff_project_work_logs SET minutes=20,version=2 WHERE id='graph-source'").execute(&mut *tx).await.unwrap();
        let actuals=PgProjectGraphRead::new(&mut tx).actuals(&CompanyId::new("company").unwrap(),"project-owned","owner").await.unwrap();
        let stale=actuals.iter().find(|row|row.id=="graph-actual").unwrap();
        assert_eq!((stale.minutes,stale.source_version,stale.current_source_version),(Some(10),1,2));
        assert!(!stale.current());
        // linked_at is not displayed but is part of legacy SELECT a.* and
        // node-pg rejects unsafe BIGINT before applying visibility filters.
        sqlx::raw_sql("INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,reviewed_by,reviewed_at,created_at,updated_at) VALUES('unsafe-source','company','project-owned','other','2026-01-02',10,'approved',1,'owner',1,1,1); INSERT INTO staff_project_package_actuals(id,company_id,project_id,milestone_id,source_kind,source_id,source_version,minutes,amount_minor,currency,linked_by,linked_at) VALUES('unsafe-actual','company','project-owned','m-child','work_log','unsafe-source',1,10,NULL,NULL,'owner',9007199254740992)").execute(&mut *tx).await.unwrap();
        let result=PgProjectGraphRead::new(&mut tx).actuals(&CompanyId::new("company").unwrap(),"project-owned","unrelated").await;
        assert!(matches!(result,Err(PgTransactionError::Persistence(ref e)) if e.code=="DATABASE_NUMBER_OUT_OF_RANGE"));
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn graph_omits_deleted_and_foreign_reused_actual_source() {
    use erpx_server_projects::public::project_graph::PgProjectGraphRead;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,reviewed_by,reviewed_at,created_at,updated_at) VALUES('removed-source','company','project-owned','other','2026-01-01',10,'approved',1,'owner',1,1,1); INSERT INTO staff_project_package_actuals(id,company_id,project_id,milestone_id,source_kind,source_id,source_version,minutes,amount_minor,currency,linked_by,linked_at) VALUES('orphan-actual','company','project-owned','m-child','work_log','removed-source',1,10,NULL,NULL,'owner',1)").execute(&mut *tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        assert_eq!(PgProjectGraphRead::new(&mut tx).actuals(&company,"project-owned","owner").await.unwrap().len(),1);
        sqlx::query("DELETE FROM staff_project_work_logs WHERE id='removed-source'").execute(&mut *tx).await.unwrap();
        assert!(PgProjectGraphRead::new(&mut tx).actuals(&company,"project-owned","owner").await.unwrap().is_empty());
        sqlx::query("INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,reviewed_by,reviewed_at,created_at,updated_at) VALUES('removed-source','foreign','project-foreign','owner','2026-01-01',10,'approved',1,'owner',1,1,1)").execute(&mut *tx).await.unwrap();
        let foreign_reused=PgProjectGraphRead::new(&mut tx).actuals(&company,"project-owned","owner").await.unwrap();
        assert!(foreign_reused.is_empty(), "foreign source must be indistinguishable from missing and never returned");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn graph_cost_sources_do_not_reveal_foreign_company_journals() {
    use erpx_server_projects::public::project_graph::PgProjectGraphRead;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('graph-chart-own','company','Own','THB','owner',0,0),('graph-chart-foreign','foreign','Foreign','THB','owner',0,0); INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('graph-period-own','company','Own','2026-01-01','2026-12-31',0),('graph-period-foreign','foreign','Foreign','2026-01-01','2026-12-31',0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at) VALUES('graph-account-own','company','graph-chart-own','100','Own','asset',0,0),('graph-account-foreign','foreign','graph-chart-foreign','100','Foreign','asset',0,0); INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,created_at,updated_at) VALUES('graph-journal-own','company','graph-chart-own','graph-period-own','OWN','2026-01-01','THB','posted','owner',0,0),('graph-journal-foreign','foreign','graph-chart-foreign','graph-period-foreign','FOREIGN','2026-01-01','THB','posted','owner',0,0); INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,ordinal) VALUES('graph-line-own','graph-journal-own','graph-account-own',100,0,0),('graph-line-foreign','graph-journal-foreign','graph-account-foreign',100,0,0); INSERT INTO staff_project_cost_allocations(id,company_id,project_id,journal_line_id,amount_minor,state,version,created_by,approved_by,created_at,updated_at) VALUES('graph-cost-own','company','project-owned','graph-line-own',100,'approved',1,'owner','other',0,0),('graph-cost-foreign','company','project-owned','graph-line-foreign',100,'approved',1,'owner','other',0,0); INSERT INTO staff_project_package_actuals(id,company_id,project_id,milestone_id,source_kind,source_id,source_version,amount_minor,currency,linked_by,linked_at) VALUES('graph-cost-actual-own','company','project-owned','m-child','cost_allocation','graph-cost-own',1,100,'THB','owner',1),('graph-cost-actual-foreign','company','project-owned','m-child','cost_allocation','graph-cost-foreign',1,100,'THB','owner',1)").execute(&mut *tx).await.unwrap();
        let rows=PgProjectGraphRead::new(&mut tx).actuals(&CompanyId::new("company").unwrap(),"project-owned","owner").await.unwrap();
        assert_eq!(rows.len(),1);
        assert_eq!((rows[0].id.as_str(),rows[0].journal_id.as_deref(),rows[0].journal_status.as_deref()),("graph-cost-actual-own",Some("graph-journal-own"),Some("posted")));
        assert!(!rows.iter().any(|row|row.journal_id.as_deref()==Some("graph-journal-foreign")));
        tx.rollback().await.unwrap();
    }).await;
}
