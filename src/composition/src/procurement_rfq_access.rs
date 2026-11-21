//! Initial Records requestAccess evaluation with actual owner facts. This is
//! not complete visibility: owner/team/approval history and routing remain.
use crate::procurement_rfq_approval::{
    RfqDesignatedStep, RfqPriorReviewer, rfq_designated_step, rfq_prior_reviewer,
};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    rfq_request_membership::{
        rfq_request_current_principals, rfq_request_service_member, rfq_request_service_owner,
    },
};
use erpx_server_organization::public::rfq_request_scope::{
    rfq_scope_branch, rfq_scope_company, rfq_scope_department,
};
use erpx_server_people_payroll::public::rfq_request_people::rfq_request_people_sql;
use erpx_server_records::public::rfq_request_source::{
    CurrentRfqQuotationRow, rfq_request_events_sql,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_support::public::rfq_request_access::{
    rfq_in_service_workspace, rfq_request_team_sources_sql,
};
use sqlx::PgConnection;
use std::collections::HashMap;
type Error = PgTransactionError<BusinessError>;

/// Support validateServiceScope in its original post-detail position. The
/// caller must separately bind the selected actor and all owner sources to the
/// enclosing transaction; this helper is not an authorization or replay grant.
pub(crate) async fn validate_rfq_request_service_scope(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    source: &CurrentRfqQuotationRow,
) -> Result<(), Error> {
    let w = current.selection().workspace();
    if source.company() != &w.company_id {
        return Err(PersistenceError::new("RFQ_REQUEST_COMPANY_MISMATCH").into());
    }
    source.require_current(&mut *c).await?;
    let row: serde_json::Value = serde_json::from_str(
        source
            .image_json()
            .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?,
    )
    .map_err(|_| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    let branch = row.get("branch_id").and_then(|v| v.as_str());
    let department = row.get("department_id").and_then(|v| v.as_str());
    if !rfq_in_service_workspace(
        w.company_id.as_str(),
        w.branch_id.as_ref().map(|id| id.as_str()),
        w.department_id.as_ref().map(|id| id.as_str()),
        w.company_id.as_str(),
        branch,
        department,
    ) || !rfq_request_service_member(
        &mut *c,
        w.company_id.as_str(),
        current.user_id().as_str(),
        branch,
        department,
    )
    .await?
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "REQUEST_SCOPE_FORBIDDEN",
            "Choose a location within your authorized workspace.",
        )
        .into());
    }
    if !rfq_scope_company(&mut *c, w.company_id.as_str()).await? {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COMPANY_UNAVAILABLE",
            "The company is unavailable.",
        )
        .into());
    }
    if let Some(branch) = branch.filter(|v| !v.is_empty()) {
        if !rfq_scope_branch(&mut *c, w.company_id.as_str(), branch).await? {
            return Err(
                BusinessError::invalid("LOCATION_INVALID", "Select an active branch.").into(),
            );
        }
    }
    if let Some(department) = department.filter(|v| !v.is_empty()) {
        if !rfq_scope_department(&mut *c, w.company_id.as_str(), branch, department).await? {
            return Err(BusinessError::invalid(
                "LOCATION_INVALID",
                "Select a department in this branch.",
            )
            .into());
        }
    }
    source.require_current(c).await?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RfqRequestTeamHandler {
    pub user_id: String,
    pub responsibility: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RfqRequestDetailEvent {
    pub id: String,
    pub action: String,
    pub actor_id: String,
    pub actor_name: String,
    pub note: String,
    pub evidence_json: String,
    pub request_version: i64,
    pub created_at: i64,
}

/// Execute Records and People's original event/name query as one statement,
/// even when there are no events. Caller must authorize before this read and
/// preserve source fences; it establishes neither permission nor visibility.
pub(crate) async fn rfq_request_detail_events(
    c: &mut PgConnection,
    company: &str,
    request: &str,
) -> Result<Vec<RfqRequestDetailEvent>, Error> {
    let sql = format!(
        "WITH {}, {} SELECT e.id,e.action,e.actor_id,COALESCE((SELECT legal_name FROM rfq_request_people p WHERE p.company_id=e.company_id AND p.user_id=e.actor_id AND p.merged_into_id IS NULL LIMIT 1),'Company member'),e.note,e.evidence_json,e.request_version,e.created_at FROM rfq_request_events e WHERE request_id=$1 AND company_id=$2 ORDER BY created_at,id LIMIT 250",
        rfq_request_events_sql(),
        rfq_request_people_sql()
    );
    let rows: Vec<(String, String, String, String, String, String, i64, i64)> =
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(request)
            .bind(company)
            .fetch_all(c)
            .await?;
    if rows.iter().any(|row| {
        row.6.unsigned_abs() > 9_007_199_254_740_991 || row.7.unsigned_abs() > 9_007_199_254_740_991
    }) {
        // Unmounted signal; HTTP must preserve JS RangeError, not map this to
        // access denial. The cross-command error boundary remains pending.
        return Err(PersistenceError::new("DATABASE_NUMBER_OUT_OF_RANGE").into());
    }
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                action,
                actor_id,
                actor_name,
                note,
                evidence_json,
                request_version,
                created_at,
            )| RfqRequestDetailEvent {
                id,
                action,
                actor_id,
                actor_name,
                note,
                evidence_json,
                request_version,
                created_at,
            },
        )
        .collect())
}

/// Compose Support's active team rows, People names, Identity's current
/// principal mapping and the original scoped assignment predicate. This is
/// not a request visibility grant. Invoke only at TS teamHandlers' position,
/// after owner lookup, and retain/recheck all source facts at the boundary.
pub(crate) async fn rfq_request_team_handlers(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    team: Option<&str>,
) -> Result<Vec<RfqRequestTeamHandler>, Error> {
    let Some(team) = team.filter(|id| !id.is_empty()) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "WITH {}, {} SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) FROM rfq_request_team_members m JOIN rfq_request_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN rfq_request_people p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=$1 AND t.company_id=$2 AND t.active=1 ORDER BY m.user_id",
        rfq_request_team_sources_sql(),
        rfq_request_people_sql()
    );
    let rows: Vec<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(team)
        .bind(company)
        .fetch_all(&mut *c)
        .await?;
    let ids: Vec<String> = rows.iter().map(|row| row.0.clone()).collect();
    let principals: HashMap<String, String> =
        rfq_request_current_principals(&mut *c, company, &ids)
            .await?
            .into_iter()
            .collect();
    // JS Map preserves a key's first insertion position while replacing its
    // value on later duplicates after principal resolution.
    let mut positions = HashMap::<String, usize>::new();
    let mut unique = Vec::<RfqRequestTeamHandler>::new();
    for (user, responsibility, name) in rows {
        let current = principals.get(&user).cloned().unwrap_or(user);
        let handler = RfqRequestTeamHandler {
            user_id: current.clone(),
            responsibility,
            name,
        };
        if let Some(index) = positions.get(&current) {
            unique[*index] = handler;
        } else {
            positions.insert(current, unique.len());
            unique.push(handler);
        }
    }
    let mut eligible = Vec::new();
    for handler in unique {
        if rfq_request_service_member(&mut *c, company, &handler.user_id, branch, department)
            .await?
        {
            eligible.push(handler);
        }
    }
    Ok(eligible)
}

/// No variant grants Records or Procurement authority. The caller must hold
/// the actual selected Identity context and retain its transaction fence.
pub(crate) enum RfqInitialRequestAccess {
    Missing,
    JsonSyntax,
    PropertyTypeError,
    RuntimeCoercionUnproven,
    DepartmentPolicyRequired,
    NotFound,
    ApprovalRouteRequired {
        owner: bool,
        handlers: Vec<RfqRequestTeamHandler>,
    },
    DetailRequired {
        owner: bool,
        handlers: Vec<RfqRequestTeamHandler>,
    },
}

/// Follow requestAccess through its initial designated-step and workspace
/// check, then the source's serviceOwner and teamHandlers reads in order.
/// No events, children, attachments or unfinished approval route are read
/// early. `policy_now` must be supplied by the trusted policy clock.
pub(crate) async fn initial_rfq_request_access(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    source: &CurrentRfqQuotationRow,
    policy_now: i64,
) -> Result<RfqInitialRequestAccess, Error> {
    let workspace = current.selection().workspace();
    if source.company() != &workspace.company_id {
        return Err(PersistenceError::new("RFQ_REQUEST_COMPANY_MISMATCH").into());
    }
    source.require_current(&mut *c).await?;
    let Some(image) = source.image_json() else {
        return Ok(RfqInitialRequestAccess::Missing);
    };
    let row: serde_json::Value = serde_json::from_str(image)
        .map_err(|_| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    let approval = row
        .get("approval_json")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    let designated = rfq_designated_step(approval, |id| current.same_person_user_raw(id));
    let result = match designated {
        RfqDesignatedStep::JsonSyntax => RfqInitialRequestAccess::JsonSyntax,
        RfqDesignatedStep::NullProperty | RfqDesignatedStep::PropertyKeyTypeError => {
            RfqInitialRequestAccess::PropertyTypeError
        }
        RfqDesignatedStep::RuntimeCoercionUnproven => {
            RfqInitialRequestAccess::RuntimeCoercionUnproven
        }
        RfqDesignatedStep::DepartmentPolicy => RfqInitialRequestAccess::DepartmentPolicyRequired,
        RfqDesignatedStep::None => {
            let branch = row.get("branch_id").and_then(|v| v.as_str());
            let department = row.get("department_id").and_then(|v| v.as_str());
            let in_workspace = rfq_in_service_workspace(
                workspace.company_id.as_str(),
                workspace.branch_id.as_ref().map(|id| id.as_str()),
                workspace.department_id.as_ref().map(|id| id.as_str()),
                workspace.company_id.as_str(),
                branch,
                department,
            );
            if !in_workspace
                || !rfq_request_service_member(
                    &mut *c,
                    workspace.company_id.as_str(),
                    current.user_id().as_str(),
                    branch,
                    department,
                )
                .await?
            {
                RfqInitialRequestAccess::NotFound
            } else {
                let owner = rfq_request_service_owner(
                    &mut *c,
                    current.selection().assignment().id.as_str(),
                    workspace.company_id.as_str(),
                    policy_now,
                )
                .await?;
                let team = row.get("team_id").and_then(|v| v.as_str());
                let handlers = rfq_request_team_handlers(
                    &mut *c,
                    workspace.company_id.as_str(),
                    branch,
                    department,
                    team,
                )
                .await?;
                let state = row
                    .get("state")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
                if state == "awaiting_approval" {
                    // Source computes route/pool before requester, history and
                    // the final visibility predicate; that owner work remains.
                    RfqInitialRequestAccess::ApprovalRouteRequired { owner, handlers }
                } else {
                    let own = ["requester_user_id", "created_by"].iter().any(|key| {
                        row.get(*key)
                            .and_then(|v| v.as_str())
                            .is_some_and(|id| current.same_person_user_raw(id))
                    });
                    let member = handlers
                        .iter()
                        .any(|handler| handler.user_id == current.user_id().as_str());
                    let visible = if own || owner || member {
                        RfqPriorReviewer::Yes
                    } else {
                        rfq_prior_reviewer(approval, |id| current.same_person_user_raw(id))
                    };
                    match visible {
                        RfqPriorReviewer::Yes => {
                            RfqInitialRequestAccess::DetailRequired { owner, handlers }
                        }
                        RfqPriorReviewer::No => RfqInitialRequestAccess::NotFound,
                        RfqPriorReviewer::JsonSyntax => RfqInitialRequestAccess::JsonSyntax,
                        RfqPriorReviewer::PropertyTypeError => {
                            RfqInitialRequestAccess::PropertyTypeError
                        }
                    }
                }
            }
        }
    };
    source.require_current(c).await?;
    Ok(result)
}
