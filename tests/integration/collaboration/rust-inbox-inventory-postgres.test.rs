//! Postgres proof of the native Inventory Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle: the readable role, the current
//! assignment authority, the owner image proof and the history purchase
//! decision, then the scoped identity read. Gate order matches each oracle
//! (landed: authority, image, purchase, read; counts: authority, purchase,
//! image, read; transits: authority, purchase, image, read; follow-ups:
//! image, authority, purchase, read). Missing rows deny with the exact
//! oracle 403/404 codes and messages. The enforced-mode tests pin
//! `PackageEnforcementMode::Enforced` through the delegate's explicit mode
//! seam instead of mutating process environment (unsafe env mutation is
//! forbidden by workspace lints).
//!
//! The unimaged fixtures (`cost-2`, `tr-2`, `fol-3`) stay visible in
//! disabled mode and deny in enforced mode, exactly like their oracles
//! skipping the image gate when enforcement is off.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxTarget, InboxTargetPort, InventoryInboxTargets, PgInboxTargets,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
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
         ('br','branch','br','Br','co','co',NULL,'active'), \
         ('br-2','branch','br-2','Br2','co','co',NULL,'active'), \
         ('dep-1','department','dep-1','Dep','br','co','br','active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','inventory_reader','Inventory reader',false,1,'active'), \
         ('*','sales_reader','Sales reader',false,1,'active'), \
         ('*','no_inventory','No inventory',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','inventory_reader',1,'[\"finance.read\",\"inventory.read\",\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','sales_reader',1,'[\"commerce.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_inventory',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','inventory_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','sales_reader','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','no_inventory','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('ch-1','co','Chart','THB','maker',0,0); \
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES \
         ('acc-1','ch-1','co','100','Cash','asset',0,0); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
         ('sup-1','co','supplier','SUP','Supplier','THB',0,0), \
         ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1), \
         ('wh-2','co','br-2','W2','Remote','active',1); \
         INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES \
         ('co','ch-1','THB','acc-1','acc-1','acc-1','acc-1','acc-1','acc-1','acc-1','acc-1','acc-1'); \
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('prod-1','co','SKU-1','Widget','each',100,0,'active',1); \
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES \
         ('ord-p','co','br','dep-1','purchase','P-1','sup-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1), \
         ('ord-1','co','br','dep-1','sales','S-1','cust-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1), \
         ('ord-q','co','br','dep-1','quote','Q-1','cust-1','wh-1','THB','2026-11-01','2026-11-10',100,0,'maker',1,1); \
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES \
         ('ol-1','ord-p','prod-1','Widget',1000,0,100,0,100,0,0); \
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES \
         ('ful-p','ord-p','F-P','2026-11-02','maker',1), \
         ('ful-x','ord-p','F-X','2026-11-03','maker',1); \
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES \
         ('fl-1','ful-p','ol-1',1000,100,100,0); \
         INSERT INTO inventory_landed_costs(id,company_id,branch_id,fulfillment_id,number,cost_date,description,allocation_method,amount_minor,currency,credit_account_id,status,version,created_by,created_at,updated_at) VALUES \
         ('cost-1','co','br','ful-p','LC-1','2026-11-02','Freight','quantity',100,'THB','acc-1','submitted',1,'maker',1,1), \
         ('cost-2','co','br','ful-x','LC-2','2026-11-03','Freight','quantity',100,'THB','acc-1','submitted',1,'maker',1,1); \
         INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,total_value_minor,created_by,prepared_by,created_at,updated_at,delivery_mode) VALUES \
         ('tfr-1','co','wh-1','wh-2','br','br-2',1,1,'T-1','2026-11-02','Restock',0,'maker','maker',1,1,'transit'), \
         ('tfr-2','co','wh-1','wh-2','br','br-2',1,1,'T-2','2026-11-03','Restock',0,'maker','maker',1,1,'direct'); \
         INSERT INTO inventory_transit(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,transfer_id,due_date,currency,chart_id,inventory_account_id,created_by,approved_by,created_at,updated_at) VALUES \
         ('tr-1','co','wh-1','wh-2','br','br-2','tfr-1','2026-11-10','THB','ch-1','acc-1','maker','maker',1,1), \
         ('tr-2','co','wh-1','wh-2','br','br-2','tfr-2','2026-11-10','THB','ch-1','acc-1','maker','maker',1,1); \
         INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,created_by,prepared_by,created_at,updated_at) VALUES \
         ('cnt-1','co','br','wh-1','C-1','2026-11-02','Cycle','draft','maker','maker',1,1); \
         INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,created_by,created_at,updated_at) VALUES \
         ('fol-1','co','ord-1','user-2','2026-12-01','Call back','open','maker',1,1), \
         ('fol-2','co','ord-q','user-2','2026-12-01','Send quote','open','maker',1,1), \
         ('fol-3','co','ord-p','user-2','2026-12-01','Check ETA','open','maker',1,1)",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:inventory-history',0)",
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
            "evidence": "test:inventory-history",
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
            let mut delegate = InventoryInboxTargets::new(branch_id.as_deref(), None);
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

fn framed(
    pool: DatabasePool,
    user_id: &str,
    source_type: &str,
    source_id: &str,
    branch_id: Option<&str>,
) -> impl std::future::Future<Output = Result<Option<InboxTarget>, PgTransactionError<BusinessError>>>
{
    async move {
        composed_target(
            pool,
            "co",
            user_id,
            source_type,
            source_id,
            branch_id,
            None,
        )
        .await
    }
}

fn visible(
    pool: DatabasePool,
    user_id: &str,
    source_type: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        framed(pool, user_id, source_type, source_id, None)
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
async fn landed_cost_visible_with_order_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "landed_cost", "cost-1")
            .await
            .expect("scoped cost must resolve");
        // The href carries the record's order id, not the cost id.
        assert_eq!(
            target.href.as_deref(),
            Some("/commerce/orders/ord-p#landed-costs")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn stock_count_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "stock_count", "cnt-1")
            .await
            .expect("scoped count must resolve");
        assert_eq!(target.href.as_deref(), Some("/stock-counts/cnt-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn transit_visible_from_either_branch_side() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The retained scope matches the source or the target branch.
        for branch in [None, Some("br"), Some("br-2")] {
            let target = framed(
                pool.clone(),
                "user-1",
                "inventory_transit",
                "tr-1",
                branch,
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("transit must resolve under frame {branch:?}"));
            assert_eq!(target.href.as_deref(), Some("/stock-transit/tr-1"));
            assert_eq!(target.source, None);
        }
    })
    .await;
}

#[tokio::test]
async fn followup_visible_for_sales_and_quote_orders() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        for followup in ["fol-1", "fol-2"] {
            let target = visible(pool.clone(), "user-1", "customer_followup", followup)
                .await
                .unwrap_or_else(|| panic!("{followup} must resolve"));
            assert_eq!(
                target.href.as_deref(),
                Some(format!("/customer-followups/{followup}").as_str())
            );
            assert_eq!(target.source, None);
        }
        // commerce.read alone proves the follow-up read.
        let target = visible(pool.clone(), "user-2", "customer_followup", "fol-1")
            .await
            .expect("sales reader resolves the follow-up");
        assert_eq!(target.href.as_deref(), Some("/customer-followups/fol-1"));
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_are_denied_with_the_access_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // commerce.read without finance.read denies the landed-cost read.
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-2", "landed_cost", "cost-1", None).await
            ),
            ("PERMISSION_REQUIRED", "Your selected role does not grant this permission.")
        );
        // commerce.read without inventory.read denies counts and transits.
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-2", "stock_count", "cnt-1", None).await
            ),
            (
                "INVENTORY_SCOPE_REQUIRED",
                "Use a branch or holding inventory or finance assignment."
            )
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-2", "inventory_transit", "tr-1", None).await
            ),
            (
                "TRANSFER_ACCESS_CHANGED",
                "Choose a current authorized warehouse assignment."
            )
        );
        // No inventory role at all denies every slice with its own code.
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-3", "landed_cost", "cost-1", None).await
            ),
            ("PERMISSION_REQUIRED", "Your selected role does not grant this permission.")
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-3", "stock_count", "cnt-1", None).await
            ),
            (
                "INVENTORY_SCOPE_REQUIRED",
                "Use a branch or holding inventory or finance assignment."
            )
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-3", "inventory_transit", "tr-1", None).await
            ),
            (
                "TRANSFER_ACCESS_CHANGED",
                "Choose a current authorized warehouse assignment."
            )
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-3", "customer_followup", "fol-1", None).await
            ),
            (
                "FOLLOWUP_ACCESS_CHANGED",
                "Choose a current authorized Sales role."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn department_frames_deny_before_the_scoped_read() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The department frame is rejected outright, exactly like the
        // oracles: landed costs and counts never scope to a department,
        // and transit authority excludes it.
        for (source_type, source_id, code) in [
            ("landed_cost", "cost-1", "FINANCE_SCOPE_REQUIRED"),
            ("stock_count", "cnt-1", "INVENTORY_SCOPE_REQUIRED"),
            ("inventory_transit", "tr-1", "TRANSFER_ACCESS_CHANGED"),
        ] {
            let (source_type, source_id) = (source_type.to_owned(), source_id.to_owned());
            let label = source_type.clone();
            let denied_code = run(pool.clone(), |connection| {
                Box::pin(async move {
                    let delegate = InventoryInboxTargets::new(None, Some("dep-1"));
                    let mut targets = PgInboxTargets::with_delegate(None, Some("dep-1"), delegate);
                    let error = targets
                        .target(connection, "co", "user-1", &source_type, &source_id, 0)
                        .await
                        .expect_err("department frame must deny");
                    Ok(error.code)
                })
            })
            .await
            .unwrap();
            assert_eq!(denied_code, code, "{label}");
        }
        // Follow-ups scope to the department instead of rejecting it: the
        // dep-1 frame still resolves the dep-1 follow-up.
        let framed_followup = run(pool.clone(), |connection| {
            Box::pin(async move {
                let delegate = InventoryInboxTargets::new(None, Some("dep-1"));
                let mut targets = PgInboxTargets::with_delegate(None, Some("dep-1"), delegate);
                targets
                    .target(connection, "co", "user-1", "customer_followup", "fol-1", 0)
                    .await
            })
        })
        .await
        .unwrap()
        .expect("department frame must resolve the department follow-up");
        assert_eq!(
            framed_followup.href.as_deref(),
            Some("/customer-followups/fol-1")
        );
    })
    .await;
}

#[tokio::test]
async fn missing_rows_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(&framed(pool.clone(), "user-1", "landed_cost", "missing", None).await),
            (
                "LANDED_COST_NOT_FOUND",
                "Landed cost record not found in your scope."
            )
        );
        assert_eq!(
            denied(&framed(pool.clone(), "user-1", "stock_count", "missing", None).await),
            ("COUNT_NOT_FOUND", "Stock count not found in your workspace.")
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-1", "inventory_transit", "missing", None).await
            ),
            (
                "TRANSIT_NOT_FOUND",
                "Transport record not found in this scope."
            )
        );
        assert_eq!(
            denied(
                &framed(pool.clone(), "user-1", "customer_followup", "missing", None).await
            ),
            (
                "CUSTOMER_FOLLOWUP_NOT_FOUND",
                "Customer follow-up not found in this scope."
            )
        );
        // Out-of-frame rows read as missing, exactly like the retained
        // scopes.
        for (source_type, source_id, code) in [
            ("landed_cost", "cost-1", "LANDED_COST_NOT_FOUND"),
            ("stock_count", "cnt-1", "COUNT_NOT_FOUND"),
            ("inventory_transit", "tr-1", "TRANSIT_NOT_FOUND"),
            ("customer_followup", "fol-1", "CUSTOMER_FOLLOWUP_NOT_FOUND"),
        ] {
            let (actual_code, _) = denied(
                &framed(pool.clone(), "user-1", source_type, source_id, Some("elsewhere")).await,
            );
            assert_eq!(actual_code, code, "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn unimaged_rows_are_visible_only_when_unenforced() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Disabled mode skips the owner image, exactly like the oracles:
        // a cost on a lineless receipt, a transit on a direct transfer
        // and a follow-up on a purchase order still resolve.
        for (source_type, source_id) in [
            ("landed_cost", "cost-2"),
            ("inventory_transit", "tr-2"),
            ("customer_followup", "fol-3"),
        ] {
            framed(pool.clone(), "user-1", source_type, source_id, None)
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("unimaged {source_type} must resolve when unenforced"));
        }
        // Enforced mode with history proves the missing image.
        seed_grants(&pool, &["supply", "supply-advanced", "sales"]).await;
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "landed_cost",
                    "cost-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "LANDED_COST_NOT_FOUND",
                "The original landed-cost receipt is unavailable in this workspace."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "inventory_transit",
                    "tr-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "TRANSIT_NOT_FOUND",
                "This warehouse movement is unavailable in the current workspace."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "customer_followup",
                    "fol-3",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND",
                "The actual customer quotation or Sales source is unavailable in this workspace."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Empty purchase facts in enforced mode: each oracle's exact
        // refusal surfaces for its own capability.
        for (source_type, source_id, message) in [
            (
                "landed_cost",
                "cost-1",
                "The original advanced inventory addon or retained history is required.",
            ),
            (
                "stock_count",
                "cnt-1",
                "Stock counts require the Supply purchase or retained history.",
            ),
            (
                "inventory_transit",
                "tr-1",
                "Warehouse transfers require the Supply purchase or retained history.",
            ),
            (
                "customer_followup",
                "fol-1",
                "The actual quotation or Sales package history is required.",
            ),
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
        seed_grants(&pool, &["supply", "supply-advanced", "sales"]).await;
        for (source_type, source_id, href) in [
            (
                "landed_cost",
                "cost-1",
                "/commerce/orders/ord-p#landed-costs",
            ),
            ("stock_count", "cnt-1", "/stock-counts/cnt-1"),
            ("inventory_transit", "tr-1", "/stock-transit/tr-1"),
            (
                "customer_followup",
                "fol-1",
                "/customer-followups/fol-1",
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
        let mut delegate = InventoryInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, InventoryInboxTargets::new(None, None));
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
                            InventoryInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "landed_cost", "cost-1", 0)
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
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM inventory_landed_costs WHERE company_id='co'), \
             (SELECT count(*) FROM inventory_counts WHERE company_id='co'), \
             (SELECT count(*) FROM inventory_transit WHERE company_id='co'), \
             (SELECT count(*) FROM customer_followups WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (2, 1, 2, 3));
    })
    .await;
}

async fn seed_initial(pool: &DatabasePool) {
    sqlx::raw_sql(
        "UPDATE finance_accounts SET currency='THB' WHERE id='acc-1'; \
         INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,status,created_at,updated_at) VALUES \
         ('acc-var','co','ch-1','5400','Variance','expense','THB','none','active',1,1), \
         ('ar','co','ch-1','110','AR','asset','THB','receivable','active',1,1), \
         ('ap','co','ch-1','210','AP','liability','THB','payable','active',1,1), \
         ('rev','co','ch-1','410','Revenue','revenue','THB','none','active',1,1), \
         ('inv','co','ch-1','120','Inventory','asset','THB','none','active',1,1), \
         ('clr','co','ch-1','220','Clearing','liability','THB','none','active',1,1), \
         ('cogs','co','ch-1','510','COGS','expense','THB','none','active',1,1), \
         ('in_tax','co','ch-1','130','Input tax','asset','THB','none','active',1,1), \
         ('out_tax','co','ch-1','230','Output tax','liability','THB','none','active',1,1), \
         ('cash','co','ch-1','140','Cash','asset','THB','bank','active',1,1); \
         UPDATE commerce_settings SET receivable_account_id='ar',payable_account_id='ap',revenue_account_id='rev',inventory_account_id='inv',clearing_account_id='clr',cogs_account_id='cogs',input_tax_account_id='in_tax',output_tax_account_id='out_tax',cash_account_id='cash' WHERE company_id='co'; \
         INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps,version,accounting_prepared_by,accounting_approved_by) VALUES \
         ('co',true,'THB','Asia/Bangkok','{\"chartId\":\"ch-1\",\"inventory\":\"inv\"}',0,1,'maker','user-2'); \
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('reg-pos-1','co','br','wh-1','POS','retail',true,1); \
         INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,created_by,prepared_by,created_at,updated_at) VALUES \
         ('count-init-sales','co','br','wh-1','C-S','2026-11-02','Initial','draft','maker','maker',1,1), \
         ('count-init-mfg','co','br','wh-1','C-M','2026-11-02','Initial','draft','maker','maker',1,1), \
         ('count-init-pos','co','br','wh-1','C-P','2026-11-02','Initial','draft','maker','maker',1,1), \
         ('count-init-svc','co','br','wh-1','C-V','2026-11-02','Initial','draft','maker','maker',1,1), \
         ('count-init-broken','co','br','wh-1','C-B','2026-11-02','Initial','draft','maker','maker',1,1); \
         INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,ordinal) VALUES \
         ('line-init-sales','count-init-sales','prod-1',1,'Widget','each',0,0,0,0,0,0), \
         ('line-init-mfg','count-init-mfg','prod-1',1,'Widget','each',0,0,0,0,0,0), \
         ('line-init-pos','count-init-pos','prod-1',1,'Widget','each',0,0,0,0,0,0), \
         ('line-init-svc','count-init-svc','prod-1',1,'Widget','each',0,0,0,0,0,0), \
         ('line-init-broken','count-init-broken','prod-1',1,'Widget','each',0,0,0,0,0,0); \
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES \
         ('init-sales','inventory.count.create','maker','req-init-sales','count-init-sales','{\"id\":\"count-init-sales\",\"status\":\"draft\",\"version\":\"1\",\"_initialPhysicalStockCount\":{\"kind\":\"initial-physical-stock-count\",\"countId\":\"count-init-sales\",\"companyId\":\"co\",\"branchId\":\"br\",\"warehouseId\":\"wh-1\",\"productId\":\"prod-1\",\"countCommandId\":\"init-sales\",\"goodsSetupCommandId\":\"goods-sales-1\",\"goodsSetupRequestHash\":\"hash-sales-1\",\"chartId\":\"ch-1\",\"inventoryAccountId\":\"inv\",\"varianceAccountId\":\"acc-var\",\"currency\":\"THB\"}}',1,'{\"companyId\":\"co\"}'), \
         ('goods-sales-1','commerce.sales-goods-posting-setup.initialize','maker','hash-sales-1','co','{\"id\":\"co\",\"status\":\"configured\",\"version\":\"1\"}',1,'{\"companyId\":\"co\"}'), \
         ('init-mfg','inventory.count.create','maker','req-init-mfg','count-init-mfg','{\"id\":\"count-init-mfg\",\"status\":\"draft\",\"version\":\"1\",\"_manufacturingInitialPhysicalStockCount\":{\"kind\":\"manufacturing-initial-physical-stock-count\",\"countId\":\"count-init-mfg\",\"companyId\":\"co\",\"branchId\":\"br\",\"warehouseId\":\"wh-1\",\"productId\":\"prod-1\",\"countCommandId\":\"init-mfg\",\"manufacturingSetupCommandId\":\"goods-mfg-1\",\"manufacturingSetupRequestHash\":\"hash-mfg-1\",\"chartId\":\"ch-1\",\"inventoryAccountId\":\"inv\",\"varianceAccountId\":\"acc-var\",\"currency\":\"THB\"}}',1,'{\"companyId\":\"co\"}'), \
         ('goods-mfg-1','commerce.manufacturing-posting-setup.initialize','maker','hash-mfg-1','co','{\"id\":\"co\",\"status\":\"configured\",\"version\":\"1\"}',1,'{\"companyId\":\"co\"}'), \
         ('goods-pos-1','commerce.pos-posting-setup.initialize','maker','hash-pos-1','co','{\"id\":\"co\",\"status\":\"configured\",\"version\":\"1\"}',1,'{\"companyId\":\"co\"}'); \
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'goods-svc-1','commerce.service-parts-posting-setup.initialize','maker','hash-svc-1','co',jsonb_build_object('id','co','status','configured','version','1','_servicePartsPostingSetup',jsonb_build_object('kind','service-parts-posting-setup','companyId','co','commandId','goods-svc-1','mode','initialize','mapping',to_jsonb(s)))::text,1,'{\"companyId\":\"co\"}' FROM commerce_settings s WHERE s.company_id='co'; \
         INSERT INTO commerce_service_parts_posting_sources(company_id,command_id,mode,mapping_json,proof_json,created_by,created_at) SELECT 'co','goods-svc-1','initialize',to_jsonb(s),jsonb_build_object('kind','service-parts-posting-setup','companyId','co','commandId','goods-svc-1','mode','initialize','mapping',to_jsonb(s)),'maker',1 FROM commerce_settings s WHERE s.company_id='co'; \
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES \
         ('init-svc','inventory.count.create','maker','req-init-svc','count-init-svc','{\"id\":\"count-init-svc\",\"status\":\"draft\",\"version\":\"1\",\"_serviceInitialPhysicalStockCount\":{\"kind\":\"service-initial-physical-stock-count\",\"countId\":\"count-init-svc\",\"companyId\":\"co\",\"branchId\":\"br\",\"warehouseId\":\"wh-1\",\"productId\":\"prod-1\",\"countCommandId\":\"init-svc\",\"serviceSetupCommandId\":\"goods-svc-1\",\"serviceSetupRequestHash\":\"hash-svc-1\",\"chartId\":\"ch-1\",\"inventoryAccountId\":\"inv\",\"varianceAccountId\":\"acc-var\",\"currency\":\"THB\"}}',1,'{\"companyId\":\"co\"}'), \
         ('init-broken','inventory.count.create','maker','req-init-broken','count-init-broken','{\"id\":\"count-init-broken\",\"status\":\"draft\",\"version\":\"1\"}',1,'{\"companyId\":\"co\"}'); \
         INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) VALUES \
         ('pos-boot-1','co','bh-1','{\"id\":\"co\",\"registerId\":\"reg-pos-1\",\"warehouseId\":\"wh-1\"}',1), \
         ('pos-appr-1','co','ah-1','{\"id\":\"co\",\"state\":\"approved\"}',1); \
         INSERT INTO pos_audit(id,company_id,actor_id,operation,record_id,details_json,created_at) VALUES \
         ('pos-boot-audit-1','co','maker','settings.bootstrap','co','{\"result\":{\"id\":\"co\",\"registerId\":\"reg-pos-1\",\"warehouseId\":\"wh-1\"}}',1), \
         ('pos-appr-audit-1','co','user-2','accounting.approve','co','{\"result\":{\"id\":\"co\",\"state\":\"approved\"}}',1); \
         INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'init-pos','inventory.count.create','maker','req-init-pos','count-init-pos',jsonb_build_object('id','count-init-pos','status','draft','version','1','_posInitialPhysicalStockCount',jsonb_build_object('kind','pos-initial-physical-stock-count','countId','count-init-pos','companyId','co','branchId','br','warehouseId','wh-1','productId','prod-1','countCommandId','init-pos','posSetupCommandId','goods-pos-1','posSetupRequestHash','hash-pos-1','posBootstrapReceiptId','pos-boot-1','posBootstrapRequestHash','bh-1','posBootstrapAuditId','pos-boot-audit-1','posApprovalReceiptId','pos-appr-1','posApprovalRequestHash','ah-1','posApprovalAuditId','pos-appr-audit-1','posAccountingImageHash',(SELECT encode(sha256(convert_to(jsonb_build_object('currency',s.currency,'accounts',s.accounts_json::jsonb,'preparedBy',s.accounting_prepared_by,'approvedBy',s.accounting_approved_by,'register',to_jsonb(r))::text,'UTF8')),'hex') FROM pos_settings s JOIN pos_registers r ON r.company_id=s.company_id AND r.active WHERE s.company_id='co' AND r.id='reg-pos-1'),'chartId','ch-1','inventoryAccountId','inv','varianceAccountId','acc-var','currency','THB'))::text,1,'{\"companyId\":\"co\"}'; \
         INSERT INTO inventory_initial_physical_stock_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,goods_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) SELECT 'count-init-sales','co','br','wh-1','prod-1','init-sales','goods-sales-1','ch-1','inv','acc-var','THB',result_json::jsonb->'_initialPhysicalStockCount','maker',1 FROM command_receipts WHERE command_id='init-sales'; \
         INSERT INTO inventory_mfg_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,manufacturing_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) SELECT 'count-init-mfg','co','br','wh-1','prod-1','init-mfg','goods-mfg-1','ch-1','inv','acc-var','THB',result_json::jsonb->'_manufacturingInitialPhysicalStockCount','maker',1 FROM command_receipts WHERE command_id='init-mfg'; \
         INSERT INTO inventory_service_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,service_setup_command_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) SELECT 'count-init-svc','co','br','wh-1','prod-1','init-svc','goods-svc-1','ch-1','inv','acc-var','THB',result_json::jsonb->'_serviceInitialPhysicalStockCount','maker',1 FROM command_receipts WHERE command_id='init-svc'; \
         INSERT INTO inventory_pos_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,pos_setup_command_id,pos_bootstrap_receipt_id,pos_approval_receipt_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) SELECT 'count-init-pos','co','br','wh-1','prod-1','init-pos','goods-pos-1','pos-boot-1','pos-appr-1','ch-1','inv','acc-var','THB',result_json::jsonb->'_posInitialPhysicalStockCount','maker',1 FROM command_receipts WHERE command_id='init-pos'",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn ordinary_counts_keep_the_base_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_initial(&pool).await;
        seed_grants(
            &pool,
            &["sales", "manufacturing", "pos", "service", "service-parts-assets"],
        )
        .await;
        // The ordinary count keeps its base href even with full grants.
        let target = visible(pool.clone(), "user-1", "stock_count", "cnt-1")
            .await
            .expect("ordinary count must resolve");
        assert_eq!(target.href.as_deref(), Some("/stock-counts/cnt-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn unpurchased_initial_marks_keep_the_base_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_initial(&pool).await;
        // Disabled mode without grants: the mode-independent purchase
        // filter collapses the override, exactly like the oracle.
        for source_id in [
            "count-init-sales",
            "count-init-mfg",
            "count-init-pos",
            "count-init-svc",
        ] {
            let target = visible(pool.clone(), "user-1", "stock_count", source_id)
                .await
                .unwrap_or_else(|| panic!("marked {source_id} must still resolve"));
            assert_eq!(
                target.href.as_deref(),
                Some(format!("/stock-counts/{source_id}").as_str()),
                "{source_id}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn initial_counts_override_with_kind_landings() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_initial(&pool).await;
        seed_grants(
            &pool,
            &["sales", "manufacturing", "pos", "service", "service-parts-assets"],
        )
        .await;
        for (source_id, href) in [
            (
                "count-init-sales",
                "/customers-sales/sales/settings/initial-stock/count-init-sales",
            ),
            (
                "count-init-mfg",
                "/production/manufacturing/settings/initial-stock/count-init-mfg",
            ),
            (
                "count-init-pos",
                "/pos/settings/initial-stock/count-init-pos",
            ),
            (
                "count-init-svc",
                "/services/field-service/settings/initial-stock/count-init-svc",
            ),
        ] {
            let target = visible(pool.clone(), "user-1", "stock_count", source_id)
                .await
                .unwrap_or_else(|| panic!("initial {source_id} must resolve"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_id}");
            assert_eq!(target.source, None, "{source_id}");
        }
        // The branch frame still scopes the rewrite.
        let target = framed(
            pool.clone(),
            "user-1",
            "stock_count",
            "count-init-sales",
            Some("br"),
        )
        .await
        .unwrap()
        .expect("framed initial count must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/customers-sales/sales/settings/initial-stock/count-init-sales")
        );
    })
    .await;
}

#[tokio::test]
async fn broken_initial_proof_keeps_the_base_href() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_initial(&pool).await;
        seed_grants(&pool, &["sales"]).await;
        // No initial marker on the receipt: a marked receipt without its
        // reciprocal native source cannot commit (the deferred provenance
        // trigger raises), so the unmarked count exercises the skip path
        // and the href stays at the base address.
        let target = visible(pool.clone(), "user-1", "stock_count", "count-init-broken")
            .await
            .expect("broken marked count must still resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/stock-counts/count-init-broken")
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_initial_counts_override_with_history_grants() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_initial(&pool).await;
        seed_grants(
            &pool,
            &[
                "supply",
                "sales",
                "manufacturing",
                "pos",
                "service",
                "service-parts-assets",
            ],
        )
        .await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        for (source_id, href) in [
            (
                "count-init-sales",
                "/customers-sales/sales/settings/initial-stock/count-init-sales",
            ),
            (
                "count-init-pos",
                "/pos/settings/initial-stock/count-init-pos",
            ),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                "user-1",
                "stock_count",
                source_id,
                None,
                enforced,
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_id} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_id}");
        }
    })
    .await;
}
