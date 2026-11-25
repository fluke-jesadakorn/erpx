//! Same-transaction live-vs-native parity for the Marketing assets, form-content
//! and campaign-fact point reads (`G4 / migrate.marketing.read-ports`).
//!
//! The live TypeScript statements are copied verbatim below (and checked against
//! their source files). Each statement and the matching owner-local native plan
//! run on ONE freshly opened transaction over the same fixture rows, including
//! an uncommitted write. Both result projections must match. Nothing commits,
//! no caller is switched, and no TypeScript reader is deleted.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_marketing::public::native_read::{self as plans, BindField};
use erpx_server_marketing::public::{bind, ReadScope};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// Retained TypeScript authority, read at compile time for source parity.
const TS_LOCAL: &str =
    include_str!("../../../packages/server-marketing/src/adapters/marketing-local-point-read-sql.ts");
const TS_TAIL: &str =
    include_str!("../../../packages/server-marketing/src/adapters/read-tail-sql.ts");
const TS_LIST: &str =
    include_str!("../../../packages/server-marketing/src/adapters/marketing-company-list-sql.ts");
const TS_OWNER: &str =
    include_str!("../../../packages/server-marketing/src/adapters/owner-point-read-sql.ts");
const TS_ACTIVE: &str =
    include_str!("../../../packages/server-marketing/src/adapters/active-form-sql.ts");
const TS_CONTENT: &str =
    include_str!("../../../packages/server-marketing/src/adapters/public-form-content-source.ts");
const TS_HEAD: &str =
    include_str!("../../../packages/server-marketing/src/adapters/form-asset-read-head.ts");
const TS_MARKETING: &str =
    include_str!("../../../packages/server-marketing/src/adapters/marketing.ts");
const TS_CAMPAIGN: &str =
    include_str!("../../../packages/server-marketing/src/adapters/campaign-existence.ts");
const TS_PURCHASE: &str =
    include_str!("../../../packages/server-marketing/src/adapters/marketing-purchase-rows-sql.ts");
const TS_PUBLIC_FORM: &str =
    include_str!("../../../packages/server-marketing/src/adapters/public-form.ts");
const TS_CRM_SOURCES: &str =
    include_str!("../../../packages/server-marketing/src/adapters/crm-purchase-sources.ts");
const TS_PACKAGE: &str =
    include_str!("../../../packages/server-marketing/src/adapters/package-obligations.ts");

// Live statement literals emitted by the retained TypeScript helpers.
const LIVE_ASSET_POINT: &str = "SELECT * FROM marketing_assets WHERE id=? AND company_id=?";
const LIVE_ASSET_LIST: &str =
    "SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC";
const LIVE_FORM_SOURCE: &str = "SELECT configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND kind='form' AND status='active'";
const LIVE_FORM_PUBLIC: &str = "SELECT id,company_id,name,version,preview_json,configuration_json FROM marketing_assets WHERE id=? AND kind='form' AND status='active'";
const LIVE_FORM_CONTENT_EXISTS: &str = "SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?";
const LIVE_FORM_CONTENT_ROW: &str = "SELECT id,version,preview_json FROM marketing_assets WHERE id=? AND company_id=? AND kind='content' AND status='active' AND version=?";
const LIVE_CAMPAIGN_EXISTS: &str = "SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?";
const LIVE_CAMPAIGN_WITNESS: &str = "SELECT campaign.id,campaign.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(campaign) witness FROM marketing_campaigns campaign";
const LIVE_CAMPAIGN_COST_WITNESS: &str = "SELECT cost.id,cost.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(cost) witness FROM marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id";

macro_rules! package_obligations {
    () => {
        concat!(
            "WITH scope AS(SELECT ?::text company_id)\n SELECT 'asset' kind,a.id,to_jsonb(a) fact,a.status IN ('draft','active') OR NOT EXISTS(SELECT 1 FROM marketing_asset_events event WHERE event.asset_id=a.id AND event.version<=a.version AND event.status=a.status AND event.action=CASE a.status WHEN 'retired' THEN 'retire' WHEN 'rejected' THEN 'reject' END AND event.actor_id=a.reviewed_by AND (a.status<>'rejected' OR event.actor_id<>a.created_by) AND EXISTS(SELECT 1 FROM command_receipts command WHERE event.id='marketing-asset-event-'||command.command_id AND command.record_id=a.id AND command.operation='marketing.asset.'||event.action AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=a.company_id AND command.result_json::jsonb->>'status'=a.status AND command.result_json::jsonb->>'version'=event.version::text)) pending FROM marketing_assets a,scope WHERE a.company_id=scope.company_id\n UNION ALL SELECT 'provider-callback-unresolved',a.id,to_jsonb(a),true FROM marketing_assets a,scope WHERE a.company_id=scope.company_id AND a.kind IN ('provider','domain') AND EXISTS(SELECT 1 FROM marketing_asset_events event WHERE event.asset_id=a.id AND event.action='activate')\n UNION ALL SELECT 'candidate',c.id,to_jsonb(c),NOT c.source_valid OR c.status<>'handed_off' FROM (",
            package_candidates!(),
            ") c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'campaign',c.id,to_jsonb(c),false FROM marketing_campaigns c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'campaign-cost-source',c.id,to_jsonb(c),NOT EXISTS(SELECT 1 FROM marketing_campaigns campaign WHERE campaign.id=c.campaign_id AND campaign.company_id=c.company_id) FROM marketing_campaign_costs c,scope WHERE c.company_id=scope.company_id\n UNION ALL SELECT 'public-receipt',receipt.key,to_jsonb(receipt),NOT c.source_valid OR receipt.request_hash!~'^[a-f0-9]{64}$' OR receipt.created_at<>c.created_at FROM marketing_public_receipts receipt JOIN (",
            package_candidates!(),
            ") c ON c.id=receipt.candidate_id CROSS JOIN scope WHERE c.company_id=scope.company_id ORDER BY kind,id"
        )
    };
}
macro_rules! package_candidates {
    () => {
        concat!(
            "SELECT c.*,valid.id IS NOT NULL AND EXISTS(SELECT 1 FROM marketing_assets form JOIN marketing_asset_events activated ON activated.asset_id=form.id AND activated.version=c.form_version AND activated.action='activate' AND activated.actor_id<>form.created_by JOIN command_receipts command ON activated.id='marketing-asset-event-'||command.command_id AND command.operation='marketing.asset.activate' AND command.record_id=form.id AND command.actor_id=activated.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id WHERE form.id=c.form_id AND form.company_id=c.company_id) source_valid FROM marketing_lead_candidates c LEFT JOIN (",
            crm_candidates!(),
            ") valid ON valid.id=c.id AND valid.company_id=c.company_id"
        )
    };
}
macro_rules! crm_candidates {
    () => {
        concat!(
            "SELECT candidate.* FROM marketing_lead_candidates candidate\n JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version\n JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'\n JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'\n WHERE length(trim(candidate.consent_evidence))>=3"
        )
    };
}
const LIVE_PACKAGE_OBLIGATIONS: &str = package_obligations!();

const DDL: &str = "\
CREATE TABLE marketing_assets(id text primary key,company_id text not null,name text,version bigint,preview_json text,configuration_json text,kind text,status text,updated_at text);\
CREATE TABLE marketing_campaigns(id text primary key,company_id text not null,code text,name text,version bigint,created_at text);\
CREATE TABLE marketing_campaign_costs(id text primary key,company_id text not null,campaign_id text,cost_minor bigint,currency text);\
INSERT INTO marketing_assets VALUES ('form-1','company','Form One',3,'{\"preview\":\"p\"}','{\"f\":\"v\"}','form','active','2026-01-02'),('content-1','company','Content One',2,'{\"p\":\"c\"}','{\"c\":\"v\"}','content','active','2026-01-02'),('asset-1','company','Asset One',1,NULL,NULL,'asset','active','2026-01-01'),('form-foreign','other','Foreign',3,NULL,NULL,'form','active','2026-01-01');\
INSERT INTO marketing_campaigns VALUES ('campaign-1','company','C1','Campaign One',1,'2026-01-01'),('campaign-foreign','other','CF','Foreign',1,'2026-01-01');\
INSERT INTO marketing_campaign_costs VALUES ('cost-1','company','campaign-1',1000,'THB');";

/// Live statement literals must stay verbatim against the retained TS source.
#[test]
fn live_statements_match_typescript_authority() {
    assert!(TS_TAIL.contains("SELECT * FROM ${table}${tail}"));
    assert!(!TS_TAIL.contains("WHERE ${predicate}"));
    assert!(TS_LOCAL.contains("marketingReadTailSql(table, ` WHERE ${predicate}${order}${forUpdate}`)"));
    assert!(TS_LOCAL.contains("marketing_assets"));
    assert!(TS_LOCAL.contains("id=? AND company_id=?"));
    assert!(TS_ACTIVE.contains("formAssetReadHead({ kind:'form', projection, company, version })"));
    assert!(TS_CONTENT.contains("formAssetReadHead({ kind: 'content', projection })"));
    assert!(TS_HEAD.contains("return `SELECT ${projection} FROM marketing_assets WHERE ${predicate}`"));
    assert!(TS_HEAD.contains("`id=?${read.company ? ' AND company_id=?' : ''}${read.version ? ' AND version=?' : ''} AND kind='form' AND status='active'`"));
    assert!(TS_HEAD.contains("id=? AND company_id=? AND kind='content' AND status='active' AND version=?"));
    assert!(TS_MARKETING.contains("marketingCompanyListSql('marketing_assets','')"));
    assert!(TS_LIST.contains("marketingReadTailSql(table, ` WHERE company_id=?${scope} ORDER BY updated_at DESC`)"));
    assert!(TS_CAMPAIGN.contains("marketingOwnerPointReadSql(\"1\", \"marketing_campaigns\")"));
    assert!(TS_OWNER.contains("SELECT ${projection} FROM ${source} WHERE ${q}id=? AND ${q}company_id=?"));
    assert!(TS_PURCHASE.contains("SELECT ${id}.id,${id}.company_id,${spec.scope},${spec.witness} witness FROM ${spec.source}"));
    assert!(TS_PURCHASE.contains("NULL::text branch_id,NULL::text department_id"));
    assert!(TS_PURCHASE.contains("to_jsonb(campaign)"));
    assert!(TS_PURCHASE.contains("marketing_campaigns campaign"));
    assert!(TS_PURCHASE.contains("to_jsonb(cost)"));
    assert!(TS_PURCHASE.contains(
        "marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id"
    ));
    assert!(TS_PUBLIC_FORM.contains("marketingFormContentSql('1')"));
    assert!(TS_PUBLIC_FORM.contains("marketingFormContentSql('id,version,preview_json')"));
    assert!(TS_PUBLIC_FORM.contains("activeFormSql('public')"));
    assert!(TS_CRM_SOURCES.contains(crm_candidates!()));
    assert!(TS_PACKAGE.contains("nativeMarketingReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(MARKETING_RD_PACKAGE_OBLIGATIONS"));
    assert!(TS_PACKAGE.contains("LOCK TABLE marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_campaign_costs,marketing_touches,marketing_public_receipts,marketing_public_limits,command_receipts IN SHARE MODE"));
    assert!(!TS_PACKAGE.contains("WITH scope AS(SELECT ?::text company_id)\n SELECT 'asset' kind"));
}
/// The retired statement has no TypeScript literal left to read; the exact
/// former live statement is frozen here and must equal the registered plan.
#[test]
fn retired_package_obligations_statement_is_byte_identical() {
    let plan = plans::plan(plans::RD_PACKAGE_OBLIGATIONS).expect("registered plan");
    assert_eq!(plan.sql, LIVE_PACKAGE_OBLIGATIONS);
    assert_eq!(plan.sql.matches("SELECT ").count(), 17);
    assert_eq!(plan.binds.len(), 1);
    assert_eq!(plan.binds, &[BindField::CompanyId]);
}

/// The registry must hold the exact statements the TypeScript caller runs.
#[test]
fn native_plans_match_live_typescript_statements() {
    let cases = [
        (plans::RD_ASSET_POINT, LIVE_ASSET_POINT),
        (plans::RD_ASSET_LIST, LIVE_ASSET_LIST),
        (plans::RD_FORM_SOURCE, LIVE_FORM_SOURCE),
        (plans::RD_FORM_PUBLIC, LIVE_FORM_PUBLIC),
        (plans::RD_FORM_CONTENT_EXISTS, LIVE_FORM_CONTENT_EXISTS),
        (plans::RD_FORM_CONTENT_ROW, LIVE_FORM_CONTENT_ROW),
        (plans::RD_CAMPAIGN_EXISTS, LIVE_CAMPAIGN_EXISTS),
        (plans::RD_CAMPAIGN_WITNESS, LIVE_CAMPAIGN_WITNESS),
        (plans::RD_CAMPAIGN_COST_WITNESS, LIVE_CAMPAIGN_COST_WITNESS),
        (plans::RD_PACKAGE_OBLIGATIONS, LIVE_PACKAGE_OBLIGATIONS),
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

/// A bind value with the same type the live TypeScript caller supplies.
#[derive(Clone, Copy)]
enum Val<'a> {
    Text(&'a str),
    Int(i64),
}

fn as_text(value: Val<'_>) -> String {
    match value {
        Val::Text(text) => text.to_owned(),
        Val::Int(number) => number.to_string(),
    }
}

/// Project every column through `to_jsonb` so mixed PG types compare as text.
async fn rows(conn: &mut PgConnection, sql: &str, values: &[Val<'_>]) -> Vec<String> {
    let wrapped = format!("SELECT to_jsonb(q)::text FROM ({}) q ORDER BY 1", pg(sql));
    let mut query = sqlx::query(AssertSqlSafe(wrapped));
    for &value in values {
        query = match value {
            Val::Text(text) => query.bind(text),
            Val::Int(number) => query.bind(number),
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

/// Run the live statement and the registered owner plan on the same unit and
/// require identical rows and identical ordered binds.
async fn parity(
    conn: &mut PgConnection,
    live: &str,
    live_values: &[Val<'_>],
    kind: &str,
    scope: &ReadScope<'_>,
) {
    let plan = plans::plan(kind).expect("registered plan");
    assert_eq!(plan.sql.trim(), live.trim(), "{kind}: exact SQL");
    let bound = bind(kind, scope).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    let expected: Vec<String> = live_values.iter().map(|&v| as_text(v)).collect();
    assert_eq!(bound.binds, expected, "{kind}: bind order");
    let native_values: Vec<Val<'_>> = bound
        .binds
        .iter()
        .zip(plan.binds.iter())
        .map(|(value, field)| match field {
            BindField::Version => Val::Int(value.parse().expect("integer version bind")),
            _ => Val::Text(value.as_str()),
        })
        .collect();
    let live_rows = rows(conn, live, live_values).await;
    let native_rows = rows(conn, bound.sql, &native_values).await;
    assert!(!live_rows.is_empty(), "{kind}: fixture must return rows");
    assert_eq!(live_rows, native_rows, "{kind}: live and native rows must match");
}

#[tokio::test]
async fn assets_forms_and_campaign_facts_match_native_rows_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping native Marketing read parity: ERPX_TEST_ADMIN_URL absent");
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
        sqlx::query("UPDATE marketing_assets SET version=4 WHERE id='form-1'")
            .execute(&mut *tx)
            .await
            .unwrap();

        let asset_scope = ReadScope {
            asset_id: Some("asset-1"),
            company_id: Some("company"),
            ..ReadScope::default()
        };
        parity(
            &mut tx,
            LIVE_ASSET_POINT,
            &[Val::Text("asset-1"), Val::Text("company")],
            plans::RD_ASSET_POINT,
            &asset_scope,
        )
        .await;
        parity(
            &mut tx,
            LIVE_ASSET_LIST,
            &[Val::Text("company")],
            plans::RD_ASSET_LIST,
            &ReadScope {
                company_id: Some("company"),
                ..ReadScope::default()
            },
        )
        .await;
        let source_scope = ReadScope {
            asset_id: Some("form-1"),
            company_id: Some("company"),
            version: Some(4),
            ..ReadScope::default()
        };
        parity(
            &mut tx,
            LIVE_FORM_SOURCE,
            &[Val::Text("form-1"), Val::Text("company"), Val::Int(4)],
            plans::RD_FORM_SOURCE,
            &source_scope,
        )
        .await;
        parity(
            &mut tx,
            LIVE_FORM_PUBLIC,
            &[Val::Text("form-1")],
            plans::RD_FORM_PUBLIC,
            &ReadScope {
                asset_id: Some("form-1"),
                ..ReadScope::default()
            },
        )
        .await;
        let content_scope = ReadScope {
            asset_id: Some("content-1"),
            company_id: Some("company"),
            version: Some(2),
            ..ReadScope::default()
        };
        parity(
            &mut tx,
            LIVE_FORM_CONTENT_EXISTS,
            &[Val::Text("content-1"), Val::Text("company"), Val::Int(2)],
            plans::RD_FORM_CONTENT_EXISTS,
            &content_scope,
        )
        .await;
        parity(
            &mut tx,
            LIVE_FORM_CONTENT_ROW,
            &[Val::Text("content-1"), Val::Text("company"), Val::Int(2)],
            plans::RD_FORM_CONTENT_ROW,
            &content_scope,
        )
        .await;
        parity(
            &mut tx,
            LIVE_CAMPAIGN_EXISTS,
            &[Val::Text("campaign-1"), Val::Text("company")],
            plans::RD_CAMPAIGN_EXISTS,
            &ReadScope {
                company_id: Some("company"),
                campaign_id: Some("campaign-1"),
                ..ReadScope::default()
            },
        )
        .await;
        parity(
            &mut tx,
            LIVE_CAMPAIGN_WITNESS,
            &[],
            plans::RD_CAMPAIGN_WITNESS,
            &ReadScope::default(),
        )
        .await;
        parity(
            &mut tx,
            LIVE_CAMPAIGN_COST_WITNESS,
            &[],
            plans::RD_CAMPAIGN_COST_WITNESS,
            &ReadScope::default(),
        )
        .await;

        tx.rollback().await.unwrap();
    })
    .await;
}
