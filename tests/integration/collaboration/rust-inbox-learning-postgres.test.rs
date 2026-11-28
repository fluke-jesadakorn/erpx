//! Postgres proof of the native Learning and Benefits Inbox visibility
//! delegate.
//!
//! Each gate mirrors its TypeScript oracle. Learning settles the reader
//! role, the module, the assignment authority and the `learn` history
//! before the scoped assignment read; benefits settles the module, the
//! reader role, the assignment authority and module predicate and the
//! `people.benefits` history before the scoped election read. With
//! enforcement off the readable check authorizes the read directly.
//! Missing rows deny with the exact oracle 403/404 codes and messages. The
//! enforced-mode tests pin `PackageEnforcementMode::Enforced` through the
//! delegate's explicit mode seam instead of mutating process environment
//! (unsafe env mutation is forbidden by workspace lints).
//!
//! History decisions require company package history rights as well as the
//! grant and its event, exactly like the commercial decision port reads
//! them; the `people` grant travels with `people-benefits` because the
//! capability declares the dependency.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, LearningInboxTargets, PgInboxTargets,
};
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES \
         ('co','company','co','Co',NULL,'co',NULL,'active'), \
         ('br','branch','br','Br','co','co',NULL,'active'), \
         ('dep-1','department','dep-1','Dep','br','co','br','active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','learn_reader','Learn reader',false,1,'active'), \
         ('*','learn_nomod','Learn no-module',false,1,'active'), \
         ('*','ben_reader','Benefits reader',false,1,'active'), \
         ('*','ben_reviewer','Benefits reviewer',false,1,'active'), \
         ('*','no_access','No access',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','learn_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','learn_nomod',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','ben_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','ben_reviewer',1,'[\"records.read\",\"benefits.review\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_access',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','learn_reader','[\"learning\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','learn_nomod','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','ben_reader','[\"benefits\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-4','user-4','ben_reviewer','[\"benefits\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-5','user-5','no_access','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co','Learner One','TH','maker',0,0), \
         ('person-3','user-3','co','Owner Three','TH','maker',0,0); \
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('eng-1','person-1','co','br','dep-1','employee','salaried','active','2020-01-01','Learner','THB','maker',0,0), \
         ('eng-3','person-3','co','br','dep-1','employee','salaried','active','2020-01-01','Owner','THB','maker',0,0); \
         INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,created_by,created_at,updated_at) VALUES \
         ('course-1','co','Safety','Stay safe','Internal',60,'maker',1,1); \
         INSERT INTO learning_assignments(id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,created_by,created_at,updated_at) VALUES \
         ('asg-1','co','br','dep-1','course-1',1,'user-1','2026-12-01','maker',1,1); \
         INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,created_by,created_at,updated_at) VALUES \
         ('plan-1','co','Health','Provider','employee','THB',0,0,'2026-01-01','maker',1,1); \
         INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,effective_on,created_by,created_at,updated_at) VALUES \
         ('win-1','co','Open','2026-01-01','2026-12-31','2026-01-01','maker',1,1); \
         INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,created_by,created_at,updated_at) VALUES \
         ('el-1','co','person-1','eng-1','plan-1',1,'win-1',1,'employee','[]','THB',0,0,'2026-01-01','maker',1,1), \
         ('el-2','co','person-3','eng-3','plan-1',1,'win-1',1,'employee','[]','THB',0,0,'2026-01-01','maker',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_grants(pool: &DatabasePool, packages: &[&str]) {
    let mut tx = pool.raw().begin().await.unwrap();
    for package in packages {
        let event = format!("evt-{package}");
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
             ('co',$1,1,'active',0,NULL,1,'fixture','test:learning-history',0)",
        )
        .bind(package)
        .execute(&mut *tx)
        .await
        .unwrap();
        let after = serde_json::json!({
            "companyId": "co",
            "packageId": package,
            "packageVersion": 1,
            "state": "active",
            "startsAt": 0,
            "endsAt": None::<i64>,
            "version": 1,
            "reason": "fixture",
            "evidence": "test:learning-history",
        })
        .to_string();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES \
             ($1,'co',$2,'fixture','test',NULL,$3::jsonb,0)",
        )
        .bind(event.clone())
        .bind(package)
        .bind(after)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES \
             ('co',$1,1,$2,'after',1,0)",
        )
        .bind(package)
        .bind(&event)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn run<T, F>(
    pool: DatabasePool,
    f: F,
) -> Result<T, PgTransactionError<BusinessError>>
where
    F: for<'c> FnOnce(
            &'c mut sqlx::PgConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, BusinessError>> + Send + 'c>,
        > + Send
        + 'static,
    T: Send + 'static,
{
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move { f(connection).await.map_err(PgTransactionError::Business) })
        })
        .await
}

async fn composed_target(
    pool: DatabasePool,
    company_id: &str,
    user_id: &str,
    source_type: &str,
    source_id: &str,
    branch_id: Option<&str>,
    mode: Option<PackageEnforcementMode>,
) -> Result<Option<InboxTarget>, PgTransactionError<BusinessError>> {
    let (company_id, user_id, source_type, source_id, branch_id) = (
        company_id.to_owned(),
        user_id.to_owned(),
        source_type.to_owned(),
        source_id.to_owned(),
        branch_id.map(str::to_owned),
    );
    run(pool, move |connection| {
        Box::pin(async move {
            let mut delegate = LearningInboxTargets::new(branch_id.as_deref(), None);
            if let Some(mode) = mode {
                delegate = delegate.enforcement_mode(mode);
            }
            let mut targets = PgInboxTargets::with_delegate(None, None, delegate);
            targets
                .target(connection, &company_id, &user_id, &source_type, &source_id, 0)
                .await
        })
    })
    .await
}

fn visible(
    pool: DatabasePool,
    user_id: &str,
    source_type: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        composed_target(pool, "co", user_id, source_type, source_id, None, None)
            .await
            .unwrap()
    }
}

fn denied(
    result: &Result<Option<InboxTarget>, PgTransactionError<BusinessError>>,
) -> (&'static str, &'static str) {
    match result {
        Err(PgTransactionError::Business(error)) => (error.code, error.message),
        _ => panic!("expected an oracle denial, got {result:?}"),
    }
}

#[tokio::test]
async fn learning_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "learning_assignment", "asg-1")
            .await
            .expect("scoped assignment must resolve");
        assert_eq!(target.href.as_deref(), Some("/learning/asg-1"));
        // No source override: the composition final map applies the
        // retained default ({kind:"learning_assignment", id, href, label}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn benefit_visible_for_owner_and_reviewer() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The owner resolves their own election.
        let target = visible(pool.clone(), "user-3", "benefit_election", "el-2")
            .await
            .expect("owner resolves their election");
        assert_eq!(target.href.as_deref(), Some("/benefits/el-2"));
        assert_eq!(target.source, None);
        // A benefits reviewer resolves any election in scope.
        let target = visible(pool.clone(), "user-4", "benefit_election", "el-1")
            .await
            .expect("reviewer resolves the election");
        assert_eq!(target.href.as_deref(), Some("/benefits/el-1"));
        // A non-owner without review rights stays invisible to the error:
        // the scoped election read denies with 404.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "benefit_election", "el-1", None, None)
                    .await
            ),
            ("BENEFIT_ELECTION_NOT_FOUND", "Election not found.")
        );
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_or_module_are_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No records.read: the readable check denies first.
        for (user_id, source_type, source_id) in [
            ("user-5", "learning_assignment", "asg-1"),
            ("user-5", "benefit_election", "el-2"),
        ] {
            assert_eq!(
                denied(
                    &composed_target(pool.clone(), "co", user_id, source_type, source_id, None, None)
                        .await
                ),
                (
                    "PERMISSION_REQUIRED",
                    "Your selected role does not grant this permission."
                ),
                "{source_type}"
            );
        }
        // records.read without the module denies with the module code.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "learning_assignment", "asg-1", None, None)
                    .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Learning.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "benefit_election", "el-2", None, None)
                    .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Benefits.")
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_authority_denies_before_the_purchase_gate() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // No current assignment at all: the authority proof denies.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-5",
                    "learning_assignment",
                    "asg-1",
                    None,
                    enforced,
                )
                .await
            ),
            (
                "LEARNING_ACCESS_CHANGED",
                "Choose a current Learning assignment."
            )
        );
        // The benefits module gate runs before the people authority.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-5",
                    "benefit_election",
                    "el-2",
                    None,
                    enforced,
                )
                .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Benefits.")
        );
    })
    .await;
}

#[tokio::test]
async fn missing_rows_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "learning_assignment",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            ("LEARNING_ASSIGNMENT_NOT_FOUND", "Learning assignment not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "benefit_election",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            ("BENEFIT_ELECTION_NOT_FOUND", "Election not found.")
        );
        // Out-of-frame assignments read as missing, exactly like the
        // retained scope.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "learning_assignment",
                    "asg-1",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            ("LEARNING_ASSIGNMENT_NOT_FOUND", "Learning assignment not found.")
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Empty purchase facts in enforced mode: each oracle's exact
        // refusal surfaces for its own capability.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "learning_assignment",
                    "asg-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This Learning work requires its purchased capability or retained history."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "benefit_election",
                    "el-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This Benefits work requires its purchased capability or retained history."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["learn", "people", "people-benefits"]).await;
        // The people grant travels with people-benefits because the
        // capability declares the dependency.
        for (user_id, source_type, source_id, href) in [
            ("user-1", "learning_assignment", "asg-1", "/learning/asg-1"),
            ("user-3", "benefit_election", "el-2", "/benefits/el-2"),
            ("user-4", "benefit_election", "el-1", "/benefits/el-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                user_id,
                source_type,
                source_id,
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_type} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_type}");
            assert_eq!(target.source, None, "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = LearningInboxTargets::new(None, None);
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = delegate
                    .target(connection, "co", "user-1", "future_source", "x", 0)
                    .await
                    .expect_err("unknown source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_SOURCE_UNSUPPORTED");
        let mut targets =
            PgInboxTargets::with_delegate(None, None, LearningInboxTargets::new(None, None));
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = targets
                    .target(connection, "co", "user-1", "support_ticket", "t-1", 0)
                    .await
                    .expect_err("unowned source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_PORT_UNAVAILABLE");
    })
    .await;
}

#[tokio::test]
async fn port_commits_nothing_and_rolls_back_with_the_caller() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<Option<InboxTarget>, _> =
            PgUnitOfWork::new(pool.clone())
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut targets = PgInboxTargets::with_delegate(
                            None,
                            None,
                            LearningInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "learning_assignment", "asg-1", 0)
                            .await?;
                        assert!(resolved.is_some());
                        Err(BusinessError::new(
                            erpx_platform_shared_server::public::FailureKind::Conflict,
                            "SYNTHETIC_ROLLBACK",
                            "Synthetic caller failure.",
                        )
                        .into())
                    })
                })
                .await;
        assert!(matches!(
            rolled_back,
            Err(PgTransactionError::Business(ref error)) if error.code == "SYNTHETIC_ROLLBACK"
        ));
        // The read-only port staged no rows: the seed image is unchanged.
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM learning_assignments WHERE company_id='co'), \
             (SELECT count(*) FROM benefit_elections WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 2));
    })
    .await;
}
