//! Same-transaction native-vs-retained-TypeScript parity for the Customer
//! Authority portal support response/thread and portal binding read tail
//! (`G4 / migrate.customer_authority.portal-read-port`).
//!
//! The retained TypeScript statements are read at compile time. Each registered
//! owner-local plan is checked byte-for-byte against its TypeScript source
//! (whitespace normalised, with the owner-owned source tokens rendered), then
//! executed with the ordered binds the live call site passes. Both sides run on
//! ONE freshly opened transaction over the same fixture rows, including an
//! uncommitted write. Nothing commits, no caller is switched and no TypeScript
//! reader is deleted. The support source lease stays independent and is never
//! bound or executed as a read.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::portal_read as plans;
use erpx_server_customer_authority::public::{ReadScope, bind};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Retained TypeScript authority, read at compile time for source parity.
const TS_SUPPORT: &str =
    include_str!("../../../packages/server-customer_authority/src/adapters/portal-support.ts");
const TS_SUPPORT_SOURCES: &str = include_str!(
    "../../../packages/server-customer_authority/src/adapters/portal-support-sources.ts"
);
const TS_BINDING_SOURCE: &str = include_str!(
    "../../../packages/server-customer_authority/src/adapters/portal-binding-source.ts"
);
const TS_ACCESS: &str =
    include_str!("../../../packages/server-customer_authority/src/adapters/portal-access.ts");
const TS_ATTACH: &str = include_str!(
    "../../../packages/server-customer_authority/src/adapters/attachments-portal-facts.ts"
);

/// Compare SQL after collapsing all whitespace runs, so TypeScript template
/// newlines and indentation do not change token identity.
fn norm(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        // TypeScript template newlines after a comma introduce a space the
        // rendered native literal does not carry; token identity is unchanged.
        .replace(", ", ",")
}

/// Render the owner-owned source tokens the way the retained callers do.
fn rendered_support() -> String {
    norm(&TS_SUPPORT.replace(plans::TS_RESPONSE_SOURCE_TOKEN, plans::RESPONSE_SOURCE_SQL))
}
fn rendered_access() -> String {
    norm(&TS_ACCESS.replace(
        plans::TS_COMMERCE_ORDERS_TOKEN,
        plans::COMMERCE_ORDERS_SOURCE_SQL,
    ))
}

/// Every registered plan must be present, verbatim, in its TypeScript source.
#[test]
fn native_plans_match_retained_typescript_statements() {
    let sources: &[(&str, String)] = &[
        (plans::RD_SUPPORT_INSTALLED, norm(TS_SUPPORT_SOURCES)),
        (plans::RD_SUPPORT_LATEST_RESPONSE, norm(TS_SUPPORT)),
        (plans::RD_SUPPORT_REQUEST_SOURCE, norm(TS_SUPPORT)),
        (plans::RD_SUPPORT_THREADS, rendered_support()),
        (plans::RD_SUPPORT_REQUEST_ID, norm(TS_SUPPORT)),
        (plans::RD_SUPPORT_STAFF_BINDING, norm(TS_SUPPORT)),
        (plans::RD_BINDING_LOCK, rendered_access()),
        (plans::RD_BINDING_IDENTITY_EXISTS, norm(TS_ACCESS)),
    ];
    assert_eq!(
        sources.len(),
        plans::PLANS.len(),
        "every plan is source-bound"
    );
    for (kind, source) in sources {
        let plan = plans::plan(kind).expect("registered plan");
        assert!(
            source.contains(&norm(plan.sql)),
            "{kind}: exact SQL missing from its TypeScript authority"
        );
        assert_eq!(plan.sql.matches('?').count(), plan.binds.len(), "{kind}");
    }
}

/// Dynamic shells and the support source lease are owner-owned literals too,
/// but are not data-read plans and must stay independent.
#[test]
fn fragments_and_source_leases_match_typescript_and_stay_independent() {
    let fragment_sources: &[(&str, &str)] = &[
        ("portal-binding.source", TS_BINDING_SOURCE),
        ("portal-binding.scope-shell", TS_ACCESS),
        ("portal-binding.attachment-facts-shell", TS_ATTACH),
        ("portal-binding.version-guard", TS_ATTACH),
    ];
    for (name, source) in fragment_sources {
        let fragment = plans::fragment(name).expect("fragment");
        assert!(norm(source).contains(&norm(fragment.sql)), "{name}");
        assert!(plans::plan(name).is_none(), "{name} is not a read plan");
    }
    // The three support-table sources are rendered from one template by the
    // retained `supportSource(table)` factory.
    for table in ["responses", "acknowledgements", "closures"] {
        assert!(TS_SUPPORT_SOURCES.contains(&format!("supportSource('{table}')")));
    }
    assert!(TS_SUPPORT_SOURCES.contains("SELECT * FROM customer_portal_support_${table}"));
    for name in [
        "portal-support.response-source",
        "portal-support.acknowledgement-source",
        "portal-support.closure-source",
    ] {
        let fragment = plans::fragment(name).expect("fragment");
        assert!(
            fragment
                .sql
                .contains("SELECT * FROM customer_portal_support_"),
            "{name}"
        );
        assert!(plans::plan(name).is_none(), "{name} is not a read plan");
    }
    for entry in plans::LOCKS {
        assert!(
            norm(TS_SUPPORT_SOURCES).contains(&norm(entry.sql)),
            "{}",
            entry.name
        );
        assert!(entry.sql.contains("LOCK TABLE"), "{}", entry.name);
        assert!(plans::plan(entry.name).is_none(), "{}", entry.name);
    }
}

/// The runtime binds `?` positionally; convert to Postgres `$n` for direct
/// execution. No registered statement contains a literal `?` in its SQL text.
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
async fn rows(conn: &mut PgConnection, sql: &str, values: &[Option<&str>]) -> Vec<String> {
    let wrapped = format!("SELECT to_jsonb(q)::text FROM ({}) q ORDER BY 1", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for value in values {
        query = match value {
            Some(v) => query.bind(*v),
            None => query.bind(Option::<&str>::None),
        };
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect()
}

/// Bind through the owner adapter and require the live call-site bind order,
/// then run the exact plan on the caller's unit and require matching rows.
async fn parity(
    conn: &mut PgConnection,
    kind: &str,
    scope: &ReadScope<'_>,
    live_values: &[Option<&str>],
) {
    let plan = plans::plan(kind).expect("registered plan");
    let bound = bind(kind, scope).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    let expected: Vec<Option<String>> = live_values.iter().map(|v| v.map(str::to_owned)).collect();
    assert_eq!(bound.binds, expected, "{kind}: bind order");
    assert_eq!(bound.sql, plan.sql, "{kind}: exact SQL");
    let actual: Vec<Option<&str>> = bound.binds.iter().map(|v| v.as_deref()).collect();
    // Live and native run the identical statement with identical ordered binds
    // on the same transaction; the source-parity test above ties that statement
    // to the retained TypeScript literal.
    let live_rows = rows(conn, plan.sql, live_values).await;
    let native_rows = rows(conn, bound.sql, &actual).await;
    assert!(!live_rows.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(
        live_rows, native_rows,
        "{kind}: live and native rows must match"
    );
}

const DDL: &str = "\
CREATE TABLE commerce_orders(id text,company_id text,branch_id text,department_id text,kind text,number text,party_id text,warehouse_id text,currency text,order_date text,due_date text,note text,state text,subtotal_minor bigint,tax_minor bigint,source_id text,version bigint,created_by text,approved_by text,created_at bigint,updated_at bigint,price_list_id text,price_list_version bigint,discount_reason text,discount_approval_required boolean,discount_approved_by text,discount_approved_at bigint);\
CREATE TABLE customer_access_bindings(id text,company_id text,order_id text,customer_id text,email text,status text,expires_at bigint,version bigint,created_by text);\
CREATE TABLE customer_access_identities(binding_id text,user_id text,email text,bound_at bigint);\
CREATE TABLE customer_portal_actions(id text,company_id text,binding_id text,order_id text,customer_id text,user_id text,action text,note text,source_version bigint,status text,created_at bigint);\
CREATE TABLE customer_portal_support_responses(id text,request_id text,binding_id text,response_version bigint,message text,actor_id text,created_at bigint);\
CREATE TABLE customer_portal_support_acknowledgements(id text,request_id text,response_id text,created_at bigint);\
CREATE TABLE customer_portal_support_closures(id text,request_id text,created_at bigint);\
INSERT INTO commerce_orders VALUES ('order-1','company','branch',NULL,'quote','Q-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'creator','creator',100,300,NULL,NULL,NULL,FALSE,NULL,NULL);\
INSERT INTO customer_access_bindings VALUES ('binding-1','company','order-1','party-1','customer@example.invalid','approved',1800000600000,2,'creator');\
INSERT INTO customer_access_identities VALUES ('binding-1','customer','customer@example.invalid',100);\
INSERT INTO customer_portal_actions VALUES ('request-1','company','binding-1','order-1','party-1','customer','request_support','Original customer request',3,'recorded',100),('request-2','company','binding-1','order-1','party-1','customer','request_support','Second request',3,'recorded',120);\
INSERT INTO customer_portal_support_responses VALUES ('response-1','request-1','binding-1',1,'A reviewed response','manager',500);\
INSERT INTO customer_portal_support_acknowledgements VALUES ('ack-1','request-1','response-1',600);\
INSERT INTO customer_portal_support_closures VALUES ('closure-2','request-2',800);";

#[tokio::test]
async fn portal_support_and_binding_reads_match_native_rows_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!(
            "skipping native Customer Authority portal read parity: ERPX_TEST_ADMIN_URL absent"
        );
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
        sqlx::query("INSERT INTO customer_portal_support_responses VALUES ('response-2','request-1','binding-1',2,'A newer reviewed response','manager',550)")
            .execute(&mut *tx)
            .await
            .unwrap();

        let full = ReadScope {
            binding_id: Some("binding-1"),
            request_id: Some("request-1"),
            user_id: Some("customer"),
            email: Some("customer@example.invalid"),
            company_id: Some("company"),
            branch_id: Some("branch"),
            department_id: None,
        };
        let staff = ReadScope {
            binding_id: Some("binding-1"),
            company_id: Some("company"),
            ..ReadScope::default()
        };

        // portal-support
        parity(&mut tx, plans::RD_SUPPORT_INSTALLED, &ReadScope::default(), &[]).await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_LATEST_RESPONSE,
            &full,
            &[Some("request-1")],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_REQUEST_SOURCE,
            &full,
            &[Some("request-1")],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_THREADS,
            &full,
            &[Some("binding-1"), Some("customer"), Some("customer")],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_THREADS,
            &staff,
            &[Some("binding-1"), None, None],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_REQUEST_ID,
            &full,
            &[Some("binding-1"), Some("customer")],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_SUPPORT_STAFF_BINDING,
            &staff,
            &[
                Some("binding-1"),
                Some("company"),
                None,
                None,
                None,
                None,
            ],
        )
        .await;
        // portal-binding
        parity(
            &mut tx,
            plans::RD_BINDING_LOCK,
            &full,
            &[Some("binding-1"), Some("customer@example.invalid")],
        )
        .await;
        parity(
            &mut tx,
            plans::RD_BINDING_IDENTITY_EXISTS,
            &full,
            &[
                Some("binding-1"),
                Some("customer"),
                Some("customer@example.invalid"),
            ],
        )
        .await;

        tx.rollback().await.unwrap();
    })
    .await;
}
