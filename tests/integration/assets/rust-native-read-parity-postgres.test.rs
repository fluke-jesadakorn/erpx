//! Same-transaction frozen-vs-native parity for the Assets field-service reference
//! and package-obligation read tail (`G4 / retire.assets.read-tail-r26`).
//!
//! The TypeScript readers are retired; their statements are frozen byte-exact
//! below and pinned to the registered owner-local native plans. Each frozen
//! statement and the matching plan run on ONE freshly opened transaction over
//! the same fixture rows, including an uncommitted write. Both result
//! projections must match. Nothing commits; cross-owner sources and the owner
//! lock/guard authority stay retained.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_assets::public::native_read as plans;
use erpx_server_assets::public::{bind, ReadScope};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Cross-owner and cutover-guard TypeScript authority, read at compile time.
const TS_FS: &str = include_str!("../../../packages/server-assets/src/adapters/field-service-references.ts");
const TS_PKG: &str = include_str!("../../../packages/server-assets/src/adapters/package-obligations.ts");
const TS_PROBE: &str = include_str!("../../../packages/server-assets/src/adapters/record-purchases.ts");
const TS_PLACEMENTS: &str = include_str!("../../../packages/server-organization/src/adapters/talent-settlement-sources.ts");
const TS_FINANCIAL: &str = include_str!("../../../packages/server-finance/src/adapters/assets-settlement-sources.ts");
const TS_REFS: &str = include_str!("../../../packages/server-maintenance/src/adapters/asset-settlement-refs.ts");
const TS_CONSUMPTIONS: &str = include_str!("../../../packages/server-inventory/src/adapters/maintenance-settlement-sources.ts");

// Verbatim fragments from the cross-owner TypeScript source adapters. Macros
// (not named consts) let `concat!` compose the byte-exact live statements.
macro_rules! placements {
    () => {
        "SELECT id,company_id,branch_id,kind,status FROM organization_nodes"
    };
}
macro_rules! financial {
    () => {
        "SELECT id,company_id,status,currency,cost_minor,residual_minor,accumulated_depreciation_minor,activation_journal_id,version FROM finance_assets"
    };
}
macro_rules! links {
    () => {
        "SELECT * FROM operational_asset_finance_links"
    };
}
macro_rules! consumptions {
    () => {
        "SELECT * FROM operational_inventory_consumptions WHERE source_kind='maintenance'"
    };
}
macro_rules! preventive {
    () => {
        "SELECT company_id,asset_id FROM maintenance_preventive_plans"
    };
}
macro_rules! parts {
    () => {
        concat!(
            "SELECT consumption.company_id,work.asset_id FROM (",
            consumptions!(),
            ") consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id"
        )
    };
}
macro_rules! asset_sources {
    () => {
        concat!(
            "SELECT asset.*,COALESCE(EXISTS(SELECT 1 FROM (",
            placements!(),
            ") branch WHERE branch.id=asset.branch_id AND branch.company_id=asset.company_id AND branch.kind IN ('branch','headquarters')) AND (asset.department_id IS NULL OR EXISTS(SELECT 1 FROM (",
            placements!(),
            ") department WHERE department.id=asset.department_id AND department.company_id=asset.company_id AND department.branch_id=asset.branch_id AND department.kind='department')),false) source_valid FROM operational_assets asset"
        )
    };
}

// Frozen native literals, byte-identical to the retired TypeScript statements.
const FROZEN_FS_ROWS: &str =
    "SELECT id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version FROM operational_assets";
const FROZEN_FS_IMAGE: &str =
    "SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?";

macro_rules! obligations {
    ($refs:expr) => {
        concat!(
            "WITH scope AS(SELECT ?::text company_id,?::boolean is_base),assets AS MATERIALIZED(SELECT asset.* FROM (",
            asset_sources!(),
            ") asset,scope WHERE asset.company_id=scope.company_id AND (scope.is_base OR EXISTS(SELECT 1 FROM (",
            $refs,
            ") source WHERE source.asset_id=asset.id AND source.company_id=asset.company_id)))\n SELECT 'asset' kind,asset.id,asset.version,asset.lifecycle_status state,NOT asset.source_valid OR (SELECT is_base FROM scope) AND (asset.lifecycle_status IN ('maintenance','retirement_pending') OR asset.cost_minor>0 AND NOT EXISTS(SELECT 1 FROM (",
            financial!(),
            ") financial JOIN (",
            links!(),
            ") link ON link.asset_id=asset.id AND link.finance_asset_id=financial.id WHERE financial.id=asset.finance_asset_id AND financial.company_id=asset.company_id AND financial.currency=asset.currency AND financial.cost_minor=asset.cost_minor AND link.cost_minor=asset.cost_minor AND link.currency=asset.currency AND link.asset_version<=asset.version AND link.finance_asset_version<=financial.version AND financial.activation_journal_id IS NOT NULL AND (asset.lifecycle_status='retired' AND financial.status='retired' OR asset.lifecycle_status<>'retired' AND financial.status='in_service'))) FROM assets asset\n UNION ALL SELECT 'asset-link',link.id,link.asset_version,'ambiguous-source',true FROM (",
            links!(),
            ") link JOIN assets asset ON asset.id=link.asset_id LEFT JOIN (",
            financial!(),
            ") financial ON financial.id=link.finance_asset_id AND financial.company_id=asset.company_id WHERE financial.id IS NULL OR link.currency<>asset.currency OR link.cost_minor<>asset.cost_minor OR financial.cost_minor<>link.cost_minor OR financial.currency<>link.currency OR link.asset_version>asset.version OR link.finance_asset_version>financial.version\n ORDER BY kind,id"
        )
    };
}
const FROZEN_OBLIGATIONS_BASE: &str = obligations!(parts!());
const FROZEN_OBLIGATIONS_PREVENTIVE: &str = obligations!(preventive!());
const FROZEN_OBLIGATIONS_PARTS: &str = obligations!(parts!());

const DDL: &str = "\
CREATE TABLE operational_assets(id text primary key,company_id text not null,branch_id text,department_id text,lifecycle_status text,warranty_end text,version bigint,code text,name text,cost_minor bigint,currency text,finance_asset_id text);\
CREATE TABLE operational_asset_finance_links(id text primary key,asset_id text,finance_asset_id text,cost_minor bigint,currency text,asset_version bigint,finance_asset_version bigint);\
CREATE TABLE organization_nodes(id text primary key,company_id text,branch_id text,kind text,status text);\
CREATE TABLE finance_assets(id text primary key,company_id text,status text,currency text,cost_minor bigint,residual_minor bigint,accumulated_depreciation_minor bigint,activation_journal_id text,version bigint);\
CREATE TABLE maintenance_preventive_plans(id text primary key,company_id text,asset_id text);\
CREATE TABLE maintenance_work_orders(id text primary key,company_id text,asset_id text);\
CREATE TABLE operational_inventory_consumptions(id text primary key,company_id text,source_id text,source_kind text);\
INSERT INTO organization_nodes VALUES ('company','company',NULL,'company','active'),('branch','company',NULL,'branch','active'),('dept','company','branch','department','active');\
INSERT INTO operational_assets VALUES ('asset-1','company','branch','dept','in_service',NULL,3,'A-1','Asset One',1000,'THB','finance-1'),('asset-2','company','branch',NULL,'maintenance',NULL,1,'A-2','Asset Two',0,'THB',NULL),('asset-3','company','missing-branch','dept','maintenance',NULL,1,'A-3','Asset Three',0,'THB',NULL),('asset-foreign','foreign','branch','dept','maintenance',NULL,1,'A-F','Foreign',0,'THB',NULL);\
INSERT INTO operational_asset_finance_links VALUES ('link-1','asset-1','finance-1',1000,'THB',3,2),('link-2','asset-2','finance-missing',0,'THB',1,1);\
INSERT INTO finance_assets VALUES ('finance-1','company','in_service','THB',1000,0,0,'journal-1',2);\
INSERT INTO maintenance_preventive_plans VALUES ('plan-1','company','asset-2');\
INSERT INTO maintenance_work_orders VALUES ('work-1','company','asset-1');\
INSERT INTO operational_inventory_consumptions VALUES ('cons-1','company','work-1','maintenance');";

/// Retained cross-owner fragments still match their TypeScript authorities.
#[test]
fn cross_owner_fragments_match_typescript_authority() {
    assert!(TS_PROBE.contains("assetProjectionFromSql(\"1\",` ${from}`,` WHERE ${where}`)"));
    assert!(TS_PKG.contains("${assetSourceExistsSql(placements"));
    assert!(TS_PLACEMENTS.contains(placements!()));
    assert!(TS_FINANCIAL.contains(financial!()));
    assert!(TS_REFS.contains(preventive!()));
    assert!(TS_REFS.contains("maintenanceSettlementConsumptions"));
    assert!(TS_CONSUMPTIONS.contains(consumptions!()));
}

/// r25/r26 retired statement SQL must be gone from the owner TS readers; the
/// callers route unconditionally to native plans and keep their lock/guards.
#[test]
fn retired_statements_are_absent_from_typescript_authority() {
    assert!(!TS_FS.contains("SELECT to_jsonb(source)::text image"));
    assert!(TS_FS.contains("nativeAssetsReadFirst<string>(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE"));
    assert!(!TS_PKG.contains("WITH scope AS(SELECT ?::text company_id,?::boolean is_base)"));
    assert!(!TS_PKG.contains("UNION ALL SELECT 'asset-link'"));
    assert!(!TS_PKG.contains("${assetSourceExistsSql(refs"));
    assert!(!TS_PKG.contains("NOT ${assetExistsSql("));
    assert!(!TS_PKG.contains("nativeAssetsReadConfigured"));
    assert!(TS_PKG.contains("nativeAssetsReadAll<{kind:string;id:string;version:number;state:string;pending:boolean}>(kind,{companyId:input.companyId,isBase:full},database)"));
    assert!(TS_PKG.contains("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE"));
    assert!(TS_PKG.contains("throw Error(\"ASSETS_PACKAGE_OBLIGATION_SOURCE_REQUIRED\")"));
    assert!(TS_PKG.contains("throw Error(\"ASSETS_PACKAGE_OBLIGATION_SCOPE_REQUIRED\")"));
    assert!(TS_PKG.contains("export const assetPackageSettlementSources={assets:"));
}

/// The registry must hold the exact frozen native statements.
#[test]
fn native_plans_match_frozen_literals() {
    let cases = [
        (plans::RD_FS_ASSET_SOURCE_ROWS, FROZEN_FS_ROWS),
        (plans::RD_FS_ASSET_ATTACHMENT_IMAGE, FROZEN_FS_IMAGE),
        (plans::RD_PKG_ASSET_SOURCES, asset_sources!()),
        (plans::RD_PKG_LINKS, links!()),
        (plans::RD_OBLIGATIONS_BASE, FROZEN_OBLIGATIONS_BASE),
        (plans::RD_OBLIGATIONS_PREVENTIVE, FROZEN_OBLIGATIONS_PREVENTIVE),
        (plans::RD_OBLIGATIONS_PARTS, FROZEN_OBLIGATIONS_PARTS),
    ];
    for (kind, live) in cases {
        let plan = plans::plan(kind).expect("registered plan");
        assert_eq!(plan.sql.trim(), live.trim(), "{kind}: exact SQL");
        assert_eq!(plan.binds.len(), live.matches('?').count(), "{kind}: binds");
    }
}

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. No registered statement contains a literal `?`.
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

/// Project every column through `to_jsonb` so mixed PG types compare as text.
async fn rows(conn: &mut PgConnection, sql: &str, values: &[&str]) -> Vec<String> {
    let wrapped = format!("SELECT to_jsonb(q)::text FROM ({}) q ORDER BY 1", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for value in values {
        query = query.bind(*value);
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect()
}

/// Run the frozen statement and the registered owner plan on the same unit and
/// require identical rows and identical ordered binds.
async fn parity(
    conn: &mut PgConnection,
    live: &str,
    live_values: &[&str],
    kind: &str,
    scope: &ReadScope<'_>,
) {
    let plan = plans::plan(kind).expect("registered plan");
    assert_eq!(plan.sql.trim(), live.trim(), "{kind}: exact SQL");
    let bound = bind(kind, scope).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    let expected: Vec<String> = live_values.iter().map(|v| (*v).to_owned()).collect();
    assert_eq!(bound.binds, expected, "{kind}: bind order");
    let live_rows = rows(conn, live, live_values).await;
    let native_values: Vec<&str> = bound.binds.iter().map(String::as_str).collect();
    let native_rows = rows(conn, bound.sql, &native_values).await;
    assert!(!live_rows.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(live_rows, native_rows, "{kind}: live and native rows must match");
}

#[tokio::test]
async fn field_service_and_package_reads_match_native_rows_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping native Assets read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(1, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();
        // An uncommitted write on this unit must be visible to both sides.
        sqlx::query("UPDATE operational_assets SET version=4 WHERE id='asset-1'")
            .execute(&mut *tx)
            .await
            .unwrap();

        parity(
            &mut tx,
            FROZEN_FS_ROWS,
            &[],
            plans::RD_FS_ASSET_SOURCE_ROWS,
            &ReadScope::default(),
        )
        .await;
        parity(
            &mut tx,
            FROZEN_FS_IMAGE,
            &["asset-1", "company"],
            plans::RD_FS_ASSET_ATTACHMENT_IMAGE,
            &ReadScope {
                asset_id: Some("asset-1"),
                company_id: Some("company"),
                ..ReadScope::default()
            },
        )
        .await;
        parity(
            &mut tx,
            asset_sources!(),
            &[],
            plans::RD_PKG_ASSET_SOURCES,
            &ReadScope::default(),
        )
        .await;
        parity(
            &mut tx,
            links!(),
            &[],
            plans::RD_PKG_LINKS,
            &ReadScope::default(),
        )
        .await;
        let obligation_scope = |base| ReadScope {
            company_id: Some("company"),
            is_base: Some(base),
            ..ReadScope::default()
        };
        parity(
            &mut tx,
            FROZEN_OBLIGATIONS_BASE,
            &["company", "true"],
            plans::RD_OBLIGATIONS_BASE,
            &obligation_scope(true),
        )
        .await;
        parity(
            &mut tx,
            FROZEN_OBLIGATIONS_PREVENTIVE,
            &["company", "false"],
            plans::RD_OBLIGATIONS_PREVENTIVE,
            &obligation_scope(false),
        )
        .await;
        parity(
            &mut tx,
            FROZEN_OBLIGATIONS_PARTS,
            &["company", "false"],
            plans::RD_OBLIGATIONS_PARTS,
            &obligation_scope(false),
        )
        .await;

        tx.rollback().await.unwrap();
    })
    .await;
}
