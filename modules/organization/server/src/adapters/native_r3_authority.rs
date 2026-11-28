//! Additive native Organization authority slice (R3 / L05).
//!
//! Scope: the `GET /api/v1/holdings/{holding_slug}/authority` overview owner
//! read — real SQLx over the organization-owned authority tables, the exact
//! `{"data": ...}` envelope and `Cache-Control: private, no-store` — plus the
//! bounded subpath/header contract shared by the authority catch-all.
//!
//! The POST commands (`appointments`, `flows`, `decisions`) and
//! `/authority/flows/preview` are NOT ported here: their decisions depend on
//! identity-access assignments, people-payroll projections, operations staff
//! facts and workflow command execution owned outside this crate. See the R3
//! L05 worker report for the exact blocker and TS authority paths.
//!
//! Cross-owner *read* facts needed by the overview projection (Current Power
//! and publish eligibility) are supplied through [`OverviewActor`], which
//! composition implements. No SQL text, session or actor is inferred from
//! caller-controlled input.

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Map, Value, json};
use sqlx::{Column, Row};
use std::collections::HashSet;
use std::{future::Future, pin::Pin};

// -- domain: bounded subpaths and exact transport contract --------------------

/// The five bounded operations behind the two L05 authority handlers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoritySubpath {
    /// GET on the catch-all root (overview).
    Overview,
    /// POST `/authority/appointments`.
    Appointments,
    /// POST `/authority/flows`.
    Flows,
    /// POST `/authority/decisions`.
    Decisions,
    /// POST `/authority/flows/preview` (dedicated handler).
    FlowsPreview,
}

impl AuthoritySubpath {
    /// The TS handlers set `Cache-Control: private, no-store` on the GET
    /// overview and on the dedicated flow preview POST only.
    pub fn cache_control(self) -> Option<&'static str> {
        match self {
            Self::Overview | Self::FlowsPreview => Some("private, no-store"),
            Self::Appointments | Self::Flows | Self::Decisions => None,
        }
    }

    /// Only command POSTs require the `Idempotency-Key` header. Preview is a
    /// read-only simulation and the TS handler does not check a key.
    pub fn requires_idempotency_key(self) -> bool {
        matches!(self, Self::Appointments | Self::Flows | Self::Decisions)
    }

    /// Stable identifier used in tests and logs; never a route path.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Appointments => "appointments",
            Self::Flows => "flows",
            Self::Decisions => "decisions",
            Self::FlowsPreview => "flows.preview",
        }
    }
}

/// Map one method and the catch-all path segments onto a bounded operation.
/// Anything outside this table is the retained `404 NOT_FOUND` branch.
pub fn classify(method: &str, segments: &[&str]) -> Option<AuthoritySubpath> {
    match (method, segments) {
        ("GET", []) => Some(AuthoritySubpath::Overview),
        ("POST", ["appointments"]) => Some(AuthoritySubpath::Appointments),
        ("POST", ["flows"]) => Some(AuthoritySubpath::Flows),
        ("POST", ["decisions"]) => Some(AuthoritySubpath::Decisions),
        ("POST", ["flows", "preview"]) => Some(AuthoritySubpath::FlowsPreview),
        _ => None,
    }
}

/// `canonicalDestinationLinks` in the retained handlers only rewrites fields
/// named `href`/`*Href`/`*_href`. The authority overview and preview payloads
/// are seat/policy/route facts and contain no destination fields, so the exact
/// envelope is the single `data` wrapper.
pub fn envelope(data: Value) -> Value {
    json!({"data": data})
}

// -- application: owner facts + exact overview projection --------------------

/// Cross-owner read seam composition must supply for the overview projection.
/// The organization crate owns the SQL and the projection; only the two
/// actor-dependent answers arrive from other owners.
pub trait OverviewActor: Send + Sync {
    /// `authorityPower(false, person, currentPowerKinds, kind) != null`.
    fn can_appoint(&self, kind: &str) -> bool;
    /// `flowPublisher(person, headPersonId, ceoPersonId, active) != null`.
    fn can_publish(&self, department_id: &str) -> bool;
}

/// The four owner-local reads of the retained `appointmentOverview`.
#[derive(Clone, Debug, Default)]
pub struct OverviewFacts {
    pub seats: Vec<Value>,
    pub history: Vec<Value>,
    pub policies: Vec<Value>,
    pub departments: Vec<Value>,
}

fn is_head_visible(row: &Value, visible: &HashSet<&str>) -> bool {
    row.get("kind").and_then(Value::as_str) == Some("head")
        && row
            .get("scope_id")
            .and_then(Value::as_str)
            .is_some_and(|scope| visible.contains(scope))
}

/// Field-for-field successor of the retained `overview()` projection: when the
/// actor holds any appointment power the full facts are returned, otherwise
/// the facts are narrowed to departments where the actor may publish a flow.
pub fn project_overview(facts: OverviewFacts, actor: &dyn OverviewActor) -> Value {
    let can_appoint_kinds: Vec<String> = ["ceo", "hr", "head"]
        .into_iter()
        .filter(|kind| actor.can_appoint(kind))
        .map(str::to_owned)
        .collect();
    let can_publish_department_ids: Vec<String> = facts
        .departments
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .filter(|id| actor.can_publish(id))
        .map(str::to_owned)
        .collect();
    if !can_appoint_kinds.is_empty() {
        return json!({
            "seats": facts.seats,
            "history": facts.history,
            "policies": facts.policies,
            "departments": facts.departments,
            "canAppointKinds": can_appoint_kinds,
            "canPublishDepartmentIds": can_publish_department_ids,
        });
    }
    let visible: HashSet<&str> = can_publish_department_ids
        .iter()
        .map(String::as_str)
        .collect();
    json!({
        "seats": facts.seats.iter().filter(|row| is_head_visible(row, &visible)).collect::<Vec<_>>(),
        "history": facts.history.iter().filter(|row| is_head_visible(row, &visible)).collect::<Vec<_>>(),
        "policies": facts.policies.iter().filter(|row| {
            row.get("department_id").and_then(Value::as_str).is_some_and(|id| visible.contains(id))
        }).collect::<Vec<_>>(),
        "departments": facts.departments.iter().filter(|row| {
            row.get("id").and_then(Value::as_str).is_some_and(|id| visible.contains(id))
        }).collect::<Vec<_>>(),
        "canAppointKinds": can_appoint_kinds,
        "canPublishDepartmentIds": can_publish_department_ids,
    })
}

pub type OverviewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<OverviewFacts, BusinessError>> + Send + 'a>>;

/// Owner reader. Composition supplies the Postgres implementation; tests may
/// inject any bounded implementation of the same port.
pub trait AuthorityOverviewRead: Send + Sync {
    fn read<'a>(&'a self, company: &'a str) -> OverviewFuture<'a>;
}

pub async fn handle_overview<R: AuthorityOverviewRead + ?Sized>(
    reader: &R,
    actor: &dyn OverviewActor,
    company: &str,
) -> Result<Value, BusinessError> {
    // Mirrors the retained native owner transport's identifier fence: a
    // non-empty Company id of at most 200 UTF-8 bytes. The Company id comes
    // from the verified work context, never from caller-controlled text.
    if company.is_empty() || company.len() > 200 {
        return Err(BusinessError::invalid(
            "ORGANIZATION_AUTHORITY_READ_INVALID",
            "Use one bounded Organization authority read.",
        ));
    }
    let facts = reader.read(company).await?;
    Ok(envelope(project_overview(facts, actor)))
}

// -- adapters: real SQLx over the owner tables --------------------------------

/// Byte-identical projection of the retained `appointmentOverview` seats read,
/// with the TypeScript `?` bind replaced by `$1`. The `hr_people` projection is
/// the people-payroll read-only source; no write or authority decision is taken
/// from it here.
pub const SEATS_SQL: &str = concat!(
    "SELECT s.*,p.legal_name person_name,pos.title position_title,n.display_name department_name ",
    "FROM authority_seats s ",
    "LEFT JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.id=s.person_id ",
    "LEFT JOIN organization_positions pos ON pos.id=s.position_id ",
    "LEFT JOIN organization_nodes n ON n.id=s.scope_id AND s.kind='head' ",
    "WHERE s.company_id=$1 ",
    "ORDER BY CASE s.kind WHEN 'ceo' THEN 0 WHEN 'hr' THEN 1 ELSE 2 END,n.display_name"
);
/// Owner-local history read from `organization_owner_write::DEPT_AUTH_READ_HISTORY`.
pub const HISTORY_SQL: &str =
    "SELECT * FROM authority_appointments WHERE company_id=$1 ORDER BY created_at DESC,id DESC LIMIT 200";
/// Owner-local policy read from `organization_owner_write::DEPT_AUTH_READ_POLICIES`.
pub const POLICIES_SQL: &str = concat!(
    "SELECT DISTINCT ON (department_id) department_id,version,steps_json,published_by,reason,created_at ",
    "FROM department_flow_versions WHERE company_id=$1 ORDER BY department_id,version DESC"
);
/// Owner-local department read from `organization_owner_write::DEPT_AUTH_READ_DEPARTMENTS`.
pub const DEPARTMENTS_SQL: &str = concat!(
    "SELECT id,display_name FROM organization_nodes WHERE company_id=$1 ",
    "AND kind='department' AND status='active' ORDER BY display_name"
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_AUTHORITY_READ_UNAVAILABLE",
        "The native Organization authority read is unavailable.",
    )
}

/// Map one Postgres column to its JSON value without changing key names or
/// row shape. Authority tables use text, integer/bigint, boolean and jsonb
/// columns; an unrecognised or NULL column stays JSON null, which is what the
/// retained `Response.json(all())` produced.
fn column_value(row: &sqlx::postgres::PgRow, name: &str) -> Result<Value, BusinessError> {
    if let Ok(Some(value)) = row.try_get::<Option<String>, _>(name) {
        return Ok(Value::String(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<i64>, _>(name) {
        return Ok(json!(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<bool>, _>(name) {
        return Ok(json!(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<f64>, _>(name) {
        return Ok(json!(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<Value>, _>(name) {
        return Ok(value);
    }
    Ok(Value::Null)
}

fn rows_value(rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<Value>, BusinessError> {
    rows.into_iter()
        .map(|row| {
            let mut object = Map::new();
            for column in row.columns() {
                object.insert(
                    column.name().to_owned(),
                    column_value(&row, column.name())?,
                );
            }
            Ok(Value::Object(object))
        })
        .collect()
}

/// Postgres-backed owner reader. All four reads share one unit so seats,
/// history, policies and departments form a consistent snapshot. No lock is
/// taken and no command is executed by this read port.
pub struct PgAuthorityOverviewRead {
    pool: DatabasePool,
}

impl PgAuthorityOverviewRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AuthorityOverviewRead for PgAuthorityOverviewRead {
    fn read<'a>(&'a self, company: &'a str) -> OverviewFuture<'a> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let seats = rows_value(
                        sqlx::query(SEATS_SQL)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?,
                    )?;
                    let history = rows_value(
                        sqlx::query(HISTORY_SQL)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?,
                    )?;
                    let policies = rows_value(
                        sqlx::query(POLICIES_SQL)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?,
                    )?;
                    let departments = rows_value(
                        sqlx::query(DEPARTMENTS_SQL)
                            .bind(company.as_str())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?,
                    )?;
                    Ok(OverviewFacts {
                        seats,
                        history,
                        policies,
                        departments,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Actor {
        appoint: Vec<&'static str>,
        publish: Vec<&'static str>,
    }

    impl OverviewActor for Actor {
        fn can_appoint(&self, kind: &str) -> bool {
            self.appoint.iter().any(|value| *value == kind)
        }
        fn can_publish(&self, department_id: &str) -> bool {
            self.publish.iter().any(|value| *value == department_id)
        }
    }

    struct Reader {
        facts: OverviewFacts,
    }

    impl AuthorityOverviewRead for Reader {
        fn read<'a>(&'a self, _: &'a str) -> OverviewFuture<'a> {
            let facts = self.facts.clone();
            Box::pin(async move { Ok(facts) })
        }
    }

    fn facts() -> OverviewFacts {
        OverviewFacts {
            seats: vec![
                json!({"kind":"ceo","scope_id":"acme-co","person_id":"p1","version":2}),
                json!({"kind":"head","scope_id":"d1","person_id":"p2","version":3}),
                json!({"kind":"head","scope_id":"d2","person_id":"p3","version":1}),
            ],
            history: vec![
                json!({"kind":"ceo","scope_id":"acme-co","version":2}),
                json!({"kind":"head","scope_id":"d1","version":3}),
                json!({"kind":"head","scope_id":"d2","version":1}),
            ],
            policies: vec![json!({"department_id":"d1","version":4})],
            departments: vec![
                json!({"id":"d1","display_name":"Engineering"}),
                json!({"id":"d2","display_name":"Sales"}),
            ],
        }
    }

    #[test]
    fn bounded_subpaths_match_the_two_handlers() {
        assert_eq!(classify("GET", &[]), Some(AuthoritySubpath::Overview));
        assert_eq!(
            classify("POST", &["appointments"]),
            Some(AuthoritySubpath::Appointments)
        );
        assert_eq!(classify("POST", &["flows"]), Some(AuthoritySubpath::Flows));
        assert_eq!(
            classify("POST", &["decisions"]),
            Some(AuthoritySubpath::Decisions)
        );
        assert_eq!(
            classify("POST", &["flows", "preview"]),
            Some(AuthoritySubpath::FlowsPreview)
        );
        assert_eq!(classify("POST", &[]), None);
        assert_eq!(classify("GET", &["appointments"]), None);
        assert_eq!(classify("POST", &["unknown"]), None);
        assert_eq!(classify("POST", &["flows", "publish"]), None);
    }

    #[test]
    fn headers_and_key_contract_match_the_ts_handlers() {
        assert_eq!(
            AuthoritySubpath::Overview.cache_control(),
            Some("private, no-store")
        );
        assert_eq!(
            AuthoritySubpath::FlowsPreview.cache_control(),
            Some("private, no-store")
        );
        assert_eq!(AuthoritySubpath::Flows.cache_control(), None);
        assert!(AuthoritySubpath::Appointments.requires_idempotency_key());
        assert!(AuthoritySubpath::Decisions.requires_idempotency_key());
        assert!(!AuthoritySubpath::FlowsPreview.requires_idempotency_key());
    }

    #[test]
    fn envelope_is_the_exact_data_wrapper() {
        assert_eq!(envelope(json!({"seats": []})), json!({"data": {"seats": []}}));
    }

    #[tokio::test]
    async fn owner_with_appointment_power_sees_full_facts() {
        let actor = Actor {
            appoint: vec!["head"],
            publish: vec!["d1"],
        };
        let value = handle_overview(&Reader { facts: facts() }, &actor, "acme-co")
            .await
            .unwrap();
        assert_eq!(value["data"]["canAppointKinds"], json!(["head"]));
        assert_eq!(value["data"]["seats"].as_array().unwrap().len(), 3);
        assert_eq!(value["data"]["canPublishDepartmentIds"], json!(["d1"]));
    }

    #[tokio::test]
    async fn limited_actor_sees_only_publishable_head_facts() {
        let actor = Actor {
            appoint: vec![],
            publish: vec!["d1"],
        };
        let value = handle_overview(&Reader { facts: facts() }, &actor, "acme-co")
            .await
            .unwrap();
        assert_eq!(value["data"]["canAppointKinds"], json!([]));
        assert_eq!(value["data"]["seats"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"]["seats"][0]["kind"], "head");
        assert_eq!(value["data"]["seats"][0]["scope_id"], "d1");
        assert_eq!(value["data"]["history"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"]["policies"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"]["departments"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn no_power_and_no_publish_returns_empty_visible_facts() {
        let actor = Actor {
            appoint: vec![],
            publish: vec![],
        };
        let value = handle_overview(&Reader { facts: facts() }, &actor, "acme-co")
            .await
            .unwrap();
        assert_eq!(value["data"]["seats"], json!([]));
        assert_eq!(value["data"]["departments"], json!([]));
    }

    #[tokio::test]
    async fn unbounded_company_fails_before_read() {
        let actor = Actor {
            appoint: vec![],
            publish: vec![],
        };
        assert!(handle_overview(&Reader { facts: facts() }, &actor, "")
            .await
            .is_err());
        assert!(handle_overview(&Reader { facts: facts() }, &actor, &"x".repeat(201))
            .await
            .is_err());
    }

    #[test]
    fn owner_statements_are_select_only_company_keyed_and_use_sqlx_binds() {
        for statement in [SEATS_SQL, HISTORY_SQL, POLICIES_SQL, DEPARTMENTS_SQL] {
            assert!(
                statement.trim_start().to_ascii_uppercase().starts_with("SELECT"),
                "owner read must be SELECT-only"
            );
            assert!(statement.contains("company_id=$1"), "must be Company-keyed");
            assert!(!statement.contains('?'), "binds must be sqlx placeholders");
        }
        assert!(SEATS_SQL.contains("person_name"));
        assert!(SEATS_SQL.contains("position_title"));
        assert!(SEATS_SQL.contains("department_name"));
        assert!(HISTORY_SQL.contains("LIMIT 200"));
    }
}
