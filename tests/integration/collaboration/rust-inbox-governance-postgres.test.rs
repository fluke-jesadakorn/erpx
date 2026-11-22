//! Postgres proof of the native Governance Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Reviews resolve through the
//! single boolean alert SELECT; organization changes through the scope
//! rule, the fresh-assignment proof and the change existence check; Company
//! acceptance through the holding frame, the manage permission, the Company
//! authority and the acceptance row whose item id equals the reader's
//! company. Invisible rows resolve to `Ok(None)` — the oracles drop
//! rather than throw on this slice — and no row sets a source override
//! (the final map applies the retained defaults).
//!
//! History decisions do not apply here: none of the three oracles carries
//! a purchase gate.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    GovernanceInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
};
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
         ('br','branch','br','Br','co','co',NULL,'active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','gov_reader','Governance reader',false,1,'active'), \
         ('*','gov_plain','Governance plain',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','gov_reader',1,'[\"iam.review\",\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','gov_plain',1,'[\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES \
         ('grant-1','user-1','gov_reader','[]','co',0,9007199254740991,1,'holding',NULL), \
         ('grant-2','user-2','gov_reader','[]','co',0,9007199254740991,1,'holding',NULL), \
         ('grant-3','user-3','gov_plain','[]','co',0,9007199254740991,1,'holding',NULL), \
         ('grant-4','user-4','gov_reader','[]','co',0,9007199254740991,1,'branch','br'); \
         INSERT INTO iam_access_review_campaigns(id,company_id,name,due_at,created_by,reason,created_at,updated_at) VALUES \
         ('camp-1','co','Annual review',1,'maker','Check access',1,1); \
         INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,display_name,expected_node_version,requested_by,reason,created_at,updated_at) VALUES \
         ('chg-1','co','br','rename','branch','Br HQ',1,'user-2','Rename branch',1,1); \
         INSERT INTO company_business_acceptance(company_id,status,version,requested_by,requested_at,reason,updated_at) VALUES \
         ('co','submitted',1,'user-2',1,'Setup review',1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
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
            let delegate = GovernanceInboxTargets::new(branch_id.as_deref(), None);
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
        composed_target(pool, "co", user_id, source_type, source_id, None)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn review_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "iam_access_review", "camp-1")
            .await
            .expect("eligible reviewer resolves the campaign");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/settings?section=reviews#camp-1")
        );
        // No source override: the composition final map applies the
        // retained default ({kind:"iam_access_review", id, href}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn review_invisible_resolves_to_none() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Role without iam.review: the boolean alert is false.
        assert!(
            composed_target(pool.clone(), "co", "user-3", "iam_access_review", "camp-1", None)
                .await
                .unwrap().is_none(),
                "expected invisible to resolve to None"
        );
        // Branch-scoped assignment: the holding-scope rule drops the row.
        assert!(
            composed_target(pool.clone(), "co", "user-4", "iam_access_review", "camp-1", None)
                .await
                .unwrap().is_none(),
                "expected invisible to resolve to None"
        );
        // Missing campaign: no joined row.
        assert!(
            composed_target(pool.clone(), "co", "user-1", "iam_access_review", "missing", None)
                .await
                .unwrap().is_none(),
                "expected invisible to resolve to None"
        );
    })
    .await;
}

#[tokio::test]
async fn organization_change_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "organization_change", "chg-1")
            .await
            .expect("scoped manager resolves the change");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/organization?view=legacy&company=co&tab=changes&changeId=chg-1#organization-change-chg-1")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn organization_change_invisible_resolves_to_none() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Branch frame: the holding-frame rule drops the row.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "organization_change",
                "chg-1",
                Some("br"),
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
        // Branch-scoped assignment: the holding-assignment rule drops it.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-4",
                "organization_change",
                "chg-1",
                None,
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
        // Missing change: the existence check drops it.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "organization_change",
                "missing",
                None,
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
    })
    .await;
}

#[tokio::test]
async fn acceptance_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The work item id is the company itself.
        let target = visible(pool.clone(), "user-1", "company_business_acceptance", "co")
            .await
            .expect("authorized manager resolves the acceptance");
        assert_eq!(
            target.href.as_deref(),
            Some("/settings/business-readiness#company-setup-acceptance")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn acceptance_invisible_resolves_to_none() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // An unrelated notified id never matches the company item.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "company_business_acceptance",
                "other",
                None,
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
        // Branch frame: the holding-frame rule drops the row.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "company_business_acceptance",
                "co",
                Some("br"),
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
        // No assignment at all: the authority proof drops it.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-9",
                "company_business_acceptance",
                "co",
                None,
            )
            .await
            .unwrap().is_none(),
            "expected invisible to resolve to None"
        );
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = GovernanceInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, GovernanceInboxTargets::new(None, None));
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
                            GovernanceInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "iam_access_review", "camp-1", 0)
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
            "SELECT (SELECT count(*) FROM iam_access_review_campaigns WHERE company_id='co'), \
             (SELECT count(*) FROM organization_node_changes WHERE company_id='co'), \
             (SELECT count(*) FROM company_business_acceptance WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}
