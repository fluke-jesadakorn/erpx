//! Support-owned ticket workspace and detail reads.
//!
//! Every query retains the exact selected Company/branch/department scope and
//! the current assignment/role read predicate used by the TypeScript reader.
//! Callers must supply the already-authorized selected context; this port only
//! reads support-owned facts and never opens, commits or retries a transaction.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Current selected Support reader. Permission booleans are presentation
/// inputs; row visibility is still re-evaluated by the SQL predicate.
#[derive(Clone, Debug)]
pub struct SupportReadScope {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub support_read: bool,
    pub records_read: bool,
    pub support_create: bool,
    pub support_work: bool,
    pub support_manage: bool,
    pub commerce_read: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketWorkspaceRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub due_date: Option<String>,
    pub owner_id: Option<String>,
    pub version: i64,
    pub updated_at: i64,
    pub source_kind: String,
    pub source_record_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportRecipient {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSlaPolicyChoice {
    pub id: String,
    pub name: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportOrderSource {
    pub id: String,
    pub number: String,
    pub version: i64,
    pub customer_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportSourceReference {
    pub kind: String,
    pub id: String,
    pub version: i64,
}

/// A stored source link is evidence; it never grants access to its owner.
pub fn parse_source_link(raw: &str) -> Option<SupportSourceReference> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let kind = value["kind"].as_str()?;
    let id = value["id"].as_str()?;
    if kind.is_empty()
        || id.is_empty()
        || id.encode_utf16().count() > 180
        || id.contains('\0')
    {
        return None;
    }
    Some(SupportSourceReference {
        kind: kind.to_owned(),
        id: id.to_owned(),
        version: value["version"].as_i64().filter(|version| *version > 0)?,
    })
}

fn serializable() -> Error {
    PersistenceError::new("SUPPORT_SERIALIZABLE_REQUIRED").into()
}

async fn require_serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(serializable());
    }
    Ok(())
}

fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPORT_READ_LIMIT",
        "The Support read exceeds this bounded response.",
    )
    .into()
}

/// The TypeScript `supportTicketReadPredicate`: support.read on the selected
/// Company assignment, or records.read for the reporter of a non-order report.
fn read_predicate(assignment: &str, user: &str, role: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM assignments g WHERE g.id={assignment} AND g.user_id={user} AND g.role={role} AND g.company_id=t.company_id
   AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
   AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id)
   AND (iam_role_allows(g.role,g.company_id,'support.read') OR t.source_kind<>'sales_order' AND t.created_by=g.user_id AND iam_role_allows(g.role,g.company_id,'records.read')))"
    )
}

/// Exact `supportRecipientEligibility()` predicate for a ticket owner.
fn recipient_eligibility() -> &'static str {
    "EXISTS(SELECT 1 FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))"
}

/// Exact `reviewers` projection retained by the TypeScript ticket reader.
fn reviewer_count() -> &'static str {
    "(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))))"
}

/// Bounded ticket list in the selected Company/branch/department scope.
pub async fn read_ticket_workspace(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<SupportTicketWorkspaceRow>, Error> {
    require_serializable(c).await?;
    let permission = read_predicate("$4", "$5", "$6");
    let sql = format!(
        "SELECT t.id,t.title,t.status,t.due_date,t.owner_id,t.version,t.updated_at,t.source_kind,t.source_record_json
         FROM support_tickets t
         WHERE t.company_id=$1 AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3)
           AND {permission}
         ORDER BY t.updated_at DESC,t.id LIMIT 200"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportTicketWorkspaceRow {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                due_date: row.try_get("due_date")?,
                owner_id: row.try_get("owner_id")?,
                version: row.try_get("version")?,
                updated_at: row.try_get("updated_at")?,
                source_kind: row.try_get("source_kind")?,
                source_record_json: row.try_get("source_record_json")?,
            })
        })
        .collect()
}

/// Full retained ticket row plus the same computed owner/order facts as the
/// TypeScript detail read. JSON is returned verbatim so column additions are
/// retained without a second hand-written projection.
pub async fn read_ticket_detail(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<Value>, Error> {
    require_serializable(c).await?;
    let permission = read_predicate("$5", "$6", "$7");
    let sql = format!(
        "SELECT ((to_jsonb(t) - 'sla_clock_json') || jsonb_build_object(
             'sla_clock_json',t.sla_clock_json::text,
             'order_number',o.number,
             'customer_name',p.name,
             'owner_name',(SELECT legal_name FROM hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1),
             'owner_eligible',CASE WHEN {eligible} THEN 1 ELSE 0 END,
             'reviewer_count',{reviewers}
         ))::text
         FROM support_tickets t
         LEFT JOIN commerce_orders o ON o.id=t.order_id AND o.company_id=t.company_id
         LEFT JOIN finance_parties p ON p.id=t.customer_id AND p.company_id=t.company_id
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
           AND {permission}",
        eligible = recipient_eligibility(),
        reviewers = reviewer_count(),
    );
    let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    serde_json::from_str(&raw).map(Some).map_err(|_| limited())
}

/// Active incident links visible from a retained ticket detail.
pub async fn read_ticket_incidents(
    c: &mut PgConnection,
    id: &str,
) -> Result<Vec<Value>, Error> {
    let raw: String = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id',i.id,'title',i.title,'status',i.status) ORDER BY i.updated_at DESC),'[]'::jsonb)::text
         FROM support_incident_tickets l JOIN support_incidents i ON i.id=l.incident_id
         WHERE l.ticket_id=$1 AND l.active",
    )
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    serde_json::from_str(&raw).map_err(|_| limited())
}

/// Immutable ticket event history, newest first.
pub async fn read_ticket_history(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let raw: String = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.version DESC),'[]'::jsonb)::text
         FROM support_ticket_events e WHERE e.ticket_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *c)
    .await?;
    serde_json::from_str(&raw).map_err(|_| limited())
}

/// Currently eligible ticket recipients. The caller already checked purchase
/// settlement authority before exposing this list.
pub async fn read_ticket_recipients(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Vec<SupportRecipient>, Error> {
    let eligible = recipient_eligibility().replace("t.owner_id", "m.user_id");
    let sql = format!(
        "SELECT DISTINCT m.user_id id,p.legal_name name
         FROM support_tickets t
         JOIN assignments m ON m.company_id=t.company_id
         JOIN hr_people p ON p.company_id=t.company_id AND p.user_id=m.user_id
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
           AND {eligible}
         ORDER BY p.legal_name"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportRecipient {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
            })
        })
        .collect()
}

/// Currently eligible recipients for an order-backed ticket create.
pub async fn read_order_recipients(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    order_id: &str,
) -> Result<Vec<SupportRecipient>, Error> {
    let eligible = recipient_eligibility();
    let sql = format!(
        "SELECT DISTINCT t.owner_id id,p.legal_name name
         FROM (SELECT o.company_id,o.branch_id,o.department_id,a.user_id owner_id
               FROM commerce_orders o JOIN assignments a ON a.company_id=o.company_id
               WHERE o.id=$1) t
         JOIN hr_people p ON p.company_id=t.company_id AND p.user_id=t.owner_id
         WHERE t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
           AND {eligible}
         ORDER BY p.legal_name"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportRecipient {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
            })
        })
        .collect()
}

/// Approved SLA policies offered by ticket detail.
pub async fn read_sla_policies(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<Vec<SupportSlaPolicyChoice>, Error> {
    let rows = sqlx::query(
        "SELECT id,name,version FROM support_sla_policies WHERE company_id=$1 AND status='approved' ORDER BY name",
    )
    .bind(company.as_str())
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportSlaPolicyChoice {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                version: row.try_get("version")?,
            })
        })
        .collect()
}

/// Active customer contact notes used by Support managers on order-backed tickets.
pub async fn read_customer_contact_notes(
    c: &mut PgConnection,
    company: &CompanyId,
    order_id: &str,
) -> Result<Vec<Value>, Error> {
    let raw: String = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'version',version,'channel',channel,'occurred_at',occurred_at,'body',body) ORDER BY occurred_at DESC),'[]'::jsonb)::text
         FROM customer_contact_notes WHERE order_id=$1 AND company_id=$2 AND status='active'",
    )
    .bind(order_id)
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await?;
    serde_json::from_str(&raw).map_err(|_| limited())
}

/// Order/quote sources offered by the ticket workspace create form.
pub async fn read_order_sources(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<SupportOrderSource>, Error> {
    let rows = sqlx::query(
        "SELECT t.id,t.number,t.version,p.name customer_name
         FROM commerce_orders t JOIN finance_parties p ON p.id=t.party_id
         WHERE t.company_id=$1 AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3)
           AND t.kind IN ('sales','quote') AND p.kind='customer'
         ORDER BY t.updated_at DESC LIMIT 200",
    )
    .bind(scope.company_id.as_str())
    .bind(scope.branch_id.as_deref())
    .bind(scope.department_id.as_deref())
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportOrderSource {
                id: row.try_get("id")?,
                number: row.try_get("number")?,
                version: row.try_get("version")?,
                customer_name: row.try_get("customer_name")?,
            })
        })
        .collect()
}

/// Exact `supportIncidentReadPredicate`: support.read on the selected Company
/// assignment within the incident branch/department.
fn incident_read_predicate(assignment: &str, user: &str, role: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM assignments g WHERE g.id={assignment} AND g.user_id={user} AND g.role={role} AND g.company_id=t.company_id
  AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id OR g.scope_kind='department' AND g.scope_node_id=t.department_id))"
    )
}

/// Exact incident `reviewers` fact: an independent eligible manager.
fn incident_reviewer() -> String {
    format!(
        "m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND {}",
        recipient_eligibility().replace("t.owner_id", "m.user_id")
    )
}

/// The retained TypeScript policy reviewer inlines the Company recipient
/// eligibility after substituting the policy/manager aliases. Keep the same
/// textual substitution so the `p` shadowing inside the eligibility subquery
/// stays identical.
fn policy_recipient() -> String {
    recipient_eligibility()
        .replace("t.company_id", "p.company_id")
        .replace("t.owner_id", "m.user_id")
        .replace("t.branch_id", "NULL::text")
        .replace("t.department_id", "NULL::text")
}

fn policy_reviewer() -> String {
    format!(
        "m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND {}",
        policy_recipient()
    )
}

/// Exact session reviewer predicate (`t` is the linked support ticket).
fn session_reviewer(user: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM assignments m JOIN hr_people p ON p.user_id=m.user_id AND p.company_id=m.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE m.company_id=t.company_id AND m.user_id={user} AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(m.user_id,m.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))"
    )
}

fn session_target_eligible() -> String {
    recipient_eligibility().replace("t.owner_id", "s.requested_for")
}

fn json_rows(rows: Vec<String>) -> Result<Vec<Value>, Error> {
    rows.into_iter()
        .map(|raw| serde_json::from_str(&raw).map_err(|_| limited()))
        .collect()
}

/// Bounded incident workspace in the selected Company/branch/department scope.
pub async fn read_incident_workspace(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<Value>, Error> {
    require_serializable(c).await?;
    let permission = incident_read_predicate("$4", "$5", "$6");
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM support_incidents t
         WHERE t.company_id=$1 AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3)
           AND {permission}
         ORDER BY t.updated_at DESC,t.id LIMIT 200"
    );
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_all(&mut *c)
        .await?;
    json_rows(rows)
}

/// Sales-order tickets offered by the incident create form.
pub async fn read_incident_workspace_tickets(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<Value>, Error> {
    let sql = "SELECT to_jsonb(t)::text FROM (
                 SELECT t.id,t.title,t.version,t.status FROM support_tickets t
                 WHERE t.company_id=$1 AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3)
                   AND t.source_kind='sales_order'
                 ORDER BY t.updated_at DESC,t.id LIMIT 200
               ) t";
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_all(&mut *c)
        .await?;
    json_rows(rows)
}

/// Full incident row plus the exact owner/reviewer eligibility facts.
pub async fn read_incident_detail(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<Value>, Error> {
    require_serializable(c).await?;
    let reviewer = incident_reviewer();
    let permission = incident_read_predicate("$6", "$7", "$8");
    let sql = format!(
        "SELECT (to_jsonb(t) || jsonb_build_object(
             'owner_eligible',CASE WHEN {eligible} THEN 1 ELSE 0 END,
             'reviewer_eligible',CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE {reviewer} AND m.user_id=$5) THEN 1 ELSE 0 END,
             'reviewer_count',(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE {reviewer})
         ))::text
         FROM support_incidents t
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
           AND {permission}",
        eligible = recipient_eligibility(),
    );
    let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.user_id)
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|_| limited()))
        .transpose()
}

/// Active tickets linked to the retained incident.
pub async fn read_incident_links(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(t)::text FROM (
           SELECT t.id,t.title,t.status,t.version,l.linked_version
           FROM support_incident_tickets l JOIN support_tickets t ON t.id=l.ticket_id
           WHERE l.incident_id=$1 AND l.active
           ORDER BY l.linked_at,t.id
         ) t",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

/// Sales-order tickets in the incident branch/department that are not linked.
pub async fn read_incident_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    id: &str,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(t)::text FROM (
           SELECT id,title,version FROM support_tickets
           WHERE company_id=$1 AND branch_id=$2 AND department_id IS NOT DISTINCT FROM $3::text
             AND source_kind='sales_order'
             AND id NOT IN(SELECT ticket_id FROM support_incident_tickets WHERE incident_id=$4 AND active)
           ORDER BY updated_at DESC LIMIT 200
         ) t",
    )
    .bind(company.as_str())
    .bind(branch)
    .bind(department)
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

/// Immutable incident history, newest first.
pub async fn read_incident_history(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(e)::text FROM support_incident_events e
         WHERE e.incident_id=$1 ORDER BY e.version DESC",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportSlaPolicyRecord {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub created_by: String,
    pub policy_json: String,
    pub actor_eligible: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportSlaTicketRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner_id: Option<String>,
    pub sla_policy_id: Option<String>,
    pub sla_policy_version: Option<i64>,
    pub sla_clock_json: String,
}

/// Company SLA policies with the exact independent reviewer fact.
pub async fn read_sla_policy_records(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &str,
) -> Result<Vec<SupportSlaPolicyRecord>, Error> {
    require_serializable(c).await?;
    let reviewer = policy_reviewer();
    let sql = format!(
        "SELECT p.id,p.name,p.version,p.status,p.created_by,p.policy_json::text policy_json,
                CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE {reviewer} AND m.user_id=$2) THEN 1::bigint ELSE 0::bigint END actor_eligible
         FROM support_sla_policies p
         WHERE p.company_id=$1
         ORDER BY p.created_at DESC,p.id"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(company.as_str())
        .bind(user)
        .fetch_all(&mut *c)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportSlaPolicyRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                version: row.try_get("version")?,
                status: row.try_get("status")?,
                created_by: row.try_get("created_by")?,
                policy_json: row.try_get("policy_json")?,
                actor_eligible: row.try_get("actor_eligible")?,
            })
        })
        .collect()
}

/// Active tickets carrying an SLA clock in the selected scope.
pub async fn read_sla_tickets(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<SupportSlaTicketRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,title,status,owner_id,sla_policy_id,sla_policy_version,sla_clock_json::text sla_clock_json
         FROM support_tickets
         WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3)
           AND sla_clock_json IS NOT NULL AND status NOT IN ('closed','cancelled')
         ORDER BY updated_at DESC,id",
    )
    .bind(scope.company_id.as_str())
    .bind(scope.branch_id.as_deref())
    .bind(scope.department_id.as_deref())
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SupportSlaTicketRecord {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                owner_id: row.try_get("owner_id")?,
                sla_policy_id: row.try_get("sla_policy_id")?,
                sla_policy_version: row.try_get("sla_policy_version")?,
                sla_clock_json: row.try_get("sla_clock_json")?,
            })
        })
        .collect()
}

/// SLA breach alerts in the selected scope with the exact acknowledgement facts.
pub async fn read_sla_alerts(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<Value>, Error> {
    let recipient = format!(
        "m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND {}",
        recipient_eligibility().replace("t.owner_id", "m.user_id")
    );
    let sql = format!(
        "SELECT (to_jsonb(s) || jsonb_build_object(
             'title',t.title,
             'company_id',t.company_id,
             'branch_id',t.branch_id,
             'department_id',t.department_id,
             'can_ack',CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE {recipient} AND m.user_id=$1) THEN 1 ELSE 0 END,
             'recipients',(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE {recipient})
         ))::text
         FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id
         WHERE t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
         ORDER BY s.breached_at,s.id"
    );
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(&scope.user_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_all(&mut *c)
        .await?;
    json_rows(rows)
}

/// Session rows visible to the actor as requester, subject or independent
/// reviewer. The linked ticket carries the selected scope, exactly as before.
pub async fn read_session_workspace(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<Value>, Error> {
    require_serializable(c).await?;
    let reviewer_select = session_reviewer("$1");
    let reviewer_where = session_reviewer("$7");
    let target = session_target_eligible();
    let sql = format!(
        "SELECT to_jsonb(s)::text FROM (
           SELECT s.*,t.title,o.number order_number,p.name customer_name,
             (SELECT legal_name FROM hr_people WHERE company_id=s.company_id AND user_id=s.requested_for AND merged_into_id IS NULL LIMIT 1) requested_for_name,
             CASE WHEN {reviewer_select} THEN 1 ELSE 0 END can_review,
             CASE WHEN {target} THEN 1 ELSE 0 END target_eligible
           FROM support_sessions s
           JOIN support_tickets t ON t.id=s.ticket_id
           JOIN commerce_orders o ON o.id=t.order_id
           JOIN finance_parties p ON p.id=t.customer_id
           WHERE t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
             AND (s.created_by=$5 OR s.requested_for=$6 OR {reviewer_where})
           ORDER BY s.updated_at DESC,s.id
         ) s"
    );
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(&scope.user_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.user_id)
        .bind(&scope.user_id)
        .bind(&scope.user_id)
        .fetch_all(&mut *c)
        .await?;
    json_rows(rows)
}

/// Active assigned sales-order tickets offered by the session request form.
pub async fn read_session_tickets(
    c: &mut PgConnection,
    scope: &SupportReadScope,
) -> Result<Vec<Value>, Error> {
    let eligible = recipient_eligibility();
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM (
           SELECT t.id,t.title,t.version FROM support_tickets t
           WHERE t.company_id=$1 AND ($2::text IS NULL OR t.branch_id=$2) AND ($3::text IS NULL OR t.department_id=$3)
             AND t.source_kind='sales_order' AND t.owner_id=$4
             AND t.status IN ('open','in_progress','waiting_customer') AND {eligible}
           ORDER BY t.updated_at DESC
         ) t"
    );
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.user_id)
        .fetch_all(&mut *c)
        .await?;
    json_rows(rows)
}

pub async fn read_session_capabilities(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(c)::text FROM support_capabilities c WHERE c.session_id=$1 ORDER BY c.code",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

pub async fn read_session_history(c: &mut PgConnection, id: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(e)::text FROM support_session_events e WHERE e.session_id=$1 ORDER BY e.version",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

pub async fn read_session_capability_history(
    c: &mut PgConnection,
    id: &str,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(e)::text FROM support_capability_events e
         JOIN support_capabilities c ON c.id=e.capability_id
         WHERE c.session_id=$1 ORDER BY e.created_at,e.id",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

/// Capability-gated customer order context for an approved temporary session.
pub async fn read_session_context(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
    now: i64,
) -> Result<Option<Value>, Error> {
    let target = session_target_eligible();
    let sql = format!(
        "SELECT to_jsonb(x)::text FROM (
           SELECT s.id session_id,s.expires_at,t.id ticket_id,t.title,t.status ticket_status,
             o.id order_id,o.number order_number,o.kind order_kind,o.state order_state,o.due_date,o.currency currency_code
           FROM support_sessions s
           JOIN support_tickets t ON t.id=s.ticket_id
           JOIN commerce_orders o ON o.id=t.order_id
           WHERE s.id=$1 AND s.requested_for=$2 AND s.status='approved' AND s.expires_at>$3
             AND t.status IN ('open','in_progress','waiting_customer')
             AND t.company_id=$4 AND ($5::text IS NULL OR t.branch_id=$5) AND ($6::text IS NULL OR t.department_id=$6)
             AND {target}
             AND EXISTS(SELECT 1 FROM support_capabilities c WHERE c.session_id=s.id AND c.code='customer_order_context' AND c.status='active')
         ) x"
    );
    let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(&scope.user_id)
        .bind(now)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|_| limited()))
        .transpose()
}

/// Metadata-only evidence offered by the `ticket_evidence_metadata` capability.
pub async fn read_session_context_evidence(
    c: &mut PgConnection,
    ticket_id: &str,
    session_id: &str,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(a)::text FROM (
           SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at
           FROM attachments a
           WHERE a.owner_type='support_ticket' AND a.owner_id=$1 AND a.revoked_at IS NULL AND a.source_state='available'
             AND EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
                        WHERE l.attachment_id=a.id AND l.state='bound' AND s.company_id=a.company_id
                          AND s.target_type='support_ticket' AND s.target_id=a.owner_id)
             AND EXISTS(SELECT 1 FROM support_capabilities WHERE session_id=$2 AND code='ticket_evidence_metadata' AND status='active')
           ORDER BY a.created_at,a.id
         ) a",
    )
    .bind(ticket_id)
    .bind(session_id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

/// Readable ticket facts used by the evidence readback. The caller still
/// applies the typed source-link visibility rule.
pub async fn read_evidence_ticket(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<Value>, Error> {
    let permission = read_predicate("$5", "$6", "$7");
    let sql = format!(
        "SELECT (to_jsonb(t) || jsonb_build_object(
             'owner_eligible',CASE WHEN {eligible} THEN 1 ELSE 0 END
         ))::text
         FROM support_tickets t
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) AND ($4::text IS NULL OR t.department_id=$4)
           AND {permission}",
        eligible = recipient_eligibility(),
    );
    let raw: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(|_| limited()))
        .transpose()
}

/// Exact ticket recipient eligibility bound to the requesting user.
pub async fn read_ticket_actor_eligible(
    c: &mut PgConnection,
    id: &str,
    user: &str,
) -> Result<bool, Error> {
    let eligible = recipient_eligibility().replace("t.owner_id", "$2::text");
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=$1 AND {eligible})"
    );
    let value: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(user)
        .fetch_one(&mut *c)
        .await?;
    Ok(value)
}

/// Bound Support evidence metadata owned by the ticket, including revoked and
/// legally held rows exactly as the retained TypeScript reader returns them.
pub async fn read_evidence_items(
    c: &mut PgConnection,
    ticket_id: &str,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar(
        "SELECT to_jsonb(a)::text FROM (
           SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at,a.created_by,a.revoked_at,a.legal_hold
           FROM attachments a
           WHERE a.owner_type='support_ticket' AND a.owner_id=$1 AND (a.company_id IS NULL OR EXISTS(
             SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
             WHERE l.attachment_id=a.id AND l.state='bound' AND s.company_id=a.company_id
               AND s.target_type='support_ticket' AND s.target_id=a.owner_id))
           ORDER BY a.created_at,a.id
         ) a",
    )
    .bind(ticket_id)
    .fetch_all(&mut *c)
    .await?;
    json_rows(rows)
}

#[cfg(test)]
mod tests {
    use super::{parse_source_link, SupportSourceReference};

    #[test]
    fn source_links_require_a_bounded_typed_reference() {
        assert_eq!(
            parse_source_link(r#"{"kind":"asset","id":"asset-1","version":3}"#),
            Some(SupportSourceReference {
                kind: "asset".into(),
                id: "asset-1".into(),
                version: 3,
            })
        );
        assert_eq!(parse_source_link("{}"), None);
        assert_eq!(parse_source_link(r#"{"kind":"asset","id":"","version":1}"#), None);
        assert_eq!(parse_source_link(r#"{"kind":"asset","id":"a","version":0}"#), None);
        assert_eq!(parse_source_link("not-json"), None);
    }
}
