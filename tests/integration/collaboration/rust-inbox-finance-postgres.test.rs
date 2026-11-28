//! Postgres proof of the native Finance Inbox visibility delegate.
//!
//! The purchase-denied path pins `PackageEnforcementMode::Enforced` through
//! the delegate's explicit mode seam instead of mutating process environment
//! (unsafe env mutation is forbidden by workspace lints). Ambient mode stays
//! disabled, so the visibility tests exercise the open purchase path exactly
//! like development. The enforced-allow path seeds one audited `finance`
//! grant (grant plus matching grant event in a single transaction, as the
//! deferred audit trigger requires).
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    FinanceInboxTargets, InboxTarget, InboxTargetPort, PgInboxTargets,
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('co','company','co','Co',NULL,'co',NULL,'active',1), \
         ('br','branch','br','Br','co','co',NULL,'active',1); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','finance_reader','Finance reader',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','finance_reader',1,'[\"finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','finance_reader','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('ch-1','co','Chart','THB','maker',0,0); \
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES \
         ('per-1','co','P1','2026-01-01','2026-12-31',0); \
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES \
         ('acc-debit','ch-1','co','110','Debit','asset',0,0), \
         ('acc-credit','ch-1','co','210','Credit','liability',0,0), \
         ('acc-cash','ch-1','co','100','Cash','asset',0,0); \
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,created_by,created_at,updated_at) VALUES \
         ('jou-1','co','ch-1','per-1','J-1','2026-11-01','THB','maker',1,1); \
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,created_by,created_at,updated_at) VALUES \
         ('inv-1','co','cust-1','receivable','INV-1','2026-11-01','2026-11-10','THB',100,0,100,100,'acc-debit','acc-credit','maker',1,1); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1); \
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-r','co','br',NULL,'sales','S-9','cust-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1); \
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES \
         ('ful-1','ord-r','F-1','2026-11-02','maker',1); \
         INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,subtotal_minor,tax_minor,total_minor,requested_by,created_at,updated_at) VALUES \
         ('ret-1','co','br','ord-r','ful-1','inv-1','customer_return','R-1','2026-11-03','Defect',100,0,100,'maker',1,1); \
         INSERT INTO finance_refund_obligations(id,company_id,branch_id,department_id,return_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES \
         ('ref-1','co','br',NULL,'ret-1','inv-1','cust-1','outgoing','THB','ch-1','acc-cash',100,100,'maker',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_finance_grant(pool: &DatabasePool) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
         ('co','finance',1,'active',0,NULL,1,'fixture','test:finance-history',0)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES ('evt-1','co','finance','fixture','test',NULL,'{"companyId":"co","packageId":"finance","packageVersion":1,"state":"active","startsAt":0,"endsAt":null,"version":1,"reason":"fixture","evidence":"test:finance-history"}',0)"#)
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
            let mut delegate = FinanceInboxTargets::new(branch_id.as_deref(), None);
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
    source_type: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        composed_target(pool, "co", "user-1", source_type, source_id, Some("br"), None)
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
async fn invoice_visible_with_exact_href_source_and_numeric_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "invoice", "inv-1")
            .await
            .expect("scoped invoice must resolve");
        assert_eq!(target.href.as_deref(), Some("/finance/invoices/inv-1"));
        let source = target.source.expect("invoice must carry a source");
        assert_eq!(source, json!({
            "kind": "invoice",
            "id": "inv-1",
            "version": 1,
            "href": "/finance/invoices/inv-1",
            "label": "Invoice",
        }));
        assert!(source["version"].is_number());
    })
    .await;
}

#[tokio::test]
async fn journal_visible_with_exact_href_source_and_numeric_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "journal", "jou-1")
            .await
            .expect("scoped journal must resolve");
        assert_eq!(target.href.as_deref(), Some("/finance/journals/jou-1"));
        let source = target.source.expect("journal must carry a source");
        assert_eq!(source, json!({
            "kind": "journal",
            "id": "jou-1",
            "version": 1,
            "href": "/finance/journals/jou-1",
            "label": "Journal",
        }));
        assert!(source["version"].is_number());
    })
    .await;
}

#[tokio::test]
async fn refund_visible_without_a_source_override() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "refund", "ref-1")
            .await
            .expect("scoped obligation must resolve");
        assert_eq!(target.href.as_deref(), Some("/finance/refunds/ref-1"));
        // No source override: the composition final map applies the retained
        // refund default ({kind:"refund", id, href, label:"Refund"}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn missing_documents_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "invoice", "missing", Some("br"), None)
                    .await
            ),
            ("INVOICE_NOT_FOUND", "Invoice not found in this company.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "journal", "missing", Some("br"), None)
                    .await
            ),
            ("JOURNAL_NOT_FOUND", "Journal not found.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "refund", "missing", Some("br"), None)
                    .await
            ),
            ("REFUND_NOT_FOUND", "Refund obligation not found.")
        );
        // The refund scope is Work-framed: an out-of-frame obligation reads
        // as missing, exactly like the retained obligation scope.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "refund",
                    "ref-1",
                    Some("elsewhere"),
                    None,
                )
                .await
            ),
            ("REFUND_NOT_FOUND", "Refund obligation not found.")
        );
        // Scope always equals the reader's own Company in production (the
        // unit filters rows by scope and the scope comes from the current
        // selection), so a cross-company scope is unreachable and keeps the
        // retained authority-first ordering: the assignment lookup runs
        // against the given scope exactly like the other delegates.
    })
    .await;
}

#[tokio::test]
async fn reader_without_current_authority_is_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No finance.read assignment: the oracle's 403 surfaces exactly.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "invoice", "inv-1", Some("br"), None)
                    .await
            ),
            (
                "FINANCE_ACCESS_CHANGED",
                "Choose a current Finance assignment."
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
        // refuses with its exact code and message, for both base
        // capabilities behind this delegate.
        for source_type in ["invoice", "refund"] {
            let (code, message) = denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    source_type,
                    if source_type == "invoice" { "inv-1" } else { "ref-1" },
                    Some("br"),
                    Some(PackageEnforcementMode::Enforced),
                )
                .await,
            );
            assert_eq!(code, "PACKAGE_REQUIRED", "{source_type}");
            assert_eq!(
                message, "This package does not permit the requested work.",
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
        seed_finance_grant(&pool).await;
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "invoice",
            "inv-1",
            Some("br"),
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("granted invoice must resolve in enforced mode");
        assert_eq!(target.href.as_deref(), Some("/finance/invoices/inv-1"));
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = FinanceInboxTargets::new(Some("br"), None);
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
            PgInboxTargets::with_delegate(None, None, FinanceInboxTargets::new(Some("br"), None));
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
                            FinanceInboxTargets::new(Some("br"), None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "invoice", "inv-1", 0)
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
            "SELECT (SELECT count(*) FROM finance_invoices WHERE company_id='co'), \
             (SELECT count(*) FROM finance_journals WHERE company_id='co'), \
             (SELECT count(*) FROM finance_refund_obligations WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}
