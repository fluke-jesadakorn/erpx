//! Postgres proof of the native People Inbox visibility delegate.
//!
//! The retained oracle defines no denial codes for these source types:
//! every negative path is invisible (`Ok(None)`), except an invalid coverage
//! week which fails with the retained 422 `ROSTER_WEEK` exactly like the
//! projection. The enforced-mode tests pin `PackageEnforcementMode::Enforced`
//! through the delegate's explicit mode seam instead of mutating process
//! environment (unsafe env mutation is forbidden by workspace lints).
//! Coverage weeks are seeded relative to the database clock, so the gap
//! assertions hold no matter when the suite runs.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, PeopleInboxTargets, PgInboxTargets,
};
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;

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
         ('*','people_manager','People manager',false,1,'active'), \
         ('*','employee','Employee',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','people_manager',1,'[\"roster.read\",\"roster.manage\",\"roster.approve\",\"expense.read\",\"expense.review.read\",\"expense.audit.read\",\"leave.evidence.read\",\"people.manage\",\"payroll.prepare\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','employee',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','people_manager',99,'[\"roster.read\",\"roster.manage\",\"roster.approve\",\"expense.read\",\"expense.review.read\",\"expense.audit.read\",\"leave.evidence.read\",\"people.manage\",\"payroll.prepare\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         UPDATE iam_roles SET published_revision=99 WHERE company_id='*' AND id='people_manager' AND published_revision<99; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','people_manager','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','employee','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','employee','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co','Worker One','TH','maker',0,0), \
         ('person-2','user-2','co','Worker Two','TH','maker',0,0), \
         ('person-3','user-3','co','Worker Three','TH','maker',0,0); \
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('eng-1','person-1','co','br','dep-1','employee','salaried','active','2020-01-01','Worker','THB','maker',0,0), \
         ('eng-2','person-2','co','br','dep-1','employee','salaried','active','2020-01-01','Worker','THB','maker',0,0), \
         ('eng-3','person-3','co','br','dep-1','employee','salaried','active','2020-01-01','Worker','THB','maker',0,0); \
         INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES \
         ('lvl-1','co','L1','Level 1',1,0,0); \
         INSERT INTO organization_positions(id,company_id,title,level_id,active,roster_required,created_by,created_at,updated_at) VALUES \
         ('pos-1','co','Crew','lvl-1',1,1,'maker',0,0); \
         UPDATE hr_engagements SET position_id='pos-1' WHERE id='eng-3'; \
         INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,state,created_by,created_at,updated_at) VALUES \
         ('wk-1','co','br','dep-1',(SELECT date_trunc('week', now() AT TIME ZONE 'Asia/Bangkok')::date),'Asia/Bangkok','published','maker',1,1), \
         ('wk-2','co','br','dep-1',((SELECT date_trunc('week', now() AT TIME ZONE 'Asia/Bangkok')::date)+7),'Asia/Bangkok','draft','maker',1,1); \
         INSERT INTO people_roster_shifts(id,week_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,state) VALUES \
         ('sh-1','wk-1','Morning',(SELECT date_trunc('week', now() AT TIME ZONE 'Asia/Bangkok')::date),'08:00','16:00',1000,2000,5,'active'), \
         ('sh-3','wk-1','Evening',(SELECT date_trunc('week', now() AT TIME ZONE 'Asia/Bangkok')::date),'18:00','22:00',3000,4000,5,'active'); \
         INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id,state) VALUES \
         ('asn-1','sh-1','person-1','eng-1','assigned'); \
         INSERT INTO people_roster_requests(id,company_id,department_id,kind,requester_person_id,target_shift_id,state,reason,version,created_by,created_at,updated_at) VALUES \
         ('req-1','co','dep-1','extra','person-1','sh-1','pending_manager','Cover',1,'maker',1,1), \
         ('req-3','co','dep-1','extra','person-3','sh-3','pending_coworker','Swap',1,'maker',1,1); \
         INSERT INTO employees(id,user_id,company_id,branch_id,display_name,status,created_at,department_id,person_id) VALUES \
         ('emp-1','user-1','co','br','Worker One','active',0,'dep-1','person-1'); \
         INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,created_by,mode,state,timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,version,created_at,updated_at) VALUES \
         ('ovt-1','co','eng-1','person-1','user-1','advance','draft','Asia/Bangkok','2026-11-16 18:00','2026-11-16 20:00',1000,2000,0,120,'Cover',1,1,1); \
         INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,state,created_by,version,created_at,updated_at,person_id,engagement_id) VALUES \
         ('lea-1','L-1','emp-1','co','br','2026-12-01',5,'draft','user-1',1,1,1,'person-1','eng-1'); \
         INSERT INTO approval_instances(id,company_id,record_type,record_id,phase,cycle,requester_user_id,state,version,created_at,updated_at) VALUES \
         ('inst-1','co','leave','lea-1','request',1,'user-1','pending',1,1,1); \
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('step-1','inst-1',0,'user-2','named','approved'); \
         INSERT INTO finance_expenses(id,company_id,employee_id,claimant_user_id,created_by,status,version,workflow_version,branch_id,department_id,created_at,updated_at) VALUES \
         ('exp-1','co','person-1','user-1','user-1','in_review',1,1,NULL,NULL,1,1), \
         ('exp-2','co',NULL,'user-9','user-9','in_review',1,1,'br','dep-1',1,1); \
         INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at) VALUES \
         ('case-1','co','person-1','eng-1','offboarding','scheduled',1,1,'{}','maker',1); \
         INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,status,assignee_id,clearance_type,version,updated_at) VALUES \
         ('t-1','co','case-1','task','Review','pending','user-1','hr',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_people_grants(pool: &DatabasePool, packages: &[&str]) {
    let mut tx = pool.raw().begin().await.unwrap();
    for package in packages {
        let event = format!("evt-{package}");
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
             ('co',$1,1,'active',0,NULL,1,'fixture','test:people-history',0)",
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
            "evidence": "test:people-history",
        })
        .to_string();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES \
             ($1,'co',$2,'fixture','test',NULL,$3::jsonb,0)",
        )
        .bind(event)
        .bind(package)
        .bind(after)
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
            let mut delegate = PeopleInboxTargets::new(branch_id.as_deref(), None);
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

async fn week_start(pool: &DatabasePool, id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT week_start::text FROM people_roster_weeks WHERE id=$1")
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn roster_week_visible_when_published_or_managed() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let week = week_start(&pool, "wk-1").await;
        // Non-manager sees the published week.
        let target = visible(pool.clone(), "user-2", "roster_week", "wk-1")
            .await
            .expect("published week must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some(format!("/people/roster?department=dep-1&week={week}").as_str())
        );
        assert_eq!(target.source, None);
        // Draft weeks stay invisible without the manager permission.
        assert!(
            visible(pool.clone(), "user-2", "roster_week", "wk-2")
                .await
                .is_none(),
            "draft week must stay invisible for non-managers"
        );
        // Managers resolve drafts too.
        let target = visible(pool.clone(), "user-1", "roster_week", "wk-2")
            .await
            .expect("managers resolve drafts");
        let draft_week = week_start(&pool, "wk-2").await;
        assert_eq!(
            target.href.as_deref(),
            Some(format!("/people/roster?department=dep-1&week={draft_week}").as_str())
        );
    })
    .await;
}

#[tokio::test]
async fn roster_request_visible_to_participant_or_manager() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let week = week_start(&pool, "wk-1").await;
        // Non-manager requester resolves their own request.
        let target = visible(pool.clone(), "user-3", "roster_request", "req-3")
            .await
            .expect("requester resolves their own request");
        assert_eq!(
            target.href.as_deref(),
            Some(
                format!("/people/roster?department=dep-1&week={week}&view=requests&request=req-3")
                    .as_str()
            )
        );
        assert_eq!(target.source, None);
        // Managers resolve any request in scope.
        let target = visible(pool.clone(), "user-1", "roster_request", "req-1")
            .await
            .expect("managers resolve requests");
        assert!(target
            .href
            .as_deref()
            .unwrap()
            .contains("&view=requests&request=req-1"));
        // Strangers see nothing.
        assert!(
            visible(pool.clone(), "user-2", "roster_request", "req-1")
                .await
                .is_none(),
            "non-participant non-manager stays invisible"
        );
        assert!(
            visible(pool.clone(), "user-2", "roster_request", "missing")
                .await
                .is_none(),
            "missing request stays invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn roster_coverage_matches_current_week_gaps() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_people_grants(&pool, &["people", "people-workforce"]).await;
        let week = week_start(&pool, "wk-1").await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // person-3 has an unassigned roster-required engagement this week.
        let source_id = format!("dep-1:{week}:person-3");
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "roster_coverage",
            &source_id,
            None,
            enforced,
        )
        .await
        .unwrap()
        .expect("matching gap must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some(format!("/people/roster?department=dep-1&week={week}").as_str())
        );
        assert_eq!(target.source, None);
        // person-1 is assigned this week: no gap, no link.
        let source_id = format!("dep-1:{week}:person-1");
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "roster_coverage",
                &source_id,
                None,
                enforced,
            )
            .await
            .unwrap()
            .is_none(),
            "covered person stays invisible"
        );
        // Without the manager permission nothing resolves.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-2",
                "roster_coverage",
                &format!("dep-1:{week}:person-3"),
                None,
                enforced,
            )
            .await
            .unwrap()
            .is_none(),
            "non-manager stays invisible"
        );
        // Malformed ids stay invisible without an error.
        assert!(
            visible(pool.clone(), "user-1", "roster_coverage", "nope")
                .await
                .is_none()
        );
        assert!(
            visible(pool.clone(), "user-1", "roster_coverage", "dep-1:not-a-date:person-3")
                .await
                .is_none()
        );
        // A well-formed non-Monday fails with the retained week code.
        let denied = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "roster_coverage",
            "dep-1:2026-11-17:person-3",
            None,
            enforced,
        )
        .await;
        assert!(
            matches!(
                denied,
                Err(PgTransactionError::Business(ref error)) if error.code == "ROSTER_WEEK"
            ),
            "non-Monday week must deny with ROSTER_WEEK"
        );
    })
    .await;
}

#[tokio::test]
async fn overtime_visible_for_creator_only() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "overtime", "ovt-1")
            .await
            .expect("creator resolves their overtime");
        // Raw id in the href, no source override for the final-map default.
        assert_eq!(target.href.as_deref(), Some("/overtime/ovt-1"));
        assert_eq!(target.source, None);
        assert!(
            visible(pool.clone(), "user-2", "overtime", "ovt-1")
                .await
                .is_none(),
            "stranger stays invisible"
        );
        assert!(
            visible(pool.clone(), "user-1", "overtime", "missing")
                .await
                .is_none(),
            "missing overtime stays invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn leave_visible_with_numeric_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        for source_type in ["leave", "leave_cancellation"] {
            let target = visible(pool.clone(), "user-1", source_type, "lea-1")
                .await
                .unwrap_or_else(|| panic!("{source_type} must resolve"));
            assert_eq!(target.href.as_deref(), Some("/tasks/people_payroll/lea-1"));
            let source = target.source.expect("leave must carry a source");
            assert_eq!(
                source,
                json!({
                    "kind": "leave",
                    "id": "lea-1",
                    "version": 1,
                    "href": "/tasks/people_payroll/lea-1",
                    "label": "Leave",
                })
            );
            assert!(source["version"].is_number());
        }
        // user-2 is the named approver on the decided step, so the retained
        // participant arm keeps the row visible to them; user-3 has no
        // relation to the request and stays invisible.
        assert!(
            visible(pool.clone(), "user-3", "leave", "lea-1")
                .await
                .is_none(),
            "stranger stays invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn leave_visible_via_approval_participant() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // user-2 approves lea-1 in the workflow: the participant arm resolves.
        let target = visible(pool.clone(), "user-2", "leave", "lea-1")
            .await
            .expect("approver resolves the leave");
        assert_eq!(target.href.as_deref(), Some("/tasks/people_payroll/lea-1"));
    })
    .await;
}

#[tokio::test]
async fn expense_visible_self_and_company_scope() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Claimant resolves their own expense.
        let target = visible(pool.clone(), "user-1", "expense", "exp-1")
            .await
            .expect("claimant resolves their expense");
        assert_eq!(target.href.as_deref(), Some("/expenses/exp-1"));
        assert_eq!(target.source, None);
        // A company-scoped reader resolves an in-review expense.
        let target = visible(pool.clone(), "user-1", "expense", "exp-2")
            .await
            .expect("company scope resolves in-review expenses");
        assert_eq!(target.href.as_deref(), Some("/expenses/exp-2"));
        assert_eq!(target.source, None);
        // No self link, no participation, no company permission.
        assert!(
            visible(pool.clone(), "user-2", "expense", "exp-2")
                .await
                .is_none(),
            "outsider stays invisible"
        );
        // Enforced without a purchase: invisible, not an error.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "expense",
                "exp-1",
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .is_none(),
            "unpurchased expense stays invisible"
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_expense_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_people_grants(&pool, &["people", "people-expenses"]).await;
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "expense",
            "exp-1",
            None,
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("granted expense must resolve in enforced mode");
        assert_eq!(target.href.as_deref(), Some("/expenses/exp-1"));
    })
    .await;
}

#[tokio::test]
async fn retirement_visible_for_assignee() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Assignee resolves their clearance task; href is the task's path.
        let target = visible(pool.clone(), "user-1", "retirement_clearance", "t-1")
            .await
            .expect("assignee resolves their clearance");
        assert_eq!(target.href.as_deref(), Some("/people/clearance/case-1"));
        assert_eq!(target.source, None);
        // No assignment link and no role permission.
        assert!(
            visible(pool.clone(), "user-2", "retirement_clearance", "t-1")
                .await
                .is_none(),
            "outsider stays invisible"
        );
        // Enforced without history: invisible, not an error.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "retirement_clearance",
                "t-1",
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .is_none(),
            "unpurchased clearance stays invisible"
        );
        // Enforced history works the same through the source proof.
        seed_people_grants(&pool, &["people"]).await;
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "retirement_clearance",
            "t-1",
            None,
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("granted assignee resolves in enforced mode");
        assert_eq!(target.href.as_deref(), Some("/people/clearance/case-1"));
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = PeopleInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, PeopleInboxTargets::new(None, None));
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
                            PeopleInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "roster_week", "wk-1", 0)
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
            "SELECT (SELECT count(*) FROM people_roster_weeks WHERE company_id='co'), \
             (SELECT count(*) FROM overtime_requests WHERE company_id='co'), \
             (SELECT count(*) FROM finance_expenses WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 2));
    })
    .await;
}
