//! Native POS add-on/package obligation read ports on isolated PostgreSQL.
//!
//! The addon family (`addon-package-obligations.ts`) owns 57 of the 96 literal
//! obligation SELECTs — the largest of the three obligation read families
//! (addon 57, core 22, pms 17). Their facts reads now live in the native POS
//! owner registry (`erpx_server_pos::public::obligation_reads`) as four
//! Company/package-scoped statement kinds. The TypeScript bridge sends only a
//! typed kind and positional values; the caller executes on its own unit.
//!
//! The settlement-source projections are owner-owned SQL text consumed by
//! Finance/Inventory/PMS callers that still embed it in their own readers; this
//! oracle compares that owner constructor against the retired TypeScript
//! fragments row-for-row. The facts queries are compared the same way, so an
//! invalid SQL shape or a caller-scope leak cannot pass as "ported".
use erpx_server_pos::public::obligation_reads as mounted;
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::str::FromStr;

const RESTAURANT_KIND: &str = "pos_addon_obligations_restaurant";
const HOTEL_KIND: &str = "pos_addon_obligations_hotel";
const PACKAGE_KIND: &str = "pos_package_obligations";
const PMS_PACKAGE_KIND: &str = "pms_pos_package_obligations";
const KINDS: &[&str] = &[RESTAURANT_KIND, HOTEL_KIND, PACKAGE_KIND, PMS_PACKAGE_KIND];

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. None of these closed statements contains a literal `?` in a
/// Postgres operator position.
fn pg(sql: &str) -> String {
    let mut out = String::new();
    let mut chunks = sql.split('?');
    if let Some(first) = chunks.next() {
        out.push_str(first);
        for (index, chunk) in chunks.enumerate() {
            out.push_str(&format!("${}", index + 1));
            out.push_str(chunk);
        }
    }
    out
}

/// Reconstruct the retired TypeScript `sourceSql` fragments verbatim as the
/// parity oracle. Any change to the inlined PMS fragments there must be
/// mirrored in `obligation_reads`; the row checks below detect divergence.
fn legacy_restaurant_fragments() -> (String, String, String, String, String, String, String) {
    let sales = concat!(
        "SELECT 'pos-restaurant'::text package_id,s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
        "  WHERE s.service_mode IN ('dine_in','takeaway') OR r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id WHERE l.sale_id=s.id AND f.kind='recipe')",
    )
    .to_owned();
    let stock_fulfillments = format!(
        "SELECT DISTINCT s.package_id,f.company_id,f.id,f.version,f.state,f.kind,f.cost_minor,f.quantity_milli,f.currency,f.accounts_json FROM ({sales}) s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id AND f.company_id=s.company_id",
        sales = sales,
    );
    let shifts = format!(
        concat!(
            "SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM ({sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)",
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
    let events = format!(
        concat!(
            "SELECT DISTINCT p.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN ({payments}) p\n",
            "    ON p.company_id=e.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode\n",
            "    WHERE e.payload_json::jsonb#>>'{{metadata,pos_payment_id}}'=p.id\n",
            "  UNION SELECT DISTINCT rp.package_id,e.company_id,e.id,e.applied_at FROM pos_stripe_events e JOIN ({refund_payments}) rp ON rp.company_id=e.company_id\n",
            "    JOIN pos_payments p ON p.id=rp.payment_id AND p.company_id=rp.company_id AND p.stripe_account=e.account_id AND p.livemode=e.livemode\n",
            "    WHERE e.payload_json::jsonb#>>'{{metadata,pos_refund_payment_id}}'=rp.id",
        ),
        payments = payments,
        refund_payments = refund_payments,
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
    (sales, stock_fulfillments, payments, refunds, refund_payments, events, posting_keys)
}

fn legacy_restaurant_settlement_sources() -> mounted::SettlementSources {
    let (sales, stock_fulfillments, payments, _refunds, _refund_payments, _events, posting_keys) = legacy_restaurant_fragments();
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
        refunds = _refunds,
        refund_payments = _refund_payments,
    );
    mounted::SettlementSources {
        sales: format!("SELECT package_id,company_id,id,version,state,invoice_id,currency,total_minor,accounts_json FROM ({sales}) source", sales = sales),
        payments: format!("SELECT package_id,company_id,id,sale_id,state,version,finance_payment_id,amount_minor,method FROM ({payments}) source", payments = payments),
        fulfillments: stock_fulfillments.clone(),
        reservations: format!("SELECT f.package_id,f.company_id,r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.version FROM pos_reservations r JOIN ({stock_fulfillments}) f ON f.id=r.fulfillment_id", stock_fulfillments = stock_fulfillments),
        stock_moves: format!("SELECT f.package_id,m.* FROM pos_stock_moves m JOIN ({stock_fulfillments}) f ON f.id=m.fulfillment_id AND f.company_id=m.company_id", stock_fulfillments = stock_fulfillments),
        required_postings,
        postings: format!("SELECT source.package_id,source.source_id expected_source_id,source.currency expected_currency,source.accounts_json expected_accounts_json,p.* FROM pos_postings p JOIN ({posting_keys}) source ON source.company_id=p.company_id AND source.source_key=p.source_key", posting_keys = posting_keys),
    }
}

fn legacy_restaurant_facts() -> String {
    let (sales, stock_fulfillments, payments, refunds, refund_payments, events, _posting_keys) = legacy_restaurant_fragments();
    let shifts = format!(
        concat!(
            "SELECT DISTINCT 'pos-restaurant'::text package_id,s.* FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=s.company_id\n",
            "  WHERE r.mode='restaurant' OR EXISTS(SELECT 1 FROM ({sales}) sale WHERE sale.package_id='pos-restaurant' AND sale.company_id=s.company_id AND sale.shift_id=s.id)",
        ),
        sales = sales,
    );
    format!(
        concat!(
            "WITH scope AS (SELECT ?::text company_id,?::text package_id),\n",
            "  sales AS MATERIALIZED (SELECT source.* FROM ({sales}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  fulfillments AS MATERIALIZED (SELECT source.* FROM ({stock_fulfillments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  payments AS MATERIALIZED (SELECT source.* FROM ({payments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  refunds AS MATERIALIZED (SELECT source.* FROM ({refunds}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  payouts AS MATERIALIZED (SELECT source.* FROM ({refund_payments}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id),\n",
            "  callbacks AS MATERIALIZED (SELECT source.* FROM ({events}) source,scope WHERE source.company_id=scope.company_id AND source.package_id=scope.package_id)\n",
            "  SELECT 'sale' kind,s.id,s.version,s.state FROM sales s WHERE s.state NOT IN ('completed','cancelled','merged')\n",
            "  UNION ALL SELECT 'shift',s.id,s.version,s.state FROM ({shifts}) s,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND s.state<>'reviewed'\n",
            "  UNION ALL SELECT 'cash',m.id,0,'unapproved' FROM pos_cash_movements m JOIN ({shifts}) s ON s.id=m.shift_id,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id AND m.approved_by IS NULL\n",
            "  UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM fulfillments f WHERE f.kind='recipe' AND f.state NOT IN ('delivered','cancelled')\n",
            "  UNION ALL SELECT 'payment',p.id,p.version,p.state FROM payments p WHERE p.state IN ('requested','pending','exception')\n",
            "  UNION ALL SELECT 'refund',r.id,r.version,r.state FROM refunds r WHERE r.state NOT IN ('completed','rejected')\n",
            "  UNION ALL SELECT 'refund-payment',p.id,0,p.state FROM payouts p WHERE p.state<>'succeeded'\n",
            "  UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r JOIN sales s ON s.id=r.sale_id AND s.company_id=r.company_id WHERE r.state IN ('requested','pos_approved')\n",
            "  UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM callbacks e WHERE e.applied_at IS NULL\n",
            "  UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed' AND (\n",
            "    j.kind='complete_sale' AND EXISTS(SELECT 1 FROM sales s WHERE s.id=j.record_id)\n",
            "    OR j.kind IN ('checkout','check_payment') AND EXISTS(SELECT 1 FROM payments p WHERE p.id=j.record_id)\n",
            "    OR j.kind='refund' AND EXISTS(SELECT 1 FROM payouts p WHERE p.id=j.record_id)\n",
            "    OR j.kind='event' AND EXISTS(SELECT 1 FROM callbacks e WHERE e.id=j.record_id))\n",
            "  ORDER BY kind,id",
        ),
        sales = sales,
        stock_fulfillments = stock_fulfillments,
        payments = payments,
        refunds = refunds,
        refund_payments = refund_payments,
        events = events,
        shifts = shifts,
    )
}

#[test]
fn obligation_kinds_are_mounted_select_only_and_match_owner_sql() {
    assert_eq!(mounted::KINDS, KINDS);
    for kind in KINDS {
        assert!(mounted::supported(kind), "obligation kind must be mounted: {kind}");
        let stmt = mounted::sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
        assert!(stmt.starts_with("WITH scope AS (SELECT "), "kind must be a scoped read: {kind}");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
        }
    }
    assert_eq!(mounted::sql(RESTAURANT_KIND).unwrap(), legacy_restaurant_facts(), "restaurant facts SQL drifted from the retired reader");
    assert!(mounted::sql("pos_obligation_unlisted").is_none());
    assert!(!mounted::supported("pos_obligation_unlisted"));
}

#[test]
fn settlement_scope_fails_closed_on_unregistered_package() {
    for bad in ["", "pos", "pms", "pos-restaurant'; DROP TABLE pos_sales; --"] {
        assert!(mounted::settlement_sources(bad).is_none(), "package {bad:?} must be rejected");
    }
    for ok in [mounted::POS_RESTAURANT, mounted::POS_HOTEL_CHARGE] {
        let sources = mounted::settlement_sources(ok).expect("registered addon");
        assert!(sources.sales.contains("package_id"));
        assert!(sources.postings.contains("expected_source_id"));
    }
}

/// Run a closed projection on one seeded database and return one normalised row
/// per result (every column rendered as text, sorted) so the native and
/// retained paths can be compared byte-for-byte.
async fn rows(connection: &mut PgConnection, sql: &str, values: &[&str]) -> Vec<Vec<Option<String>>> {
    let wrapped = format!("SELECT row_to_json(t)::text FROM ({}) t", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for value in values {
        query = query.bind(*value);
    }
    let found = query.fetch_all(&mut *connection).await.expect("projection must execute on seeded PostgreSQL");
    let mut normalised: Vec<Vec<Option<String>>> = found
        .iter()
        .map(|row| serde_json::from_str::<Value>(&row.try_get::<String, _>(0).unwrap()).unwrap())
        .map(|value| match value {
            Value::Object(map) => map.values().map(|v| Some(v.to_string())).collect(),
            _ => vec![Some(value.to_string())],
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
        let native = mounted::settlement_sources(mounted::POS_RESTAURANT).unwrap();
        let legacy = legacy_restaurant_settlement_sources();
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
                rows(&mut connection, native_sql, &[]).await,
                rows(&mut connection, legacy_sql, &[]).await,
                "native {name} projection diverged from the retained reader",
            );
        }
    });
}

#[test]
fn addon_facts_match_retained_typescript_on_postgres() {
    with_database(|mut connection| async move {
        let native = mounted::sql(RESTAURANT_KIND).unwrap();
        let legacy = legacy_restaurant_facts();
        assert_eq!(
            rows(&mut connection, native, &["company", "pos-restaurant"]).await,
            rows(&mut connection, &legacy, &["company", "pos-restaurant"]).await,
            "native addon facts diverged from the retained reader",
        );
        // Cross-company package scope never leaks rows from another Holding.
        assert!(rows(&mut connection, native, &["other", "pos-restaurant"]).await.is_empty());
    });
}
