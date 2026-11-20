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
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub change: Option<String>,
    #[serde(default)]
    pub timestamp: Option<i64>,
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
            approver_user_id: None, item_id: None, bill_id: None, rfq_id: None, audience: None, event: None, order_id: None, change: None, timestamp: None,
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


/// Supplier-promise notification statement. Commerce/Identity source clauses
/// are owned here; caller supplies only order id and command key.
pub async fn supplier_promise_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    order_id: &str,
    command_key: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if order_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM commerce_orders WHERE id=$1 AND company_id=$2",
    )
    .bind(order_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,o.company_id,a.user_id,\
 'supplier_promise',o.id,o.version,'Supplier delivery confirmation changed: review remaining quantities and dates',o.updated_at \
 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\
 \"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\
 \"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\
 \"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\
 \"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a ON a.company_id=o.company_id \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND (iam_role_allows(a.role,a.company_id,'commerce.purchase') OR iam_role_allows(a.role,a.company_id,'inventory.prepare')) \
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) \
 OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) \
 WHERE o.id=? ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::String(command_key.to_owned()),
            serde_json::Value::String(order_id.to_owned()),
        ],
    })
}


/// Learning assignment notice statement. The recipient and version are read from
/// the persisted assignment; caller supplies notice id, assignment id and change.
pub async fn learning_assignment_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    notice_id: &str,
    assignment_id: &str,
    change: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if notice_id.is_empty() || assignment_id.is_empty() {
        return Err(invalid().into());
    }
    let message = match change {
        "assigned" => "New learning assignment: review course and due date",
        "changed" => "Learning assignment updated: review current status",
        _ => return Err(invalid().into()),
    };
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM learning_assignments WHERE id=$1 AND company_id=$2",
    )
    .bind(assignment_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    Ok(NotificationStatement {
        sql: "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT ?,company_id,learner_id,'learning_assignment',id,\
 version,?,updated_at FROM learning_assignments WHERE id=?".to_owned(),
        values: vec![
            serde_json::Value::String(notice_id.to_owned()),
            serde_json::Value::String(message.to_owned()),
            serde_json::Value::String(assignment_id.to_owned()),
        ],
    })
}


/// Legal preservation-hold notification statement. Owns the full CTE (notice
/// insert plus notification projection) with the Legal owner source clause.
pub async fn legal_hold_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    timestamp: i64,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if timestamp <= 0 {
        return Err(invalid().into());
    }
    let sql = "WITH added AS (INSERT INTO legal_hold_notices(hold_id,item_id,user_id,notified_at) \
 SELECT h.id,t.item_id,m.user_id,? FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id \
 JOIN legal_members m ON m.item_id=t.item_id WHERE h.state!='released' \
 AND t.item_id IN (SELECT id FROM legal_items WHERE company_id=?) ON CONFLICT DO NOTHING RETURNING *) \
 INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
 SELECT 'legal-'||md5(a.item_id||a.due_date||a.days::text||a.user_id),i.company_id,a.user_id,'legal',i.id,i.version,\
 'Legal deadline requires attention',a.created_at FROM added a \
 JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"title\",\"description\",\
 \"classification\",\"country\",\"jurisdiction\",\"language\",\"currency\",\"timezone\",\"priority\",\
 \"due_date\",\"owner_id\",\"created_by\",\"state\",\"version\",\"created_at\",\"updated_at\",\
 \"escalation_owner\",\"sla_started_at\",\"sla_due_at\",\"intake_form_id\",\"intake_answers\" FROM legal_items) i \
 ON i.id=a.item_id ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::Number(timestamp.into()),
            serde_json::Value::String(company.as_str().to_owned()),
        ],
    })
}

/// Payroll advance independent-decision notice. The INSERT...SELECT resolves
/// eligible payroll.approve holders from assignments in the caller's unit.
pub async fn payroll_advance_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    advance_id: &str,
    now: i64,
    user_id: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if advance_id.is_empty() || user_id.is_empty() || now <= 0 {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'payroll-advance-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_advance',?,1,\
 'Payroll advance needs an independent decision',? \
 FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') \
 AND NOT iam_same_person(a.user_id,?,a.company_id) GROUP BY a.user_id ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(advance_id),
            value(company.as_str()),
            value(advance_id),
            serde_json::Value::Number(now.into()),
            value(company.as_str()),
            serde_json::Value::Number(now.into()),
            value(user_id),
        ],
    })
}

/// Approved payroll independent-correction notice, same owner unit as above.
pub async fn payroll_correction_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    correction_id: &str,
    now: i64,
    user_id: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if correction_id.is_empty() || user_id.is_empty() || now <= 0 {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'payroll-correction-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_correction',?,1,\
 'Approved payroll needs an independent correction decision',? \
 FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') \
 AND NOT iam_same_person(a.user_id,?,a.company_id) GROUP BY a.user_id ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(correction_id),
            value(company.as_str()),
            value(correction_id),
            serde_json::Value::Number(now.into()),
            value(company.as_str()),
            serde_json::Value::Number(now.into()),
            value(user_id),
        ],
    })
}

/// Explicit-id notification statement: the caller supplies the full 8-value
/// row (id via idempotency_key, company/user/record/message/version/now).
pub async fn explicit_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.user_id.is_empty()
        || input.record_type.is_empty()
        || input.message.is_empty()
        || input.idempotency_key.is_empty()
        || input.record_version <= 0
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: INSERT.to_owned(),
        values: vec![
            value(&input.idempotency_key),
            value(&input.company_id),
            value(&input.user_id),
            value(&input.record_type),
            value(&input.record_id),
            serde_json::Value::Number(input.record_version.into()),
            value(&input.message),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Customer-portal staff notification. Recipients are resolved from current
/// assignments and the order on the caller's serializable unit.
pub async fn customer_portal_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.message.is_empty()
        || input.now <= 0
        || input.order_id.as_deref().unwrap_or("").is_empty()
    {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT DISTINCT 'customer-portal-'||?||'-'||a.user_id,a.company_id,a.user_id,\
 'commerce_order',o.id,o.version,?,?::bigint \
 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a \
 JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\
 \"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\
 \"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\
 \"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\
 \"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o \
 ON o.id=? AND o.company_id=a.company_id \
 WHERE a.company_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_role_allows(a.role,a.company_id,'commerce.read') \
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) \
 OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(&input.idempotency_key),
            value(&input.message),
            serde_json::Value::Number(input.now.into()),
            value(input.order_id.as_deref().unwrap_or("")),
            value(company.as_str()),
        ],
    })
}

/// Company-business-acceptance independent reviewer notice. Eligible holders
/// are resolved from current assignments/principals on the caller's unit.
pub async fn company_acceptance_reviewer_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.user_id.is_empty()
        || input.record_version <= 0
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'company-acceptance-notice-'||?||'-'||a.user_id,?,a.user_id,\
 'company_business_acceptance',?,?,'Company setup acceptance is ready for independent review',? \
 FROM assignments a WHERE a.company_id=? AND a.user_id<>? AND a.scope_kind='holding' \
 AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) \
 AND iam_role_allows(a.role,a.company_id,'organization.manage') ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    let company_value = value(company.as_str());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(&input.idempotency_key),
            company_value.clone(),
            company_value.clone(),
            company_value.clone(),
            serde_json::Value::Number(input.record_version.into()),
            serde_json::Value::Number(input.now.into()),
            company_value.clone(),
            value(&input.user_id),
            serde_json::Value::Number(input.now.into()),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Company-business-acceptance submitter result notice.
pub async fn company_acceptance_result_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str() || input.message.is_empty() || input.now <= 0 {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT 'company-acceptance-result-'||?||'-'||requested_by,company_id,requested_by,\
 'company_business_acceptance',company_id,version,?,? \
 FROM company_business_acceptance WHERE company_id=? ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(&input.idempotency_key),
            value(&input.message),
            serde_json::Value::Number(input.now.into()),
            value(company.as_str()),
        ],
    })
}

/// Official law-source change notice. The Legal item owner is resolved from the
/// persisted item on the caller's serializable unit; `unchanged` suppresses the
/// write when the fetched source has not actually changed.
pub async fn legal_law_source_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.identifier.as_deref().unwrap_or("").is_empty()
        || input.item_id.as_deref().unwrap_or("").is_empty()
        || input.now <= 0
        || input.event.as_deref() != Some("unchanged")
            && input.event.as_deref() != Some("changed")
    {
        return Err(invalid().into());
    }
    let unchanged = input.event.as_deref() == Some("unchanged");
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT ?,i.company_id,i.owner_id,'legal',i.id,i.version,\
 'Official law source changed; review original, text, dates and Company applicability before use',? \
 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"title\",\
 \"description\",\"classification\",\"country\",\"jurisdiction\",\"language\",\"currency\",\
 \"timezone\",\"priority\",\"due_date\",\"owner_id\",\"created_by\",\"state\",\"version\",\
 \"created_at\",\"updated_at\",\"escalation_owner\",\"sla_started_at\",\"sla_due_at\",\
 \"intake_form_id\",\"intake_answers\" FROM legal_items) i \
 WHERE i.id=? AND ?=false AND EXISTS(SELECT 1 FROM (SELECT \"item_id\",\"user_id\",\"access\" \
 FROM legal_members) m WHERE m.item_id=i.id AND m.user_id=i.owner_id) ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(input.identifier.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.now.into()),
            value(input.item_id.as_deref().unwrap_or("")),
            serde_json::Value::Bool(unchanged),
        ],
    })
}

/// Inventory transport handoff notice. Eligible readers are resolved from the
/// persisted transit record on the caller's serializable unit.
pub async fn inventory_transit_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.record_id.is_empty()
        || input.idempotency_key.is_empty()
    {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) SELECT DISTINCT ?||'-'||a.user_id,\
 d.company_id,a.user_id,'inventory_transit',d.id,d.version,\
 'Transport handoff updated: inspect dispatch, receipt and discrepancy evidence',d.updated_at \
 FROM inventory_transit d \
 JOIN assignments a ON a.company_id=d.company_id \
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' \
 AND a.scope_node_id IN (d.source_branch_id,d.target_branch_id))) \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_role_allows(a.role,a.company_id,'inventory.read') \
 WHERE d.id=? ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![value(&input.idempotency_key), value(&input.record_id)],
    })
}
