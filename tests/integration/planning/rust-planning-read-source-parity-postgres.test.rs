//! Same-transaction live-vs-native parity for the Planning read-source tail
//! (`G4 / migrate.planning.read-source-port`).
//!
//! For each retained statement the exact TypeScript SQL (the registered plan
//! literal, frozen against the TypeScript source in the owner crate's unit
//! tests) is executed with the ordered values the live caller passes. The
//! owner-local native plan is bound through `bind()` and executed on the SAME
//! freshly opened transaction over the same fixture rows, including an
//! uncommitted write. Row projections must match. Nothing commits, no caller
//! is switched and no TypeScript reader is deleted.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_planning::public::read_source as plans;
use erpx_server_planning::public::{BindField, ReadScope, bind};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

/// The retained TypeScript authority, read at compile time. The owner crate's
/// `byte_parity_with_retained_typescript_statements` test binds every plan to
/// these exact files; this suite binds the same files again at suite scope.
const TS_COMMANDS: &str =
    include_str!("../../../packages/server-planning/src/adapters/production-command-sources-read.ts");
const TS_RESOURCES: &str =
    include_str!("../../../packages/server-planning/src/adapters/resources-read.ts");

const DDL: &str = "\
CREATE TABLE organization_nodes(id text primary key,company_id text,display_name text);\
CREATE TABLE commerce_products(id text primary key,company_id text,name text);\
CREATE TABLE commerce_warehouses(id text primary key,company_id text,name text);\
CREATE TABLE manufacturing_specifications(id text primary key,company_id text,name text,output_product_id text);\
CREATE TABLE planning_scenarios(id text primary key,company_id text,branch_id text,warehouse_id text,specification_id text,specification_version bigint,demand_quantity_milli bigint,due_date text,capacity_minutes bigint,shortages_json text,context_json text,status text,created_by text,version bigint,created_at bigint,updated_at bigint);\
CREATE TABLE planning_recommendations(id text primary key,scenario_id text,scenario_version bigint,recommended_quantity_milli bigint,required_minutes bigint,feasible_date text,status text,created_by text,reviewed_by text,version bigint,created_at bigint,updated_at bigint);\
CREATE TABLE production_resources(id text primary key,company_id text,branch_id text,code text,name text,daily_minutes bigint,work_weekdays_json text,status text,version bigint,created_by text,created_at bigint,updated_at bigint);\
CREATE TABLE production_resource_calendar(resource_id text,work_date text,available_minutes bigint,reason text,version bigint,updated_by text,updated_at bigint,primary key(resource_id,work_date));\
CREATE TABLE planning_resource_allocations(id text primary key,scenario_id text,resource_id text,work_date text,minutes bigint,created_by text,created_at bigint,released_at bigint);\
INSERT INTO organization_nodes VALUES ('company','company','Company'),('branch','company','Branch'),('branch-b','company','Branch B'),('foreign','foreign','Foreign');\
INSERT INTO commerce_products VALUES ('output','company','Bounded output'),('foreign-output','foreign','Foreign output');\
INSERT INTO commerce_warehouses VALUES ('warehouse','company','Warehouse'),('warehouse-b','company','Warehouse B'),('warehouse-f','foreign','Foreign warehouse');\
INSERT INTO manufacturing_specifications VALUES ('spec','company','Spec','output'),('spec-b','company','Spec B','output'),('spec-f','foreign','Foreign Spec','foreign-output');\
INSERT INTO planning_scenarios VALUES \
 ('scenario','company','branch','warehouse','spec',2,2000,'2026-10-04',120,'[{\"productId\":\"material\",\"requiredMilli\":4000,\"availableMilli\":1000,\"incomingMilli\":2000,\"shortageMilli\":1000}]','{}','calculated','planner',3,100,300),\
 ('scenario-b','company','branch-b','warehouse-b','spec-b',2,3000,'2026-10-05',60,'[]','{}','calculated','planner',4,100,200),\
 ('scenario-f','foreign','foreign','warehouse-f','spec-f',2,1000,'2026-10-06',30,'[]','{}','calculated','planner',1,100,100);\
INSERT INTO planning_recommendations VALUES \
 ('recommendation','scenario',1,2000,60,'2026-10-04','proposed','planner',NULL,2,100,300),\
 ('recommendation-b','scenario-b',1,3000,30,'2026-10-05','approved','other','reviewer',4,100,200),\
 ('recommendation-f','scenario-f',1,1000,15,'2026-10-06','proposed','planner',NULL,1,100,100);\
INSERT INTO production_resources VALUES \
 ('resource','company','branch','RES','Resource',480,'[1,2,3,4,5]','active',5,'planner',100,300),\
 ('resource-b','company','branch-b','RESB','Resource B',360,'[1,2,3]','archived',2,'planner',100,200),\
 ('resource-f','foreign','foreign','RESF','Foreign resource',480,'[]','active',1,'planner',100,100);\
INSERT INTO production_resource_calendar VALUES ('resource','2026-11-24',480,'shift',2,'planner',300),('resource','2026-11-25',0,'closed',1,'planner',200);\
INSERT INTO planning_resource_allocations VALUES ('allocation','scenario','resource','2026-11-24',30,'planner',100,NULL),('allocation-released','scenario','resource','2026-11-25',60,'planner',100,200);";

/// Convert the runtime `?` placeholders to Postgres `$n`. A `?` inside a
/// single-quoted literal (the retained jsonpath) is data, not a bind.
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

/// A bound value whose Postgres type follows the registered bind field.
#[derive(Clone, Debug)]
enum Value {
    Text(Option<String>),
    Int(Option<i64>),
}

impl Value {
    fn from_field(field: BindField, value: &Option<String>) -> Self {
        match field {
            BindField::Version | BindField::Minutes | BindField::AvailableMinutes => {
                Value::Int(value.as_deref().and_then(|text| text.parse::<i64>().ok()))
            }
            _ => Value::Text(value.clone()),
        }
    }
}

/// Read every cell as text so text and bigint projections compare uniformly.
fn cell(row: &sqlx::postgres::PgRow, index: usize) -> Option<String> {
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(|number| number.to_string());
    }
    row.try_get::<Option<String>, _>(index).unwrap_or(None)
}

async fn execute(
    conn: &mut PgConnection,
    sql: &str,
    values: &[Value],
) -> Vec<Vec<Option<String>>> {
    // Guards are predicates inside the caller's atomic assertion; the native
    // Workflow assertion wraps them as a scalar. Do the same for this harness.
    let wrapped = if sql.starts_with("EXISTS(")
        || sql.starts_with("NOT EXISTS(")
        || sql.starts_with("COALESCE(")
    {
        format!("SELECT ({sql})::text AS ok")
    } else {
        sql.to_owned()
    };
    let mut query = sqlx::query(AssertSqlSafe(pg(&wrapped)));
    for value in values {
        query = match value {
            Value::Text(text) => query.bind(text.clone()),
            Value::Int(number) => query.bind(*number),
        };
    }
    query
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_else(|error| panic!("failed SQL: {wrapped}\nerror: {error:?}"))
        .iter()
        .map(|row| (0..row.columns().len()).map(|index| cell(row, index)).collect())
        .collect()
}

struct Case<'a> {
    kind: &'static str,
    scope: ReadScope<'a>,
    /// The exact ordered values the retained TypeScript caller binds.
    live: Vec<Option<String>>,
}

fn cases<'a>() -> Vec<Case<'a>> {
    let s = |record, company, branch| ReadScope {
        record_id: record,
        company_id: Some(company),
        branch_id: branch,
        specification_id: Some("spec"),
        resource_id: Some("resource"),
        work_date: Some("2026-11-24"),
        from: Some("2026-01-01"),
        to: Some("2026-12-31"),
        version: Some(3),
        user_id: Some("planner"),
        minutes: Some(30),
        available_minutes: Some(480),
    };
    let t = |value: &str| Some(value.to_owned());
    vec![
        Case { kind: plans::RD_SCENARIOS, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_RECOMMENDATIONS, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_RESOURCES, scope: s(None, "company", Some("branch")), live: vec![t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_CALENDAR, scope: s(None, "company", Some("branch")), live: vec![t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_EXECUTIVE_SHORTAGE, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("scenario"), t("company"), t("branch"), t("branch"), t("2026-01-01"), t("2026-12-31")] },
        Case { kind: plans::RD_EXECUTIVE_RECOMMENDATION_PROPOSED, scope: s(Some("recommendation"), "company", Some("branch")), live: vec![t("recommendation"), t("company"), t("branch"), t("branch"), t("2026-01-01"), t("2026-12-31")] },
        Case { kind: plans::RD_EXECUTIVE_RECOMMENDATION_CAPACITY, scope: s(Some("recommendation"), "company", Some("branch")), live: vec![t("recommendation"), t("company"), t("branch"), t("branch"), t("2026-01-01"), t("2026-12-31")] },
        Case { kind: plans::RD_GUARD_ROUTING_RESOURCE, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_COMPARISON, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("scenario"), t("company"), t("branch"), t("spec")] },
        Case { kind: plans::RD_GUARD_COMPARISON, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("scenario"), t("company"), t("branch"), t("spec"), t("3")] },
        Case { kind: plans::RD_SCENARIO, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("scenario"), t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_GUARD_UNRELEASED_SCENARIO, scope: s(Some("scenario"), "company", Some("branch")), live: vec![t("scenario"), t("company"), t("3")] },
        Case { kind: plans::RD_SCOPED_RECOMMENDATION, scope: s(Some("recommendation"), "company", Some("branch")), live: vec![t("recommendation"), t("company"), t("branch"), t("branch")] },
        Case { kind: plans::RD_GUARD_INDEPENDENT_RECOMMENDATION, scope: s(Some("recommendation"), "company", Some("branch")), live: vec![t("recommendation"), t("3"), t("planner")] },
        Case { kind: plans::RD_GUARD_CAPACITY_RESERVATION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("2026-11-24"), t("30"), t("480")] },
        Case { kind: plans::RD_GUARD_RESOURCE_VERSION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("company"), t("branch"), t("3")] },
        Case { kind: plans::RD_GUARD_NO_ACTIVE_ALLOCATION, scope: s(None, "company", Some("branch")), live: vec![t("resource")] },
        Case { kind: plans::RD_GUARD_ACTIVE_RESOURCE_VERSION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("company"), t("3"), t("branch"), t("branch")] },
        Case { kind: plans::RD_GUARD_CALENDAR_VERSION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("2026-11-24"), t("3")] },
        Case { kind: plans::RD_GUARD_RELEASED_CAPACITY, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("2026-11-24"), t("480")] },
        Case { kind: plans::RD_GUARD_ALLOCATED_RESOURCE_VERSION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("company"), t("3")] },
        Case { kind: plans::RD_GUARD_ALLOCATED_CALENDAR_VERSION, scope: s(None, "company", Some("branch")), live: vec![t("resource"), t("2026-11-24"), t("3")] },
    ]
}

#[tokio::test]
async fn retained_typescript_reads_match_native_plans_on_one_transaction() {
    if std::env::var("ERPX_TEST_ADMIN_URL").is_err() {
        eprintln!("skipping Planning read-source parity: ERPX_TEST_ADMIN_URL absent");
        return;
    }
    // Compile-time authority binding: both TypeScript readers are present.
    assert!(TS_COMMANDS.contains("productionCommandSources"));
    assert!(TS_RESOURCES.contains("createPlanningResourceApplication"));

    fixture::with_database(2, |pool| async move {
        sqlx::raw_sql(AssertSqlSafe(DDL.to_owned()))
            .execute(pool.raw())
            .await
            .unwrap();
        let mut conn = pool.raw().acquire().await.unwrap();
        let mut tx = conn.begin().await.unwrap();

        // An uncommitted write on this unit must be visible to both sides.
        sqlx::query("INSERT INTO planning_resource_allocations VALUES ('allocation-live','scenario','resource','2026-11-24',15,'planner',100,NULL)")
            .execute(&mut *tx)
            .await
            .unwrap();

        let all = cases();
        assert_eq!(all.len(), plans::PLANS.len(), "every registered plan is exercised");
        for case in all {
            let plan = plans::plan(case.kind).expect("registered plan");
            let bound = bind(case.kind, &case.scope).expect("bindable plan");
            assert_eq!(bound.binds, case.live, "{}: live call-site bind order", case.kind);

            let live_values: Vec<Value> = plan
                .binds
                .iter()
                .zip(case.live.iter())
                .map(|(field, value)| Value::from_field(*field, value))
                .collect();
            let native_values: Vec<Value> = plan
                .binds
                .iter()
                .zip(bound.binds.iter())
                .map(|(field, value)| Value::from_field(*field, value))
                .collect();

            let live_rows = execute(&mut tx, plan.sql, &live_values).await;
            let native_rows = execute(&mut tx, bound.sql, &native_values).await;
            assert_eq!(live_rows, native_rows, "{}: live and native rows must match", case.kind);
        }

        // The unscoped-branch form binds SQL NULL in both positions and keeps
        // the retained `(?::text IS NULL OR ...)` predicate intact.
        let scope = ReadScope {
            company_id: Some("company"),
            ..ReadScope::default()
        };
        let unscoped = bind(plans::RD_SCENARIOS, &scope).unwrap();
        assert_eq!(unscoped.binds, vec![Some("company".to_owned()), None, None]);
        assert!(unscoped.sql.contains("(?::text IS NULL OR p.branch_id=?)"));
        let plan = plans::plan(plans::RD_SCENARIOS).unwrap();
        let values: Vec<Value> = plan
            .binds
            .iter()
            .zip(unscoped.binds.iter())
            .map(|(field, value)| Value::from_field(*field, value))
            .collect();
        let rows = execute(&mut tx, plan.sql, &values).await;
        assert_eq!(rows.len(), 2, "unscoped company sees both branch scenarios");
    })
    .await;
}
