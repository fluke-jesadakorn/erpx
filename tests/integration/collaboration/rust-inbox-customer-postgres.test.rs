//! Postgres proof of the native Customer Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Identity and duplicate reads
//! settle the two commerce roles before the company row read (with the
//! customer joins). Access settles the roles, then the purchase path
//! (queued authority, the scoped binding/order source, the portal
//! history), then the scoped binding read. Missing rows deny with the
//! exact oracle 404 codes and messages. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` through the delegate's explicit mode
//! seam instead of mutating process environment (unsafe env mutation is
//! forbidden by workspace lints).
//!
//! History decisions require company package history rights as well as the
//! grant and its event, exactly like the commercial decision port reads
//! them.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    CustomerInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
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
         ('br','branch','br','Br','co','co',NULL,'active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','cust_reader','Customer reader',false,1,'active'), \
         ('*','cust_partial','Customer partial',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','cust_reader',1,'[\"commerce.read\",\"commerce.sales\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','cust_partial',1,'[\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','cust_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','cust_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','cust_partial','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('cust-A','co','customer','CUST-A','Customer A','USD',1,1), \
         ('cust-B','co','customer','CUST-B','Customer B','USD',1,1); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES \
         ('wh-1','co','br','WH-1','Main warehouse'); \
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-1','co','br','sales','ORD-1','cust-A','wh-1','USD','2026-11-01','2026-11-30',0,0,'maker',1,1); \
         INSERT INTO customer_identity_changes(id,company_id,customer_id,expected_customer_version,previous_json,proposed_json,evidence,created_by,created_at,updated_at) VALUES \
         ('idc-1','co','cust-A',1,'{}'::jsonb,'{}'::jsonb,'review','user-2',1,1); \
         INSERT INTO customer_duplicate_reviews(id,company_id,left_customer_id,right_customer_id,evidence,created_by,created_at,updated_at) VALUES \
         ('dup-1','co','cust-A','cust-B','review','user-2',1,1); \
         INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,reason,created_by,created_at,updated_at) VALUES \
         ('acc-1','co','ord-1','cust-A','user@example.com',2,'portal','user-2',1,1)",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:customer-history',0)",
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
            "evidence": "test:customer-history",
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
            let mut delegate = CustomerInboxTargets::new(branch_id.as_deref(), None);
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
async fn identity_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "customer_identity", "idc-1")
            .await
            .expect("permitted reader resolves the change");
        assert_eq!(target.href.as_deref(), Some("/customer-identity/idc-1"));
        // No source override: the composition final map applies the
        // retained default.
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn duplicate_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "customer_duplicate", "dup-1")
            .await
            .expect("permitted reader resolves the review");
        assert_eq!(target.href.as_deref(), Some("/customer-duplicates/dup-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn access_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "customer_access", "acc-1")
            .await
            .expect("permitted reader resolves the binding");
        assert_eq!(target.href.as_deref(), Some("/customer-access/acc-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn denied_rows_keep_oracle_codes() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Missing commerce.sales on the role: the role proof denies first.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "customer_identity",
                    "idc-1",
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
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "customer_access",
                    "acc-1",
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
        // Missing rows deny with each oracle's exact 404.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "customer_identity",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "CUSTOMER_IDENTITY_CHANGE_NOT_FOUND",
                "Identity change not found in this Company."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "customer_duplicate",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "CUSTOMER_DUPLICATE_NOT_FOUND",
                "Duplicate review not found in this Company."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "customer_access",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "CUSTOMER_ACCESS_NOT_FOUND",
                "Customer access request not found in this scope."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_access_denies_without_history() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["sales"]).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // The portal history is unpurchased: the commercial decision code
        // surfaces for the access capability.
        let (code, _) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "customer_access",
                "acc-1",
                None,
                enforced,
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        // No assignment at all: the role proof denies before the purchase.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-9",
                    "customer_access",
                    "acc-1",
                    None,
                    enforced,
                )
                .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["sales", "sales-portal"]).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        for (source_type, source_id, href) in [
            ("customer_identity", "idc-1", "/customer-identity/idc-1"),
            ("customer_duplicate", "dup-1", "/customer-duplicates/dup-1"),
            ("customer_access", "acc-1", "/customer-access/acc-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                "user-1",
                source_type,
                source_id,
                None,
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
        let mut delegate = CustomerInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, CustomerInboxTargets::new(None, None));
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
                            CustomerInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "customer_identity", "idc-1", 0)
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
            "SELECT (SELECT count(*) FROM customer_identity_changes WHERE company_id='co'), \
             (SELECT count(*) FROM customer_duplicate_reviews WHERE company_id='co'), \
             (SELECT count(*) FROM customer_access_bindings WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}
