//! Postgres proof of the native Procurement Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle: the `supply.purchasing`
//! history purchase decision, the database-current role (`finance.read`
//! for bills, `commerce.read` for RFQs and promises), then the scoped
//! identity read. Missing rows deny with the exact oracle 403/404 codes
//! and messages. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` through the delegate's explicit mode
//! seam instead of mutating process environment (unsafe env mutation is
//! forbidden by workspace lints).
//!
//! The bill image gate runs only when enforcement is enabled, exactly like
//! `withSupplierBillPurchasedRead`: `bill-2` has no receipt row, so it
//! stays visible in disabled mode and denies in enforced mode.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, ProcurementInboxTargets,
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
         ('*','procurement_reader','Procurement reader',false,1,'active'), \
         ('*','commerce_reader','Commerce reader',false,1,'active'), \
         ('*','no_procurement','No procurement',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','procurement_reader',1,'[\"finance.read\",\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','commerce_reader',1,'[\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_procurement',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','procurement_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','commerce_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','no_procurement','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('sup-1','co','supplier','SUP','Supplier','THB',0,0), \
         ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1); \
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-1','co','br','dep-1','purchase','P-1','sup-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1), \
         ('ord-2','co','br','dep-1','sales','S-2','cust-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1); \
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES \
         ('ful-1','ord-1','F-1','2026-11-02','maker',1), \
         ('ful-2','ord-1','F-2','2026-11-03','maker',1); \
         INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,status,created_by,created_at,updated_at) VALUES \
         ('bill-1','co','br','dep-1','ord-1','ful-1','sup-1','THB','ch-1','clearing','payable','tax','REF-1','2026-11-02','2026-11-10','Evidence',100,100,0,true,'review','maker',1,1), \
         ('bill-2','co','br','dep-1','ord-1','ful-2','sup-1','THB','ch-1','clearing','payable','tax','REF-2','2026-11-03','2026-11-10','Evidence',100,100,0,true,'review','maker',1,1); \
         INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version,active) VALUES \
         ('bmr-1','bill-1','ful-1',1,true); \
         INSERT INTO procurement_rfqs(id,company_id,branch_id,department_id,warehouse_id,currency,title,demand_evidence,needed_date,status,created_by,created_at,updated_at) VALUES \
         ('rfq-1','co','br','dep-1','wh-1','THB','Widgets','Demand','2026-12-01','collecting','maker',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_supply_grant(pool: &DatabasePool) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
         ('co','supply',1,'active',0,NULL,1,'fixture','test:supply-history',0)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES ('evt-1','co','supply','fixture','test',NULL,'{"companyId":"co","packageId":"supply","packageVersion":1,"state":"active","startsAt":0,"endsAt":null,"version":1,"reason":"fixture","evidence":"test:supply-history"}',0)"#)
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
            let mut delegate = ProcurementInboxTargets::new(branch_id.as_deref(), None);
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
async fn bill_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "supplier_bill", "bill-1")
            .await
            .expect("scoped bill must resolve");
        assert_eq!(target.href.as_deref(), Some("/supplier-bills/bill-1"));
        // No source override: the composition final map applies the retained
        // bill default ({kind:"supplier_bill", id, href, label}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn rfq_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "procurement_rfq", "rfq-1")
            .await
            .expect("scoped RFQ must resolve");
        assert_eq!(target.href.as_deref(), Some("/procurement-rfqs/rfq-1"));
        assert_eq!(target.source, None);
        // commerce.read alone proves the RFQ read.
        let target = visible(pool.clone(), "user-2", "procurement_rfq", "rfq-1")
            .await
            .expect("commerce reader resolves the RFQ");
        assert_eq!(target.href.as_deref(), Some("/procurement-rfqs/rfq-1"));
    })
    .await;
}

#[tokio::test]
async fn promise_visible_with_exact_href_and_fragment() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "supplier_promise", "ord-1")
            .await
            .expect("scoped purchase order must resolve");
        // The retained fragment rides the encoded order id.
        assert_eq!(
            target.href.as_deref(),
            Some("/commerce/orders/ord-1#supplier-promises")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_are_denied_with_the_access_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // commerce.read without finance.read denies the bill read.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "supplier_bill", "bill-1", None, None)
                    .await
            ),
            (
                "SUPPLIER_BILL_ACCESS_CHANGED",
                "Choose a current authorized supplier-invoice work role."
            )
        );
        // No procurement role at all denies every slice with its own code.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-3", "supplier_bill", "bill-1", None, None)
                    .await
            ),
            (
                "SUPPLIER_BILL_ACCESS_CHANGED",
                "Choose a current authorized supplier-invoice work role."
            )
        );
        for (source_type, source_id) in [
            ("procurement_rfq", "rfq-1"),
            ("supplier_promise", "ord-1"),
        ] {
            assert_eq!(
                denied(
                    &composed_target(
                        pool.clone(),
                        "co",
                        "user-3",
                        source_type,
                        source_id,
                        None,
                        None
                    )
                    .await
                ),
                (
                    "PROCUREMENT_ACCESS_CHANGED",
                    "Choose a current authorized purchasing work role."
                ),
                "{source_type}"
            );
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
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_bill",
                    "missing",
                    None,
                    None
                )
                .await
            ),
            (
                "SUPPLIER_BILL_NOT_FOUND",
                "Supplier invoice review not found in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "procurement_rfq",
                    "missing",
                    None,
                    None
                )
                .await
            ),
            ("RFQ_NOT_FOUND", "Quotation request not found in this scope.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_promise",
                    "missing",
                    None,
                    None
                )
                .await
            ),
            (
                "PURCHASE_ORDER_NOT_FOUND",
                "Purchase order not found in this scope."
            )
        );
        // A sales order is not a purchase order: the kind filter reads as
        // missing, exactly like the retained promise read.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_promise",
                    "ord-2",
                    None,
                    None
                )
                .await
            ),
            (
                "PURCHASE_ORDER_NOT_FOUND",
                "Purchase order not found in this scope."
            )
        );
        // The reads are Work-framed: out-of-frame rows read as missing,
        // exactly like the retained scopes.
        for (source_type, source_id, code) in [
            (
                "supplier_bill",
                "bill-1",
                "SUPPLIER_BILL_NOT_FOUND",
            ),
            ("procurement_rfq", "rfq-1", "RFQ_NOT_FOUND"),
            ("supplier_promise", "ord-1", "PURCHASE_ORDER_NOT_FOUND"),
        ] {
            let (actual_code, _) = denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
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
async fn bill_without_receipt_proof_is_visible_only_when_unenforced() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Disabled mode proves the scoped bill row alone, exactly like
        // `withSupplierBillPurchasedRead` skipping its image gate.
        let target = visible(pool.clone(), "user-1", "supplier_bill", "bill-2")
            .await
            .expect("unimaged bill resolves when unenforced");
        assert_eq!(target.href.as_deref(), Some("/supplier-bills/bill-2"));
        // Enforced mode with history proves the missing image as 404.
        seed_supply_grant(&pool).await;
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_bill",
                    "bill-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "SUPPLIER_BILL_NOT_FOUND",
                "Supplier invoice review not found in this scope."
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
            ("supplier_bill", "bill-1"),
            ("procurement_rfq", "rfq-1"),
            ("supplier_promise", "ord-1"),
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
        seed_supply_grant(&pool).await;
        // One `supply` grant covers every capability behind this delegate.
        for (source_type, source_id, href) in [
            ("supplier_bill", "bill-1", "/supplier-bills/bill-1"),
            ("procurement_rfq", "rfq-1", "/procurement-rfqs/rfq-1"),
            (
                "supplier_promise",
                "ord-1",
                "/commerce/orders/ord-1#supplier-promises",
            ),
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
        let mut delegate = ProcurementInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, ProcurementInboxTargets::new(None, None));
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
                            ProcurementInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "supplier_bill", "bill-1", 0)
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
            "SELECT (SELECT count(*) FROM procurement_bill_matches WHERE company_id='co'), \
             (SELECT count(*) FROM procurement_rfqs WHERE company_id='co'), \
             (SELECT count(*) FROM commerce_orders WHERE company_id='co' AND kind='purchase')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 1));
    })
    .await;
}
