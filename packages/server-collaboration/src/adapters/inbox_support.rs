//! Native Support visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript gates for the
//! notification-visible Support sources:
//!
//! - `support_sla_alert`: `getSupportAlert` — `support.sla` history gate plus
//!   the current `support.read` role and a frame-scoped alert behind its
//!   ticket; 404 `SUPPORT_ALERT_NOT_FOUND` when missing.
//! - `support_policy`: `getSupportPolicy` — `support.sla` history gate plus
//!   the current `support.read` role and a company-scoped policy read; 404
//!   `SUPPORT_POLICY_NOT_FOUND` when missing. Policies are company-wide: the
//!   selected Work frame never filters them, exactly like the oracle.
//! - `support_ticket`: `getSupportTicket` — `support.tickets` history gate,
//!   the scoped ticket read with the retained read predicate (`support.read`,
//!   or a non-`sales_order` record the reader created with `records.read`),
//!   and the linked-source check; 404/403 `SUPPORT_TICKET_NOT_FOUND` when the
//!   row is missing (403 exactly when the reader lacks `support.read`) and
//!   404 with the source message when a linked source record is present.
//! - `support_incident`: `getSupportIncident` — `support.incidents` history
//!   gate plus the scoped incident read with the retained assignment
//!   predicate; 404 `SUPPORT_INCIDENT_NOT_FOUND` when missing.
//! - `support_session`: `supportSessionDetail` — `support.tickets` history
//!   gate plus the scoped session read where the reader created it, is the
//!   requester, or satisfies the retained manager review predicate; 404
//!   `SUPPORT_SESSION_NOT_FOUND` when missing.
//!
//! The retained per-row catch swallows 403/404 for the notification page, so
//! composition drops these denials exactly like the retained catch: every
//! negative path here surfaces as `Err` with the exact TypeScript code.
//! None of the sources sets an override: the composition final map applies
//! the retained defaults.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The JWT-static `hasPermission` halves are enforced database-current via
//!   `iam_role_allows` on the reader's resolved assignment. Alert and policy
//!   reads have no assignment predicate in the oracle; the role check is the
//!   only authority gate.
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing, purchase boundaries) authorize commands, not
//!   reads, and are not part of this read-only port.
//! - Linked ticket sources (`source_record_json` set) fail closed with the
//!   retained source-unavailable code: proving another owner's source record
//!   belongs to that owner, not this port. Self-contained tickets (no linked
//!   source) are unaffected.
//! - `iam_principal_active` is mirrored exactly where the oracle calls it
//!   (incident and ticket predicates, session reviewer, alert/policy role
//!   checks via assignment currency); the command-side guards omit it
//!   exactly like their oracles.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{encode_uri_component, retained_inbox_destination};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Support rows.
pub fn is_support_inbox_target(source_type: &str) -> bool {
    matches!(
        source_type,
        "support_sla_alert"
            | "support_incident"
            | "support_policy"
            | "support_ticket"
            | "support_session"
    )
}

/// Retained destination address for one incident: the raw id, exactly like
/// the TypeScript `` `/support/incidents/${id}` ``.
pub fn support_incident_href(source_id: &str) -> String {
    format!("/support/incidents/{source_id}")
}

/// Retained destination address for one ticket, byte-identical to the
/// TypeScript `` `/support/${encodeURIComponent(id)}` ``.
pub fn support_ticket_href(source_id: &str) -> String {
    format!("/support/{}", encode_uri_component(source_id))
}

/// Retained destination address for one session, byte-identical to the
/// TypeScript `` `/support/sessions/${encodeURIComponent(id)}` ``.
pub fn support_session_href(source_id: &str) -> String {
    format!("/support/sessions/{}", encode_uri_component(source_id))
}

/// Retained destination addresses for the static SLA routes.
pub fn support_sla_href() -> String {
    "/support/sla".into()
}

/// Retained read capability per source type.
pub fn support_history_capability(source_type: &str) -> Option<&'static str> {
    match source_type {
        "support_sla_alert" | "support_policy" => Some("support.sla"),
        "support_ticket" | "support_session" => Some("support.tickets"),
        "support_incident" => Some("support.incidents"),
        _ => None,
    }
}

fn alert_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_ALERT_NOT_FOUND",
        "Target escalation is not available in this scope.",
    )
}

fn policy_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_POLICY_NOT_FOUND",
        "Policy not found in this Company.",
    )
}

fn ticket_not_found(kind: FailureKind) -> BusinessError {
    BusinessError::new(
        kind,
        "SUPPORT_TICKET_NOT_FOUND",
        "Ticket not found in this scope.",
    )
}

fn ticket_source_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_TICKET_NOT_FOUND",
        "Ticket source is unavailable in this scope.",
    )
}

fn incident_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_INCIDENT_NOT_FOUND",
        "Incident is not available in this scope.",
    )
}

fn session_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_SESSION_NOT_FOUND",
        "Support session not found in this scope.",
    )
}

fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
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

fn pivot(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => storage(),
    }
}

/// Native Support visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Support-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct SupportInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl SupportInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
            enforcement: None,
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

    /// Pin the package-enforcement mode. Without it the delegate mirrors the
    /// retained support environment-driven mode exactly. This seam exists so
    /// tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for SupportInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained `supportPackageEnforcementMode`, mirrored from the TypeScript
/// oracle: disabled opens the read, a non-local environment fails with the
/// environment code, and an unknown value fails closed with the
/// configuration code.
fn enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    match std::env::var("ERPX_SUPPORT_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PACKAGE_ENFORCEMENT_ENVIRONMENT",
                    "Package enforcement is unavailable in this environment.",
                ));
            }
            Ok(PackageEnforcementMode::Enforced)
        }
        _ => Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_CONFIGURATION",
            "Package enforcement is unavailable.",
        )),
    }
}

/// Retained purchase-history gate through the native Commercial capability
/// port. The refusal carries the exact decision code and message from the
/// TypeScript throw.
async fn purchase_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(());
    }
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
    reader
        .require(
            capability,
            CapabilityPurpose::History,
            mode,
            TimestampMillis::new(now),
        )
        .await
        .map_err(pivot)?;
    Ok(())
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
/// `false` means the oracle's `requirePermission` would throw 403
/// `PERMISSION_REQUIRED`.
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

struct SupportGate {
    company: CompanyId,
    user_id: String,
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    mode: PackageEnforcementMode,
    now: i64,
}

/// Retained alert proof: history gate, current `support.read` role, and a
/// frame-scoped alert behind its ticket. Authority failures deny with the
/// retained 403 `PERMISSION_REQUIRED`; missing rows deny with the retained
/// 404, exactly like the oracle's authorize-then-read order.
async fn alert_target(
    c: &mut PgConnection,
    gate: &SupportGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(c, &gate.company, "support.sla", gate.mode, gate.now).await?;
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let allowed = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
        None => false,
    };
    if !allowed {
        return Err(permission_required());
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT s.id FROM support_sla_alerts s \
          JOIN support_tickets t ON t.id=s.ticket_id \
          WHERE s.id=$1 AND t.company_id=$2 \
          AND ($3::text IS NULL OR t.branch_id=$3) \
          AND ($4::text IS NULL OR t.department_id=$4)",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(alert_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(support_sla_href()),
        source: None,
    }))
}

/// Retained policy proof: history gate, current `support.read` role, and a
/// company-scoped policy row. Policies are company-wide by oracle design.
/// Authority failures deny with 403 `PERMISSION_REQUIRED`; missing rows deny
/// with the retained 404.
async fn policy_target(
    c: &mut PgConnection,
    gate: &SupportGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(c, &gate.company, "support.sla", gate.mode, gate.now).await?;
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let allowed = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
        None => false,
    };
    if !allowed {
        return Err(permission_required());
    }
    let visible: Option<String> = sqlx::query_scalar(
        "SELECT id FROM support_sla_policies WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if visible.is_none() {
        return Err(policy_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(support_sla_href()),
        source: None,
    }))
}

/// Retained ticket proof: history gate, the scoped ticket row with the
/// retained read predicate, and the linked-source check. Missing rows deny
/// with 404 — or 403 exactly when the reader lacks `support.read`, mirroring
/// the oracle's status split. Tickets carrying a linked source record deny
/// with the retained source message instead of inventing the owner proof.
async fn ticket_target(
    c: &mut PgConnection,
    gate: &SupportGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "support.tickets",
        gate.mode,
        gate.now,
    )
    .await?;
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    // Retained `supportRead` authorize: `support.read`, or `records.read` for
    // the reporter fallback. Neither database-current means the oracle's
    // 403 `PERMISSION_REQUIRED`.
    let readable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
        None => false,
    };
    let writable = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "records.read").await?,
        None => false,
    };
    if !readable && !writable {
        return Err(permission_required());
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (Some(id), Some(role)),
        None => (None, None),
    };
    let row = sqlx::query(
        "SELECT t.id,t.source_kind,t.source_record_json,t.branch_id,t.department_id \
          FROM support_tickets t \
          WHERE t.id=$1 AND t.company_id=$2 \
          AND ($3::text IS NULL OR t.branch_id=$3) \
          AND ($4::text IS NULL OR t.department_id=$4) \
          AND EXISTS(SELECT 1 FROM assignments g \
            WHERE g.id=$5 AND g.user_id=$6 AND g.role=$7 AND g.company_id=t.company_id \
            AND iam_assignment_active(g.id,$8) \
            AND iam_principal_active(g.user_id,g.company_id,$8) \
            AND (g.scope_kind='holding' \
              OR (g.scope_kind='branch' AND g.scope_node_id=t.branch_id) \
              OR (g.scope_kind='department' AND g.scope_node_id=t.department_id)) \
            AND (iam_role_allows(g.role,g.company_id,'support.read') \
              OR t.source_kind<>'sales_order' AND t.created_by=g.user_id \
                AND iam_role_allows(g.role,g.company_id,'records.read')))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .bind(assignment_id.as_deref())
    .bind(&gate.user_id)
    .bind(role.as_deref())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        let readable = match &assignment_id {
            Some(id) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
            None => false,
        };
        return Err(ticket_not_found(if readable {
            FailureKind::NotFound
        } else {
            FailureKind::Forbidden
        }));
    };
    let source_record: Option<String> = row.try_get("source_record_json").map_err(|_| storage())?;
    if source_record.as_deref().is_some_and(|value| !value.is_empty()) {
        return Err(ticket_source_not_found());
    }
    let href = support_ticket_href(source_id);
    Ok(Some(InboxTarget {
        href: Some(href),
        source: None,
    }))
}

/// Retained incident proof: history gate plus the scoped incident read with
/// the retained `support.read` assignment predicate over the record scope.
async fn incident_target(
    c: &mut PgConnection,
    gate: &SupportGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "support.incidents",
        gate.mode,
        gate.now,
    )
    .await?;
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    // Retained authorize: the oracle throws 403 before the scoped read when
    // the reader lacks `support.read`. The missing row then denies with the
    // retained 404.
    let allowed = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
        None => false,
    };
    if !allowed {
        return Err(permission_required());
    }
    let (assignment_id, role) = match assignment {
        Some((id, role)) => (Some(id), Some(role)),
        None => (None, None),
    };
    let row = sqlx::query(
        "SELECT t.id,t.branch_id,t.department_id FROM support_incidents t \
          WHERE t.id=$1 AND t.company_id=$2 \
          AND ($3::text IS NULL OR t.branch_id=$3) \
          AND ($4::text IS NULL OR t.department_id=$4) \
          AND EXISTS(SELECT 1 FROM assignments g \
            WHERE g.id=$5 AND g.user_id=$6 AND g.role=$7 AND g.company_id=t.company_id \
            AND iam_assignment_active(g.id,$8) \
            AND iam_principal_active(g.user_id,g.company_id,$8) \
            AND iam_role_allows(g.role,g.company_id,'support.read') \
            AND (g.scope_kind='holding' \
              OR (g.scope_kind='branch' AND g.scope_node_id=t.branch_id) \
              OR (g.scope_kind='department' AND g.scope_node_id=t.department_id)))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .bind(assignment_id.as_deref())
    .bind(&gate.user_id)
    .bind(role.as_deref())
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if row.is_none() {
        return Err(incident_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(support_incident_href(source_id)),
        source: None,
    }))
}

/// Retained session proof: history gate, the current `support.read` role,
/// and the scoped session read where the reader created it, is the
/// requester, or satisfies the retained manager review predicate. The scope
/// filters the linked ticket and the reviewer predicate ranges over it,
/// exactly like the oracle workspace join.
async fn session_target(
    c: &mut PgConnection,
    gate: &SupportGate,
    source_id: &str,
) -> Result<Option<InboxTarget>, BusinessError> {
    purchase_history(
        c,
        &gate.company,
        "support.tickets",
        gate.mode,
        gate.now,
    )
    .await?;
    let assignment = resolve_assignment(
        c,
        gate.company.as_str(),
        &gate.user_id,
        gate.assignment_id.as_deref(),
        gate.now,
    )
    .await?;
    let allowed = match &assignment {
        Some((id, _)) => role_allows(c, gate.company.as_str(), id, "support.read").await?,
        None => false,
    };
    if !allowed {
        return Err(permission_required());
    }
    let row = sqlx::query(
        "SELECT s.id FROM support_sessions s \
          JOIN support_tickets t ON t.id=s.ticket_id \
          WHERE s.id=$1 AND t.company_id=$2 \
          AND ($3::text IS NULL OR t.branch_id=$3) \
          AND ($4::text IS NULL OR t.department_id=$4) \
          AND (s.created_by=$5 OR s.requested_for=$6 OR EXISTS( \
            SELECT 1 FROM assignments m \
            JOIN hr_people p ON p.user_id=m.user_id AND p.company_id=m.company_id \
            JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
            WHERE m.company_id=t.company_id AND m.user_id=$7 \
            AND iam_assignment_active(m.id,$8) \
            AND iam_principal_active(m.user_id,m.company_id,$8) \
            AND iam_role_allows(m.role,m.company_id,'support.manage') \
            AND (m.scope_kind='holding' \
              OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) \
              OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) \
            AND p.status='active' AND p.merged_into_id IS NULL \
            AND e.status='active' \
            AND e.start_date<=to_char((to_timestamp($8/1000.0) AT TIME ZONE 'UTC'),'YYYY-MM-DD') \
            AND (e.end_date IS NULL OR e.end_date>=to_char((to_timestamp($8/1000.0) AT TIME ZONE 'UTC'),'YYYY-MM-DD'))))",
    )
    .bind(source_id)
    .bind(gate.company.as_str())
    .bind(gate.branch_id.as_deref())
    .bind(gate.department_id.as_deref())
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(&gate.user_id)
    .bind(gate.now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if row.is_none() {
        return Err(session_not_found());
    }
    Ok(Some(InboxTarget {
        href: Some(support_session_href(source_id)),
        source: None,
    }))
}

impl InboxTargetPort for SupportInboxTargets {
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
            let mode = match self.enforcement {
                Some(mode) => mode,
                None => enforcement_mode()?,
            };
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            let gate = SupportGate {
                company,
                user_id: user_id.to_owned(),
                assignment_id: self.assignment_id.clone(),
                branch_id: self.branch_id.clone(),
                department_id: self.department_id.clone(),
                mode,
                now,
            };
            match source_type {
                "support_sla_alert" => alert_target(connection, &gate, source_id).await,
                "support_policy" => policy_target(connection, &gate, source_id).await,
                "support_ticket" => ticket_target(connection, &gate, source_id).await,
                "support_incident" => incident_target(connection, &gate, source_id).await,
                "support_session" => session_target(connection, &gate, source_id).await,
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
    fn routing_covers_only_the_support_slice() {
        for source_type in [
            "support_sla_alert",
            "support_incident",
            "support_policy",
            "support_ticket",
            "support_session",
        ] {
            assert!(is_support_inbox_target(source_type), "{source_type}");
        }
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "commerce_order",
            "invoice",
            "payroll_correction",
            "roster_week",
            "overtime",
            "support",
            "future_source",
            "",
        ] {
            assert!(!is_support_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(support_sla_href(), "/support/sla");
        assert_eq!(support_incident_href("inc-1/2"), "/support/incidents/inc-1/2");
        assert_eq!(support_ticket_href("seg/one two"), "/support/seg%2Fone%20two");
        assert_eq!(support_ticket_href("t-1"), "/support/t-1");
        assert_eq!(
            support_session_href("seg/one two"),
            "/support/sessions/seg%2Fone%20two"
        );
        assert_eq!(support_session_href("ses-1"), "/support/sessions/ses-1");
    }

    #[test]
    fn history_capabilities_match_the_retained_gates() {
        assert_eq!(support_history_capability("support_sla_alert"), Some("support.sla"));
        assert_eq!(support_history_capability("support_policy"), Some("support.sla"));
        assert_eq!(support_history_capability("support_ticket"), Some("support.tickets"));
        assert_eq!(support_history_capability("support_session"), Some("support.tickets"));
        assert_eq!(
            support_history_capability("support_incident"),
            Some("support.incidents")
        );
        assert_eq!(support_history_capability("support"), None);
        assert_eq!(support_history_capability(""), None);
    }

    #[test]
    fn support_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("support_sla_alert"), "Support sla alert");
        assert_eq!(source_record_label("support_incident"), "Support incident");
        assert_eq!(source_record_label("support_policy"), "Support policy");
        assert_eq!(source_record_label("support_ticket"), "Support ticket");
        assert_eq!(source_record_label("support_session"), "Support session");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let permission = permission_required();
        assert_eq!(permission.kind, FailureKind::Forbidden);
        assert_eq!(permission.code, "PERMISSION_REQUIRED");
        assert_eq!(
            permission.message,
            "Your selected role does not grant this permission."
        );
        let alert = alert_not_found();
        assert_eq!(alert.kind, FailureKind::NotFound);
        assert_eq!(alert.code, "SUPPORT_ALERT_NOT_FOUND");
        assert_eq!(
            alert.message,
            "Target escalation is not available in this scope."
        );
        let policy = policy_not_found();
        assert_eq!(policy.kind, FailureKind::NotFound);
        assert_eq!(policy.code, "SUPPORT_POLICY_NOT_FOUND");
        assert_eq!(policy.message, "Policy not found in this Company.");
        let missing = ticket_not_found(FailureKind::NotFound);
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "SUPPORT_TICKET_NOT_FOUND");
        assert_eq!(missing.message, "Ticket not found in this scope.");
        let forbidden = ticket_not_found(FailureKind::Forbidden);
        assert_eq!(forbidden.kind, FailureKind::Forbidden);
        assert_eq!(forbidden.code, "SUPPORT_TICKET_NOT_FOUND");
        let source = ticket_source_not_found();
        assert_eq!(source.kind, FailureKind::NotFound);
        assert_eq!(source.code, "SUPPORT_TICKET_NOT_FOUND");
        assert_eq!(
            source.message,
            "Ticket source is unavailable in this scope."
        );
        let incident = incident_not_found();
        assert_eq!(incident.kind, FailureKind::NotFound);
        assert_eq!(incident.code, "SUPPORT_INCIDENT_NOT_FOUND");
        assert_eq!(incident.message, "Incident is not available in this scope.");
        let session = session_not_found();
        assert_eq!(session.kind, FailureKind::NotFound);
        assert_eq!(session.code, "SUPPORT_SESSION_NOT_FOUND");
        assert_eq!(session.message, "Support session not found in this scope.");
    }
}
