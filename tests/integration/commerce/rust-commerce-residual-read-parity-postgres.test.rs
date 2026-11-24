//! Same-transaction live-vs-native parity for the Commerce single-site
//! product/warehouse/settings configuration read family
//! (`G4 / migrate.commerce.residual-read-port`).
//!
//! Every registered plan is executed with the exact ordered values the retained
//! TypeScript caller passes, on ONE freshly opened transaction over the same
//! fixture rows, including an uncommitted write. The live statement and the
//! owner-local native plan must return identical rows. The live statement text
//! is the registered plan literal, bound to the TypeScript source at compile
//! time. Nothing commits, no caller is switched and no TypeScript reader is
//! deleted.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commerce::public::native_read as plans;
use erpx_server_commerce::public::{ReadPlan, bind};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Retained TypeScript authority, read at compile time for source parity.
const TS_GUARDS: &str =
    include_str!("../../../packages/server-commerce/src/adapters/guarded-source-dedupe-sql.ts");
const TS_SETTINGS: &str =
    include_str!("../../../packages/server-commerce/src/adapters/settings-dedupe-sql.ts");
const TS_PAGES: &str =
    include_str!("../../../packages/server-commerce/src/adapters/commerce-pages.ts");
const TS_UNIT: &str = include_str!("../../../packages/server-commerce/src/adapters/commerce.ts");
const TS_ECOMMERCE: &str =
    include_str!("../../../packages/server-commerce/src/adapters/ecommerce-sources.ts");
const TS_RECORDS: &str = include_str!(
    "../../../packages/server-commerce/src/adapters/records-record-details-owned-facts.ts"
);
const TS_POS: &str =
    include_str!("../../../packages/server-commerce/src/adapters/pos-core-facts.ts");

const DDL: &str = "\
CREATE TABLE commerce_products(id text primary key,company_id text not null,sku text,name text,status text,item_type text,tracking_mode text,unit text,price_minor bigint,tax_bps bigint,description text,version bigint);\
CREATE TABLE commerce_warehouses(id text primary key,company_id text not null,branch_id text,code text,name text,status text,version bigint);\
CREATE TABLE commerce_settings(id text primary key,company_id text not null,currency text,chart_id text,receivable_account_id text,payable_account_id text,revenue_account_id text,inventory_account_id text,clearing_account_id text,cogs_account_id text,input_tax_account_id text,output_tax_account_id text,cash_account_id text);\
INSERT INTO commerce_products VALUES ('p1','company','SKU-1','Widget One','active','stock','none','ea',1000,700,'desc',3),('p2','company','SKU-2','Widget Two','active','service','none','hr',2000,0,'desc',1),('p3','company','SKU-3','Draft Item','draft','stock','none','ea',500,0,'desc',1),('pf','foreign','SKU-F','Foreign Item','active','stock','none','ea',900,0,'desc',1);\
INSERT INTO commerce_warehouses VALUES ('w1','company','branch','WH-1','Main Warehouse','active',2),('w2','company','branch-b','WH-2','Second Warehouse','active',1),('w3','company','branch','WH-3','Closed Warehouse','inactive',1),('wf','foreign','branch','WH-F','Foreign Warehouse','active',1);\
INSERT INTO commerce_settings VALUES ('s1','company','THB','chart-1','rec','pay','rev','inv','clr','cogs','input','output','cash'),('sf','foreign','JPY',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);";

/// The live statement literals must stay verbatim against the retained TS source.
#[test]
fn live_statements_match_typescript_authority() {
    let sources = [
        TS_GUARDS,
        TS_SETTINGS,
        TS_PAGES,
        TS_UNIT,
        TS_ECOMMERCE,
        TS_RECORDS,
        TS_POS,
    ];
    for entry in plans::PLANS {
        assert!(
            sources
                .iter()
                .any(|source| plans::norm(source).contains(&plans::norm(entry.sql))),
            "{}: plan SQL missing from retained TypeScript authority",
            entry.kind
        );
    }
    // The two settings constants and the four guard constants are shared text.
    assert!(plans::norm(TS_SETTINGS).contains(&plans::norm(
        "SELECT currency FROM commerce_settings WHERE company_id=?"
    )));
    assert!(plans::norm(TS_GUARDS).contains(&plans::norm(
        "NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)"
    )));
}

/// Convert runtime `?` placeholders to Postgres `$n`.
fn pg(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut quoted = false;
    let mut index = 1;
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            out.push(ch);
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    out.push(chars.next().unwrap());
                } else {
                    quoted = false;
                }
            }
        } else if ch == '\'' {
            quoted = true;
            out.push(ch);
        } else if ch == '?' {
            out.push('$');
            out.push_str(&index.to_string());
            index += 1;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Project every column through `to_jsonb` so mixed PG types compare as text,
/// ordering rows deterministically. Guard predicates are wrapped as a scalar.
async fn rowset(conn: &mut PgConnection, sql: &str, values: &[Option<String>]) -> Vec<String> {
    let wrapped = if sql.starts_with("EXISTS(") || sql.starts_with("NOT EXISTS(") {
        format!("SELECT ({sql})::text AS ok")
    } else {
        format!("SELECT to_jsonb(q)::text FROM ({sql}) q ORDER BY 1")
    };
    let mut query = sqlx::query(AssertSqlSafe(pg(&wrapped)));
    for value in values {
        query = query.bind(value.clone());
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_else(|error| panic!("failed SQL: {wrapped}\nerror: {error:?}"))
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect()
}

struct Case<'a> {
    kind: &'static str,
    scope: erpx_server_commerce::public::ReadScope<'a>,
    /// Exact ordered values the retained TypeScript caller binds.
    live: Vec<Option<String>>,
}

fn cases<'a>() -> Vec<Case<'a>> {
    let s = |record, company, branch| erpx_server_commerce::public::ReadScope {
        record_id: record,
        company_id: Some(company),
        branch_id: branch,
    };
    let t = |value: &str| Some(value.to_owned());
    vec![
        Case {
            kind: plans::RD_PRODUCT_PAGE_ROWS,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_MASTER_ROWS,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_ACTIVE,
            scope: s(Some("p1"), "company", None),
            live: vec![t("p1"), t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_COMPANY_ACTIVE_GUARD,
            scope: s(Some("p1"), "company", None),
            live: vec![t("p1"), t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_ACTIVE_OPTIONS,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_MASTER_DETAIL,
            scope: s(Some("p2"), "company", None),
            live: vec![t("p2"), t("company")],
        },
        Case {
            kind: plans::RD_PRODUCT_POINT,
            scope: s(Some("p1"), "company", None),
            live: vec![t("p1"), t("company")],
        },
        // A NULL branch scope must keep the unscoped predicate working.
        Case {
            kind: plans::RD_WAREHOUSE_PAGE_ROWS,
            scope: s(None, "company", None),
            live: vec![t("company"), None, None],
        },
        Case {
            kind: plans::RD_WAREHOUSE_MASTER_ROWS,
            scope: s(None, "company", Some("branch")),
            live: vec![t("company"), t("branch"), t("branch")],
        },
        Case {
            kind: plans::RD_WAREHOUSE_ACTIVE,
            scope: s(Some("w1"), "company", Some("branch")),
            live: vec![t("w1"), t("company"), t("branch"), t("branch")],
        },
        Case {
            kind: plans::RD_WAREHOUSES_ACTIVE,
            scope: s(None, "company", Some("branch")),
            live: vec![t("company"), t("branch"), t("branch")],
        },
        Case {
            kind: plans::RD_WAREHOUSE_BRANCH_ACTIVE,
            scope: s(None, "company", Some("branch-b")),
            live: vec![t("company"), t("branch-b")],
        },
        Case {
            kind: plans::RD_WAREHOUSE_IDENTITY,
            scope: s(None, "company", None),
            live: vec![],
        },
        Case {
            kind: plans::RD_SETTINGS_BY_COMPANY,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_CURRENCY_BY_COMPANY,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_POSTING_MAPPING_STATE,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
        Case {
            kind: plans::RD_POSTING_ABSENT_GUARD,
            scope: s(None, "company", None),
            live: vec![t("company")],
        },
    ]
}

/// One transaction: run the live statement and the native plan on the same
/// snapshot and require identical rows, ordered binds and non-empty results.
async fn parity(
    conn: &mut PgConnection,
    plan: &ReadPlan,
    live: &[Option<String>],
    scope: &erpx_server_commerce::public::ReadScope<'_>,
) {
    let bound = bind(plan.kind, scope).unwrap_or_else(|error| panic!("{}: {error:?}", plan.kind));
    assert_eq!(bound.binds, live, "{}: bind order", plan.kind);
    assert_eq!(bound.sql, plan.sql, "{}: exact SQL", plan.kind);
    let live_rows = rowset(conn, plan.sql, live).await;
    let native_rows = rowset(conn, bound.sql, &bound.binds).await;
    assert!(
        !live_rows.is_empty(),
        "{}: fixture must return rows",
        plan.kind
    );
    assert_eq!(live_rows, native_rows, "{}: rows must match", plan.kind);
}

#[tokio::test]
async fn commerce_single_site_reads_match_native_plans_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping Commerce residual-read parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    fixture::with_database(2, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();

        // Uncommitted writes on this unit must be visible to both sides.
        sqlx::query("INSERT INTO commerce_products VALUES ('p4','company','SKU-4','Live Insert','active','stock','none','ea',3000,0,'live',1)")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE commerce_warehouses SET name='Main Warehouse Live' WHERE id='w1'")
            .execute(&mut *tx)
            .await
            .unwrap();

        let all = cases();
        assert_eq!(all.len(), plans::PLANS.len(), "every registered plan is exercised");
        for case in all {
            let plan = plans::plan(case.kind).expect("registered plan");
            parity(&mut tx, plan, &case.live, &case.scope).await;
        }

        tx.rollback().await.unwrap();
    })
    .await;
}
