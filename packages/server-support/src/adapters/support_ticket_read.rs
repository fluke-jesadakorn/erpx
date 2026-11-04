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
