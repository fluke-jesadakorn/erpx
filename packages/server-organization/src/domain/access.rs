//! Bounded Organization access projection.
//!
//! These are the current rules from the retained TypeScript owner
//! (`application/access.ts` and `domain/access.ts`): one permission fence,
//! the caller's branch/department window, HR-only private contact fields and
//! the reports-to issue projection. The module performs no I/O; the adapter
//! supplies only the rows the same transaction authorizes.
use crate::domain::organization_nodes::OrganizationScopeKind;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

/// Permission facts projected from the current work context. The adapter
/// re-checks the underlying assignment; this value grants nothing by itself.
#[derive(Clone, Debug)]
pub struct OrganizationAccessActor {
    pub actor_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub scope_kind: OrganizationScopeKind,
    pub assignment_scope: OrganizationScopeKind,
    pub may_read_iam: bool,
    pub may_manage_people: bool,
    pub may_manage_organization: bool,
}

/// Structural row from `organization_nodes`; no roster or IAM authority.
#[derive(Clone, Debug, Serialize)]
pub struct AccessNode {
    pub id: String,
    pub kind: String,
    pub slug: String,
    pub display_name: String,
    pub branch_id: Option<String>,
    pub status: String,
    pub hierarchy_revision: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccessLevel {
    pub id: String,
    pub name: String,
    pub level_number: i64,
}

/// One non-ended employee engagement in the caller's Company.
#[derive(Clone, Debug, Serialize)]
pub struct AccessPerson {
    pub person_id: String,
    pub user_id: Option<String>,
    pub legal_name: String,
    pub email: Option<String>,
    pub engagement_id: String,
    pub engagement_version: i64,
    pub person_version: i64,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub position_id: Option<String>,
    pub manager_person_id: Option<String>,
    pub level_id: Option<String>,
    pub level_number: Option<i64>,
    pub level_name: Option<String>,
    pub status: String,
    pub person_status: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub job_title: String,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccessPosition {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub level_id: String,
    pub level_number: i64,
    pub reports_to_position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
    pub is_ceo: i64,
    pub roster_required: i64,
    pub active: i64,
    pub version: i64,
}

/// Exactly the retained `OrganizationAccessData` wire shape.
#[derive(Clone, Debug, Serialize)]
pub struct OrganizationAccessData {
    pub positions: Vec<AccessPosition>,
    pub people: Vec<AccessPerson>,
    pub nodes: Vec<AccessNode>,
    pub levels: Vec<AccessLevel>,
    #[serde(rename = "canManage")]
    pub can_manage: bool,
    #[serde(rename = "canMovePeople")]
    pub can_move_people: bool,
    #[serde(rename = "actorId")]
    pub actor_id: String,
}

#[derive(Clone, Debug)]
pub struct OrganizationAccessSnapshot {
    pub people: Vec<AccessPerson>,
    pub positions: Vec<AccessPosition>,
    pub nodes: Vec<AccessNode>,
    pub levels: Vec<AccessLevel>,
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role cannot view organization and access.",
    )
}

/// The retained gate: any of IAM read, people management or organization
/// management may open the bounded projection.
pub fn assert_organization_access(actor: &OrganizationAccessActor) -> Result<(), BusinessError> {
    if !actor.may_read_iam && !actor.may_manage_people && !actor.may_manage_organization {
        return Err(permission_required());
    }
    Ok(())
}

fn in_scope(
    actor: &OrganizationAccessActor,
    branch: Option<&str>,
    department: Option<&str>,
) -> bool {
    (actor.branch_id.is_none() || actor.branch_id.as_deref() == branch)
        && (actor.department_id.is_none() || actor.department_id.as_deref() == department)
}

/// Reports-to edges express actual management, not inferred rank adjacency.
/// Ported from `reportingIssues` without widening the input rows.
pub fn reporting_issues(
    person: &AccessPerson,
    people: &[AccessPerson],
    positions: &[AccessPosition],
    today: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    let position = positions.iter().find(|position| {
        Some(position.id.as_str()) == person.position_id.as_deref() && position.active == 1
    });
    if person.level_number.unwrap_or(0) == 0 {
        issues.push("Assign one of the six staff levels.".to_owned());
    }
    if position.is_none() {
        issues.push("Assign an active position.".to_owned());
    }
    if let Some(position) = position {
        let placement_changed = position.company_id != person.company_id
            || position
                .branch_id
                .as_deref()
                .is_some_and(|value| Some(value) != person.branch_id.as_deref())
            || position
                .department_id
                .as_deref()
                .is_some_and(|value| Some(value) != person.department_id.as_deref());
        if placement_changed {
            issues.push("Position does not match staff placement.".to_owned());
        }
        if position.is_ceo == 1 && person.level_number != Some(1) {
            issues.push("The CEO must have Level 1.".to_owned());
        }
    }
    if person.manager_person_id.is_none() {
        if position.map(|position| position.is_ceo == 1) != Some(true) {
            issues.push("Assign a reporting manager.".to_owned());
        }
        return issues;
    }
    let mut seen = std::collections::BTreeSet::from([person.person_id.clone()]);
    let mut current = person;
    while let Some(manager_id) = current.manager_person_id.as_deref() {
        if seen.contains(manager_id) {
            issues.push("Correct the circular reporting relationship.".to_owned());
            break;
        }
        seen.insert(manager_id.to_owned());
        let managers = people
            .iter()
            .filter(|candidate| {
                candidate.person_id == manager_id
                    && candidate.status == "active"
                    && candidate.person_status == "active"
                    && candidate.start_date.as_str() <= today
                    && candidate.end_date.as_deref().is_none_or(|end| end >= today)
            })
            .collect::<Vec<_>>();
        let manager = (managers.len() == 1).then(|| managers[0]);
        let Some(manager) = manager else {
            issues.push(
                "The reporting manager needs one active engagement in this company.".to_owned(),
            );
            break;
        };
        if manager.company_id != person.company_id {
            issues.push(
                "The reporting manager needs one active engagement in this company.".to_owned(),
            );
            break;
        }
        if manager.level_number.unwrap_or(0) == 0
            || current.level_number.unwrap_or(0) == 0
            || manager.level_number.unwrap_or(0) >= current.level_number.unwrap_or(0)
        {
            issues.push("The reporting manager must have a more senior level.".to_owned());
        }
        current = manager;
    }
    issues
}

/// Project only what the same authority window allows. Placement readers see
/// the reporting graph; only `people.manage` sees private contact and dates.
pub fn project_organization_access(
    actor: &OrganizationAccessActor,
    snapshot: OrganizationAccessSnapshot,
    today: &str,
) -> OrganizationAccessData {
    let company = actor.scope_kind == OrganizationScopeKind::Holding
        && actor.assignment_scope == OrganizationScopeKind::Holding
        && actor.branch_id.is_none()
        && actor.department_id.is_none();
    let OrganizationAccessSnapshot {
        people: source_people,
        positions: source_positions,
        nodes,
        levels,
    } = snapshot;
    let positions = source_positions
        .iter()
        .filter(|position| {
            position.active == 1
                && in_scope(
                    actor,
                    position.branch_id.as_deref(),
                    position.department_id.as_deref(),
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    let people = source_people
        .iter()
        .filter(|person| {
            in_scope(
                actor,
                person.branch_id.as_deref(),
                person.department_id.as_deref(),
            )
        })
        .cloned()
        .map(|mut person| {
            if !actor.may_manage_people {
                person.email = None;
                person.start_date = String::new();
                person.end_date = None;
            }
            person.issues = reporting_issues(&person, &source_people, &source_positions, today);
            person
        })
        .collect::<Vec<_>>();
    OrganizationAccessData {
        positions,
        people,
        nodes,
        levels,
        can_manage: company && actor.may_manage_organization && actor.may_manage_people,
        can_move_people: company && actor.may_manage_people,
        actor_id: actor.actor_id.clone(),
    }
}
