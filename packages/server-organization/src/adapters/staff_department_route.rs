//! Read-only Organization department approval route facts.
//!
//! This is the native port of the retained
//! `createDepartmentAuthorityReads(...).departmentRoute` read. Organization
//! reads only its own policy, seat and legacy-head tables; People staff,
//! engagement and reporting-chain facts arrive from the caller. The port opens
//! no transaction, writes nothing and never reads another owner's tables.
//!
//! `DepartmentPersonFact` carries the retained `StaffPerson` columns the route
//! consumes (`department_id` and `manager_person_id` are required by the
//! original algorithm even though the Workflow-facing projection is narrower).
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::HashSet;

/// Current People staff facts consumed by the department route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepartmentPersonFact {
    pub user_id: String,
    pub person_id: String,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub level_number: Option<i64>,
    pub manager_person_id: Option<String>,
    pub engagement_id: String,
    pub engagement_version: i64,
}

/// One selected department approval step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepartmentRouteStepFact {
    pub user_id: String,
    /// Always `"department"`.
    pub authority: String,
    pub candidate_user_ids: Vec<String>,
    /// Always `"department_approval"`.
    pub stage: Option<String>,
    pub snapshot_json: Option<String>,
}

/// One source-version check that must still hold when the route is consumed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepartmentRouteCheckFact {
    pub kind: String,
    pub id: String,
    pub company_id: String,
    pub version: i64,
}

/// The complete route or the retained business blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepartmentRouteFact {
    pub steps: Vec<DepartmentRouteStepFact>,
    pub checks: Vec<DepartmentRouteCheckFact>,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Selector {
    Person(String),
    Position(String),
}

impl Selector {
    fn matches(&self, person: &DepartmentPersonFact) -> bool {
        match self {
            Self::Person(person_id) => person.person_id == *person_id,
            Self::Position(position_id) => person.position_id.as_deref() == Some(position_id),
        }
    }

    fn snapshot(&self) -> Value {
        match self {
            Self::Person(person_id) => {
                json!({"kind":"person","personId":person_id})
            }
            Self::Position(position_id) => {
                json!({"kind":"position","positionId":position_id})
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Policy {
    id: String,
    version: i64,
    steps: Vec<Selector>,
}

enum RouteFailure {
    Blocked(String),
    Sql,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_STAFF_ROUTE_UNAVAILABLE",
        "The Organization staff department route is unavailable.",
    )
}

fn link_required(multiple: bool) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "STAFF_LINK_REQUIRED",
        if multiple {
            "Multiple active engagements need correction before this action."
        } else {
            "An active employee or contractor engagement and account are required in this company."
        },
    )
}

fn parse_steps(value: Value) -> Option<Vec<Selector>> {
    let Value::Array(items) = value else {
        return None;
    };
    items
        .into_iter()
        .map(|item| {
            let Value::Object(object) = item else {
                return None;
            };
            let kind = object.get("kind")?.as_str()?;
            match kind {
                "person" => Some(Selector::Person(
                    object.get("personId")?.as_str()?.to_owned(),
                )),
                "position" => Some(Selector::Position(
                    object.get("positionId")?.as_str()?.to_owned(),
                )),
                _ => None,
            }
        })
        .collect()
}

async fn read_policy(
    c: &mut PgConnection,
    company: &str,
    department_id: &str,
) -> Result<Option<Policy>, RouteFailure> {
    let row: Option<(String, i64, sqlx::types::Json<Value>)> = sqlx::query_as(
        "SELECT id,version::bigint,steps_json FROM department_flow_versions \
         WHERE company_id=$1 AND department_id=$2 ORDER BY version DESC LIMIT 1",
    )
    .bind(company)
    .bind(department_id)
    .fetch_optional(c)
    .await
    .map_err(|_| RouteFailure::Sql)?;
    match row {
        None => Ok(None),
        Some((id, version, sqlx::types::Json(steps))) => {
            let steps = parse_steps(steps).ok_or(RouteFailure::Sql)?;
            Ok(Some(Policy { id, version, steps }))
        }
    }
}

async fn read_head_seat(
    c: &mut PgConnection,
    company: &str,
    department_id: &str,
) -> Result<Option<Option<String>>, RouteFailure> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT person_id FROM authority_seats \
         WHERE company_id=$1 AND kind='head' AND scope_id=$2",
    )
    .bind(company)
    .bind(department_id)
    .fetch_optional(c)
    .await
    .map_err(|_| RouteFailure::Sql)
}

async fn read_legacy_head(
    c: &mut PgConnection,
    company: &str,
    department_id: &str,
) -> Result<Option<String>, RouteFailure> {
    sqlx::query_scalar::<_, String>(
        "SELECT person_id FROM department_heads \
         WHERE company_id=$1 AND department_id=$2",
    )
    .bind(company)
    .bind(department_id)
    .fetch_optional(c)
    .await
    .map_err(|_| RouteFailure::Sql)
}

async fn read_ceo_person(
    c: &mut PgConnection,
    company: &str,
) -> Result<Option<String>, RouteFailure> {
    let seat: Option<Option<String>> = sqlx::query_scalar(
        "SELECT person_id FROM authority_seats \
         WHERE company_id=$1 AND kind='ceo' AND scope_id=$1",
    )
    .bind(company)
    .fetch_optional(c)
    .await
    .map_err(|_| RouteFailure::Sql)?;
    Ok(seat.flatten())
}

/// The retained `authorityReportingChain`: follow `manager_person_id` through
/// the current roster, rejecting cycles and unavailable managers.
fn reporting_chain<'a>(
    people: &'a [DepartmentPersonFact],
    person_id: &str,
) -> Result<Vec<&'a DepartmentPersonFact>, RouteFailure> {
    let mut chain = Vec::new();
    let mut seen = HashSet::from([person_id.to_owned()]);
    let mut next = people
        .iter()
        .find(|person| person.person_id == person_id)
        .and_then(|person| person.manager_person_id.clone());
    while let Some(manager_id) = next {
        if !seen.insert(manager_id.clone()) {
            return Err(RouteFailure::Blocked(
                "A reporting cycle must be corrected before submission.".into(),
            ));
        }
        let matches: Vec<_> = people
            .iter()
            .filter(|person| person.person_id == manager_id)
            .collect();
        if matches.len() != 1 {
            return Err(RouteFailure::Blocked(
                "The reporting manager needs one active engagement and account.".into(),
            ));
        }
        chain.push(matches[0]);
        next = matches[0].manager_person_id.clone();
        if chain.len() > 100 {
            return Err(RouteFailure::Blocked(
                "The reporting chain is too deep to resolve safely.".into(),
            ));
        }
    }
    Ok(chain)
}

async fn resolve(
    c: &mut PgConnection,
    company: &str,
    requester: &DepartmentPersonFact,
    department_id: &str,
    people: &[DepartmentPersonFact],
) -> Result<DepartmentRouteFact, RouteFailure> {
    let policy = read_policy(c, company, department_id).await?;
    let recorded = read_head_seat(c, company, department_id).await?;
    let head_id = match recorded {
        Some(person_id) => person_id,
        None => read_legacy_head(c, company, department_id).await?,
    };
    let selectors = match &policy {
        Some(policy) => policy.steps.clone(),
        None => head_id
            .clone()
            .map(Selector::Person)
            .into_iter()
            .collect(),
    };
    if selectors.is_empty() {
        return Err(RouteFailure::Blocked(
            "Appoint a department head or publish an acting flow.".into(),
        ));
    }

    let mut forbidden = HashSet::from([requester.user_id.clone()]);
    let mut steps = Vec::new();
    let mut checks = Vec::new();
    if let Some(policy) = &policy {
        checks.push(DepartmentRouteCheckFact {
            kind: "department-policy".into(),
            id: policy.id.clone(),
            company_id: company.to_owned(),
            version: policy.version,
        });
    }

    for (ordinal, selector) in selectors.iter().enumerate() {
        let matches: Vec<_> = people
            .iter()
            .filter(|person| selector.matches(person))
            .collect();
        if matches.len() != 1 {
            return Err(RouteFailure::Blocked(format!(
                "Step {} needs exactly one active position holder and account.",
                ordinal + 1
            )));
        }
        let original = matches[0];
        let mut chosen = original;
        if forbidden.contains(&chosen.user_id) || chosen.person_id == requester.person_id {
            let above = reporting_chain(people, &chosen.person_id)?.into_iter().find(|person| {
                person.level_number.is_some()
                    && chosen.level_number.is_some()
                    && person.level_number < chosen.level_number
                    && !forbidden.contains(&person.user_id)
            });
            match above {
                Some(manager) => chosen = manager,
                None => {
                    let ceo_person = match read_ceo_person(c, company).await? {
                        Some(person_id) => people.iter().find(|person| {
                            person.person_id == person_id
                                && !forbidden.contains(&person.user_id)
                        }),
                        None => None,
                    };
                    match ceo_person {
                        Some(person) => chosen = person,
                        None => {
                            return Err(RouteFailure::Blocked(
                                "Appoint a higher reporting manager or independent CEO for this request."
                                    .into(),
                            ));
                        }
                    }
                }
            }
        }
        if forbidden.contains(&chosen.user_id) {
            return Err(RouteFailure::Blocked(
                "The requester or preparer cannot approve this request.".into(),
            ));
        }
        forbidden.insert(chosen.user_id.clone());
        let snapshot = json!({
            "departmentId": department_id,
            "departmentPolicyId": policy.as_ref().map(|policy| policy.id.clone()),
            "departmentPolicyVersion": policy.as_ref().map(|policy| policy.version).unwrap_or(0),
            "selector": selector.snapshot(),
            "selectedPersonId": chosen.person_id,
            "selectedPositionId": chosen.position_id,
            "originalPersonId": original.person_id,
        });
        let snapshot_json = serde_json::to_string(&snapshot).map_err(|_| RouteFailure::Sql)?;
        steps.push(DepartmentRouteStepFact {
            user_id: chosen.user_id.clone(),
            authority: "department".into(),
            candidate_user_ids: vec![chosen.user_id.clone()],
            stage: Some("department_approval".into()),
            snapshot_json: Some(snapshot_json),
        });
        checks.push(DepartmentRouteCheckFact {
            kind: "staff-engagement".into(),
            id: chosen.engagement_id.clone(),
            company_id: company.to_owned(),
            version: chosen.engagement_version,
        });
    }

    Ok(DepartmentRouteFact {
        steps,
        checks,
        blocked_reason: None,
    })
}

/// Read the retained department route for one requester.
///
/// A missing/ambiguous requester row is the original `activeStaff`
/// `STAFF_LINK_REQUIRED` failure. Every retained `BusinessRuleError` becomes a
/// blocked route with no steps or checks; persistence failures are an
/// unavailable Organization fact.
pub async fn staff_department_route(
    c: &mut PgConnection,
    company_id: &str,
    requester_user_id: &str,
    people: &[DepartmentPersonFact],
) -> Result<DepartmentRouteFact, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let requester_user = UserId::new(requester_user_id)?;
    let requester_rows: Vec<_> = people
        .iter()
        .filter(|person| person.user_id == requester_user.as_str())
        .collect();
    let requester = match requester_rows.as_slice() {
        [requester] => *requester,
        rows => return Err(link_required(rows.len() > 1)),
    };
    let Some(department_id) = requester.department_id.as_deref() else {
        return Ok(DepartmentRouteFact {
            steps: Vec::new(),
            checks: Vec::new(),
            blocked_reason: Some("Assign the requester to a department.".into()),
        });
    };
    match resolve(c, company.as_str(), requester, department_id, people).await {
        Ok(route) => Ok(route),
        Err(RouteFailure::Blocked(reason)) => Ok(DepartmentRouteFact {
            steps: Vec::new(),
            checks: Vec::new(),
            blocked_reason: Some(reason),
        }),
        Err(RouteFailure::Sql) => Err(unavailable()),
    }
}
