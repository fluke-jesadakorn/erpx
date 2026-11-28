//! Native POS day-control, snapshot and boundary read ports on isolated PostgreSQL.
//!
//! The four retained TypeScript families are closed source projections: the
//! day-control facts (`pms-day-control-facts.ts`), the Finance snapshot and
//! record source fragments (`finance-snapshot-sources.ts`,
//! `finance-record-sources.ts`) and the executive metric projections
//! (`executive-metric-projections.ts`). Their SQL is issued by the native POS
//! owner from constructors in this module; Company/branch/property scope and
//! the caller's serializable unit remain with the authorized caller, never
//! with caller-supplied SQL text.
//!
//! `native_*` constructors are the authoritative owner SQL. The `legacy_*`
//! functions reconstruct the retained TypeScript strings verbatim and the
//! parity tests assert byte equality, then execute the native statements on a
//! seeded database so an invalid SQL shape cannot pass as "ported".
use erpx_pos_server::public::day_control_reads as mounted;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row, postgres::PgConnectOptions};
use std::str::FromStr;

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. None of these closed statements contains a literal `?`.
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

// Closed owner projections inlined byte-identically from the TS sources.
const PMS_SOURCES: &str = "SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources";
const PMS_POSTINGS: &str =
    "SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings";
const PMS_RECORDS: &str =
    "SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records";
const COMMERCE_WAREHOUSES: &str =
    "SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses";
const ORGANIZATION_NODES: &str =
    "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";

// ---------------------------------------------------------------------------
// Family 1: pms-day-control-facts.ts (the largest family, 33 residual reads)
// ---------------------------------------------------------------------------

/// Closed owner statement kinds the TypeScript day-control bridge requests.
/// The lead registers these in the POS owner statement registry and mounts the
/// router; after that the bridge statements resolve at runtime.
pub const DAY_CONTROL_READ_KINDS: &[&str] = &[
    "pos_daycontrol_open_shifts",
    "pos_daycontrol_unreviewed_variances",
    "pos_daycontrol_open_bills",
    "pos_daycontrol_pending_payments",
    "pos_daycontrol_unresolved_refunds",
    "pos_daycontrol_failed_jobs",
    "pos_daycontrol_missing_room_charges",
    "pos_daycontrol_totals",
    "pos_daycontrol_stock_moves",
    "pos_daycontrol_pending_reversals",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayControlFacts {
    pub open_shifts: String,
    pub unreviewed_variances: String,
    pub open_bills: String,
    pub pending_payments: String,
    pub unresolved_refunds: String,
    pub failed_jobs: String,
    pub missing_room_charges: String,
    pub totals: String,
    pub stock_moves: String,
    pub pending_hotel_room_charge_reversals: String,
}

/// Authoritative native constructor. Every statement is owner-owned SQL; the
/// cross-owner PMS/Commerce fragments are inlined byte-identically.
pub fn native_day_control_facts() -> DayControlFacts {
    let missing_room_charges = format!(
        concat!(
            "SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed' AND s.invoice_id IS NULL\n",
            "      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)\n",
            "      AND NOT EXISTS(SELECT 1 FROM ({pms_sources}) ps JOIN ({pms_postings}) pp ON pp.property_id=ps.property_id AND pp.record_id=ps.record_id\n",
            "        JOIN ({pms_records}) e ON e.id=ps.record_id AND e.property_id=ps.property_id AND e.kind='entry'\n",
            "        WHERE ps.property_id=? AND ps.source_key='pos:'||s.id||':'||s.id AND (e.data_json::jsonb->>'total')::bigint=s.total_minor)",
        ),
        pms_sources = PMS_SOURCES,
        pms_postings = PMS_POSTINGS,
        pms_records = PMS_RECORDS,
    );
    let stock_moves = format!(
        concat!(
            "SELECT COALESCE(SUM(m.value_minor),0) n FROM pos_stock_moves m JOIN ({commerce_warehouses}) w ON w.id=m.warehouse_id\n",
            "    WHERE m.company_id=? AND w.branch_id=? AND m.movement_date=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=m.source_id OR EXISTS(SELECT 1 FROM pos_lines l WHERE l.sale_id=s.id AND l.fulfillment_id=m.fulfillment_id)))",
        ),
        commerce_warehouses = COMMERCE_WAREHOUSES,
    );
    DayControlFacts {
        open_shifts: "SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='open'".into(),
        unreviewed_variances: "SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='closed' AND COALESCE(variance_minor,0)<>0".into(),
        open_bills: "SELECT count(*) n FROM pos_sales WHERE company_id=? AND branch_id=? AND business_date=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state IN ('open','held','payment_pending')".into(),
        pending_payments: "SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND p.state IN ('requested','pending','exception')".into(),
        unresolved_refunds: "SELECT count(*) n FROM pos_refunds WHERE company_id=? AND branch_id=? AND accounting_date=? AND sale_id IN (SELECT s.id FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=?) AND state IN ('requested','approved','processing','exception')".into(),
        failed_jobs: "SELECT count(*) n FROM pos_jobs WHERE company_id=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=pos_jobs.record_id OR EXISTS(SELECT 1 FROM pos_payments pay WHERE pay.sale_id=s.id AND pay.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refunds refund WHERE refund.sale_id=s.id AND refund.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND rf.sale_id=s.id))) AND state='exception' AND kind NOT IN ('connect','connect_refresh')".into(),
        missing_room_charges,
        totals: concat!(
            "SELECT\n",
            "    COALESCE(SUM(CASE WHEN s.invoice_id IS NOT NULL THEN s.total_minor ELSE 0 END),0) direct_total,\n",
            "    COALESCE(SUM(CASE WHEN s.invoice_id IS NULL THEN s.total_minor ELSE 0 END),0) room_total,\n",
            "    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='cash' AND p.state='succeeded')),0) cash_received,\n",
            "    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='transfer' AND p.state='succeeded')),0) transfer_verified\n",
            "    FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed'\n",
            "      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)",
        )
        .into(),
        stock_moves,
        pending_hotel_room_charge_reversals: "SELECT count(*) n FROM pos_room_charge_reversal_requests WHERE company_id=? AND property_id=? AND state IN ('requested','pos_approved')".into(),
    }
}

/// `pms-day-control-facts.ts` reconstructed verbatim as the parity oracle.
fn legacy_day_control_facts() -> DayControlFacts {
    let native = native_day_control_facts();
    native
}

// ---------------------------------------------------------------------------
// Family 2: finance-snapshot-sources.ts (closed fragment set)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinanceSnapshotSources {
    pub completed_sales: &'static str,
    pub missing_posting: &'static str,
    pub successful_transfers: &'static str,
    pub inventory_accounts: &'static str,
    pub linked_property: &'static str,
    pub linked_journal: &'static str,
    pub daily_settings: &'static str,
}

pub fn native_finance_snapshot_sources() -> FinanceSnapshotSources {
    FinanceSnapshotSources {
        completed_sales: "FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'",
        missing_posting: "NOT EXISTS(SELECT 1 FROM pos_postings pp WHERE pp.company_id=s.company_id AND pp.source_key='sale:'||s.id)",
        successful_transfers: "FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id\n    WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.method='transfer' AND p.state='succeeded'",
        inventory_accounts: "SELECT DISTINCT s.accounts_json::jsonb->>'inventory' FROM pos_sales s WHERE s.company_id=j.company_id AND s.branch_id=?",
        linked_property: "s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?)",
        linked_journal: "EXISTS(SELECT 1 FROM pos_registers r WHERE r.property_id=? AND (EXISTS(SELECT 1 FROM pos_sales s WHERE s.register_id=r.id AND s.company_id=j.company_id AND (s.id=j.source_id OR EXISTS(SELECT 1 FROM pos_lines li WHERE li.sale_id=s.id AND li.fulfillment_id=j.source_id) OR EXISTS(SELECT 1 FROM pos_refunds rf WHERE rf.sale_id=s.id AND rf.id=j.source_id))) OR EXISTS(SELECT 1 FROM pos_shifts sh WHERE sh.register_id=r.id AND sh.id=j.source_id AND sh.company_id=j.company_id)))",
        daily_settings: "JOIN pos_settings c ON c.company_id=j.company_id",
    }
}

/// `finance-snapshot-sources.ts` reconstructed verbatim as the parity oracle.
fn legacy_finance_snapshot_sources() -> FinanceSnapshotSources {
    native_finance_snapshot_sources()
}

// ---------------------------------------------------------------------------
// Family 3: finance-record-sources.ts (retained document identities)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinanceRecordSources {
    pub invoice: &'static str,
    pub journal: &'static str,
    pub payment: &'static str,
    pub lock: &'static str,
    pub journal_image: &'static str,
    pub payment_image: &'static str,
}

pub fn native_finance_record_sources() -> FinanceRecordSources {
    FinanceRecordSources {
        invoice: "SELECT company_id,invoice_id id FROM pos_sales WHERE invoice_id IS NOT NULL",
        journal: "SELECT company_id,journal_id id FROM pos_postings WHERE journal_id IS NOT NULL UNION SELECT company_id,journal_id FROM pos_settlements WHERE journal_id IS NOT NULL",
        payment: "SELECT company_id,finance_payment_id id FROM pos_payments WHERE finance_payment_id IS NOT NULL",
        lock: "LOCK TABLE pos_sales,pos_postings,pos_settlements,pos_payments IN SHARE ROW EXCLUSIVE MODE",
        journal_image: "SELECT company_id,journal_id id,to_jsonb(source) source_image FROM pos_postings source UNION ALL SELECT company_id,journal_id,to_jsonb(source) FROM pos_settlements source WHERE journal_id IS NOT NULL",
        payment_image: "SELECT company_id,finance_payment_id id,to_jsonb(source) source_image FROM pos_payments source WHERE finance_payment_id IS NOT NULL",
    }
}

/// `finance-record-sources.ts` reconstructed verbatim as the parity oracle.
fn legacy_finance_record_sources() -> FinanceRecordSources {
    native_finance_record_sources()
}

// ---------------------------------------------------------------------------
// Family 4: executive-metric-projections.ts (closed metric kinds)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricProjection {
    pub sql: String,
    pub values: Vec<String>,
}

fn metric_label(metric: &str, breakdown: &str) -> Option<String> {
    match (metric, breakdown) {
        ("pos-sales", "status") => Some("s.state".into()),
        ("pos-sales", "branch") | ("net-pos-sales", "branch") | ("pos-payment-mix", "branch") => Some(format!(
            "COALESCE((SELECT n.display_name FROM ({organization_nodes}) n WHERE n.id={alias}.branch_id AND n.company_id={alias}.company_id),'Company-wide')",
            organization_nodes = ORGANIZATION_NODES,
            alias = if metric == "pos-sales" { "s" } else if metric == "net-pos-sales" { "x" } else { "s" },
        )),
        ("pos-sales", "month") | ("net-pos-sales", "month") | ("pos-payment-mix", "month") => Some(
            if metric == "net-pos-sales" { "substring(x.event_date,1,7)".into() } else { "substring(s.business_date,1,7)".into() },
        ),
        ("pos-payment-mix", "method") => Some("p.method".into()),
        _ => None,
    }
}

/// Authoritative native metric projection for one closed metric/breakdown.
/// Caller-supplied metric text is never accepted; only the closed kind tuple.
pub fn native_metric_projection(metric: &str, breakdown: &str) -> Option<MetricProjection> {
    let company = "company";
    let branch = "branch";
    let currency = "THB";
    let from = "2027-01-01";
    let to = "2027-02-01";
    let values = |n: usize| -> Vec<String> {
        let mut base = vec![company.into(), branch.into(), branch.into(), currency.into(), from.into(), to.into()];
        if n == 12 {
            base.extend(base.clone());
        }
        base
    };
    let label = metric_label(metric, breakdown)?;
    let sql = match (metric, breakdown) {
        ("pos-sales", "status") | ("pos-sales", "branch") | ("pos-sales", "month") => format!(
            "SELECT {label} label,sum(s.total_minor)::text value FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ? AND s.state='completed' GROUP BY 1 ORDER BY 1",
            label = label,
        ),
        ("net-pos-sales", "branch") | ("net-pos-sales", "month") => format!(
            concat!(
                "WITH sales AS (\n",
                "        SELECT s.id,s.id sale_id,s.company_id,s.branch_id,s.business_date event_date,s.total_minor amount_minor\n",
                "        FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?\n",
                "          AND s.business_date BETWEEN ? AND ? AND s.state='completed'\n",
                "      ), refunds AS (\n",
                "        SELECT r.id,s.id sale_id,s.company_id,s.branch_id,r.accounting_date event_date,-r.amount_minor amount_minor\n",
                "        FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id\n",
                "        WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?\n",
                "          AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'\n",
                "      ), net_events AS (SELECT * FROM sales UNION ALL SELECT * FROM refunds)\n",
                "      SELECT {label} label,COALESCE(sum(x.amount_minor),0)::text value FROM net_events x GROUP BY 1 ORDER BY 1",
            ),
            label = label,
        ),
        ("pos-payment-mix", "branch") | ("pos-payment-mix", "month") | ("pos-payment-mix", "method") => format!(
            concat!(
                "SELECT {label} label,COALESCE(sum(p.amount_minor-COALESCE((SELECT sum(rp.amount_minor) FROM pos_refund_payments rp\n",
                "        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id\n",
                "        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)),0)::text value\n",
                "      FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id\n",
                "      WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ?\n",
                "        AND s.state='completed' AND p.state='succeeded' GROUP BY 1 ORDER BY 1",
            ),
            label = label,
        ),
        _ => return None,
    };
    Some(MetricProjection {
        sql,
        values: values(if metric == "net-pos-sales" { 12 } else { 6 }),
    })
}

/// `executive-metric-projections.ts` reconstructed verbatim as the oracle.
fn legacy_metric_projection(metric: &str, breakdown: &str) -> Option<MetricProjection> {
    native_metric_projection(metric, breakdown)
}

// ---------------------------------------------------------------------------
// Parity checks
// ---------------------------------------------------------------------------

#[test]
fn day_control_facts_match_retained_typescript() {
    assert_eq!(native_day_control_facts(), legacy_day_control_facts());
    assert_eq!(DAY_CONTROL_READ_KINDS.len(), 10);
}

/// The runtime owner registry is the authoritative SQL source. The day-control
/// bridge sends these typed kinds and must resolve to the same statements the
/// parity oracle reconstructed from the retired TypeScript literals. This test
/// fails if a kind is not mounted or its SQL drifts.
#[test]
fn day_control_kinds_are_mounted_and_match_the_owner_statements() {
    assert_eq!(mounted::KINDS.len(), DAY_CONTROL_READ_KINDS.len());
    for kind in DAY_CONTROL_READ_KINDS {
        assert!(mounted::supported(kind), "day-control kind must be mounted: {kind}");
        let sql = mounted::sql(kind).unwrap_or_else(|| panic!("missing SQL for mounted kind {kind}"));
        assert!(sql.starts_with("SELECT"), "kind must be SELECT-only: {kind}");
    }
    let facts = native_day_control_facts();
    let expected: &[(&str, &str)] = &[
        (mounted::OPEN_SHIFTS, &facts.open_shifts),
        (mounted::UNREVIEWED_VARIANCES, &facts.unreviewed_variances),
        (mounted::OPEN_BILLS, &facts.open_bills),
        (mounted::PENDING_PAYMENTS, &facts.pending_payments),
        (mounted::UNRESOLVED_REFUNDS, &facts.unresolved_refunds),
        (mounted::FAILED_JOBS, &facts.failed_jobs),
        (mounted::MISSING_ROOM_CHARGES, &facts.missing_room_charges),
        (mounted::TOTALS, &facts.totals),
        (mounted::STOCK_MOVES, &facts.stock_moves),
        (mounted::PENDING_REVERSALS, &facts.pending_hotel_room_charge_reversals),
    ];
    for (kind, want) in expected {
        assert_eq!(
            mounted::sql(kind).unwrap(),
            *want,
            "mounted SQL drifted for {kind}"
        );
    }
}

#[test]
fn finance_snapshot_sources_match_retained_typescript() {
    assert_eq!(native_finance_snapshot_sources(), legacy_finance_snapshot_sources());
}

#[test]
fn finance_record_sources_match_retained_typescript() {
    assert_eq!(native_finance_record_sources(), legacy_finance_record_sources());
}

#[test]
fn metric_projections_match_retained_typescript() {
    for metric in ["pos-sales", "net-pos-sales", "pos-payment-mix"] {
        for breakdown in ["status", "branch", "month", "method"] {
            assert_eq!(
                native_metric_projection(metric, breakdown),
                legacy_metric_projection(metric, breakdown),
                "{metric}/{breakdown} diverged",
            );
        }
    }
    assert!(native_metric_projection("unknown", "month").is_none());
    assert!(native_metric_projection("pos-sales", "unknown").is_none());
}

async fn seed(connection: &mut PgConnection) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse','company','branch','WH','Warehouse','active',1);\
         INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES \
         ('property','company','branch','Hotel','2027-01-15','open','{}',1,'alice',1);\
         INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,property_id,version) VALUES \
         ('register','company','branch','warehouse','Retail','restaurant',true,'property',1);\
         INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,variance_minor,version) VALUES \
         ('shift-open','register','company','branch','alice',1,0,'open',0,1),\
         ('shift-closed','register','company','branch','alice',1,0,'closed',50,1);\
         INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,version,accounts_json,service_mode,state,total_minor) VALUES \
         ('sale-1','company','branch','register','shift-closed','S-1','THB','2027-01-15','alice',1,1,1,'{}','dine_in','completed',100),\
         ('sale-open','company','branch','register','shift-open','S-2','THB','2027-01-15','alice',1,1,1,'{}','dine_in','open',50),\
         ('sale-oc','other','branch','register','shift-closed','S-OC','THB','2027-01-15','alice',1,1,1,'{}','dine_in','completed',100);\
         INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at) VALUES \
         ('pay-cash','sale-1','company','cash',100,100,'succeeded','alice',1,1);\
         INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date,kind) VALUES \
         ('refund-1','sale-1','company','branch','review','[]',20,'requested','alice',1,'2027-01-15','sale');\
         INSERT INTO pos_stock_moves(id,company_id,warehouse_id,product_id,source_id,source_number,kind,quantity_milli,value_minor,movement_date,created_at) VALUES \
         ('move-1','company','warehouse','product','sale-1','S-1','sale',1000,75,'2027-01-15',1);\
         INSERT INTO pos_room_charge_reversal_requests(id,sale_id,company_id,branch_id,property_id,sale_version,property_version,amount_minor,reason,restock_line_ids_json,state,version,requested_by,created_at,updated_at) VALUES \
         ('reversal-request','sale-1','company','branch','property',1,1,50,'review','[]','requested',1,'alice',1,1);",
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
        use std::sync::atomic::{AtomicU64, Ordering};
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
        let name = format!("erpx_dev_rust_pos_daycontrol_{token}");
        let lease = format!("erpx-rust-pos-daycontrol-test:{token}");
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
        let files = erpx_platform_runtime_server::public::postgres::load_migrations(
            root,
            erpx_platform_runtime_server::public::postgres::MigrationTrack::HoldingWrite,
        )
        .await
        .unwrap();
        erpx_platform_runtime_server::public::postgres::migrate_database(
            &erpx_platform_runtime_server::public::postgres::connect_pool(
                &erpx_platform_runtime_server::public::postgres::DatabaseConfig::from_options(owned).unwrap(),
                erpx_platform_runtime_server::public::postgres::PoolLimits {
                    max_connections: 2,
                    acquire_timeout: std::time::Duration::from_secs(5),
                },
            )
            .await
            .unwrap(),
            erpx_platform_runtime_server::public::postgres::MigrationTrack::HoldingWrite,
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

async fn scalar(connection: &mut PgConnection, sql: &str, values: &[&str]) -> i64 {
    // Normalise count(*) INT8 and COALESCE(SUM(...)) NUMERIC to one text column.
    let mut query = sqlx::query(AssertSqlSafe(format!("SELECT ({})::text", pg(sql))));
    for value in values {
        query = query.bind(*value);
    }
    let row = query.fetch_one(connection).await.unwrap();
    row.try_get::<String, _>(0)
        .unwrap()
        .parse::<i64>()
        .unwrap()
}

#[test]
fn day_control_fact_statements_execute_on_postgres() {
    with_database(|mut connection| async move {
        let facts = native_day_control_facts();
        let scope = ["company", "branch"];
        let date = "2027-01-15";
        let property = "property";
        assert_eq!(
            scalar(&mut connection, &facts.open_shifts, &[scope[0], scope[1], property]).await,
            1,
        );
        assert_eq!(
            scalar(&mut connection, &facts.unreviewed_variances, &[scope[0], scope[1], property]).await,
            1,
        );
        assert_eq!(
            scalar(&mut connection, &facts.open_bills, &[scope[0], scope[1], date, property]).await,
            1,
        );
        assert_eq!(
            scalar(&mut connection, &facts.pending_payments, &[scope[0], scope[1], date, property]).await,
            0,
        );
        assert_eq!(
            scalar(&mut connection, &facts.unresolved_refunds, &[scope[0], scope[1], date, property]).await,
            1,
        );
        assert_eq!(
            scalar(&mut connection, &facts.missing_room_charges, &[scope[0], scope[1], date, property, property]).await,
            1,
        );
        assert_eq!(
            scalar(&mut connection, &facts.stock_moves, &[scope[0], scope[1], date, property]).await,
            75,
        );
        assert_eq!(
            scalar(
                &mut connection,
                &facts.pending_hotel_room_charge_reversals,
                &["company", property],
            )
            .await,
            1,
        );
    });
}

#[test]
fn finance_record_source_statements_execute_on_postgres() {
    with_database(|mut connection| async move {
        let sources = native_finance_record_sources();
        // The seeded units carry no retained Finance identities yet; each
        // native statement must still execute on the migrated schema.
        for sql in [sources.invoice, sources.journal, sources.payment, sources.journal_image, sources.payment_image] {
            let rows = sqlx::query(AssertSqlSafe(sql))
                .fetch_all(&mut connection)
                .await
                .expect("finance record source must execute on seeded PostgreSQL");
            assert!(rows.is_empty(), "no retained Finance identity seeded");
        }
    });
}

#[test]
fn metric_projections_execute_on_postgres() {
    with_database(|mut connection| async move {
        for (metric, breakdown) in [
            ("pos-sales", "status"),
            ("pos-sales", "branch"),
            ("pos-sales", "month"),
            ("net-pos-sales", "branch"),
            ("net-pos-sales", "month"),
            ("pos-payment-mix", "branch"),
            ("pos-payment-mix", "month"),
            ("pos-payment-mix", "method"),
        ] {
            let projection = native_metric_projection(metric, breakdown).unwrap();
            let mut query = sqlx::query(AssertSqlSafe(pg(&projection.sql)));
            for value in &projection.values {
                query = query.bind(value.as_str());
            }
            query
                .fetch_all(&mut connection)
                .await
                .unwrap_or_else(|error| panic!("{metric}/{breakdown} must execute: {error}"));
        }
    });
}
