//! Postgres proof of the native Commerce Inbox visibility delegate.
//!
//! The purchase-denied path pins `PackageEnforcementMode::Enforced` through
//! the delegate's explicit mode seam instead of mutating process environment
//! (unsafe env mutation is forbidden by workspace lints). Ambient mode stays
//! disabled, so the visibility tests exercise the open purchase path exactly
//! like development.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    CommerceInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
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
         ('*','commerce_reader','Commerce reader',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','commerce_reader',1,'[\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','commerce_reader','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1); \
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-sales','co','br',NULL,'sales','S-1','cust-1','wh-1','THB','2026-11-01','2026-11-10',0,0,'maker',1,1), \
         ('ord-quote','co','br',NULL,'quote','Q-1','cust-1','wh-1','THB','2026-11-01','2026-11-10',0,0,'maker',1,1), \
         ('ord-purchase','co','br',NULL,'purchase','P-1','cust-1','wh-1','THB','2026-11-01','2026-11-10',0,0,'maker',1,1)",
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
    source_id: &str,
    branch_id: Option<&str>,
    mode: Option<PackageEnforcementMode>,
) -> Result<Option<InboxTarget>, PgTransactionError<BusinessError>> {
    let (company_id, user_id, source_id, branch_id) = (
        company_id.to_owned(),
        user_id.to_owned(),
        source_id.to_owned(),
        branch_id.map(str::to_owned),
    );
    run(pool, move |connection| {
        Box::pin(async move {
            let mut delegate = CommerceInboxTargets::new(branch_id.as_deref(), None);
            if let Some(mode) = mode {
                delegate = delegate.enforcement_mode(mode);
            }
            let mut targets = PgInboxTargets::with_delegate(None, None, delegate);
            targets
                .target(connection, &company_id, &user_id, "commerce_order", &source_id, 0)
                .await
        })
    })
    .await
}

fn visible(
    pool: DatabasePool,
    company_id: &str,
    user_id: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        composed_target(pool, company_id, user_id, source_id, Some("br"), None)
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
async fn order_visible_with_exact_href_and_source_without_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "co", "user-1", "ord-sales")
            .await
            .expect("scoped order must resolve");
        assert_eq!(target.href.as_deref(), Some("/commerce/orders/ord-sales"));
        let source = target.source.expect("order must carry a source");
        assert_eq!(source, json!({
            "kind": "commerce_order",
            "id": "ord-sales",
            "href": "/commerce/orders/ord-sales",
            "label": "Commerce order",
        }));
        // The retained projection sets no version on Commerce sources.
        assert!(source.get("version").is_none());
    })
    .await;
}

#[tokio::test]
async fn every_order_kind_maps_to_its_capability() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // quote -> sales.quotes, purchase -> supply.purchasing: the mapping
        // resolves instead of failing the kind gate.
        for order_id in ["ord-quote", "ord-purchase"] {
            let target = visible(pool.clone(), "co", "user-1", order_id)
                .await
                .unwrap_or_else(|| panic!("{order_id} must resolve"));
            assert_eq!(
                target.href.as_deref(),
                Some(format!("/commerce/orders/{order_id}").as_str())
            );
        }
    })
    .await;
}

#[tokio::test]
async fn missing_and_out_of_frame_orders_keep_the_oracle_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "missing", Some("br"), None).await
            ),
            ("ORDER_NOT_FOUND", "Order not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "ord-sales",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            ("ORDER_NOT_FOUND", "Order not found in this workspace.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "foreign",
                    "user-1",
                    "ord-sales",
                    Some("br"),
                    None,
                )
                .await
            ),
            ("ORDER_NOT_FOUND", "Order not found in this workspace.")
        );
    })
    .await;
}

#[tokio::test]
async fn reader_without_current_authority_is_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No commerce.read assignment: the oracle's 403 surfaces exactly.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "ord-sales", Some("br"), None).await
            ),
            (
                "COMMERCE_ACCESS_CHANGED",
                "Choose a current authorized Commerce work role."
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
        // refuses with its exact code and message.
        let (code, message) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "ord-sales",
                Some("br"),
                Some(PackageEnforcementMode::Enforced),
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        assert_eq!(message, "This package does not permit the requested work.");
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = CommerceInboxTargets::new(Some("br"), None);
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
            PgInboxTargets::with_delegate(None, None, CommerceInboxTargets::new(Some("br"), None));
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
                            CommerceInboxTargets::new(Some("br"), None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "commerce_order", "ord-sales", 0)
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
        let orders: i64 =
            sqlx::query_scalar("SELECT count(*) FROM commerce_orders WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(orders, 3);
    })
    .await;
}
