//! Native service-team use cases.
//!
//! Current Company ownership, membership, organization scope, coverage access,
//! replay and the authoritative write stay in caller-supplied ports. This layer
//! preserves the TypeScript validation order and messages.
#![allow(async_fn_in_trait)]

use crate::domain::service_teams::{
    ServiceTeamInput, normalize_service_team, validate_local_team_rules,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceTeamActor {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub holding_owner: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceTeamRow {
    pub id: String,
    pub company_id: String,
    pub version: i64,
    pub configuration_json: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrgNode {
    pub kind: String,
    pub branch_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTeamResult {
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTeamPreview {
    pub previous_version: i64,
    pub new_version: i64,
    pub previous: Option<Value>,
    pub proposed: Value,
    pub warnings: Vec<String>,
    pub effect: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceTeamPlan {
    pub id: String,
    pub operation: &'static str,
    pub version: i64,
    pub create: bool,
    pub input: ServiceTeamInput,
    pub current: Option<ServiceTeamRow>,
    pub snapshot: String,
}

pub trait ServiceTeamPorts {
    fn now(&self) -> i64;
    fn categories(&self) -> &[&'static str];

    async fn authorize_owner(&mut self, actor: &ServiceTeamActor) -> Result<(), BusinessError>;

    async fn current(
        &mut self,
        actor: &ServiceTeamActor,
        id: &str,
    ) -> Result<Option<ServiceTeamRow>, BusinessError>;

    async fn company_member(
        &mut self,
        company_id: &str,
        user_id: &str,
    ) -> Result<bool, BusinessError>;

    async fn node(&mut self, company_id: &str, id: &str) -> Result<Option<OrgNode>, BusinessError>;

    async fn validate_scope(
        &mut self,
        actor: &ServiceTeamActor,
        scope: &RecordScope,
    ) -> Result<(), BusinessError>;

    async fn scope_member(
        &mut self,
        scope: &RecordScope,
        user_id: &str,
    ) -> Result<bool, BusinessError>;

    /// Current People/Organization/IAM image used to reject a concurrent change.
    async fn authority_snapshot(&mut self, company_id: &str) -> Result<String, BusinessError>;

    async fn replay(
        &mut self,
        actor: &ServiceTeamActor,
        key: &str,
        operation: &str,
        id: &str,
        input: &Value,
    ) -> Result<Option<Value>, BusinessError>;

    async fn overlaps(
        &mut self,
        actor: &ServiceTeamActor,
        id: Option<&str>,
        coverage: &crate::domain::service_teams::Coverage,
    ) -> Result<Vec<String>, BusinessError>;

    /// Apply owner rows and the caller's receipt/audit/outbox. The same
    /// connection and command key must be used for the whole unit.
    async fn commit(
        &mut self,
        actor: &ServiceTeamActor,
        key: &str,
        plan: &ServiceTeamPlan,
        input: &Value,
        result: &Value,
    ) -> Result<(), BusinessError>;
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// TypeScript `inServiceWorkspace` containment rule.
pub fn in_service_workspace(actor: &ServiceTeamActor, scope: &RecordScope) -> bool {
    actor.company_id == scope.company_id
        && actor
            .branch_id
            .as_ref()
            .is_none_or(|branch| Some(branch) == scope.branch_id.as_ref())
        && actor
            .department_id
            .as_ref()
            .is_none_or(|department| Some(department) == scope.department_id.as_ref())
}

pub fn parse_service_team(
    ports: &impl ServiceTeamPorts,
    raw: &Value,
) -> Result<ServiceTeamInput, BusinessError> {
    normalize_service_team(raw, ports.categories())
        .map_err(|code| invalid(code, "Provide a valid service team."))
}

fn local_rules(input: &ServiceTeamInput) -> Result<(), BusinessError> {
    validate_local_team_rules(input).map_err(|code| match code {
        "DUPLICATE_MEMBER" => invalid("DUPLICATE_MEMBER", "Choose each team member once."),
        "DUPLICATE_COVERAGE" => invalid("DUPLICATE_COVERAGE", "Choose each coverage once."),
        "TEAM_LEAD_REQUIRED" => invalid(
            "TEAM_LEAD_REQUIRED",
            "Choose a lead before activating this team.",
        ),
        _ => invalid("VALIDATION_FAILED", "Provide a valid service team."),
    })
}

async fn validate<P: ServiceTeamPorts>(
    ports: &mut P,
    actor: &ServiceTeamActor,
    id: Option<&str>,
    input: &ServiceTeamInput,
) -> Result<Option<ServiceTeamRow>, BusinessError> {
    ports.authorize_owner(actor).await?;
    let current = match id {
        Some(id) => Some(ports.current(actor, id).await?.ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "TEAM_NOT_FOUND",
                "Service team not found.",
            )
        })?),
        None => None,
    };
    if let Some(current) = &current
        && current.version != input.expected_version
    {
        return Err(conflict(
            "VERSION_CONFLICT",
            "Team setup changed. Refresh before saving.",
        ));
    }
    local_rules(input)?;
    for member in &input.members {
        if !ports
            .company_member(&actor.company_id, &member.user_id)
            .await?
        {
            return Err(invalid(
                "TEAM_MEMBER_INVALID",
                "Every handler needs current company membership. Use People to invite them first.",
            ));
        }
    }
    for coverage in &input.coverage {
        let node = ports
            .node(&actor.company_id, &coverage.scope_node_id)
            .await?;
        let expected = if coverage.scope_kind == "holding" {
            "company"
        } else {
            coverage.scope_kind.as_str()
        };
        let Some(node) = node.filter(|node| node.kind == expected) else {
            return Err(invalid(
                "COVERAGE_INVALID",
                "Choose active coverage in this company.",
            ));
        };
        let scope = RecordScope {
            company_id: actor.company_id.clone(),
            branch_id: if coverage.scope_kind == "branch" {
                Some(coverage.scope_node_id.clone())
            } else {
                node.branch_id.clone()
            },
            department_id: if coverage.scope_kind == "department" {
                Some(coverage.scope_node_id.clone())
            } else {
                None
            },
        };
        if !in_service_workspace(actor, &scope) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "REQUEST_SCOPE_FORBIDDEN",
                "Choose a location within your authorized workspace.",
            ));
        }
        ports.validate_scope(actor, &scope).await?;
        if input.active {
            let mut covered = false;
            for member in input
                .members
                .iter()
                .filter(|member| member.responsibility == "lead")
            {
                if ports.scope_member(&scope, &member.user_id).await? {
                    covered = true;
                }
            }
            if !covered {
                return Err(invalid(
                    "TEAM_COVERAGE_ACCESS_REQUIRED",
                    "At least one team lead needs an assignment covering each selected location.",
                ));
            }
        }
    }
    Ok(current)
}

pub async fn save<P: ServiceTeamPorts>(
    ports: &mut P,
    actor: &ServiceTeamActor,
    id: Option<&str>,
    raw: &Value,
    key: &str,
) -> Result<Value, BusinessError> {
    crate::domain::ticket_portal::validate_command_key(key)?;
    let input = parse_service_team(ports, raw)?;
    let wire = serde_json::to_value(&input)
        .map_err(|_| invalid("SERVICE_TEAM_INPUT_INVALID", "Invalid service team."))?;
    let snapshot = ports.authority_snapshot(&actor.company_id).await?;
    let team_id = id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("team-{key}"));
    let operation = if id.is_some() {
        "service.team.update"
    } else {
        "service.team.create"
    };
    if let Some(prior) = ports.replay(actor, key, operation, &team_id, raw).await? {
        return Ok(prior);
    }
    let current = validate(ports, actor, id, &input).await?;
    let version = current.as_ref().map_or(1, |row| row.version + 1);
    let plan = ServiceTeamPlan {
        id: team_id,
        operation,
        version,
        create: current.is_none(),
        input,
        current,
        snapshot,
    };
    let result = ServiceTeamResult {
        id: plan.id.clone(),
        version,
    };
    let result = serde_json::to_value(&result)
        .map_err(|_| invalid("SERVICE_TEAM_INPUT_INVALID", "Invalid service team."))?;
    ports.commit(actor, key, &plan, &wire, &result).await?;
    Ok(result)
}

pub async fn preview<P: ServiceTeamPorts>(
    ports: &mut P,
    actor: &ServiceTeamActor,
    id: Option<&str>,
    raw: &Value,
) -> Result<ServiceTeamPreview, BusinessError> {
    let input = parse_service_team(ports, raw)?;
    let current = validate(ports, actor, id, &input).await?;
    let mut warnings = vec![];
    if input.active {
        for coverage in &input.coverage {
            for team in ports.overlaps(actor, id, coverage).await? {
                warnings.push(format!(
                    "{team} already covers {} at this scope. Overlapping active teams may prevent routing.",
                    coverage.category
                ));
            }
        }
    }
    Ok(ServiceTeamPreview {
        previous_version: current.as_ref().map_or(0, |row| row.version),
        new_version: current.as_ref().map_or(1, |row| row.version + 1),
        previous: current
            .as_ref()
            .map(|row| serde_json::from_str(&row.configuration_json))
            .transpose()
            .map_err(|_| invalid("SERVICE_TEAM_INPUT_INVALID", "Invalid stored service team."))?,
        proposed: serde_json::to_value(&input)
            .map_err(|_| invalid("SERVICE_TEAM_INPUT_INVALID", "Invalid service team."))?,
        warnings,
        effect: "New intake uses current coverage. Existing jobs retain their team until an explicit recorded repair.",
    })
}
