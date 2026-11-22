//! Postgres proof of the native Operations Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Exceptions settle the reader
//! role and the operations module before the scoped candidate read (the
//! external native work source has no caller-transaction equivalent, so
//! field-service rows fail closed with the fallback 404). Tasks settle
//! the project purchase for project tasks and the social source, the work
//! authority and the capability history for ordinary tasks before the
//! scoped participant read; a linked source post must itself resolve.
//! Posts resolve through the published, open-audience read with the work
//! purchase scope. Missing rows deny with the exact oracle 403/404 codes
//! and messages. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` (apps) and the projects mode through
//! the delegate's explicit mode seams instead of mutating process
//! environment (unsafe env mutation is forbidden by workspace lints).
//!
//! History decisions require company package history rights as well as the
//! grant and its event, exactly like the commercial decision port reads
//! them.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxTarget, InboxTargetPort, OperationsInboxTargets, PgInboxTargets,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
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
         ('*','ops_reader','Ops reader',false,1,'active'), \
         ('*','task_reader','Task reader',false,1,'active'), \
         ('*','no_ops','No ops',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','ops_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','task_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_ops',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','ops_reader','[\"operations\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','task_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','ops_reader','[\"operations\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-4','user-4','no_ops','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,created_by,created_at,updated_at) VALUES \
         ('pol-1','co','ops','Default',1,'[]',60,'maker',1,1); \
         INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,status,due_at,created_by,created_at,updated_at) VALUES \
         ('exc-1','co','br',NULL,'ops','staff_task','t-1',1,'{}','pol-1','{}','Jam','Stuck','normal','user-1','open',1,'maker',1,1), \
         ('exc-field','co','br',NULL,'ops','field_service_order','job-1',1,'{}','pol-1','{}','Field jam','Stuck onsite','normal','user-1','open',1,'maker',1,1); \
         INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES \
         ('proj-1','co','Build','user-9',1,1); \
         INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES \
         ('proj-1','user-1','maker',1); \
         INSERT INTO staff_tasks(id,company_id,branch_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES \
         ('t-1','co','br','Task','user-1','user-2',1,1,1); \
         INSERT INTO staff_tasks(id,company_id,project_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES \
         ('t-proj','co','proj-1','Project task','user-9','user-9',1,1,1); \
         INSERT INTO staff_tasks(id,company_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES \
         ('t-post','co','Post task','user-1','user-2',1,1,1); \
         INSERT INTO social_posts(id,company_id,author_id,author_name,body,state,version,created_at,updated_at) VALUES \
         ('post-1','co','user-1','Worker','Team update','published',1,1,1), \
         ('post-2','co','user-2','Helper','Hello','published',1,1,1); \
         INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,created_by,created_at) VALUES \
         ('post-1','t-post',1,'hash','maker',1)",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:operations-history',0)",
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
            "evidence": "test:operations-history",
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
    projects_mode: Option<PackageEnforcementMode>,
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
            let mut delegate = OperationsInboxTargets::new(branch_id.as_deref(), None);
            if let Some(mode) = mode {
                delegate = delegate.enforcement_mode(mode);
            }
            if let Some(projects_mode) = projects_mode {
                delegate = delegate.projects_enforcement_mode(projects_mode);
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
        composed_target(pool, "co", user_id, source_type, source_id, None, None, None)
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
async fn exception_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "operational_exception", "exc-1")
            .await
            .expect("scoped exception must resolve");
        assert_eq!(target.href.as_deref(), Some("/operations/exc-1"));
        // No source override: the composition final map applies the
        // retained default ({kind:"operational_exception", id, href}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn task_visible_for_participants_and_members() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The creator resolves their task.
        let target = visible(pool.clone(), "user-1", "task", "t-1")
            .await
            .expect("creator resolves their task");
        assert_eq!(target.href.as_deref(), Some("/staff-tasks/t-1"));
        assert_eq!(target.source, None);
        // The assignee resolves the same task.
        let target = visible(pool.clone(), "user-2", "task", "t-1")
            .await
            .expect("assignee resolves their task");
        assert_eq!(target.href.as_deref(), Some("/staff-tasks/t-1"));
        // A project member resolves the project task.
        let target = visible(pool.clone(), "user-1", "task", "t-proj")
            .await
            .expect("member resolves the project task");
        assert_eq!(target.href.as_deref(), Some("/staff-tasks/t-proj"));
        // A stranger to every arm stays invisible.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "task", "t-1", None, None, None)
                    .await
            ),
            ("TASK_NOT_FOUND", "Task not found.")
        );
    })
    .await;
}

#[tokio::test]
async fn task_with_source_post_resolves_the_post_arm() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "task", "t-post")
            .await
            .expect("task with a visible post must resolve");
        assert_eq!(target.href.as_deref(), Some("/staff-tasks/t-post"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn post_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-4", "post", "post-2")
            .await
            .expect("published post resolves without authority");
        assert_eq!(target.href.as_deref(), Some("/posts/post-2"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_or_module_are_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No records.read: the readable check denies first.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-4",
                    "operational_exception",
                    "exc-1",
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // records.read without the operations module denies the exception.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "operational_exception",
                    "exc-1",
                    None,
                    None,
                    None,
                )
                .await
            ),
            ("MODULE_FORBIDDEN", "Operations is unavailable to the selected role.")
        );
        // Field-service rows fail closed without the native work source.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "operational_exception",
                    "exc-field",
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "OPERATIONS_EXCEPTION_NOT_FOUND",
                "Exception or source is unavailable in this scope."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_authority_denies_before_the_purchase_gate() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // Project tasks settle assignment currency first.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-4",
                    "task",
                    "t-proj",
                    None,
                    enforced,
                    enforced,
                )
                .await
            ),
            (
                "PROJECT_ACCESS_CHANGED",
                "Choose a current Project assignment."
            )
        );
        // Ordinary tasks settle the work assignment next: user-9 holds no
        // assignment row, so the oracle's current() denies before the
        // capability history (user-4 owns grant-4 and would instead reach
        // PACKAGE_REQUIRED, pinned by the unpurchased test below).
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-9",
                    "task",
                    "t-1",
                    None,
                    enforced,
                    enforced,
                )
                .await
            ),
            (
                "WORK_TASK_AUTHORITY_CHANGED",
                "Choose the current task account and Company role."
            )
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
                    "operational_exception",
                    "missing",
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "OPERATIONS_EXCEPTION_NOT_FOUND",
                "Exception or source is unavailable in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "task", "missing", None, None, None)
                    .await
            ),
            ("TASK_NOT_FOUND", "Task not found.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "post", "missing", None, None, None)
                    .await
            ),
            ("POST_NOT_FOUND", "This post is unavailable.")
        );
        // Out-of-frame rows read as missing, exactly like the retained
        // scopes.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "operational_exception",
                    "exc-1",
                    Some("elsewhere"),
                    None,
                    None,
                )
                .await
            ),
            (
                "OPERATIONS_EXCEPTION_NOT_FOUND",
                "Exception or source is unavailable in this scope."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // Empty purchase facts in enforced mode: each oracle's exact
        // refusal surfaces for its own capability.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "task", "t-1", None, enforced, enforced)
                    .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This ordinary Work task requires its purchased capability or retained history."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "post",
                    "post-2",
                    None,
                    enforced,
                    None,
                )
                .await
            ),
            ("POST_NOT_FOUND", "This post is unavailable.")
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["work", "work-social", "projects"]).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        for (user_id, source_type, source_id, href) in [
            ("user-1", "task", "t-1", "/staff-tasks/t-1"),
            ("user-1", "task", "t-proj", "/staff-tasks/t-proj"),
            ("user-1", "task", "t-post", "/staff-tasks/t-post"),
            ("user-1", "post", "post-1", "/posts/post-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                user_id,
                source_type,
                source_id,
                None,
                enforced,
                enforced,
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_type} {source_id} must resolve in enforced mode"));
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
        let mut delegate = OperationsInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, OperationsInboxTargets::new(None, None));
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
                            OperationsInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "task", "t-1", 0)
                            .await?;
                        assert!(resolved.is_some());
                        Err(BusinessError::new(
                            erpx_server_shared::public::FailureKind::Conflict,
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
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM operations_exceptions WHERE company_id='co'), \
             (SELECT count(*) FROM staff_tasks WHERE company_id='co'), \
             (SELECT count(*) FROM social_posts WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 3, 2));
    })
    .await;
}
