//! Postgres proof of the native Records Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Leave settles the workforce
//! history and the People authority before the scoped leave read (both
//! `leave` and `leave_cancellation` map to source kind `"leave"`).
//! Expenses settle the expenses history and the Expense authority before
//! the scoped expense read with the audit/participant proof. Requests have
//! no purchase gate and resolve through the workspace + participation
//! gates. Missing rows deny with the exact oracle 404 codes and messages.
//! The enforced-mode tests pin `PackageEnforcementMode::Enforced` through
//! the delegate's explicit mode seam instead of mutating process
//! environment (unsafe env mutation is forbidden by workspace lints).
//!
//! History decisions require company package history rights as well as the
//! grant and its event, exactly like the commercial decision port reads
//! them.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, RecordsInboxTargets,
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
         ('br','branch','br','Br','co','co',NULL,'active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','records_reader','Records reader',false,1,'active'), \
         ('*','no_read','No read',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','records_reader',1,'[\"leave.evidence.read\",\"expense.read\",\"expense.review.read\",\"expense.audit.read\",\"workflow.people_manager\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_read',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','records_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','records_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','records_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-4','user-4','no_read','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO hr_people(id,company_id,user_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES \
         ('person-1','co','user-1','Worker One','TH','active',1,'maker',1,1), \
         ('person-2','co','user-2','Worker Two','TH','active',1,'maker',1,1); \
         INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,created_at,department_id,person_id) VALUES \
         ('emp-1','user-1','co','br','Worker One','active',1,'dep-1','person-1'), \
         ('emp-2','user-2','co','br','Worker Two','active',1,'dep-1','person-2'); \
         INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,state,version,created_by,decided_by,created_at,updated_at) VALUES \
         ('leave-1','L-1','emp-1','co','br','2026-11-01',2,'approved',3,'user-1','user-2',1,1), \
         ('leave-2','L-2','emp-2','co','br','2026-11-02',1,'pending_approval',1,'user-2',NULL,1,1); \
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('eng-exp-1','person-1','co','br','dep-1','employee','salaried','active','2020-01-01','Worker','THB','maker',0,0), \
         ('eng-exp-2','person-2','co','br','dep-1','employee','salaried','active','2020-01-01','Worker','THB','maker',0,0); \
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) VALUES \
         ('chart-exp','co','Expense chart','THB','active','maker',0,0); \
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) VALUES \
         ('coa-exp','chart-exp','co','6000','Travel','expense','THB','active',0,0); \
         INSERT INTO finance_expenses(id,company_id,employee_id,claimant_user_id,claimant_name,status,version,created_by,created_at,updated_at,branch_id,department_id,workflow_version,engagement_id,expense_date,merchant,currency,amount_minor,category_account_id,business_purpose,receipt_reference) VALUES \
         ('exp-1','co','person-1','user-1','Worker One','in_review',2,'user-1',1,1,'br',NULL,2,'eng-exp-1','2026-11-01','Airline','THB',50000,'coa-exp','Client visit','RC-1'), \
         ('exp-2','co','person-2','user-2','Worker Two','in_review',1,'user-2',1,1,'br',NULL,2,'eng-exp-2','2026-11-02','Hotel','THB',30000,'coa-exp','Client stay','RC-2'); \
         INSERT INTO approval_instances(id,company_id,record_type,record_id,phase,cycle,requester_user_id,state,current_step,version,created_at,updated_at) VALUES \
         ('ai-exp-1','co','expense','exp-1','request',1,'user-1','pending',0,1,1,1); \
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('as-exp-1','ai-exp-1',0,'user-2','named','pending'); \
         INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) VALUES \
         ('ad-exp-1','ai-exp-1','as-exp-1','approve','user-2','ok',2,'cmd-1',1); \
         INSERT INTO service_teams(id,company_id,name,active,version,created_by,created_at,updated_at) VALUES \
         ('team-1','co','Crew',1,1,'maker',1,1); \
         INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES \
         ('co','team-1','user-1','member'); \
         INSERT INTO employee_requests(id,company_id,branch_id,kind,category,title,description,location,items_json,currency,requester_user_id,created_by,state,team_id,approval_json,version,evidence_version,created_at,updated_at) VALUES \
         ('req-1','co','br','purchase','it','Laptop','Need laptop','HQ','[]','USD','user-1','user-1','queued','team-1','{\"currentStep\":0,\"pool\":[],\"decisions\":[],\"route\":{\"steps\":[]}}',4,1,1,1), \
         ('req-2','co','br','service','it','Fix printer','Printer jam','HQ','[]','','user-2','user-2','queued',NULL,'{\"currentStep\":0,\"pool\":[\"user-2\"],\"decisions\":[],\"route\":{\"steps\":[{\"requiredRole\":\"people_manager\"}]}}',1,1,1,1)",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:records-history',0)",
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
            "evidence": "test:records-history",
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
            let mut delegate = RecordsInboxTargets::new(branch_id.as_deref(), None);
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
async fn leave_visible_with_exact_href_and_versioned_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        for source_type in ["leave", "leave_cancellation"] {
            let target = visible(pool.clone(), "user-1", source_type, "leave-1")
                .await
                .expect("creator resolves their leave");
            assert_eq!(
                target.href.as_deref(),
                Some("/tasks/people_payroll/leave-1"),
                "{source_type}"
            );
            let source = target.source.expect("versioned source");
            assert_eq!(source["kind"], serde_json::json!("leave"), "{source_type}");
            assert_eq!(source["id"], serde_json::json!("leave-1"), "{source_type}");
            assert_eq!(source["version"], serde_json::json!(3), "{source_type}");
            assert_eq!(
                source["href"],
                serde_json::json!("/tasks/people_payroll/leave-1"),
                "{source_type}"
            );
            assert_eq!(source["label"], serde_json::json!("Leave"), "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn expense_visible_with_exact_href_and_versioned_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The claimant resolves their own expense (direct arm).
        let target = visible(pool.clone(), "user-1", "expense", "exp-1")
            .await
            .expect("claimant resolves their expense");
        assert_eq!(target.href.as_deref(), Some("/expenses/exp-1"));
        let source = target.source.expect("versioned source");
        assert_eq!(source["kind"], serde_json::json!("expense"));
        assert_eq!(source["id"], serde_json::json!("exp-1"));
        assert_eq!(source["version"], serde_json::json!(2));
        assert_eq!(source["href"], serde_json::json!("/expenses/exp-1"));
        assert_eq!(source["label"], serde_json::json!("Expense"));
        // The recorded approver resolves via the decision arm.
        let target = visible(pool.clone(), "user-2", "expense", "exp-1")
            .await
            .expect("deciding approver resolves the expense");
        assert_eq!(target.href.as_deref(), Some("/expenses/exp-1"));
    })
    .await;
}

#[tokio::test]
async fn request_visible_with_kind_source_and_exact_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "request", "req-1")
            .await
            .expect("requester resolves their request");
        assert_eq!(target.href.as_deref(), Some("/requests/req-1"));
        let source = target.source.expect("versioned source");
        assert_eq!(source["kind"], serde_json::json!("purchase_request"));
        assert_eq!(source["id"], serde_json::json!("req-1"));
        assert_eq!(source["version"], serde_json::json!(4));
        assert_eq!(source["href"], serde_json::json!("/requests/req-1"));
        assert_eq!(source["label"], serde_json::json!("Purchase request"));
    })
    .await;
}

#[tokio::test]
async fn strangers_and_missing_rows_keep_oracle_codes() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // A stranger to every leave arm reads as missing (user-4 holds no
        // permissions; records_reader roles carry leave.evidence.read and
        // would resolve through the oracle's evidence arm).
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "leave", "leave-2", None, None)
                    .await
            ),
            ("NOT_FOUND", "Leave request not found.")
        );
        // A stranger to the expense reads as missing.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "expense", "exp-2", None, None)
                    .await
            ),
            ("EXPENSE_NOT_FOUND", "Expense claim not found in your workspace.")
        );
        // A stranger to the request is unavailable, not missing.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "request", "req-2", None, None)
                    .await
            ),
            ("REQUEST_NOT_FOUND", "This request is not available to you.")
        );
        // Out-of-frame reads as outside the workspace.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "request",
                    "req-1",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            (
                "REQUEST_NOT_FOUND",
                "This request is outside your current workspace."
            )
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "leave", "missing", None, None)
                    .await
            ),
            ("NOT_FOUND", "Leave request not found.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "expense", "missing", None, None)
                    .await
            ),
            ("EXPENSE_NOT_FOUND", "Expense claim not found in your workspace.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "request", "missing", None, None)
                    .await
            ),
            ("REQUEST_NOT_FOUND", "Request not found.")
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_authority_denies_before_the_scoped_read() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["people", "people-workforce", "people-expenses"]).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // user-9 holds no assignment: the People authority denies first.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-9",
                    "leave",
                    "leave-1",
                    None,
                    enforced,
                )
                .await
            ),
            (
                "PEOPLE_ACCESS_CHANGED",
                "Choose a current authorized People work role."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-9",
                    "expense",
                    "exp-1",
                    None,
                    enforced,
                )
                .await
            ),
            (
                "EXPENSE_ACCESS_CHANGED",
                "The original Expense assignment or session is no longer current."
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
        // Empty purchase facts in enforced mode: the commercial decision
        // code surfaces for its own capability.
        let (code, _) = denied(
            &composed_target(pool.clone(), "co", "user-1", "leave", "leave-1", None, enforced)
                .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        let (code, _) = denied(
            &composed_target(pool.clone(), "co", "user-1", "expense", "exp-1", None, enforced)
                .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["people", "people-workforce", "people-expenses"]).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        for (user_id, source_type, source_id, href) in [
            ("user-1", "leave", "leave-1", "/tasks/people_payroll/leave-1"),
            (
                "user-1",
                "leave_cancellation",
                "leave-1",
                "/tasks/people_payroll/leave-1",
            ),
            ("user-1", "expense", "exp-1", "/expenses/exp-1"),
            ("user-1", "request", "req-1", "/requests/req-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                user_id,
                source_type,
                source_id,
                None,
                enforced,
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_type} {source_id} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_type}");
            assert!(target.source.is_some(), "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = RecordsInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, RecordsInboxTargets::new(None, None));
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
                            RecordsInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "leave", "leave-1", 0)
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
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM leave_requests WHERE company_id='co'), \
             (SELECT count(*) FROM finance_expenses WHERE company_id='co'), \
             (SELECT count(*) FROM employee_requests WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 2, 2));
    })
    .await;
}
