//! Postgres proof of the native Quality and Legal Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Holds run authority, the
//! lineage image, the origin purchase and the scoped read; alerts read the
//! scoped alert first and then run the same hold gates; legal runs the
//! permission, the scoped item read, the assignment authority and the
//! kind-capability purchase. Missing rows deny with the exact oracle 403/404
//! codes and messages. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` through the delegate's explicit mode
//! seam instead of mutating process environment (unsafe env mutation is
//! forbidden by workspace lints).
//!
//! The alert href and source carry the hold id with no version, exactly like
//! the retained push; holds and legal records carry no source override.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, QualityInboxTargets,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES \
         ('co','company','co','Co',NULL,'co',NULL,'active'), \
         ('br','branch','br','Br','co','co',NULL,'active'), \
         ('dep-1','department','dep-1','Dep','br','co','br','active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','no_quality','No quality',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','no_quality',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','quality_inspector','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','legal_counsel','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','no_quality','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-4','user-4','legal_requester','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('sup-1','co','supplier','SUP','Supplier','THB',0,0); \
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('prod-1','co','SKU-1','Widget','each',100,0,'active',1); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1); \
         INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES \
         ('wh-1','prod-1',0,0); \
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-p','co','br','dep-1','purchase','P-1','sup-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1); \
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES \
         ('ol-1','ord-p','prod-1','Widget',1000,0,100,0,100,0,0); \
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES \
         ('ful-1','ord-p','F-1','2026-11-02','maker',1); \
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES \
         ('fl-1','ful-1','ol-1',1000,100,100,0); \
         INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES \
         ('lot-1','co','prod-1','lot','LOT-1','maker',0); \
         INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor,version) VALUES \
         ('lot-1','wh-1',1000,100,1); \
         INSERT INTO inventory_quality_specs(id,company_id,product_id,name,created_by,created_at,updated_at) VALUES \
         ('spec-1','co','prod-1','Visual','maker',0,0); \
         INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,lot_id,held_quantity_milli,source_type,source_id,created_at,updated_at) VALUES \
         ('hold-1','co','br','wh-1','prod-1','Inspect','2026-12-01','held','maker','lot-1',1000,'goods_receipt','fl-1',1,1), \
         ('hold-2','co','br','wh-1','prod-1','Inspect','2026-12-01','held','maker',NULL,NULL,NULL,NULL,1,1); \
         UPDATE inventory_quality_holds SET spec_id='spec-1' WHERE id='hold-2'; \
         INSERT INTO inventory_fulfillment_lot_allocations(id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id) VALUES \
         ('al-1','fl-1','lot-1',1000,100,'quarantine','hold-1'); \
         INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at) VALUES \
         ('alert-1','hold-1','due_soon',1,'2026-12-01',1); \
         INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at) VALUES \
         ('leg-1','co','br',NULL,'matter','Dispute','internal','TH','TH','th','THB','Asia/Bangkok','normal','user-2','maker',1,1), \
         ('leg-2','co','br',NULL,'request','Access','confidential','TH','TH','th','THB','Asia/Bangkok','normal','user-9','user-9',1,1); \
         INSERT INTO legal_members(item_id,user_id,access) VALUES \
         ('leg-2','user-2','read')",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:quality-history',0)",
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
            "evidence": "test:quality-history",
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
            let mut delegate = QualityInboxTargets::new(branch_id.as_deref(), None);
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
async fn hold_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        for hold in ["hold-1", "hold-2"] {
            let target = visible(pool.clone(), "user-1", "quality_hold", hold)
                .await
                .unwrap_or_else(|| panic!("{hold} must resolve"));
            assert_eq!(
                target.href.as_deref(),
                Some(format!("/quality/{hold}").as_str())
            );
            // No source override: the composition final map applies the
            // retained hold default ({kind:"quality_hold", id, href}).
            assert_eq!(target.source, None);
        }
    })
    .await;
}

#[tokio::test]
async fn alert_visible_with_hold_href_and_hold_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "quality_alert", "alert-1")
            .await
            .expect("scoped alert must resolve");
        assert_eq!(target.href.as_deref(), Some("/quality/hold-1"));
        // The retained push carries the hold source with no version.
        assert_eq!(
            target.source,
            Some(json!({
                "kind": "quality_hold",
                "id": "hold-1",
                "href": "/quality/hold-1",
                "label": "Quality hold",
            }))
        );
    })
    .await;
}

#[tokio::test]
async fn legal_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-2", "legal", "leg-1")
            .await
            .expect("scoped record must resolve");
        assert_eq!(target.href.as_deref(), Some("/legal?record=leg-1"));
        assert_eq!(target.source, None);
        // A confidential record resolves through its membership.
        let target = visible(pool.clone(), "user-2", "legal", "leg-2")
            .await
            .expect("member resolves the confidential record");
        assert_eq!(target.href.as_deref(), Some("/legal?record=leg-2"));
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_are_denied_with_the_access_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No quality.read: the readable check denies first, exactly like
        // the oracle with enforcement off.
        for (source_type, source_id) in [("quality_hold", "hold-1"), ("quality_alert", "alert-1")] {
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
        // No legal.read: the permission check denies first.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "legal", "leg-1", None, None).await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // Counsel without quality.read denies the hold read.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "quality_hold", "hold-1", None, None)
                    .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // Inspector without legal.read denies the legal read.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "legal", "leg-1", None, None).await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // A requester sees neither a matter nor another requester's record.
        for record in ["leg-1", "leg-2"] {
            assert_eq!(
                denied(
                    &composed_target(pool.clone(), "co", "user-4", "legal", record, None, None).await
                ),
                (
                    "LEGAL_NOT_FOUND",
                    "Legal record is unavailable in your current scope."
                ),
                "{record}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn department_frames_deny_before_the_scoped_read() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The department frame is rejected by the readable check,
        // exactly like the oracles.
        for (source_type, source_id, code, message) in [
            (
                "quality_hold",
                "hold-1",
                "QUALITY_SCOPE_REQUIRED",
                "Use a Company or Branch quality assignment.",
            ),
            (
                "quality_alert",
                "alert-1",
                "QUALITY_SCOPE_REQUIRED",
                "Use a Company or Branch quality assignment.",
            ),
        ] {
            let (source_type, source_id) = (source_type.to_owned(), source_id.to_owned());
            let label = source_type.clone();
            let denied_code = run(pool.clone(), |connection| {
                Box::pin(async move {
                    let delegate = QualityInboxTargets::new(None, Some("dep-1"));
                    let mut targets = PgInboxTargets::with_delegate(None, Some("dep-1"), delegate);
                    let error = targets
                        .target(connection, "co", "user-1", &source_type, &source_id, 0)
                        .await
                        .expect_err("department frame must deny");
                    Ok((error.code, error.message))
                })
            })
            .await
            .unwrap();
            assert_eq!(denied_code, (code, message), "{label}");
        }
    })
    .await;
}

#[tokio::test]
async fn missing_rows_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "quality_hold", "missing", None, None)
                    .await
            ),
            ("QUALITY_HOLD_NOT_FOUND", "Quality hold not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "quality_alert",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            (
                "QUALITY_ALERT_NOT_FOUND",
                "Quality escalation is not available in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "legal", "missing", None, None).await
            ),
            (
                "LEGAL_NOT_FOUND",
                "Legal record is unavailable in your current scope."
            )
        );
        // Out-of-frame rows read as missing, exactly like the retained
        // scopes.
        for (source_type, source_id, code) in [
            ("quality_hold", "hold-1", "QUALITY_HOLD_NOT_FOUND"),
            ("quality_alert", "alert-1", "QUALITY_ALERT_NOT_FOUND"),
            ("legal", "leg-1", "LEGAL_NOT_FOUND"),
        ] {
            let (actual_code, _) = denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    if source_type == "legal" { "user-2" } else { "user-1" },
                    source_type,
                    source_id,
                    Some("elsewhere"),
                    None,
                )
                .await,
            );
            assert_eq!(actual_code, code, "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The hold purchase gate runs before the application read, so a
        // permissionless reader denies with the authority code; the alert
        // reads first, so it denies with the permission code. Both match
        // their oracle order exactly.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "quality_hold",
                    "hold-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "QUALITY_ACCESS_CHANGED",
                "Choose a current authorized Quality assignment."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "quality_alert",
                    "alert-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // Empty purchase facts in enforced mode: each oracle's exact
        // refusal surfaces for its own capability.
        for (source_type, source_id, message) in [
            (
                "quality_hold",
                "hold-1",
                "This Quality source requires its original purchase or retained history.",
            ),
            (
                "quality_alert",
                "alert-1",
                "This Quality source requires its original purchase or retained history.",
            ),
            (
                "legal",
                "leg-1",
                "This Legal source requires its purchased capability or retained history.",
            ),
        ] {
            assert_eq!(
                denied(
                    &composed_target(
                        pool.clone(),
                        "co",
                        if source_type == "legal" { "user-2" } else { "user-1" },
                        source_type,
                        source_id,
                        None,
                        Some(PackageEnforcementMode::Enforced),
                    )
                    .await
                ),
                ("PACKAGE_REQUIRED", message),
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
        seed_grants(&pool, &["supply", "supply-quality", "legal"]).await;
        // The receipt-origin hold settles receiving plus stock; the spec
        // hold settles the spec capability; the matter settles matters.
        for (source_type, source_id, user_id, href) in [
            ("quality_hold", "hold-1", "user-1", "/quality/hold-1"),
            ("quality_hold", "hold-2", "user-1", "/quality/hold-2"),
            ("quality_alert", "alert-1", "user-1", "/quality/hold-1"),
            ("legal", "leg-1", "user-2", "/legal?record=leg-1"),
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
            .unwrap_or_else(|| panic!("granted {source_type} {source_id} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn spec_hold_requires_its_own_capability() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The supply grant alone settles the receipt-origin hold but not
        // the spec hold, proving the lineage capability resolution.
        seed_grants(&pool, &["supply"]).await;
        composed_target(
            pool.clone(),
            "co",
            "user-1",
            "quality_hold",
            "hold-1",
            None,
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("receipt-origin hold must resolve with the supply grant");
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "quality_hold",
                    "hold-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This Quality source requires its original purchase or retained history."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = QualityInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, QualityInboxTargets::new(None, None));
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
                            QualityInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "quality_hold", "hold-1", 0)
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
            "SELECT (SELECT count(*) FROM inventory_quality_holds WHERE company_id='co'), \
             (SELECT count(*) FROM inventory_quality_alerts), \
             (SELECT count(*) FROM legal_items WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 2));
    })
    .await;
}
