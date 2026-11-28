//! Postgres proof of the native Support Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle: the history purchase decision,
//! the database-current `support.read` (or `records.read` reporter fallback
//! for tickets) authority, and the scoped identity read. Missing rows deny
//! with the exact oracle 403/404 codes and messages; composition drops those
//! exactly like the retained per-row catch. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` through the delegate's explicit mode
//! seam instead of mutating process environment (unsafe env mutation is
//! forbidden by workspace lints).
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, SupportInboxTargets,
};
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;

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
         ('*','no_support','No support',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','no_support',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','support_manager','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','support_agent','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','no_support','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co','Manager One','TH','maker',0,0); \
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('eng-1','person-1','co','br','dep-1','employee','salaried','active','2020-01-01','Manager','THB','maker',0,0); \
         INSERT INTO support_tickets(id,company_id,branch_id,department_id,source_kind,title,issue,owner_id,due_date,status,version,created_by,source_record_json,created_at,updated_at) VALUES \
         ('t-1','co','br','dep-1','general','Help','Broken','user-2','2026-12-01','open',1,'user-2',NULL,1,1), \
         ('t-2','co','br','dep-1','pos_sale','POS help','Broken','user-1','2026-12-01','open',1,'user-1','{\"kind\":\"pos_sale\",\"id\":\"pos-1\",\"version\":1}',1,1); \
         INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at) VALUES \
         ('al-1','t-1',1,1000,1000); \
         INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,version,created_by,created_at,updated_at) VALUES \
         ('pol-1','co','Response','{}','draft',1,'maker',1,1); \
         INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES \
         ('inc-1','co','br','dep-1','Outage','Down','high','user-2','2026-12-01','open',1,'user-1',1,1); \
         INSERT INTO support_sessions(id,company_id,branch_id,department_id,ticket_id,ticket_version,requested_for,reason,expires_at,status,created_by,created_at,updated_at) VALUES \
         ('ses-1','co','br','dep-1','t-1',1,'user-2','Help',9999999999999,'pending','user-1',1,1), \
         ('ses-2','co','br','dep-1','t-1',1,'user-3','Help',9999999999999,'pending','user-2',1,1), \
         ('ses-3','co','br','dep-1','t-1',1,'user-1','Help',9999999999999,'pending','user-1',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_support_grant(pool: &DatabasePool) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
         ('co','support',1,'active',0,NULL,1,'fixture','test:support-history',0)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES ('evt-1','co','support','fixture','test',NULL,'{"companyId":"co","packageId":"support","packageVersion":1,"state":"active","startsAt":0,"endsAt":null,"version":1,"reason":"fixture","evidence":"test:support-history"}',0)"#)
    .execute(&mut *tx)
    .await
    .unwrap();
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
            let mut delegate = SupportInboxTargets::new(branch_id.as_deref(), None);
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
async fn alert_visible_with_static_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "support_sla_alert", "al-1")
            .await
            .expect("scoped alert must resolve");
        assert_eq!(target.href.as_deref(), Some("/support/sla"));
        // No source override: the composition final map applies the retained
        // alert default ({kind:"support_sla_alert", id, href, label}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn policy_visible_company_wide_with_static_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Policies ignore the Work frame, exactly like the oracle's
        // company-scoped read: a branch frame still resolves.
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "support_policy",
            "pol-1",
            Some("br"),
            None,
        )
        .await
        .unwrap()
        .expect("company policy must resolve under any frame");
        assert_eq!(target.href.as_deref(), Some("/support/sla"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn ticket_visible_for_reader_and_reporter() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // support.read on the holding assignment resolves the ticket.
        let target = visible(pool.clone(), "user-1", "support_ticket", "t-1")
            .await
            .expect("reader resolves their ticket");
        assert_eq!(target.href.as_deref(), Some("/support/t-1"));
        assert_eq!(target.source, None);
        // The reporter without support.read resolves their own non-order
        // report through the retained records.read arm.
        let target = visible(pool.clone(), "user-2", "support_ticket", "t-1")
            .await
            .expect("reporter resolves their own report");
        assert_eq!(target.href.as_deref(), Some("/support/t-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn ticket_denials_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Missing rows deny with 404 when the reader holds support.read.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "support_ticket", "missing", None, None)
                    .await
            ),
            ("SUPPORT_TICKET_NOT_FOUND", "Ticket not found in this scope.")
        );
        // No support.read and no records.read: the retained authorize denies
        // with 403 before the scoped read.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "support_ticket", "t-1", None, None)
                    .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // A linked source record fails closed with the retained source
        // message instead of inventing the owning workspace proof.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "support_ticket", "t-2", None, None)
                    .await
            ),
            (
                "SUPPORT_TICKET_NOT_FOUND",
                "Ticket source is unavailable in this scope."
            )
        );
        // The ticket scope is Work-framed: an out-of-frame ticket reads as
        // missing, exactly like the retained ticket scope.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_ticket",
                    "t-1",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            ("SUPPORT_TICKET_NOT_FOUND", "Ticket not found in this scope.")
        );
    })
    .await;
}

#[tokio::test]
async fn alert_policy_and_incident_denials_keep_their_oracle_codes() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Missing rows deny with their retained 404 codes.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_sla_alert",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "SUPPORT_ALERT_NOT_FOUND",
                "Target escalation is not available in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_policy",
                    "missing",
                    None,
                    None
                )
                .await
            ),
            ("SUPPORT_POLICY_NOT_FOUND", "Policy not found in this Company.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_incident",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "SUPPORT_INCIDENT_NOT_FOUND",
                "Incident is not available in this scope."
            )
        );
        // Readers without support.read deny with 403 before the scoped read.
        for source_type in [
            "support_sla_alert",
            "support_policy",
            "support_incident",
            "support_session",
        ] {
            let source_id = match source_type {
                "support_sla_alert" => "al-1",
                "support_policy" => "pol-1",
                "support_incident" => "inc-1",
                _ => "ses-1",
            };
            assert_eq!(
                denied(
                    &composed_target(pool.clone(), "co", "user-3", source_type, source_id, None, None)
                        .await
                ),
                (
                    "PERMISSION_REQUIRED",
                    "Your selected role does not grant this permission."
                ),
                "{source_type}"
            );
        }
        // The incident scope is Work-framed: an out-of-frame incident reads
        // as missing, exactly like the retained incident scope.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_incident",
                    "inc-1",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            (
                "SUPPORT_INCIDENT_NOT_FOUND",
                "Incident is not available in this scope."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn incident_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "support_incident", "inc-1")
            .await
            .expect("scoped incident must resolve");
        // The raw id rides the path, exactly like the retained projection.
        assert_eq!(target.href.as_deref(), Some("/support/incidents/inc-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn session_visible_for_creator_requester_and_reviewer() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The creator resolves their session.
        let target = visible(pool.clone(), "user-1", "support_session", "ses-1")
            .await
            .expect("creator resolves their session");
        assert_eq!(target.href.as_deref(), Some("/support/sessions/ses-1"));
        assert_eq!(target.source, None);
        // The requester resolves the session addressed to them.
        let target = visible(pool.clone(), "user-2", "support_session", "ses-1")
            .await
            .expect("requester resolves their session");
        assert_eq!(target.href.as_deref(), Some("/support/sessions/ses-1"));
        // An eligible manager who neither created nor was requested resolves
        // through the retained review predicate.
        let target = visible(pool.clone(), "user-1", "support_session", "ses-2")
            .await
            .expect("reviewer resolves the session");
        assert_eq!(target.href.as_deref(), Some("/support/sessions/ses-2"));
        // A reader who is neither creator, requester nor reviewer denies
        // with the retained 404.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "support_session",
                    "ses-3",
                    None,
                    None
                )
                .await
            ),
            (
                "SUPPORT_SESSION_NOT_FOUND",
                "Support session not found in this scope."
            )
        );
        // Missing sessions deny with the same code.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_session",
                    "missing",
                    None,
                    None
                )
                .await
            ),
            (
                "SUPPORT_SESSION_NOT_FOUND",
                "Support session not found in this scope."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Empty purchase facts in enforced mode: the capability decision
        // refuses with its exact code and message, for every capability
        // behind this delegate.
        for (source_type, source_id) in [
            ("support_sla_alert", "al-1"),
            ("support_policy", "pol-1"),
            ("support_ticket", "t-1"),
            ("support_incident", "inc-1"),
            ("support_session", "ses-1"),
        ] {
            assert_eq!(
                denied(
                    &composed_target(
                        pool.clone(),
                        "co",
                        "user-1",
                        source_type,
                        source_id,
                        None,
                        Some(PackageEnforcementMode::Enforced),
                    )
                    .await
                ),
                ("PACKAGE_REQUIRED", "This package does not permit the requested work."),
                "{source_type}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_support_grant(&pool).await;
        // One `support` grant covers every capability behind this delegate.
        for (source_type, source_id, href) in [
            ("support_sla_alert", "al-1", "/support/sla"),
            ("support_policy", "pol-1", "/support/sla"),
            ("support_ticket", "t-1", "/support/t-1"),
            ("support_incident", "inc-1", "/support/incidents/inc-1"),
            ("support_session", "ses-1", "/support/sessions/ses-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                "user-1",
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
        let mut delegate = SupportInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, SupportInboxTargets::new(None, None));
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = targets
                    .target(connection, "co", "user-1", "crm_lead", "lead-1", 0)
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
                            SupportInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "support_ticket", "t-1", 0)
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
            "SELECT (SELECT count(*) FROM support_tickets WHERE company_id='co'), \
             (SELECT count(*) FROM support_incidents WHERE company_id='co'), \
             (SELECT count(*) FROM support_sessions WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 3));
    })
    .await;
}
