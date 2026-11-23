//! Native POS addon package-obligation settlement source projections on isolated PostgreSQL.
//!
//! The addon family (`addon-package-obligations.ts`) owns 57 of the 96 literal
//! obligation SELECTs — the largest of the three obligation read families
//! (addon 57, core 22, pms 17). Its settlement source projections are the
//! authoritative closed-source SQL consumed by Finance/Stock settlement
//! readers; they must be issued by the native owner with Company/package scope
//! taken from the caller's authorized selection, never from caller SQL text.
//!
//! `native_addon_settlement_sources` is the owner-side constructor: it accepts
//! only a validated addon package id and returns parameter-free closed SQL with
//! the package id fixed to the requested addon (the Company scope is applied by
//! the caller's transaction, exactly as the retained reader scopes its lease).
//! The parity test executes the native SQL and the retained TypeScript
//! fragments side by side on the same seeded database and compares row-for-row.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::str::FromStr;

pub const POS_RESTAURANT: &str = "pos-restaurant";
pub const POS_HOTEL_CHARGE: &str = "pos-hotel-charge";

fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED",
        "A registered addon package id is required.",
    )
}

/// The closed PMS projection fragments inlined by the hotel addon, copied
/// byte-identically from `pmsPackageSettlementSources` in
/// `packages/server-pms/src/adapters/package-obligations.ts`. Any change there
/// must be mirrored here; the parity suite compares both paths row-for-row.
const PMS_ROOM_CHARGES: &str =
    "SELECT p.company_id,s.property_id,s.source_key FROM pms_sources s JOIN pms_properties p ON p.id=s.property_id WHERE s.source_key LIKE 'pos:%'";
const PMS_PROPERTIES: &str = "SELECT id,company_id,branch_id,config_json FROM pms_properties";

/// Native owner projections for one addon package. Every value is closed owner
/// SQL with no caller-supplied text; package scope is fixed by the validated
/// constructor, Company scope remains the caller's transaction lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonSettlementSources {
    pub sales: String,
    pub payments: String,
    pub fulfillments: String,
    pub reservations: String,
    pub stock_moves: String,
    pub required_postings: String,
    pub postings: String,
}

/// Owner-side constructor for the addon settlement source family. Fails closed
/// on any package id outside the two registered addons (the retained reader's
/// `POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED` guard, now native).
pub fn native_addon_settlement_sources(package_id: &str) -> Result<AddonSettlementSources, BusinessError> {
    let sales = match package_id {
        POS_RESTAURANT => concat!(
            "SELECT 'pos-restaurant'::text package_id,s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE s.service_mode IN ('dine_in','takeaway') OR r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id WHERE l.sale_id=s.id AND f.kind='recipe')",
        )
        .to_owned(),
        POS_HOTEL_CHARGE => format!(
            concat!(
                "SELECT 'pos-hotel-charge'::text package_id,s.* FROM pos_sales s WHERE EXISTS(\n",
                "    SELECT 1 FROM ({room_charges}) source JOIN ({properties}) p ON p.id=source.property_id AND p.company_id=source.company_id\n",
                "    WHERE source.company_id=s.company_id AND p.branch_id=s.branch_id AND source.source_key='pos:'||s.id||':'||s.id)\n",
                "    OR EXISTS(SELECT 1 FROM pos_room_charge_reversal_requests request WHERE request.company_id=s.company_id AND request.sale_id=s.id)",
            ),
            room_charges = PMS_ROOM_CHARGES,
            properties = PMS_PROPERTIES,
        ),
        _ => return Err(scope_error()),
    };

    let stock_fulfillments = format!(
        concat!(
            "SELECT DISTINCT s.package_id,f.company_id,f.id,f.version,f.state,f.kind,f.cost_minor,f.quantity_milli,f.currency,f.accounts_json FROM ({sales}) s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id",
        ),
        sales = sales,
    );
    let payments = format!(
        "SELECT s.package_id,p.* FROM ({sales}) s JOIN pos_payments p ON p.sale_id=s.id AND p.company_id=s.company_id",
        sales = sales,
    );
    let refunds = format!(
        "SELECT s.package_id,r.* FROM ({sales}) s JOIN pos_refunds r ON r.sale_id=s.id AND r.company_id=s.company_id",
        sales = sales,
    );
    let refund_payments = format!(
        "SELECT r.package_id,r.company_id,r.sale_id,p.* FROM ({refunds}) r JOIN pos_refund_payments p ON p.refund_id=r.id",
        refunds = refunds,
    );
    let posting_keys = format!(
        concat!(
            "SELECT s.package_id,s.company_id,'sale:'||s.id source_key,s.id source_id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'allocate:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT p.package_id,p.company_id,'receipt:'||p.id,s.id,s.currency,s.accounts_json FROM ({payments}) p JOIN ({sales}) s ON s.id=p.sale_id AND s.package_id=p.package_id AND s.company_id=p.company_id\n",
            "  UNION SELECT f.package_id,f.company_id,prefix||f.id,f.id,f.currency,f.accounts_json FROM ({stock_fulfillments}) f CROSS JOIN (VALUES ('consume:'),('deliver:'),('waste:')) source(prefix)\n",
            "  UNION SELECT r.package_id,r.company_id,'refund:'||r.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT r.package_id,r.company_id,'return:'||r.id||':'||l.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN pos_lines l ON l.sale_id=r.sale_id JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id,rp.refund_id,s.currency,s.accounts_json FROM ({refund_payments}) rp JOIN ({sales}) s ON s.id=rp.sale_id AND s.package_id=rp.package_id AND s.company_id=rp.company_id\n",
            "  -- Old cash/shift rows do not capture currency/account policy. Bind their\n",
            "  -- exact native source and memo; never reinterpret history using today's settings.\n",
            "  UNION SELECT s.package_id,s.company_id,'shift:'||s.id,s.id,NULL::text,NULL::text FROM ({shifts}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'cash:'||m.id,s.id,NULL::text,NULL::text FROM ({shifts}) s JOIN pos_cash_movements m ON m.shift_id=s.id",
        ),
        sales = sales,
        payments = payments,
        stock_fulfillments = stock_fulfillments,
        refunds = refunds,
        refund_payments = refund_payments,
        shifts = format!(
            concat!(
                "SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
                "  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM ({sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)",
            ),
            sales = sales,
        ),
    );

    let sales_projection = format!(
        "SELECT package_id,company_id,id,version,state,invoice_id,currency,total_minor,accounts_json FROM ({sales}) source",
        sales = sales,
    );
    let payments_projection = format!(
        "SELECT package_id,company_id,id,sale_id,state,version,finance_payment_id,amount_minor,method FROM ({payments}) source",
        payments = payments,
    );
    let required_postings = format!(
        concat!(
            "SELECT s.package_id,s.company_id,prefix||s.id source_key FROM ({sales}) s CROSS JOIN (VALUES ('sale:'),('allocate:')) source(prefix) WHERE s.invoice_id IS NOT NULL AND s.total_minor>0\n",
            "    UNION SELECT p.package_id,p.company_id,'receipt:'||p.id FROM ({payments}) p WHERE p.state='succeeded' AND p.amount_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'consume:'||f.id FROM ({stock_fulfillments}) f WHERE f.cost_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'deliver:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='delivered' AND f.cost_minor>0\n",
            "    UNION SELECT f.package_id,f.company_id,'waste:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='cancelled' AND f.cost_minor>0\n",
            "    UNION SELECT r.package_id,r.company_id,'refund:'||r.id FROM ({refunds}) r WHERE r.state IN ('processing','completed') AND r.amount_minor>0\n",
            "    UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id FROM ({refund_payments}) rp WHERE rp.state='succeeded'\n",
            "    UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id FROM ({sales}) s JOIN pos_room_charge_reversals r ON r.sale_id=s.id AND r.company_id=s.company_id WHERE r.restored_cost_minor+r.wasted_cost_minor>0",
        ),
        sales = sales,
        payments = payments,
        stock_fulfillments = stock_fulfillments,
        refunds = refunds,
        refund_payments = refund_payments,
    );
    let postings = format!(
        "SELECT source.package_id,source.source_id expected_source_id,source.currency expected_currency,source.accounts_json expected_accounts_json,p.* FROM pos_postings p JOIN ({posting_keys}) source ON source.company_id=p.company_id AND source.source_key=p.source_key",
        posting_keys = posting_keys,
    );

    Ok(AddonSettlementSources {
        sales: sales_projection,
        payments: payments_projection,
        fulfillments: stock_fulfillments.clone(),
        reservations: format!(
            "SELECT f.package_id,f.company_id,r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.version FROM pos_reservations r JOIN ({stock_fulfillments}) f ON f.id=r.fulfillment_id",
            stock_fulfillments = stock_fulfillments,
        ),
        stock_moves: format!(
            "SELECT f.package_id,m.* FROM pos_stock_moves m JOIN ({stock_fulfillments}) f ON f.id=m.fulfillment_id AND f.company_id=m.company_id",
            stock_fulfillments = stock_fulfillments,
        ),
        required_postings,
        postings,
    })
}

/// The retained TypeScript `sourceSql`/`posAddonPackageSettlementSources`
/// strings for `pos-restaurant`, reconstructed verbatim as the parity oracle.
fn legacy_restaurant_sources() -> AddonSettlementSources {
    let sales = concat!(
        "SELECT 'pos-restaurant'::text package_id,s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
        "  WHERE s.service_mode IN ('dine_in','takeaway') OR r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id WHERE l.sale_id=s.id AND f.kind='recipe')",
    )
    .to_owned();
    let stock_fulfillments = format!(
        "SELECT DISTINCT s.package_id,f.company_id,f.id,f.version,f.state,f.kind,f.cost_minor,f.quantity_milli,f.currency,f.accounts_json FROM ({sales}) s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id",
        sales = sales,
    );
    let payments = format!(
        "SELECT s.package_id,p.* FROM ({sales}) s JOIN pos_payments p ON p.sale_id=s.id AND p.company_id=s.company_id",
        sales = sales,
    );
    let refunds = format!(
        "SELECT s.package_id,r.* FROM ({sales}) s JOIN pos_refunds r ON r.sale_id=s.id AND r.company_id=s.company_id",
        sales = sales,
    );
    let refund_payments = format!(
        "SELECT r.package_id,r.company_id,r.sale_id,p.* FROM ({refunds}) r JOIN pos_refund_payments p ON p.refund_id=r.id",
        refunds = refunds,
    );
    let shifts = format!(
        concat!(
            "SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM ({sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)",
        ),
        sales = sales,
    );
    let posting_keys = format!(
        concat!(
            "SELECT s.package_id,s.company_id,'sale:'||s.id source_key,s.id source_id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'allocate:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id,s.id,s.currency,s.accounts_json FROM ({sales}) s\n",
            "  UNION SELECT p.package_id,p.company_id,'receipt:'||p.id,s.id,s.currency,s.accounts_json FROM ({payments}) p JOIN ({sales}) s ON s.id=p.sale_id AND s.package_id=p.package_id AND s.company_id=p.company_id\n",
            "  UNION SELECT f.package_id,f.company_id,prefix||f.id,f.id,f.currency,f.accounts_json FROM ({stock_fulfillments}) f CROSS JOIN (VALUES ('consume:'),('deliver:'),('waste:')) source(prefix)\n",
            "  UNION SELECT r.package_id,r.company_id,'refund:'||r.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT r.package_id,r.company_id,'return:'||r.id||':'||l.id,r.id,s.currency,s.accounts_json FROM ({refunds}) r JOIN pos_lines l ON l.sale_id=r.sale_id JOIN ({sales}) s ON s.id=r.sale_id AND s.package_id=r.package_id AND s.company_id=r.company_id\n",
            "  UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id,rp.refund_id,s.currency,s.accounts_json FROM ({refund_payments}) rp JOIN ({sales}) s ON s.id=rp.sale_id AND s.package_id=rp.package_id AND s.company_id=rp.company_id\n",
            "  -- Old cash/shift rows do not capture currency/account policy. Bind their\n",
            "  -- exact native source and memo; never reinterpret history using today's settings.\n",
            "  UNION SELECT s.package_id,s.company_id,'shift:'||s.id,s.id,NULL::text,NULL::text FROM ({shifts}) s\n",
            "  UNION SELECT s.package_id,s.company_id,'cash:'||m.id,s.id,NULL::text,NULL::text FROM ({shifts}) s JOIN pos_cash_movements m ON m.shift_id=s.id",
        ),
        sales = sales,
        payments = payments,
        stock_fulfillments = stock_fulfillments,
        refunds = refunds,
        refund_payments = refund_payments,
        shifts = shifts,
    );
    AddonSettlementSources {
        sales: format!("SELECT package_id,company_id,id,version,state,invoice_id,currency,total_minor,accounts_json FROM ({sales}) source", sales = sales),
        payments: format!("SELECT package_id,company_id,id,sale_id,state,version,finance_payment_id,amount_minor,method FROM ({payments}) source", payments = payments),
        fulfillments: stock_fulfillments.clone(),
        reservations: format!("SELECT f.package_id,f.company_id,r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.version FROM pos_reservations r JOIN ({stock_fulfillments}) f ON f.id=r.fulfillment_id", stock_fulfillments = stock_fulfillments),
        stock_moves: format!("SELECT f.package_id,m.* FROM pos_stock_moves m JOIN ({stock_fulfillments}) f ON f.id=m.fulfillment_id AND f.company_id=m.company_id", stock_fulfillments = stock_fulfillments),
        required_postings: format!(
            concat!(
                "SELECT s.package_id,s.company_id,prefix||s.id source_key FROM ({sales}) s CROSS JOIN (VALUES ('sale:'),('allocate:')) source(prefix) WHERE s.invoice_id IS NOT NULL AND s.total_minor>0\n",
                "    UNION SELECT p.package_id,p.company_id,'receipt:'||p.id FROM ({payments}) p WHERE p.state='succeeded' AND p.amount_minor>0\n",
                "    UNION SELECT f.package_id,f.company_id,'consume:'||f.id FROM ({stock_fulfillments}) f WHERE f.cost_minor>0\n",
                "    UNION SELECT f.package_id,f.company_id,'deliver:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='delivered' AND f.cost_minor>0\n",
                "    UNION SELECT f.package_id,f.company_id,'waste:'||f.id FROM ({stock_fulfillments}) f WHERE f.kind='recipe' AND f.state='cancelled' AND f.cost_minor>0\n",
                "    UNION SELECT r.package_id,r.company_id,'refund:'||r.id FROM ({refunds}) r WHERE r.state IN ('processing','completed') AND r.amount_minor>0\n",
                "    UNION SELECT rp.package_id,rp.company_id,'refund-payment:'||rp.id FROM ({refund_payments}) rp WHERE rp.state='succeeded'\n",
                "    UNION SELECT s.package_id,s.company_id,'room-reversal:'||s.id FROM ({sales}) s JOIN pos_room_charge_reversals r ON r.sale_id=s.id AND r.company_id=s.company_id WHERE r.restored_cost_minor+r.wasted_cost_minor>0",
            ),
            sales = sales, payments = payments, stock_fulfillments = stock_fulfillments,
            refunds = refunds, refund_payments = refund_payments,
        ),
        postings: format!("SELECT source.package_id,source.source_id expected_source_id,source.currency expected_currency,source.accounts_json expected_accounts_json,p.* FROM pos_postings p JOIN ({posting_keys}) source ON source.company_id=p.company_id AND source.source_key=p.source_key", posting_keys = posting_keys),
    }
}

#[test]
fn native_constructor_fails_closed_on_unregistered_package() {
    for bad in ["", "pos", "pms", "pos-restaurant'; DROP TABLE pos_sales; --"] {
        assert!(native_addon_settlement_sources(bad).is_err(), "package {bad:?} must be rejected");
    }
}

#[test]
fn native_constructor_accepts_both_registered_addons() {
    for ok in [POS_RESTAURANT, POS_HOTEL_CHARGE] {
        let sources = native_addon_settlement_sources(ok).expect("registered addon");
        assert!(sources.sales.contains("package_id"));
        assert!(sources.postings.contains("expected_source_id"));
    }
}

/// Run a closed projection on one seeded database and return one normalised row
/// per result (every column rendered as text, sorted) so the native and
/// retained paths can be compared byte-for-byte.
async fn projection_rows(connection: &mut PgConnection, sql: &str) -> Vec<Vec<Option<String>>> {
    let mut rows = sqlx::query(AssertSqlSafe(sql))
        .fetch_all(&mut *connection)
        .await
        .expect("projection must execute on seeded PostgreSQL");
    let mut normalised: Vec<Vec<Option<String>>> = rows
        .iter_mut()
        .map(|row| {
            (0..row.columns().len())
                .map(|index| row.try_get::<Option<String>, _>(index).unwrap_or(None))
                .collect()
        })
        .collect();
    normalised.sort();
    normalised
}

async fn seed(connection: &mut PgConnection) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,version) VALUES \
         ('register','company','branch','warehouse','Retail','restaurant',true,1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,version) VALUES \
         ('shift','register','company','branch','alice',1,0,'open',1);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,version,accounts_json,service_mode,invoice_id,total_minor) VALUES \
         ('sale-1','company','branch','register','shift','S-1','THB','2027-01-15','alice',1,1,1,'{}','dine_in',NULL,100),\
         ('sale-oc','other','branch','register','shift','S-OC','THB','2027-01-15','alice',1,1,1,'{}','dine_in',NULL,100);\
         INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES \
         ('pay-1','sale-1','company','edc',100,100,'succeeded','alice',1,1);",
    )
    .execute(&mut *connection)
    .await
    .expect("seed must apply on migrated schema");
}

fn with_database<F, Fut>(test: F)
where
    F: FnOnce(PgConnection) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    sqlx::test_block_on(async move {
    use std::sync::atomic::{AtomicU64, Ordering};
    let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
        .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
    let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
    assert_eq!(admin_options.get_host(), "127.0.0.1");
    assert_eq!(admin_options.get_port(), 5432);
    assert_eq!(admin_options.get_database(), Some("postgres"));
    assert_eq!(admin_options.get_username(), "erpx_test_admin");
    let mut admin = PgConnection::connect_with(&admin_options)
        .await
        .expect("test administrator unavailable");
    static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);
    let token = format!(
        "{}{}{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_DATABASE.fetch_add(1, Ordering::SeqCst)
    );
    let name = format!("erpx_dev_rust_pos_obligation_{token}");
    let lease = format!("erpx-rust-pos-obligation-test:{token}");
    sqlx::raw_sql(AssertSqlSafe(format!("CREATE DATABASE \"{name}\" TEMPLATE template0")))
        .execute(&mut admin)
        .await
        .unwrap();
    sqlx::raw_sql(AssertSqlSafe(format!(
        "REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC; COMMENT ON DATABASE \"{name}\" IS '{lease}'"
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    let owned = admin_options.clone().database(&name);
    let mut connection = PgConnection::connect_with(&owned).await.unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = erpx_server_runtime::public::postgres::load_migrations(
        root,
        erpx_server_runtime::public::postgres::MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    erpx_server_runtime::public::postgres::migrate_database(
        &erpx_server_runtime::public::postgres::connect_pool(
            &erpx_server_runtime::public::postgres::DatabaseConfig::from_options(owned).unwrap(),
            erpx_server_runtime::public::postgres::PoolLimits {
                max_connections: 2,
                acquire_timeout: std::time::Duration::from_secs(5),
            },
        )
        .await
        .unwrap(),
        erpx_server_runtime::public::postgres::MigrationTrack::HoldingWrite,
        &files,
    )
    .await
    .unwrap();
    seed(&mut connection).await;
    test(connection).await;
    let ownership = sqlx::query(
        "SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=$1",
    )
    .bind(&name)
    .fetch_one(&mut admin)
    .await
    .unwrap();
    assert_eq!(ownership.get::<String, _>("owner"), "erpx_test_admin");
    assert_eq!(ownership.get::<String, _>("lease"), lease);
    sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE \"{name}\"")))
        .execute(&mut admin)
        .await
        .unwrap();
    admin.close().await.unwrap();
    });
}

#[test]
fn addon_settlement_projections_match_retained_typescript_on_postgres() {
    with_database(|mut connection| async move {
        let native = native_addon_settlement_sources(POS_RESTAURANT).unwrap();
        let legacy = legacy_restaurant_sources();
        for (name, native_sql, legacy_sql) in [
            ("sales", &native.sales, &legacy.sales),
            ("payments", &native.payments, &legacy.payments),
            ("fulfillments", &native.fulfillments, &legacy.fulfillments),
            ("reservations", &native.reservations, &legacy.reservations),
            ("stock_moves", &native.stock_moves, &legacy.stock_moves),
            ("required_postings", &native.required_postings, &legacy.required_postings),
            ("postings", &native.postings, &legacy.postings),
        ] {
            assert_eq!(
                projection_rows(&mut connection, native_sql).await,
                projection_rows(&mut connection, legacy_sql).await,
                "native {name} projection diverged from the retained reader",
            );
        }
    });
}
