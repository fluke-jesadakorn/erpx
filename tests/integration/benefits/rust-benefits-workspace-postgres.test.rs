//! Native Benefits enrollment workspace reads over real migrated PostgreSQL.
//! These prove owner read SQL, Company isolation, current worker/reviewer
//! scope, serializable transaction guards, rollback and persisted-source
//! readback. They are not a Benefits command, package settlement or browser
//! journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_benefits::public::payroll_source::{
    BenefitsPayrollQuery, BenefitsPayrollScope, BenefitsPayrollSourceRepository,
    PgBenefitsPayrollSource, payroll_benefit_inputs,
};
use erpx_server_benefits::public::workspace::{
    BenefitsActor, BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture,
    BenefitsPeoplePort, BenefitsWorker, PgBenefitsWorkspaceRead, benefits_election_detail,
    benefits_workspace,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(company: &str, permissions: &[&str]) -> BenefitsActor {
    BenefitsActor {
        user_id: if company == "foreign" {
            "foreign-owner".into()
        } else {
            "owner".into()
        },
        company_id: CompanyId::new(company).unwrap(),
        branch_id: None,
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["benefits".into()],
        work_context: "b".repeat(64),
        expires_at: NOW.get() + 60_000,
    }
}

fn owner_worker() -> BenefitsWorker {
    BenefitsWorker {
        person_id: "person-owner".into(),
        engagement_id: "engagement-owner".into(),
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
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES \
         ('person-owner','owner','company','Owner Worker','TH','active',1,'owner',10,10),\
         ('person-other','other','company','Other Worker','TH','active',1,'owner',10,10),\
         ('person-foreign','foreign-owner','foreign','Foreign Worker','TH','active',1,'owner',10,10);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) VALUES \
         ('engagement-owner','person-owner','company','branch','department','employee','salaried','active','2024-01-01','Engineer','THB',1,'owner',10,10),\
         ('engagement-other','person-other','company','branch',NULL,'employee','salaried','active','2024-01-01','Designer','THB',1,'owner',10,10),\
         ('engagement-foreign','person-foreign','foreign','foreign-branch',NULL,'employee','salaried','active','2024-01-01','Operator','THB',1,'owner',10,10);\
         INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,reviewed_by,version,created_at,updated_at,required_eligibility_code) VALUES \
         ('plan-a','company','Plan A','Synthetic provider','family','THB',12550,30000,'2026-01-01','2026-12-31','published','owner','reviewer',1,10,100,'PROBATION_COMPLETE'),\
         ('plan-b','company','Plan B','Synthetic provider','employee','THB',5000,15000,'2026-01-01',NULL,'draft','owner',NULL,2,11,200,NULL),\
         ('plan-foreign','foreign','Foreign Plan','Foreign provider','employee','THB',1000,2000,'2026-01-01',NULL,'published','foreign-owner','reviewer',1,12,300,NULL);\
         INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,effective_on,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('window-open','company','Annual enrollment','2026-11-01','2026-11-30','2027-01-01','open','owner','reviewer',1,10,150),\
         ('window-closed','company','Closed enrollment','2025-11-01','2025-11-30','2026-01-01','closed','owner','reviewer',1,10,50),\
         ('window-foreign','foreign','Foreign enrollment','2026-11-01','2026-11-30','2027-01-01','open','foreign-owner','reviewer',1,10,250);\
         INSERT INTO benefit_dependents(id,company_id,person_id,name,relationship,birth_date,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('dependent-owner','company','person-owner','Owner Child','child','2018-01-01','verified','owner','reviewer',1,10,120),\
         ('dependent-owner-old','company','person-owner','Owner Parent','parent','1960-01-01','pending','owner',NULL,1,10,80),\
         ('dependent-other','company','person-other','Other Child','child','2019-01-01','verified','owner','reviewer',1,10,200),\
         ('dependent-foreign','foreign','person-foreign','Foreign Child','child','2017-01-01','verified','foreign-owner','reviewer',1,10,300);\
         INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,version,created_at,updated_at) VALUES \
         ('election-owner','company','person-owner','engagement-owner','plan-a',1,'window-open',1,'family','[\"dependent-owner\"]','THB',12550,30000,'2027-01-01','2027-12-31','elected','owner',1,10,300),\
         ('election-other','company','person-other','engagement-other','plan-a',1,'window-open',1,'employee','[]','THB',12550,30000,'2027-01-01','2027-12-31','submitted','owner',2,11,400),\
         ('election-foreign','foreign','person-foreign','engagement-foreign','plan-foreign',1,'window-foreign',1,'employee','[]','THB',1000,2000,'2027-01-01',NULL,'elected','foreign-owner',1,12,500);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Packages;

impl BenefitsEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async {
            Ok(BenefitsEntitlement {
                enforced: true,
                history_allowed: true,
                new_work: true,
                settle_existing: true,
                revision: "7".into(),
            })
        })
    }
}

struct People {
    worker: Option<BenefitsWorker>,
    names: Vec<(String, String)>,
}

impl BenefitsPeoplePort for People {
    type Error = BusinessError;

    fn worker<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        Box::pin(async move { Ok(self.worker.clone()) })
    }

    fn employee_names<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        Box::pin(async move { Ok(self.names.clone()) })
    }
}

fn owner_people() -> People {
    People {
        worker: Some(owner_worker()),
        names: vec![("person-owner".into(), "Owner Worker".into())],
    }
}

async fn counts(pool: &DatabasePool) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM benefit_plans),(SELECT count(*) FROM benefit_enrollment_windows),\
         (SELECT count(*) FROM benefit_dependents),(SELECT count(*) FROM benefit_elections)",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn workspace_reads_company_scoped_rows_in_exact_order_with_enriched_names() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let before = counts(&pool).await;
        let current = actor("company", &["records.read", "records.write"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = current.clone();
                Box::pin(async move {
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = Packages;
                    let workspace = benefits_workspace(
                        &current,
                        false,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        workspace
                            .plans
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["plan-b", "plan-a"]
                    );
                    assert_eq!(workspace.plans[0].version, 2);
                    assert_eq!(workspace.plans[1].employee_deduction_minor, 12_550);
                    assert_eq!(
                        workspace.plans[1].required_eligibility_code.as_deref(),
                        Some("PROBATION_COMPLETE")
                    );
                    assert!(
                        workspace
                            .plans
                            .iter()
                            .all(|row| row.company_id == "company")
                    );
                    assert_eq!(
                        workspace
                            .windows
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["window-open", "window-closed"]
                    );
                    assert_eq!(
                        workspace
                            .dependents
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["dependent-owner", "dependent-owner-old"]
                    );
                    assert!(
                        workspace
                            .dependents
                            .iter()
                            .all(|row| row.person_id == "person-owner")
                    );
                    assert_eq!(workspace.elections.len(), 1);
                    assert_eq!(workspace.elections[0].id, "election-owner");
                    assert_eq!(workspace.elections[0].plan_name, "Plan A");
                    assert_eq!(workspace.elections[0].employee_name, "Owner Worker");
                    assert_eq!(workspace.elections[0].status, "elected");
                    assert_eq!(workspace.elections[0].version, 1);
                    assert_eq!(workspace.purchase.unwrap().revision, "7");
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert_eq!(counts(&pool).await, before);
    })
    .await;
}

#[tokio::test]
async fn reviewer_reads_all_company_elections_and_personal_forces_employee_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let reviewer = actor("company", &["records.read", "benefits.review"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = reviewer.clone();
                Box::pin(async move {
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = People {
                        worker: Some(owner_worker()),
                        names: vec![
                            ("person-owner".into(), "Owner Worker".into()),
                            ("person-other".into(), "Other Worker".into()),
                        ],
                    };
                    let mut packages = Packages;
                    let workspace = benefits_workspace(
                        &current,
                        false,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        workspace
                            .elections
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["election-other", "election-owner"]
                    );
                    assert_eq!(workspace.elections[0].employee_name, "Other Worker");
                    assert_eq!(workspace.elections[1].employee_name, "Owner Worker");
                    assert!(workspace.can_review);
                    Ok(())
                })
            })
            .await
            .unwrap();

        let personal = actor("company", &["records.read", "benefits.review"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = personal.clone();
                Box::pin(async move {
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = Packages;
                    let workspace = benefits_workspace(
                        &current,
                        true,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(workspace.elections.len(), 1);
                    assert_eq!(workspace.elections[0].id, "election-owner");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn foreign_company_reads_only_its_own_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let foreign = actor("foreign", &["records.read", "benefits.review"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = People {
                        worker: Some(BenefitsWorker {
                            person_id: "person-foreign".into(),
                            engagement_id: "engagement-foreign".into(),
                        }),
                        names: vec![("person-foreign".into(), "Foreign Worker".into())],
                    };
                    let mut packages = Packages;
                    let workspace = benefits_workspace(
                        &current,
                        false,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(
                        workspace
                            .plans
                            .iter()
                            .map(|row| row.id.as_str())
                            .collect::<Vec<_>>(),
                        vec!["plan-foreign"]
                    );
                    assert_eq!(workspace.windows[0].id, "window-foreign");
                    assert_eq!(workspace.dependents[0].id, "dependent-foreign");
                    assert_eq!(workspace.elections[0].id, "election-foreign");
                    assert!(
                        workspace
                            .plans
                            .iter()
                            .all(|row| row.company_id == "foreign")
                    );
                    assert!(
                        workspace
                            .elections
                            .iter()
                            .all(|row| row.company_id == "foreign")
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback_hold() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", &["records.read"]);

        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgBenefitsWorkspaceRead::new(&mut outside)
            .plans(&CompanyId::new("company").unwrap(), 501)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "BENEFITS_WORKSPACE_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgBenefitsWorkspaceRead::new(&mut default_isolation)
            .windows(&CompanyId::new("company").unwrap(), 501)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "BENEFITS_WORKSPACE_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before = counts(&pool).await;
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = Packages;
                    let workspace =
                        benefits_workspace(&current, false, &mut reader, &mut people, &mut packages, NOW)
                            .await?;
                    assert_eq!(workspace.plans.len(), 2);
                    assert_eq!(workspace.elections.len(), 1);
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert_eq!(counts(&pool).await, before);

        let rollback_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = rollback_owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
                         ('plan-rollback','company','Rollback Plan','Synthetic provider','employee','THB',100,200,'2026-01-01',NULL,'draft','owner',NULL,1,10,999);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = Packages;
                    let workspace =
                        benefits_workspace(&current, false, &mut reader, &mut people, &mut packages, NOW)
                            .await?;
                    assert_eq!(workspace.plans[0].id, "plan-rollback");
                    assert_eq!(workspace.plans[0].name, "Rollback Plan");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM benefit_plans WHERE id='plan-rollback'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 0);
        assert_eq!(counts(&pool).await, before);
    })
    .await;
}

#[tokio::test]
async fn permission_and_package_denial_fail_before_the_owner_source_is_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let denied = actor("company", &[]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    sqlx::raw_sql("ALTER TABLE benefit_plans RENAME TO benefit_plans_hidden")
                        .execute(&mut *connection)
                        .await?;
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = Packages;
                    let error = benefits_workspace(
                        &current,
                        false,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(error).code, "BENEFITS_PERMISSION_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let denied = actor("company", &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "ALTER TABLE benefit_elections RENAME TO benefit_elections_hidden",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = owner_people();
                    let mut packages = DeniedPackage;
                    let error = benefits_workspace(
                        &current,
                        false,
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(error).code, "PACKAGE_REQUIRED");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

struct DeniedPackage;

impl BenefitsEntitlementPort for DeniedPackage {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async {
            Ok(BenefitsEntitlement {
                enforced: true,
                history_allowed: false,
                new_work: false,
                settle_existing: false,
                revision: "9".into(),
            })
        })
    }
}

#[tokio::test]
async fn election_detail_reads_company_scoped_history_and_released_payroll_input() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", &["records.read", "benefits.review"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,submitted_by,provider_reference,acknowledged_by,released_by,version,created_at,updated_at) VALUES \
                         ('election-released','company','person-owner','engagement-owner','plan-b',2,'window-closed',1,'family','[\"dependent-owner\"]','THB',12550,30000,'2027-01-01','2027-12-31','released','owner','owner','SIM-ACK-1','reviewer','releaser',2,10,600);\
                         INSERT INTO benefit_election_events(id,election_id,version,action,actor_id,reason,status,created_at) VALUES \
                         ('event-released-1','election-released',1,'elect','owner','Employee elected reviewed coverage','elected',10),\
                         ('event-released-2','election-released',2,'release','releaser','Payroll amount and provider receipt reconciled','released',600);\
                         INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,source_election_version,status,released_by,created_at) VALUES \
                         ('payroll-released-1','election-released','company','person-owner','engagement-owner','THB',12550,30000,'2027-01-01','2027-12-31',2,'released','releaser',600);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgBenefitsWorkspaceRead::new(connection);
                    let mut people = People {
                        worker: Some(owner_worker()),
                        names: vec![("person-owner".into(), "Owner Worker".into())],
                    };
                    let mut packages = Packages;
                    let detail = benefits_election_detail(
                        &current,
                        "election-released",
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await?;
                    assert_eq!(detail.election.id, "election-released");
                    assert_eq!(detail.election.plan_name, "Plan B");
                    assert_eq!(detail.election.employee_name, "Owner Worker");
                    assert_eq!(detail.election.plan_current_version, 2);
                    assert_eq!(detail.election.plan_status, "draft");
                    assert_eq!(detail.election.dependent_ids, vec!["dependent-owner"]);
                    assert_eq!(detail.election.provider_reference.as_deref(), Some("SIM-ACK-1"));
                    assert_eq!(detail.election.released_by.as_deref(), Some("releaser"));
                    assert_eq!(
                        detail.history.iter().map(|row| row.action.as_str()).collect::<Vec<_>>(),
                        vec!["elect", "release"]
                    );
                    assert_eq!(detail.history[1].version, 2);
                    let payroll = detail.payroll.as_ref().unwrap();
                    assert_eq!(payroll.source_election_version, 2);
                    assert_eq!(payroll.employee_deduction_minor, 12_550);
                    assert_eq!(payroll.employer_cost_minor, 30_000);
                    assert!(detail.is_owner);
                    assert!(detail.can_review);

                    let error = benefits_election_detail(
                        &current,
                        "election-foreign",
                        &mut reader,
                        &mut people,
                        &mut packages,
                        NOW,
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(business(error).code, "BENEFIT_ELECTION_NOT_FOUND");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn payroll_source_reads_released_inputs_for_the_exact_engagement_and_period() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,released_by,version,created_at,updated_at) VALUES \
                         ('election-payroll-a','company','person-owner','engagement-owner','plan-b',2,'window-closed',1,'employee','[]','THB',12550,30000,'2027-01-01','2027-12-31','released','owner','releaser',2,10,700);\
                         INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,source_election_version,status,released_by,created_at) VALUES \
                         ('payroll-a','election-payroll-a','company','person-owner','engagement-owner','THB',12550,30000,'2027-01-01','2027-12-31',2,'released','releaser',700);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let scope = BenefitsPayrollScope::new(CompanyId::new("company").unwrap());
                    let query = BenefitsPayrollQuery {
                        engagement_id: "engagement-owner".into(),
                        currency: "THB".into(),
                        period_start: "2027-01-01".into(),
                        period_end: "2027-01-31".into(),
                    };
                    let mut reader = PgBenefitsPayrollSource::new(connection);
                    let rows = payroll_benefit_inputs(&mut reader, &scope, &query).await?;
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].id, "payroll-a");
                    assert_eq!(rows[0].plan_name, "Plan B");
                    assert_eq!(rows[0].employee_deduction_minor, 12_550);
                    assert_eq!(rows[0].employer_cost_minor, 30_000);
                    assert_eq!(rows[0].source_election_version, 2);
                    assert_eq!(rows[0].status, "released");

                    let foreign = BenefitsPayrollScope::new(CompanyId::new("foreign").unwrap());
                    let mut reader = PgBenefitsPayrollSource::new(connection);
                    let rows = payroll_benefit_inputs(&mut reader, &foreign, &query).await?;
                    assert!(rows.is_empty());

                    let narrow = BenefitsPayrollQuery {
                        period_start: "2028-01-01".into(),
                        period_end: "2028-01-31".into(),
                        ..query
                    };
                    let mut reader = PgBenefitsPayrollSource::new(connection);
                    let rows = payroll_benefit_inputs(&mut reader, &scope, &narrow).await?;
                    assert!(rows.is_empty());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn payroll_source_requires_the_serializable_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut outside = pool.raw().acquire().await.unwrap();
        let scope = BenefitsPayrollScope::new(CompanyId::new("company").unwrap());
        let query = BenefitsPayrollQuery {
            engagement_id: "engagement-owner".into(),
            currency: "THB".into(),
            period_start: "2027-01-01".into(),
            period_end: "2027-01-31".into(),
        };
        let error = PgBenefitsPayrollSource::new(&mut outside)
            .released_inputs(&scope, &query, 10)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "BENEFITS_PAYROLL_SOURCE_TRANSACTION_REQUIRED"
        ));
        drop(outside);
        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgBenefitsPayrollSource::new(&mut default_isolation)
            .released_inputs(&scope, &query, 10)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "BENEFITS_PAYROLL_SOURCE_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();
    })
    .await;
}
