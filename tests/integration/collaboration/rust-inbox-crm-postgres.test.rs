//! Postgres proof of the native CRM Inbox visibility delegate.
//!
//! The purchase-denied path pins `PackageEnforcementMode::Enforced` through
//! the delegate's explicit mode seam instead of mutating process environment
//! (unsafe env mutation is forbidden by workspace lints). Ambient mode stays
//! disabled, so the visibility tests exercise the open purchase path exactly
//! like development.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    CrmInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
};
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('co','company','co','Co',NULL,'co',NULL,'active',1), \
         ('br','branch','br','Br','co','co',NULL,'active',1); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','crm_reader','CRM reader',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','crm_reader',1,'[\"commerce.read\",\"commerce.sales\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','crm_reader','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO crm_accounts(id,company_id,kind,name,finance_party_id,review_evidence,reviewed_by,created_by,version,created_at,updated_at) VALUES \
         ('acct-1','co','business','Acme',NULL,NULL,NULL,'maker',1,1,1), \
         ('acct-opp','co','business','Opp Co','cust-1','evidence','reviewer','maker',1,1,1), \
         ('acct-opp-bare','co','business','Bare Co','cust-1',NULL,NULL,'maker',1,1,1); \
         INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,version,created_at,updated_at) VALUES \
         ('ct-1','co','acct-1','Contact','Detail','maker',1,1,1), \
         ('ct-opp','co','acct-opp','Opp Contact','Detail','maker',1,1,1), \
         ('ct-opp-bare','co','acct-opp-bare','Bare Contact','Detail','maker',1,1,1); \
         INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at,account_id,contact_id) VALUES \
         ('lead-1','co','br',NULL,'Lead','Contact','Detail','Staff','Brief','owner-1','2026-12-01','open',1,'maker',1,1,'acct-1','ct-1'), \
         ('lead-bare','co','br',NULL,'Bare','Contact','Detail','Staff','Brief','owner-1','2026-12-01','open',1,'maker',1,1,NULL,NULL), \
         ('lead-opp','co','br',NULL,'Opp','Contact','Detail','Staff','Brief','owner-1','2026-12-01','open',1,'maker',1,1,'acct-opp','ct-opp'), \
         ('lead-opp-bare','co','br',NULL,'Opp Bare','Contact','Detail','Staff','Brief','owner-1','2026-12-01','open',1,'maker',1,1,'acct-opp-bare','ct-opp-bare'); \
         INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,created_by,created_at,updated_at) VALUES \
         ('act-1','co','lead-1','call','Call','Discuss','owner-1','2026-11-20','open','maker',1,1); \
         INSERT INTO crm_opportunities(id,lead_id,customer_id,stage,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES \
         ('opp-1','lead-opp','cust-1','discovery',10000,'THB','2026-12-31','maker',1,1), \
         ('opp-bare','lead-opp-bare','cust-1','discovery',10000,'THB','2026-12-31','maker',1,1)",
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
            let mut delegate = CrmInboxTargets::new(branch_id.as_deref(), None);
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
    company_id: &str,
    user_id: &str,
    source_type: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        composed_target(pool, company_id, user_id, source_type, source_id, Some("br"), None)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn lead_visible_with_exact_href_and_source_without_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "co", "user-1", "crm_lead", "lead-1")
            .await
            .expect("scoped lead must resolve");
        assert_eq!(target.href.as_deref(), Some("/crm/lead-1"));
        let source = target.source.expect("lead must carry a source");
        assert_eq!(source, json!({
            "kind": "crm_lead",
            "id": "lead-1",
            "href": "/crm/lead-1",
            "label": "Crm lead",
        }));
        // The retained projection sets no version on CRM sources.
        assert!(source.get("version").is_none());
    })
    .await;
}

#[tokio::test]
async fn activity_resolves_through_lead_with_fragment_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "co", "user-1", "crm_activity", "act-1")
            .await
            .expect("activity must resolve through its lead");
        assert_eq!(target.href.as_deref(), Some("/crm/lead-1#crm-activities"));
        let source = target.source.expect("activity must carry a source");
        assert_eq!(source, json!({
            "kind": "crm_activity",
            "id": "act-1",
            "href": "/crm/lead-1#crm-activities",
            "label": "Crm activity",
        }));
        assert!(source.get("version").is_none());
    })
    .await;
}

#[tokio::test]
async fn missing_link_stays_invisible_and_missing_sources_are_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No activity row: the retained read skips without an error.
        assert!(
            visible(pool.clone(), "co", "user-1", "crm_activity", "missing")
                .await
                .is_none(),
            "activity without a lead link must stay invisible"
        );
        // The oracle throws CRM_SOURCE_NOT_FOUND for missing or
        // account/contact-less leads; the delegate surfaces the exact code.
        for source_id in ["missing", "lead-bare"] {
            assert!(
                matches!(
                    composed_target(
                        pool.clone(),
                        "co",
                        "user-1",
                        "crm_lead",
                        source_id,
                        Some("br"),
                        None,
                    )
                    .await,
                    Err(PgTransactionError::Business(ref error))
                        if error.code == "CRM_SOURCE_NOT_FOUND"
                ),
                "{source_id} must deny with CRM_SOURCE_NOT_FOUND"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn out_of_scope_and_unauthorized_readers_are_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Selected frame outside the lead's branch: source scope fails.
        assert!(
            matches!(
                composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "crm_lead",
                    "lead-1",
                    Some("elsewhere"),
                    None,
                )
                .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "CRM_SOURCE_NOT_FOUND"
            ),
            "out-of-scope lead must deny with CRM_SOURCE_NOT_FOUND"
        );
        assert!(
            matches!(
                composed_target(pool.clone(), "foreign", "user-1", "crm_lead", "lead-1", Some("br"), None)
                    .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "CRM_SOURCE_NOT_FOUND"
            ),
            "foreign company must deny with CRM_SOURCE_NOT_FOUND"
        );
        // No current commerce.read assignment: authority fails.
        assert!(
            matches!(
                composed_target(pool.clone(), "co", "user-2", "crm_lead", "lead-1", Some("br"), None)
                    .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "CRM_ACCESS_CHANGED"
            ),
            "reader without authority must deny with CRM_ACCESS_CHANGED"
        );
    })
    .await;
}

#[tokio::test]
async fn opportunity_rule_needs_reviewed_finance_linkage() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Reviewed Finance linkage: the CRM-owned necessary conditions hold.
        // (The Finance-customer row match and quote/sale/reservation validity
        // belong to their owners and stay unproven; see the delegate docs.)
        let target = visible(pool.clone(), "co", "user-1", "crm_lead", "lead-opp")
            .await
            .expect("reviewed opportunity lead must resolve");
        assert_eq!(target.href.as_deref(), Some("/crm/lead-opp"));
        // Same opportunity shape without reviewed linkage: unavailable.
        assert!(
            matches!(
                composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "crm_lead",
                    "lead-opp-bare",
                    Some("br"),
                    None,
                )
                .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "CRM_SOURCE_NOT_FOUND"
            ),
            "unreviewed opportunity lead must deny with CRM_SOURCE_NOT_FOUND"
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_retained_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let denied = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "crm_lead",
            "lead-1",
            Some("br"),
            Some(PackageEnforcementMode::Enforced),
        )
        .await;
        assert!(
            matches!(
                denied,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "PACKAGE_REQUIRED"
            ),
            "enforced mode without sales.crm history must deny with PACKAGE_REQUIRED"
        );
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert!(
            matches!(
                composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "future_source",
                    "x",
                    Some("br"),
                    None,
                )
                .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "INBOX_TARGET_SOURCE_UNSUPPORTED"
            ),
            "unknown source must fail closed"
        );
        assert!(
            matches!(
                composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "support_ticket",
                    "t-1",
                    Some("br"),
                    None,
                )
                .await,
                Err(PgTransactionError::Business(ref error))
                    if error.code == "INBOX_TARGET_PORT_UNAVAILABLE"
            ),
            "unowned source must report its missing owner"
        );
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
                            CrmInboxTargets::new(Some("br"), None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "crm_lead", "lead-1", 0)
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
        let leads: i64 =
            sqlx::query_scalar("SELECT count(*) FROM crm_leads WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let activities: i64 =
            sqlx::query_scalar("SELECT count(*) FROM crm_lead_activities WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!((leads, activities), (4, 1));
    })
    .await;
}
