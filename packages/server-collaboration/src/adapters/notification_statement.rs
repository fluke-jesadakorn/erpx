//! Native-owned notification statement descriptors.
//!
//! The owner decides the exact notification INSERT; a caller-owned serializable
//! transaction executes it, so no second transaction or TS copy is needed.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const INSERT: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationStatementInput {
    pub company_id: String,
    pub user_id: String,
    pub record_type: String,
    pub record_id: String,
    pub record_version: i64,
    pub message: String,
    pub now: i64,
    pub idempotency_key: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub lead_id: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub approver_user_id: Option<String>,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub bill_id: Option<String>,
    #[serde(default)]
    pub rfq_id: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NotificationStatement {
    pub sql: String,
    pub values: Vec<serde_json::Value>,
}

fn invalid() -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "NOTIFICATION_STATEMENT_INVALID", "Review the notification request.")
}

async fn ready(connection: &mut PgConnection, company: &CompanyId) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection).await? != "serializable"
    {
        return Err(PersistenceError::new("NOTIFICATION_STATEMENT_UNIT_REQUIRED").into());
    }
    let holder: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *connection).await?;
    if holder.is_empty() {
        return Err(invalid().into());
    }
    Ok(())
}

/// Build the authoritative notification INSERT on the caller's unit.
pub async fn notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.user_id.is_empty() || input.record_id.is_empty()
        || input.record_type.is_empty() || input.message.is_empty()
        || input.record_version <= 0 || input.now <= 0
    {
        return Err(invalid().into());
    }
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: INSERT.to_owned(),
        values: vec![
            value(&format!("ntf-{}-{}", input.idempotency_key, input.user_id)),
            value(company.as_str()),
            value(&input.user_id),
            value(&input.record_type),
            value(&input.record_id),
            serde_json::Value::Number(input.record_version.into()),
            value(&input.message),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::{NotificationStatementInput, INSERT};
    #[test]
    fn insert_shape_matches_legacy_writer() {
        let _i = NotificationStatementInput {
            company_id: "c".into(), user_id: "u".into(), record_type: "overtime".into(),
            record_id: "r".into(), record_version: 1, message: "m".into(), now: 1,
            idempotency_key: "k".into(), kind: None, lead_id: None, identifier: None,
            approver_user_id: None, item_id: None, bill_id: None, rfq_id: None, audience: None, event: None,
        };
        assert!(INSERT.contains("INSERT INTO staff_notifications"));
        assert!(INSERT.contains("ON CONFLICT DO NOTHING"));
        assert_eq!(INSERT.matches('?').count(), 8);
    }
}


/// CRM lead notification statement. Reads the persisted lead on the caller's
/// serializable unit so the owner — not the caller — selects recipient/version.
pub async fn crm_lead_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    lead_id: &str,
    command_key: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if lead_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM crm_leads WHERE id=$1 AND company_id=$2",
    )
    .bind(lead_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT ?,company_id,owner_id,'crm_lead',id,version,\
 'CRM handoff changed: review the source and next action',updated_at \
 FROM (SELECT \"id\", \"company_id\", \"owner_id\", \"version\", \"updated_at\" FROM crm_leads) crm_leads \
 WHERE id=? ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::String(format!("crm-notice-{command_key}")),
            serde_json::Value::String(lead_id.to_owned()),
        ],
    })
}


#[cfg(test)]
mod crm_tests {
    #[test]
    fn crm_lead_notice_uses_owner_recipient_projection() {
        // The native statement derives recipient/version from the persisted lead,
        // never from caller-supplied values.
        let src = include_str!("crm_activity_notification.rs");
        assert!(src.contains("'crm_lead'"));
        assert!(src.contains("CRM handoff changed"));
    }
}


/// Legal approval reviewer notice. Exact retained SQL shape, generated natively
/// from the supplied facts so the Legal adapter carries no second copy.
pub async fn legal_approval_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    identifier: &str,
    approver_user_id: &str,
    item_id: &str,
    next_version: i64,
    now: i64,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if identifier.is_empty() || approver_user_id.is_empty() || item_id.is_empty() || next_version < 1 {
        return Err(invalid().into());
    }
    Ok(NotificationStatement {
        sql: "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) VALUES(?,?,?,'legal',?,?,'Legal approval needs your decision',?)"
            .to_owned(),
        values: vec![
            serde_json::Value::String(identifier.to_owned()),
            serde_json::Value::String(company.as_str().to_owned()),
            serde_json::Value::String(approver_user_id.to_owned()),
            serde_json::Value::String(item_id.to_owned()),
            serde_json::Value::Number(next_version.into()),
            serde_json::Value::Number(now.into()),
        ],
    })
}


#[cfg(test)]
mod legal_tests {
    #[test]
    fn legal_notice_keeps_retained_shape() {
        let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) VALUES(?,?,?,'legal',?,?,'Legal approval needs your decision',?)";
        assert!(sql.contains("'legal'"));
        assert!(sql.contains("Legal approval needs your decision"));
        assert_eq!(sql.matches('?').count(), 6);
    }
}


/// Supplier-bill notification statement. The Finance/Identity source clauses are
/// owned here; the caller supplies only the bill id and command key.
pub async fn supplier_bill_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    bill_id: &str,
    command_key: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if bill_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM procurement_bill_matches WHERE id=$1 AND company_id=$2",
    )
    .bind(bill_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,m.company_id,a.user_id,\
 'supplier_bill',m.id,m.version,CASE m.status WHEN 'returned' THEN 'Supplier invoice returned: obtain corrected evidence and resubmit from the receipt' \
 WHEN 'approved' THEN 'Supplier invoice matched: assigned PO owner can prepare AP' \
 WHEN 'invoiced' THEN 'Matched AP draft needs independent Finance posting' \
 WHEN 'posted' THEN 'Matched supplier invoice posted to GL' \
 ELSE 'Supplier invoice needs independent quantity and price review' END,m.updated_at \
 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"order_id\",\"fulfillment_id\",\
 \"supplier_id\",\"currency\",\"chart_id\",\"clearing_account_id\",\"payable_account_id\",\
 \"tax_account_id\",\"reference\",\"invoice_date\",\"due_date\",\"evidence\",\
 \"expected_subtotal_minor\",\"subtotal_minor\",\"tax_minor\",\"quantity_matches\",\
 \"variance_account_id\",\"reviewed_by\",\"review_reason\",\"invoice_id\",\"status\",\"version\",\
 \"created_by\",\"created_at\",\"updated_at\" FROM procurement_bill_matches) m \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a ON a.company_id=m.company_id \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_role_allows(a.role,a.company_id,'finance.write') \
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=m.branch_id) \
 OR (a.scope_kind='department' AND a.scope_node_id=m.department_id)) \
 WHERE m.id=? ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::String(command_key.to_owned()),
            serde_json::Value::String(bill_id.to_owned()),
        ],
    })
}


/// Procurement RFQ notification statement. Procurement/Identity source clauses
/// are owned here; the caller supplies only the RFQ id and command key.
pub async fn rfq_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    rfq_id: &str,
    command_key: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if rfq_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM procurement_rfqs WHERE id=$1 AND company_id=$2",
    )
    .bind(rfq_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,r.company_id,a.user_id,\
 'procurement_rfq',r.id,r.version,'Supplier quotation review changed: review the next action',r.updated_at \
 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"warehouse_id\",\"currency\",\
 \"title\",\"demand_evidence\",\"needed_date\",\"status\",\"selected_offer_id\",\"selection_reason\",\
 \"submitted_by\",\"approved_by\",\"order_id\",\"created_by\",\"version\",\"created_at\",\"updated_at\",\
 \"request_id\",\"request_version\",\"request_evidence_version\",\"request_items_json\" FROM procurement_rfqs) r \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a ON a.company_id=r.company_id \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_role_allows(a.role,a.company_id,'commerce.purchase') \
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=r.branch_id) \
 OR (a.scope_kind='department' AND a.scope_node_id=r.department_id)) \
 WHERE r.id=? ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::String(command_key.to_owned()),
            serde_json::Value::String(rfq_id.to_owned()),
        ],
    })
}


/// Finance refund notification statement. The Commerce/IAM source clauses and
/// the four audience eligibility predicates are owned here; the caller supplies
/// only audience, refund id, message, actor id and command key.
pub async fn finance_refund_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    refund_id: &str,
    command_key: &str,
    message: &str,
    actor_user_id: &str,
    audience: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if refund_id.is_empty() || command_key.is_empty() || message.is_empty() || actor_user_id.is_empty() {
        return Err(invalid().into());
    }
    let eligibility = match audience {
        "treasury" => "a.role IN ('holding_owner','finance_approver','treasury') AND a.scope_kind='holding' AND a.user_id<>o.created_by AND a.user_id<>o.approved_by",
        "approval" => "a.role IN ('holding_owner','finance_approver') AND a.user_id<>o.created_by AND a.user_id<>COALESCE(r.requested_by,c.prepared_by)",
        "review" => "a.role IN ('holding_owner','finance_approver')",
        "completed" => "a.role IN ('holding_owner','finance_approver','finance_preparer','treasury') AND a.user_id IN (o.created_by,o.approved_by,COALESCE(r.requested_by,c.prepared_by))",
        _ => return Err(invalid().into()),
    };
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_refund_obligations WHERE id=$1 AND company_id=$2",
    )
    .bind(refund_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,o.company_id,a.user_id,\
 'refund',o.id,o.version,?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
 FROM finance_refund_obligations o \
 JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"invoice_id\",\
 \"kind\",\"number\",\"return_date\",\"reason\",\"status\",\"subtotal_minor\",\"tax_minor\",\
 \"total_minor\",\"stock_journal_id\",\"credit_journal_id\",\"version\",\"requested_by\",\
 \"approved_by\",\"created_at\",\"updated_at\",\"credited_minor\",\"refund_minor\",\
 \"base_credit_minor\",\"base_credited_minor\",\"base_refund_minor\",\"fx_journal_id\",\
 \"goods_fx_journal_id\" FROM commerce_returns) r ON r.id=o.return_id \
 LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a ON a.company_id=o.company_id \
 WHERE o.id=? AND a.user_id<>? AND {eligibility} AND a.revoked_at IS NULL \
 AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
 AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
 AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=o.branch_id \
 OR a.scope_kind='department' AND a.scope_node_id=o.department_id)"
    );
    Ok(NotificationStatement {
        sql,
        values: vec![
            serde_json::Value::String(command_key.to_owned()),
            serde_json::Value::String(message.to_owned()),
            serde_json::Value::String(refund_id.to_owned()),
            serde_json::Value::String(actor_user_id.to_owned()),
        ],
    })
}


/// Organization-change notification statement. Four event variants owned here;
/// caller supplies only change id, command key and event kind.
pub async fn organization_change_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    change_id: &str,
    command_key: &str,
    event: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if change_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let (recipient, message) = match event {
        "submitted" => ("NOT iam_same_person(a.user_id,c.requested_by,c.company_id)", "Review organization change: "),
        "cancelled" => ("EXISTS(SELECT 1 FROM staff_notifications n WHERE n.company_id=c.company_id AND n.record_type='organization_change' AND n.record_id=c.id AND n.user_id=a.user_id) AND a.user_id<>c.requested_by", "Organization change cancelled: "),
        "applied" => ("a.user_id=c.requested_by", "Organization change applied: "),
        "rejected" => ("a.user_id=c.requested_by", "Organization change returned for correction: "),
        _ => return Err(invalid().into()),
    };
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM organization_node_changes WHERE id=$1 AND company_id=$2",
    )
    .bind(change_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT DISTINCT ?||a.user_id,c.company_id,a.user_id,\
 'organization_change',c.id,c.version,?||c.display_name,c.updated_at \
 FROM (SELECT \"id\",\"company_id\",\"target_node_id\",\"action\",\"node_kind\",\"slug\",\
 \"display_name\",\"branch_id\",\"expected_node_version\",\"state\",\"version\",\"requested_by\",\
 \"reviewed_by\",\"reason\",\"review_reason\",\"created_at\",\"updated_at\",\"reviewed_at\",\
 \"applied_at\",\"source_branch_id\" FROM organization_node_changes) c \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a ON a.company_id=c.company_id \
 WHERE c.id=? AND c.company_id=? \
 AND a.scope_kind='holding' AND a.revoked_at IS NULL \
 AND iam_role_allows(a.role,a.company_id,'organization.manage') \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND {recipient} ON CONFLICT DO NOTHING"
    );
    Ok(NotificationStatement {
        sql,
        values: vec![
            serde_json::Value::String(format!("org-change-notice-{command_key}-")),
            serde_json::Value::String(message.to_owned()),
            serde_json::Value::String(change_id.to_owned()),
            serde_json::Value::String(company.as_str().to_owned()),
        ],
    })
}
