//! Native Governance visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Governance slice:
//!
//! - `iam_access_review`: `canReadAccessReviewAlert` — one boolean SELECT
//!   (holding-scope assignment, `iam_assignment_active`, `iam_role_allows`
//!   `'iam.review'` over the campaign's company); true resolves the
//!   retained `` `/people/settings?section=reviews#${encodeURIComponent(id)}` ``
//!   href, false drops the row. No source override.
//! - `organization_change`: `organizationChangeNotificationTarget` — the
//!   scope rule (`organization.manage`, holding assignment scope, holding
//!   frame) and the fresh-assignment proof, then the change existence
//!   check; a missing proof or a missing row drops the row (`null`). The
//!   href is the retained `organizationChangeHref` (legacy view with the
//!   `organization-change-` fragment). No source override.
//! - `company_business_acceptance`: `companyBusinessAcceptanceWork` — the
//!   holding frame, `organization.manage` and the Company authority, then
//!   the acceptance row (anything but `draft`/`revoked`); the notified id
//!   must equal the reader's company (the work item id). A missing proof,
//!   a missing row or an id mismatch drops the row (empty list /
//!   unmatched find). The href is the retained constant
//!   `/settings/business-readiness#company-setup-acceptance`. No source
//!   override.
//!
//! IMPORTANT CHAINING NOTE: unlike the purchase-gated delegates, the
//! oracles here drop invisible rows (boolean false, `null`, absent item)
//! instead of throwing. This delegate therefore returns `Ok(None)` for
//! invisible rows and `Err` only for storage failures (or a misconfigured
//! environment, of which there is none — this slice has no purchase gate).
//! The retained per-row drop mapping lives in the port, exactly like the
//! TypeScript `visibleNotifications` catch-and-continue.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static permission halves (`iam.review`,
//!   `organization.manage`) are enforced database-current via
//!   `iam_role_allows` on the resolved assignment. A stale claim fails
//!   closed to `Ok(None)` instead of an assignment-selection denial.
//! - The review alert has no purchase gate and no principal check in the
//!   oracle; both absences are mirrored exactly.
//! - The organization-change query params are encoded with the retained
//!   `encodeURIComponent` mirror; the oracle builds them with
//!   `URLSearchParams`, which differs only for exotic ids (space becomes
//!   `+`). Seeds use simple ids where both agree.
//! - The acceptance beta workspaces are not distinguished: the port
//!   carries no beta flag, and the oracle returns `[]` for beta actors.
//!   A beta reader would resolve here while the oracle drops them. Seeds
//!   are non-beta.
//! - Presentation joins and facts (change rows, checks, events,
//!   relationships, queues) are not visibility: rows passing the identity
//!   proof resolve without them.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgConnection;

/// Source types this delegate can prove against Governance rows.
pub fn is_governance_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "iam_access_review" | "organization_change" | "company_business_acceptance"
    )
}

/// Retained destination address for one access-review campaign,
/// byte-identical to the TypeScript
/// `` `/people/settings?section=reviews#${encodeURIComponent(id)}` ``.
pub fn access_review_href(source_id: &str) -> String {
    format!(
        "/people/settings?section=reviews#{}",
        encode_uri_component(source_id)
    )
}

/// Retained destination address for one organization change, byte-identical
/// to the retained `organizationChangeHref` for simple ids.
pub fn organization_change_href(source_id: &str, company_id: &str) -> String {
    format!(
        "/people/organization?view=legacy&company={}&tab=changes&changeId={}#organization-change-{}",
        encode_uri_component(company_id),
        encode_uri_component(source_id),
        encode_uri_component(source_id),
    )
}

/// Retained destination address for the Company acceptance work item: the
/// constant the oracle attaches to the matched item.
pub fn business_acceptance_href() -> String {
    "/settings/business-readiness#company-setup-acceptance".to_owned()
}

/// Retained authority per source type. None of the three reads settles a
/// commercial capability (the oracles have no purchase gate here); the
/// proof is the assignment authority alone.
pub fn governance_history_capabilities(source_type: &str) -> &'static [&'static str] {
    match source_type {
        "iam_access_review" | "organization_change" | "company_business_acceptance" => &[],
        _ => &[],
    }
}

/// Retained database-current permission behind each governance read.
pub fn governance_required_permission(source_type: &str) -> Option<&'static str> {
    match source_type {
        "iam_access_review" => Some("iam.review"),
        "organization_change" | "company_business_acceptance" => Some("organization.manage"),
        _ => None,
    }
}

fn unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_SOURCE_UNSUPPORTED",
        "This Inbox source is not part of the retained projection.",
    )
}

fn owner_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_PORT_UNAVAILABLE",
        "This Inbox destination requires its owning workspace.",
    )
}

fn storage() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_QUERY_FAILED",
        "The Inbox destination is unavailable.",
    )
}

/// Native Governance visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Governance-only: every other source
/// type defers to the existing fail-closed behavior instead of inventing a
/// link. Invisible rows resolve to `Ok(None)` — the oracles drop rather
/// than throw on this slice.
pub struct GovernanceInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
}

impl GovernanceInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
        }
    }

    /// Bind the exact selected assignment. Without it the unit resolves the
    /// highest active assignment for the current user and Company.
    pub fn assignment(mut self, assignment_id: Option<&str>) -> Self {
        self.assignment_id = assignment_id
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        self
    }
}

impl Default for GovernanceInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

struct GovernanceGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    now: i64,
}

impl GovernanceGate {
    fn holding_frame(&self) -> bool {
        self.branch_id.is_none() && self.department_id.is_none()
    }
}

/// Resolve the reader's assignment: the bound selection, else the highest
/// active assignment for the current user and Company.
async fn resolve_assignment(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    now: i64,
) -> Result<Option<(String, String)>, BusinessError> {
    sqlx::query_as(
        "SELECT g.id,g.role FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.revoked_at IS NULL \
          AND g.valid_from<=$4 AND g.valid_until>$4 \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Database-current half of the retained JWT-static permission checks.
async fn role_allows(
    c: &mut PgConnection,
    company_id: &str,
    assignment_id: &str,
    permission: &str,
) -> Result<bool, BusinessError> {
    sqlx::query_scalar(
        "SELECT iam_role_allows((SELECT role FROM assignments WHERE id=$1), $2, $3)",
    )
    .bind(assignment_id)
    .bind(company_id)
    .bind(permission)
    .fetch_one(&mut *c)
    .await
    .map_err(|_| storage())
}

/// Retained review-alert proof: one boolean SELECT over the campaign's
/// company — holding-scope assignment, active, allowing `iam.review`.
/// False reads as invisible (`Ok(None)`), exactly like the oracle.
async fn review_target(
    c: &mut PgConnection,
    gate: &GovernanceGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM iam_access_review_campaigns c \
          JOIN assignments a ON a.company_id=c.company_id AND a.user_id=$1 \
          WHERE c.id=$2 AND c.company_id=$3 \
            AND a.scope_kind='holding' \
            AND a.revoked_at IS NULL \
            AND iam_assignment_active(a.id,$4) \
            AND iam_role_allows(a.role,a.company_id,'iam.review') \
          LIMIT 1",
    )
    .bind(gate.user_id.as_str())
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if hit.is_none() {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(access_review_href(source_id)),
        source: None,
    }))
}

/// Retained organization-change proof: the scope rule (manage permission,
/// holding assignment scope, holding frame) and the fresh-assignment
/// proof, then the change existence check. Any miss reads as invisible.
async fn organization_change_target(
    c: &mut PgConnection,
    gate: &GovernanceGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (id, role),
        None => return Ok(None),
    };
    if !role_allows(c, gate.company.as_str(), &assignment_id, "organization.manage").await? {
        return Ok(None);
    }
    let holding_assignment: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g WHERE g.id=$1 AND g.scope_kind='holding'",
    )
    .bind(assignment_id.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if holding_assignment.is_none() || !gate.holding_frame() {
        return Ok(None);
    }
    // Fresh-assignment proof, literal mirror of
    // `organizationChangeAssignmentActive`: the exact assignment row is
    // current, holding-scoped, role-allowing, over the active company.
    let fresh: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a JOIN organization_nodes c ON c.id=a.company_id AND c.status='active' \
          WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.role=$4 \
          AND a.scope_kind='holding' AND a.revoked_at IS NULL \
          AND iam_role_allows(a.role,a.company_id,'organization.manage') \
          AND iam_assignment_active(a.id,$5) \
          AND iam_principal_active(a.user_id,a.company_id,$5)",
    )
    .bind(assignment_id.as_str())
    .bind(gate.user_id.as_str())
    .bind(gate.company.as_str())
    .bind(role.as_str())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if fresh.is_none() {
        return Ok(None);
    }
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM organization_node_changes WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if exists.is_none() {
        return Ok(None);
    }
    Ok(Some(InboxTarget {
        href: Some(organization_change_href(source_id, gate.company.as_str())),
        source: None,
    }))
}

/// Retained Company-acceptance proof: the holding frame, the manage
/// permission and the Company authority, then the acceptance row (anything
/// but `draft`/`revoked`) whose item id equals the reader's company. Any
/// miss reads as invisible.
async fn acceptance_target(
    c: &mut PgConnection,
    gate: &GovernanceGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    if !gate.holding_frame() {
        return Ok(None);
    }
    // The work item id is the reader's company; an unrelated notified id
    // never matches the list find.
    if source_id != gate.company.as_str() {
        return Ok(None);
    }
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let assignment_id = match assignment {
        Some((id, _)) => id,
        None => return Ok(None),
    };
    if !role_allows(c, gate.company.as_str(), &assignment_id, "organization.manage").await? {
        return Ok(None);
    }
    // Company authority, literal mirror of `readCompanyAuthorityEffect`:
    // the exact assignment is holding-scoped, current, principal-active
    // and role-allowing.
    let authority: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 \
          AND iam_role_allows(role,company_id,'organization.manage') \
          AND iam_assignment_active(id,$3) \
          AND scope_kind='holding' AND revoked_at IS NULL \
          AND iam_assignment_active(id,$3) \
          AND iam_principal_active(user_id,company_id,$3) \
          AND valid_from<=$3 AND valid_until>$3 LIMIT 1",
    )
    .bind(assignment_id.as_str())
    .bind(gate.company.as_str())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if authority.is_none() {
        return Ok(None);
    }
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM company_business_acceptance WHERE company_id=$1",
    )
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    match status.as_deref() {
        Some("draft") | Some("revoked") | None => Ok(None),
        Some(_) => Ok(Some(InboxTarget {
            href: Some(business_acceptance_href()),
            source: None,
        })),
    }
}

impl InboxTargetPort for GovernanceInboxTargets {
    fn target<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            let company = CompanyId::new(company_id).map_err(|_| storage())?;
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = GovernanceGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                now,
            };
            match source_type {
                "iam_access_review" => review_target(connection, &gate, source_id).await,
                "organization_change" => {
                    organization_change_target(connection, &gate, source_id).await
                }
                "company_business_acceptance" => {
                    acceptance_target(connection, &gate, source_id).await
                }
                _ if retained_inbox_destination(source_type, source_id).is_some() => {
                    Err(owner_unavailable())
                }
                _ => Err(unsupported()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::inbox_targets::source_record_label;

    #[test]
    fn routing_covers_only_the_governance_slice() {
        for source_type in [
            "iam_access_review",
            "organization_change",
            "company_business_acceptance",
        ] {
            assert!(is_governance_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "leave",
            "expense",
            "request",
            "task",
            "post",
            "operational_exception",
            "future_source",
            "",
        ] {
            assert!(!is_governance_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(
            access_review_href("camp-1"),
            "/people/settings?section=reviews#camp-1"
        );
        assert_eq!(
            access_review_href("a/b c"),
            "/people/settings?section=reviews#a%2Fb%20c"
        );
        assert_eq!(
            organization_change_href("chg-1", "co"),
            "/people/organization?view=legacy&company=co&tab=changes&changeId=chg-1#organization-change-chg-1"
        );
        assert_eq!(
            organization_change_href("a/b c", "co"),
            "/people/organization?view=legacy&company=co&tab=changes&changeId=a%2Fb%20c#organization-change-a%2Fb%20c"
        );
        assert_eq!(
            business_acceptance_href(),
            "/settings/business-readiness#company-setup-acceptance"
        );
    }

    #[test]
    fn authority_mapping_matches_the_retained_gates() {
        assert_eq!(governance_history_capabilities("iam_access_review"), &[] as &[&str]);
        assert_eq!(
            governance_history_capabilities("organization_change"),
            &[] as &[&str]
        );
        assert_eq!(
            governance_history_capabilities("company_business_acceptance"),
            &[] as &[&str]
        );
        assert_eq!(
            governance_history_capabilities("organization"),
            &[] as &[&str]
        );
        assert_eq!(governance_history_capabilities(""), &[] as &[&str]);
        assert_eq!(
            governance_required_permission("iam_access_review"),
            Some("iam.review")
        );
        assert_eq!(
            governance_required_permission("organization_change"),
            Some("organization.manage")
        );
        assert_eq!(
            governance_required_permission("company_business_acceptance"),
            Some("organization.manage")
        );
        assert_eq!(governance_required_permission("organization"), None);
        assert_eq!(governance_required_permission(""), None);
    }

    #[test]
    fn governance_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("iam_access_review"), "Iam access review");
        assert_eq!(source_record_label("organization_change"), "Organization change");
        assert_eq!(
            source_record_label("company_business_acceptance"),
            "Company business acceptance"
        );
        // No source override on this slice: the final map applies these
        // retained defaults.
        assert!(retained_inbox_destination("iam_access_review", "x").is_some());
        assert!(retained_inbox_destination("organization_change", "x").is_some());
        assert!(retained_inbox_destination("company_business_acceptance", "x").is_some());
    }
}
