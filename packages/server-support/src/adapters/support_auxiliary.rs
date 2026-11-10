//! Support auxiliary reads consumed by Sales workspace and Collaboration
//! notification payloads.
//!
//! Callers retain the current purchase/release gate and Holding transaction.
//! Every query rechecks the selected Company/branch/department and the exact
//! Support assignment predicate; a notification row never grants record access.
use crate::adapters::support_ticket_read::{
    SupportOrderSource, SupportReadScope, SupportTicketWorkspaceRow, read_order_sources,
    read_ticket_workspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Support auxiliary scope. `read` carries the selected assignment and the
/// existing Support permission facts; the extra booleans are only the remaining
/// TypeScript presentation inputs.
#[derive(Clone, Debug)]
pub struct SupportAuxiliaryScope {
    pub read: SupportReadScope,
    pub records_write: bool,
    pub support_create: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportSalesWorkspace {
    pub items: Vec<SupportTicketWorkspaceRow>,
    pub sources: Vec<SupportOrderSource>,
    pub can_create: bool,
    pub can_create_order: bool,
    pub own_reports_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportTicketSourceSnapshot {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub status: String,
    pub source_kind: String,
    pub owner_id: Option<String>,
    pub order_id: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportPolicySourceSnapshot {
    pub id: String,
    pub version: i64,
    pub name: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportAlertSourceSnapshot {
    pub id: String,
    pub version: i64,
    pub ticket_id: String,
    pub status: String,
    pub breached_at: Option<i64>,
    pub can_ack: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportIncidentSourceSnapshot {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportSessionSourceSnapshot {
    pub id: String,
    pub version: i64,
    pub ticket_id: String,
    pub status: String,
    pub requested_for: String,
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPORT_READ_UNAVAILABLE",
        "The Support record is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_TRANSACTION_REQUIRED",
            "Use the Support owner transaction.",
        )
        .into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_SERIALIZABLE_REQUIRED",
            "Use the Support owner transaction.",
        )
        .into());
    }
    Ok(())
}

fn read_predicate(alias: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM assignments g WHERE g.id=$5 AND g.user_id=$6 AND g.role=$7 \
         AND g.company_id={alias}.company_id \
         AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_role_allows(g.role,g.company_id,'support.read') \
         AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id={alias}.branch_id \
           OR g.scope_kind='department' AND g.scope_node_id={alias}.department_id))"
    )
}

/// Sales customer support workspace: current retained ticket rows, current
/// order sources and the original presentation flags. Source visibility for a
/// ticket's foreign record stays with Records; the stored row is returned.
pub async fn read_sales_workspace(
    c: &mut PgConnection,
    scope: &SupportAuxiliaryScope,
) -> Result<SupportSalesWorkspace, Error> {
    serializable(c).await?;
    let items = read_ticket_workspace(c, &scope.read).await?;
    let can_create_order = scope.support_create && scope.read.commerce_read;
    let sources = if can_create_order {
        read_order_sources(c, &scope.read).await?
    } else {
        vec![]
    };
    Ok(SupportSalesWorkspace {
        items,
        sources,
        can_create: scope.records_write,
        can_create_order,
        own_reports_only: !scope.read.support_read,
    })
}

pub async fn read_ticket_source(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<SupportTicketSourceSnapshot>, Error> {
    let permission = read_predicate("t");
    let sql = format!(
        "SELECT t.id,t.version,t.title,t.status,t.source_kind,t.owner_id,t.order_id,t.due_date \
         FROM support_tickets t \
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) \
           AND ($4::text IS NULL OR t.department_id=$4) AND {permission}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok::<_, Error>(SupportTicketSourceSnapshot {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
            title: row.try_get("title")?,
            status: row.try_get("status")?,
            source_kind: row.try_get("source_kind")?,
            owner_id: row.try_get("owner_id")?,
            order_id: row.try_get("order_id")?,
            due_date: row.try_get("due_date")?,
        })
    })
    .transpose()
}

pub async fn read_policy_source(
    c: &mut PgConnection,
    company_id: &str,
    id: &str,
) -> Result<Option<SupportPolicySourceSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,version,name,status FROM support_sla_policies WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(SupportPolicySourceSnapshot {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
            name: row.try_get("name")?,
            status: row.try_get("status")?,
        })
    })
    .transpose()
}

/// SLA alert visibility uses the same recipient predicate as the native SLA
/// workspace: an active Support manager covering the ticket scope.
pub async fn read_alert_source(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<SupportAlertSourceSnapshot>, Error> {
    let sql = format!(
        "SELECT s.id,s.version,s.ticket_id, \
           CASE WHEN s.acknowledged_at IS NULL THEN 'breached' ELSE 'acknowledged' END status, \
           s.breached_at, \
           CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE m.company_id=t.company_id \
             AND m.user_id=$2 \
             AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(m.role,m.company_id,'support.manage') \
             AND iam_role_allows(m.role,m.company_id,'support.read') \
             AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) \
               OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))) THEN true ELSE false END can_ack \
         FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id \
         WHERE s.id=$1 AND t.company_id=$3 AND ($4::text IS NULL OR t.branch_id=$4) \
           AND ($5::text IS NULL OR t.department_id=$5)"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(&scope.user_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok::<_, Error>(SupportAlertSourceSnapshot {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
            ticket_id: row.try_get("ticket_id")?,
            status: row.try_get("status")?,
            breached_at: row.try_get("breached_at")?,
            can_ack: row.try_get("can_ack")?,
        })
    })
    .transpose()
}

pub async fn read_incident_source(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<SupportIncidentSourceSnapshot>, Error> {
    let permission = read_predicate("t");
    let sql = format!(
        "SELECT t.id,t.version,t.title,t.status FROM support_incidents t \
         WHERE t.id=$1 AND t.company_id=$2 AND ($3::text IS NULL OR t.branch_id=$3) \
           AND ($4::text IS NULL OR t.department_id=$4) AND {permission}"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .bind(&scope.assignment_id)
        .bind(&scope.user_id)
        .bind(&scope.role)
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok::<_, Error>(SupportIncidentSourceSnapshot {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
            title: row.try_get("title")?,
            status: row.try_get("status")?,
        })
    })
    .transpose()
}

/// Session visibility: requester, requested-for principal, or an independent
/// reviewer with Support manage/read coverage of the linked ticket.
pub async fn read_session_source(
    c: &mut PgConnection,
    scope: &SupportReadScope,
    id: &str,
) -> Result<Option<SupportSessionSourceSnapshot>, Error> {
    let reviewer = format!(
        "EXISTS(SELECT 1 FROM assignments m WHERE m.company_id=t.company_id \
           AND m.user_id=$2 \
           AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_role_allows(m.role,m.company_id,'support.manage') \
           AND iam_role_allows(m.role,m.company_id,'support.read') \
           AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) \
             OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)))"
    );
    let sql = format!(
        "SELECT s.id,s.version,s.ticket_id,s.status,s.requested_for \
         FROM support_sessions s JOIN support_tickets t ON t.id=s.ticket_id \
         WHERE s.id=$1 AND t.company_id=$3 AND ($4::text IS NULL OR t.branch_id=$4) \
           AND ($5::text IS NULL OR t.department_id=$5) \
           AND (s.created_by=$2 OR s.requested_for=$2 OR {reviewer})"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(&scope.user_id)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .bind(scope.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok::<_, Error>(SupportSessionSourceSnapshot {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
            ticket_id: row.try_get("ticket_id")?,
            status: row.try_get("status")?,
            requested_for: row.try_get("requested_for")?,
        })
    })
    .transpose()
}

/// Fail closed when a caller asks for a source that its current scope does not
/// reveal. Notification visibility never falls back to a TypeScript reader.
pub fn require_source<T>(value: Option<T>) -> Result<T, Error> {
    value.ok_or_else(unavailable)
}
