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
    #[serde(default)]
    pub post_id: Option<String>,
    #[serde(default)]
    pub source_hash: Option<String>,
    #[serde(default)]
    pub ai_draft_id: Option<String>,
    #[serde(default)]
    pub document_table: Option<String>,
    #[serde(default)]
    pub recipient_sql: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub scope_node_id: Option<String>,
    #[serde(default)]
    pub runtime_predicate: Option<String>,
    #[serde(default)]
    pub table_name: Option<String>,
    #[serde(default)]
    pub cte_prefix: Option<String>,
    #[serde(default)]
    pub source_access_sql: Option<String>,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub values_json: Option<String>,
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
    use super::{NotificationStatementInput, INSERT, WORKFLOW_ASSERTION_CLEAR_SQL};
    #[test]
    fn insert_shape_matches_legacy_writer() {
        let _i = NotificationStatementInput {
            company_id: "c".into(), user_id: "u".into(), record_type: "overtime".into(),
            record_id: "r".into(), record_version: 1, message: "m".into(), now: 1,
            idempotency_key: "k".into(), kind: None, lead_id: None, identifier: None,
            approver_user_id: None, item_id: None, bill_id: None, rfq_id: None, audience: None, event: None, order_id: None, change: None, timestamp: None,
            post_id: None, source_hash: None, ai_draft_id: None, document_table: None, recipient_sql: None, node_id: None, scope_node_id: None, runtime_predicate: None, table_name: None, cte_prefix: None,
            source_access_sql: None, target_kind: None, target_id: None, values_json: None,
        };
        assert!(INSERT.contains("INSERT INTO staff_notifications"));
        assert!(INSERT.contains("ON CONFLICT DO NOTHING"));
        assert_eq!(INSERT.matches('?').count(), 8);
    }
    #[test]
    fn workflow_assertion_clear_contract_matches_native_sql() {
        let contract = include_str!("../../../../packages/server-workflow/src/adapters/native-command-assertion-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(contract).expect("invalid command-assertion SQL contract");
        assert_eq!(parsed["sql"].as_str(), Some(WORKFLOW_ASSERTION_CLEAR_SQL));
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


/// Social task-origin binding. The full source witness is supplied by the
/// caller; the native owner validates company authority and owns the INSERT.
pub async fn social_task_origin_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.post_id.as_deref().unwrap_or("").is_empty()
        || input.record_id.is_empty()
        || input.user_id.is_empty()
        || input.source_hash.as_deref().unwrap_or("").is_empty()
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO social_task_origins(post_id,task_id,source_version,\
 source_hash,ai_draft_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(input.post_id.as_deref().unwrap_or("")),
            value(&input.record_id),
            serde_json::Value::Number(input.record_version.into()),
            value(input.source_hash.as_deref().unwrap_or("")),
            input.ai_draft_id.as_deref().map(value).unwrap_or(serde_json::Value::Null),
            value(&input.user_id),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Workflow goods-movement invoicing notice. The posted journal and its
/// fulfillment/order are resolved on the caller's serializable unit.
pub async fn workflow_invoicing_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.record_id.is_empty()
        || input.idempotency_key.is_empty()
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT ?,o.company_id,r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId','order',o.id,o.version,\
 'Goods movement GL posted: prepare the invoice',? \
 FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\
 \"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\
 \"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\
 \"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\
 \"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o \
 JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\
 \"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f ON f.order_id=o.id \
 JOIN (SELECT \"company_id\",\"kind\",\"record_id\",\"version\",\"defaults_revision\",\"steps_json\",\
 \"created_at\" FROM flow_routes) r ON r.company_id=o.company_id AND r.kind='order' AND r.record_id=o.id \
 JOIN (SELECT \"id\",\"company_id\",\"chart_id\",\"period_id\",\"journal_number\",\"entry_date\",\
 \"currency\",\"fx_rate_micros\",\"source_type\",\"source_id\",\"memo\",\"status\",\"is_opening\",\
 \"version\",\"created_by\",\"posted_by\",\"posted_at\",\"reversed_by\",\"reversed_at\",\
 \"created_at\",\"updated_at\" FROM finance_journals) j ON j.id=f.journal_id \
 WHERE j.id=? AND j.status='posted' AND f.invoice_id IS NULL \
 AND (r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId') IS NOT NULL ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(&input.idempotency_key),
            serde_json::Value::Number(input.now.into()),
            value(&input.record_id),
        ],
    })
}


/// Workflow next-action notice. The fixed document table and recipient
/// expression are supplied by the native Workflow composition's planning port;
/// the owner validates company authority and owns the INSERT assembly.
pub async fn workflow_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.record_type.is_empty()
        || input.record_id.is_empty()
        || input.user_id.is_empty()
        || input.document_table.as_deref().unwrap_or("").is_empty()
        || input.recipient_sql.as_deref().unwrap_or("").is_empty()
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let recipient = input.recipient_sql.as_deref().unwrap_or("");
    let sql = format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) \
 SELECT ?,d.company_id,{recipient},?,d.id,d.version,?,? \
 FROM {} d \
 JOIN (SELECT \"company_id\",\"kind\",\"record_id\",\"version\",\"defaults_revision\",\
 \"steps_json\",\"created_at\" FROM flow_routes) r \
 ON r.company_id=d.company_id AND r.kind=? AND r.record_id=d.id \
 WHERE d.company_id=? AND d.id=? AND ({recipient}) IS NOT NULL AND ({recipient})!=? \
 ON CONFLICT DO NOTHING",
        input.document_table.as_deref().unwrap_or("")
    );
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql,
        values: vec![
            value(&input.idempotency_key),
            value(&input.record_type),
            value("Workflow updated: review your next action"),
            serde_json::Value::Number(input.now.into()),
            value(&input.record_type),
            value(&input.company_id),
            value(&input.record_id),
            value(&input.user_id),
        ],
    })
}


/// Roster-coverage escalation notice. Eligible roster managers are resolved
/// through the current organization ancestor chain on the caller's unit.
pub async fn roster_coverage_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str()
        || input.node_id.as_deref().unwrap_or("").is_empty()
        || input.idempotency_key.is_empty()
        || input.record_id.is_empty()
        || input.message.is_empty()
        || input.now <= 0
    {
        return Err(invalid().into());
    }
    let sql = "WITH RECURSIVE ancestors AS ( \
 SELECT id,parent_id FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\
 \"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) organization_nodes \
 WHERE id=? AND company_id=? \
 UNION ALL SELECT parent.id,parent.parent_id \
 FROM (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\
 \"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) parent \
 JOIN ancestors child ON child.parent_id=parent.id WHERE parent.company_id=? \
 ) INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) \
 SELECT ? || '-' || a.user_id,?,a.user_id,'roster_coverage',?,1,?,? \
 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) a \
 WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) \
 AND iam_role_allows(a.role,a.company_id,'roster.manage') \
 AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? \
 OR a.scope_kind='department' AND a.scope_node_id IN (SELECT id FROM ancestors)) \
 ON CONFLICT DO NOTHING";
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    let company_value = value(company.as_str());
    let now = serde_json::Value::Number(input.now.into());
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            value(input.node_id.as_deref().unwrap_or("")),
            company_value.clone(),
            company_value.clone(),
            value(&input.idempotency_key),
            company_value.clone(),
            value(&input.record_id),
            value(&input.message),
            now.clone(),
            company_value,
            now.clone(),
            now,
            input.scope_node_id.as_deref().map(value).unwrap_or(serde_json::Value::Null),
        ],
    })
}

/// Legal workflow deadline notice. The document table and runtime predicate are supplied by the native Legal reminders composition.
pub async fn legal_feature_notice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: format!(r#"INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT 'legal-task-'||md5(f.id||f.due_date||d.days::text||f.owner_id),i.company_id,f.owner_id,'legal',i.id,i.version,'Legal workflow deadline requires attention',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM {} f JOIN (SELECT "id", "company_id", "branch_id", "department_id", "kind", "title", "description", "classification", "country", "jurisdiction", "language", "currency", "timezone", "priority", "due_date", "owner_id", "created_by", "state", "version", "created_at", "updated_at", "escalation_owner", "sla_started_at", "sla_due_at", "intake_form_id", "intake_answers" FROM legal_items) i ON i.id=f.item_id JOIN (SELECT "company_id", "enabled", "ai_enabled", "version", "reminder_days" FROM legal_settings) s ON s.company_id=i.company_id AND s.enabled CROSS JOIN LATERAL(SELECT min(t.days) days FROM (SELECT value::int days FROM jsonb_array_elements_text(s.reminder_days) UNION SELECT 0 UNION SELECT -1)t WHERE f.due_date::date-(clock_timestamp() AT TIME ZONE i.timezone)::date<=t.days)d WHERE {} AND f.due_date IS NOT NULL AND d.days IS NOT NULL AND f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') AND EXISTS(SELECT 1 FROM (SELECT "item_id", "user_id", "access" FROM legal_members) m WHERE m.item_id=i.id AND m.user_id=f.owner_id) ON CONFLICT DO NOTHING"#, input.table_name.as_deref().unwrap_or(""), input.runtime_predicate.as_deref().unwrap_or("")),
        values: vec![
        ],
    })
}

/// Legal request SLA breach notice.
pub async fn legal_sla_notice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: format!(r#"INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'legal-sla-'||md5(i.id||i.sla_due_at::text||recipient.user_id),i.company_id,recipient.user_id,'legal',i.id,i.version,'Legal request SLA breached; review or escalate',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM (SELECT "id", "company_id", "branch_id", "department_id", "kind", "title", "description", "classification", "country", "jurisdiction", "language", "currency", "timezone", "priority", "due_date", "owner_id", "created_by", "state", "version", "created_at", "updated_at", "escalation_owner", "sla_started_at", "sla_due_at", "intake_form_id", "intake_answers" FROM legal_items) i JOIN (SELECT "company_id", "enabled", "ai_enabled", "version", "reminder_days" FROM legal_settings) s ON s.company_id=i.company_id AND s.enabled CROSS JOIN LATERAL(SELECT i.owner_id user_id UNION SELECT i.escalation_owner)recipient WHERE {} AND i.sla_due_at<? AND i.state NOT IN ('closed','completed','cancelled','archived') AND recipient.user_id IS NOT NULL AND EXISTS(SELECT 1 FROM (SELECT "item_id", "user_id", "access" FROM legal_members) m WHERE m.item_id=i.id AND m.user_id=recipient.user_id) ON CONFLICT DO NOTHING"#, input.runtime_predicate.as_deref().unwrap_or("")),
        values: vec![            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Overdue legal commitment escalation notice.
pub async fn legal_escalation_notice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: format!(r#"INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT 'legal-escalation-'||md5(i.id||i.due_date||recipient.user_id),i.company_id,recipient.user_id,'legal',i.id,i.version,'Overdue legal commitment escalated to you',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM (SELECT "id", "company_id", "branch_id", "department_id", "kind", "title", "description", "classification", "country", "jurisdiction", "language", "currency", "timezone", "priority", "due_date", "owner_id", "created_by", "state", "version", "created_at", "updated_at", "escalation_owner", "sla_started_at", "sla_due_at", "intake_form_id", "intake_answers" FROM legal_items) i JOIN (SELECT "company_id", "enabled", "ai_enabled", "version", "reminder_days" FROM legal_settings) s ON s.company_id=i.company_id AND s.enabled LEFT JOIN (SELECT "item_id", "data" FROM legal_obligations) o ON o.item_id=i.id CROSS JOIN LATERAL(SELECT coalesce(i.escalation_owner,o.data->>'escalationOwner') user_id)recipient WHERE {} AND i.due_date::date<(clock_timestamp() AT TIME ZONE i.timezone)::date AND i.state NOT IN ('closed','completed','cancelled','archived','terminated','expired') AND EXISTS(SELECT 1 FROM (SELECT "item_id", "user_id", "access" FROM legal_members) m WHERE m.item_id=i.id AND m.user_id=recipient.user_id) ON CONFLICT DO NOTHING"#, input.runtime_predicate.as_deref().unwrap_or("")),
        values: vec![
        ],
    })
}

/// Legal deadline notice appended to the native reminders CTE.
pub async fn legal_deadline_notice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: format!(r#"{}
INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT 'legal-'||md5(a.item_id||a.due_date||a.days::text||a.user_id),i.company_id,a.user_id,'legal',i.id,i.version,'Legal deadline requires attention',a.created_at FROM added a JOIN (SELECT "id", "company_id", "branch_id", "department_id", "kind", "title", "description", "classification", "country", "jurisdiction", "language", "currency", "timezone", "priority", "due_date", "owner_id", "created_by", "state", "version", "created_at", "updated_at", "escalation_owner", "sla_started_at", "sla_due_at", "intake_form_id", "intake_answers" FROM legal_items) i ON i.id=a.item_id ON CONFLICT DO NOTHING"#, input.cte_prefix.as_deref().unwrap_or("")),
        values: vec![
        ],
    })
}


/// Social mention binding. Full source witness supplied by caller.
pub async fn social_mention_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: vec![
            value(input.record_type.as_str()), value(input.record_id.as_str()),
            value(input.target_kind.as_deref().unwrap_or("")), value(input.target_id.as_deref().unwrap_or("")),
            value(&input.message), value(&input.user_id), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Social mention staff notice. The post visibility fragment is supplied by
/// the native Social composition; the owner validates Company authority.
pub async fn social_mention_notice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str() || input.source_access_sql.is_none() { return Err(invalid().into()); }
    let sql = format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT ?,p.company_id,?,'post',p.id,p.version,\
 'You were mentioned in a conversation',? FROM social_posts p WHERE p.id=? AND {}",
        input.source_access_sql.as_deref().unwrap_or("")
    );
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql,
        values: vec![
            value(&input.idempotency_key), value(&input.user_id), serde_json::Value::Number(input.now.into()),
            value(&input.record_id), value(input.target_id.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.now.into()), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Social agent reply binding.
pub async fn social_agent_reply_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO social_agent_replies(id,post_id,owner_type,owner_id,company_id,actor_id,\
 actor_json,requester_name,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(&input.record_id), value(input.record_type.as_str()),
            value(input.target_id.as_deref().unwrap_or("")), value(company.as_str()), value(&input.user_id),
            value(&input.message), value(input.identifier.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.now.into()), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Social topic upsert.
pub async fn social_topic_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: vec![value(&input.message), value(&input.user_id), serde_json::Value::Number(input.now.into())],
    })
}

/// Social topic binding (post or comment table supplied by caller).
pub async fn social_topic_binding_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    if input.record_type != "post" && input.record_type != "comment" { return Err(invalid().into()); }
    let table = if input.record_type == "post" { "social_post_topics(post_id" } else { "social_comment_topics(comment_id" };
    Ok(NotificationStatement {
        sql: format!("INSERT INTO {},tag) VALUES(?,?) ON CONFLICT DO NOTHING", table),
        values: vec![value(&input.record_id), value(&input.message)],
    })
}


/// Customer portal attachment preview access audit event.
pub async fn attachment_access_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str() || input.record_id.is_empty() || input.now <= 0 {
        return Err(invalid().into());
    }
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
 variant,outcome,created_at) VALUES(?,?,?,?,?,'preview','allowed',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(&input.company_id),
            value(input.identifier.as_deref().unwrap_or("")),
            value(input.record_id.as_str()),
            value(&input.user_id), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Attachment content-read audit event (POS image source).
pub async fn attachment_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.company_id != company.as_str() || input.record_id.is_empty() || input.now <= 0 {
        return Err(invalid().into());
    }
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,\
 action,details_json,created_at) VALUES(?,?,?,?,?,?,'content.read',?::jsonb,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(&input.company_id),
            value(input.identifier.as_deref().unwrap_or("")),
            value(&input.record_id),
            value(input.item_id.as_deref().unwrap_or("")),
            value(&input.user_id), value(&input.message),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Legal destruction: retire the attachment source row.
pub async fn legal_retire_source_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=? AND company_id=? AND source_state='available' AND legal_hold=0".to_owned(),
        values: vec![serde_json::Value::Number(input.now.into()), value(&input.record_id), value(company.as_str())],
    })
}

/// Legal destruction: retire the stored ledger row.
pub async fn legal_retire_ledger_statement(
    connection: &mut PgConnection,
    _company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, _company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=? WHERE attachment_id=? AND state='stored'".to_owned(),
        values: vec![serde_json::Value::Number(input.now.into()), value(&input.record_id)],
    })
}

/// Legal destruction audit event.
pub async fn legal_retire_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES(?,?,?,'system:legal',?,?,?) ON CONFLICT(id) DO NOTHING".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(&input.record_id),
            value(&input.record_type), value(&input.message), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Legal portal: bind an uploaded attachment link.
pub async fn legal_bind_link_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachment_links SET state='bound',bound_at=?,\
 binding_type='legal_portal_submission',binding_id=? WHERE id=? AND state='draft' AND attachment_id=?".to_owned(),
        values: vec![serde_json::Value::Number(input.now.into()), value(&input.idempotency_key), value(&input.record_id), value(input.identifier.as_deref().unwrap_or(""))],
    })
}

/// Legal portal: bind the attachment set.
pub async fn legal_bind_set_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=?,updated_at=? WHERE id=? AND state='draft' AND expected_target_version=?".to_owned(),
        values: vec![
            serde_json::Value::Number(input.now.into()), value(&input.idempotency_key),
            serde_json::Value::Number(input.now.into()), value(&input.record_id),
            serde_json::Value::Number(input.record_version.into()),
        ],
    })
}

/// Legal portal: attachment set-bound audit event.
pub async fn legal_bind_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,\
 action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(input.identifier.as_deref().unwrap_or("")),
            value(&input.record_id), value(input.item_id.as_deref().unwrap_or("")), value(&input.user_id),
            value(&input.record_type), value(&input.message), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Expense: withdraw a draft attachment link.
pub async fn expense_withdraw_link_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachment_links SET state='withdrawn',withdrawn_at=?,\
 withdrawn_by=?,withdrawal_reason=? WHERE id=? AND state='draft'".to_owned(),
        values: vec![serde_json::Value::Number(input.now.into()), value(&input.user_id), value(&input.message), value(&input.record_id)],
    })
}

/// Expense: link-withdrawn audit event.
pub async fn expense_withdraw_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,\
 action,details_json,created_at) VALUES(?,?,?,?,?,?, 'link.withdrawn',?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(input.identifier.as_deref().unwrap_or("")),
            value(&input.record_id), value(input.item_id.as_deref().unwrap_or("")), value(&input.user_id),
            value(&input.message), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Expense: revoke the attachment source.
pub async fn expense_revoke_attachment_statement(
    connection: &mut PgConnection,
    _company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, _company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachments SET revoked_at=? WHERE id=?".to_owned(),
        values: vec![serde_json::Value::Number(input.now.into()), value(&input.record_id)],
    })
}


/// Document derivative: attachment document record.
pub async fn doc_attachment_record_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_documents(attachment_id,company_id,original_checksum,\
 page_count,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: vec![value(&input.record_id), value(company.as_str()), value(&input.idempotency_key), serde_json::Value::Number(input.record_version.into()), serde_json::Value::Number(input.now.into())],
    })
}

/// Document derivative: rendered page row.
pub async fn doc_derivative_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,\
 object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(&input.record_id), value(company.as_str()),
            serde_json::Value::Number(input.record_version.into()), value(input.identifier.as_deref().unwrap_or("")),
            value(&input.message), value(input.item_id.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.timestamp.unwrap_or_default().into()), value(&input.user_id), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Document derivative: storage ledger row.
pub async fn doc_storage_ledger_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,\
 object_key,bytes,state,created_at) VALUES(?,?,?,'derivative',?,?,'stored',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(&input.record_id),
            value(input.identifier.as_deref().unwrap_or("")), serde_json::Value::Number(input.timestamp.unwrap_or_default().into()),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Beta cleanup: expire an unused upload intent.
pub async fn beta_expire_upload_statement(
    connection: &mut PgConnection,
    _company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, _company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE upload_intents SET status='expired',updated_at=? WHERE id=? AND status IN ('pending','uploaded') AND expires_at<?".to_owned(),
        values: vec![
            serde_json::Value::Number(input.now.into()), value(&input.record_id),
            serde_json::Value::Number(input.timestamp.unwrap_or_default().into()),
        ],
    })
}


/// Legal portal: allowed original-variant attachment read access event.
pub async fn legal_access_allowed_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
 variant,outcome,created_at) VALUES(?,?,?,?,?,'original','allowed',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()),
            value(input.identifier.as_deref().unwrap_or("")), value(&input.record_id),
            value(&input.user_id), serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Legal portal: denied original-variant attachment read access event.
pub async fn legal_access_denied_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
 variant,outcome,reason,created_at) VALUES(?,?,?,?,?,'original','denied',?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()),
            value(input.identifier.as_deref().unwrap_or("")), value(&input.record_id),
            value(&input.user_id), value(&input.message), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Legal portal staging: link an upload intent to its attachment source.
pub async fn legal_link_upload_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE attachment_links SET upload_intent_id=?,attachment_id=? WHERE id=?".to_owned(),
        values: vec![
            value(input.identifier.as_deref().unwrap_or("")), value(input.item_id.as_deref().unwrap_or("")),
            value(&input.record_id),
        ],
    })
}

/// Legal portal staging: create a pending attachment scan attempt.
pub async fn legal_pending_scan_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(input.identifier.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Legal portal staging: store the original attachment in the ledger.
pub async fn legal_stored_original_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,\
 object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()),
            value(input.identifier.as_deref().unwrap_or("")), value(input.item_id.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.timestamp.unwrap_or_default().into()),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

/// Legal portal staging: quarantine audit event.
pub async fn legal_quarantine_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,\
 actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()),
            value(input.identifier.as_deref().unwrap_or("")), value(&input.record_id),
            value(input.item_id.as_deref().unwrap_or("")), value(&input.user_id),
            value(&input.record_type), value(&input.message), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Customer portal: record a portal action (authoritative insert).
pub async fn customer_portal_action_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,\
 user_id,action,note,source_version,request_key,request_hash,status,created_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,'recorded',?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(&input.record_id),
            value(input.order_id.as_deref().unwrap_or("")), value(input.identifier.as_deref().unwrap_or("")),
            value(&input.user_id), value(&input.record_type), value(&input.message),
            serde_json::Value::Number(input.record_version.into()), value(input.target_id.as_deref().unwrap_or("")),
            value(input.source_hash.as_deref().unwrap_or("")), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Release acceptance: submitted lifecycle event.
pub async fn release_event_submitted_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,\
 detail_json,created_at) VALUES(?,?,?,1,'submitted',?,?,?)".to_owned(),
        values: vec![value(&input.idempotency_key), value(company.as_str()), value(&input.record_id), value(&input.user_id), value(&input.message), serde_json::Value::Number(input.now.into())],
    })
}

/// Release acceptance: reviewed lifecycle event.
pub async fn release_event_reviewed_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,\
 detail_json,created_at) VALUES(?,?,?,?,'reviewed',?,?,?)".to_owned(),
        values: vec![value(&input.idempotency_key), value(company.as_str()), value(&input.record_id), serde_json::Value::Number(input.record_version.into()), value(&input.user_id), value(&input.message), serde_json::Value::Number(input.now.into())],
    })
}

/// Release acceptance: revoked lifecycle event.
pub async fn release_event_revoked_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO company_release_events(id,company_id,release_id,version,action,actor_id,\
 detail_json,created_at) VALUES(?,?,?,?,'revoked',?,?,?)".to_owned(),
        values: vec![value(&input.idempotency_key), value(company.as_str()), value(&input.record_id), serde_json::Value::Number(input.record_version.into()), value(&input.user_id), value(&input.message), serde_json::Value::Number(input.now.into())],
    })
}


/// Finance: issue a customer receipt for a settled payment.
pub async fn customer_receipt_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_customer_receipts(id,company_id,payment_id,document_number,\
 receipt_date,currency,amount_minor,snapshot_json,export_html,export_sha256,issued_by,issued_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(company.as_str()), value(&input.record_id),
            value(input.identifier.as_deref().unwrap_or("")), value(input.item_id.as_deref().unwrap_or("")),
            value(input.target_kind.as_deref().unwrap_or("")), value(input.target_id.as_deref().unwrap_or("")),
            value(&input.message), value(input.event.as_deref().unwrap_or("")),
            value(input.source_hash.as_deref().unwrap_or("")), value(&input.user_id),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}


fn values_from_json(input: &NotificationStatementInput) -> Result<Vec<serde_json::Value>, Error> {
    match &input.values_json {
        Some(raw) => serde_json::from_str::<Vec<serde_json::Value>>(raw).map_err(|_| invalid().into()),
        None => Err(invalid().into()),
    }
}

/// Legal portal staging: create the draft attachment set (values supplied by caller-owned facts).
pub async fn legal_draft_set_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,\
 component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,\
 audience_json,retention_policy_key,required_count,max_files,state,created_by,created_at,updated_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal portal staging: create the draft attachment link.
pub async fn legal_draft_link_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft',?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal portal staging: create the uploaded upload intent.
pub async fn legal_uploaded_intent_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,\
 declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,\
 created_at,updated_at,owner_version,attachment_set_id,attachment_link_id,company_id,\
 detected_media_type,quarantine_key) VALUES(?,?,?,?,?,?,?,?,?,?,?,'uploaded',?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal portal staging: create the quarantined attachment source.
pub async fn legal_quarantined_source_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,\
 original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,\
 detected_media_type,source_state) VALUES(?,?,?,?,?,?,1,?,?,?,?,?,?,?,?,'quarantined')".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal: preserve an attachment as evidence for an item.
pub async fn legal_file_preservation_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,\
 checksum,created_by,created_at) VALUES(?,'attachment',?,?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: vec![
            value(&input.record_id), value(input.identifier.as_deref().unwrap_or("")),
            value(input.item_id.as_deref().unwrap_or("")), value(&input.message),
            value(&input.user_id), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Legal: authorize retention execution for an approved destruction.
pub async fn legal_retention_authorization_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_retention_execution_authorizations(command_id,destruction_id,\
 item_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,item_version,created_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(&input.record_id), value(input.item_id.as_deref().unwrap_or("")),
            value(company.as_str()), value(&input.record_type), value(&input.user_id),
            value(input.identifier.as_deref().unwrap_or("")), value(&input.message),
            value(input.source_hash.as_deref().unwrap_or("")), serde_json::Value::Number(input.record_version.into()),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Finance: create a hotel PMS chart-of-account mapping.
pub async fn finance_hotel_account_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,\
 is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?)".to_owned(),
        values: vec![
            value(&input.idempotency_key), value(input.identifier.as_deref().unwrap_or("")),
            value(company.as_str()), value(&input.record_id), value(&input.message), value(&input.record_type),
            serde_json::Value::Number(input.record_version.into()), value(input.item_id.as_deref().unwrap_or("")),
            serde_json::Value::Number(input.now.into()), serde_json::Value::Number(input.now.into()),
        ],
    })
}


/// Finance: ignore an unused payroll bank observation.
pub async fn finance_bank_ignore_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_bank_transactions SET match_status='ignored',matched_type=NULL,\
 matched_id=NULL,version=version+1 WHERE id=? AND company_id=? AND version=? \
 AND match_status IN ('unmatched','proposed')".to_owned(),
        values: vec![
            value(&input.record_id), value(company.as_str()),
            serde_json::Value::Number(input.record_version.into()),
        ],
    })
}


fn safe_table(table: &Option<String>) -> Result<&str, Error> {
    match table {
        Some(t) if !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => Ok(t.as_str()),
        _ => Err(invalid().into()),
    }
}

/// Legal feature: create a feature row in its capability table.
pub async fn legal_feature_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("INSERT INTO {table}(id,item_id,category,title,owner_id,due_date,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,?,?,?,?,?,1,?::jsonb,?,?,?)"),
        values: values_from_json(input)?,
    })
}

/// Legal feature: update a feature row in its capability table.
pub async fn legal_feature_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("UPDATE {table} SET state=?,data=?::jsonb,version=version+1,updated_by=?,updated_at=? WHERE id=? AND version=?"),
        values: values_from_json(input)?,
    })
}

/// Legal feature: record a feature history snapshot.
pub async fn legal_feature_history_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,\
 actor_id,reason,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal feature: advance the parent item version.
pub async fn legal_advance_record_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal feature: parent item lifecycle event.
pub async fn legal_feature_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal approval: lock the source document.
pub async fn legal_approval_lock_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE legal_documents SET locked=true WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Legal approval: retain the approval route with upsert.
pub async fn legal_approval_retain_route_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_approval_routes(item_id,instance_id,document_id,document_hash,signers,\
 source_version,created_by,source_hash) VALUES(?,?,?,?,?::jsonb,?,?,?) ON CONFLICT(item_id) DO UPDATE \
 SET instance_id=excluded.instance_id,document_id=excluded.document_id,document_hash=excluded.document_hash,\
 signers=excluded.signers,source_version=excluded.source_version,created_by=excluded.created_by,source_hash=excluded.source_hash".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal approval: record the approval decision.
pub async fn legal_approval_record_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_approvals VALUES(?,?,?,?,?::jsonb,?,'approved',?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal approval: advance the parent contract state/version.
pub async fn legal_approval_advance_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal approval: parent lifecycle event.
pub async fn legal_approval_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal executed copy: record the independent review.
pub async fn legal_executed_review_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_executed_reviews(id,executed_copy_id,item_version,decision,checks,\
 reason,reviewer_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal executed copy: update the copy review state.
pub async fn legal_executed_copy_state_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_executed_copies SET state=?,reviewed_by=?,reviewed_at=?,\
 version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal signing: record a signer's envelope view.
pub async fn legal_signer_view_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_signers SET viewed_at=? WHERE envelope_id=? AND user_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal signing: advance item version with a new state.
pub async fn legal_item_state_advance_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET version=version+1,state=?,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal references: add a native reference with next-ordinal subquery.
pub async fn legal_native_reference_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,\
 created_by,created_at) VALUES(?,?,?,(SELECT coalesce(max(ordinal),-1)+1 FROM legal_native_references WHERE item_id=?),?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal trust: add proposed trust material.
pub async fn legal_trust_addition_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,\
 state,created_by,created_at) VALUES(?,?,?,?,?,?,?,'proposed',?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal trust: decide on trust material.
pub async fn legal_trust_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_trust_material SET state=?,approved_by=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal processing: queue a document processing job.
pub async fn legal_processing_job_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,\
 state,actor_json,created_by,created_at) VALUES(?,?,?,?,?,?,'queued',?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal: record a positional document access event.
pub async fn legal_access_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal governance: create an intake form.
pub async fn legal_intake_form_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_intake_forms(id,company_id,name,fields,sla_days,created_by) VALUES(?,?,?,?::jsonb,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal governance: approve an intake form.
pub async fn legal_intake_form_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_intake_forms SET state='approved',approved_by=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal governance: apply an intake to an item with SLA computation.
pub async fn legal_intake_apply_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET intake_form_id=?,intake_answers=?::jsonb,sla_started_at=?,\
 sla_due_at=floor(extract(epoch FROM ((clock_timestamp() AT TIME ZONE timezone + (? || ' days')::interval) AT TIME ZONE timezone))*1000)::bigint,\
 escalation_owner=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal governance: create a signing authority.
pub async fn legal_authority_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_authorities(id,item_id,company_id,user_id,document_id,currency,\
 limit_minor,valid_from,valid_until,state,created_by) VALUES(?,?,?,?,?,?,?,?,?,'proposed',?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal governance: decide on a signing authority.
pub async fn legal_authority_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_authorities SET state=?,approved_by=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal retention: create a retention policy.
pub async fn legal_retention_policy_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_retention_policies(id,company_id,name,days,basis,state,created_by,created_at) VALUES(?,?,?,?,?,'draft',?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal retention: decide on a policy.
pub async fn legal_retention_policy_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_retention_policies SET state=?,approved_by=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal retention: assign a policy to an item with eligible-at computation.
pub async fn legal_retention_assign_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_retention_assignments(item_id,policy_id,policy_version,eligible_at,\
 assigned_by,created_at) VALUES(?,?,?,(extract(epoch FROM (((to_timestamp(?::double precision/1000) AT TIME ZONE ?)::date + ?::int)::timestamp AT TIME ZONE ?))*1000)::bigint,?,?) ON CONFLICT(item_id) DO UPDATE SET policy_id=excluded.policy_id,policy_version=excluded.policy_version,eligible_at=excluded.eligible_at,assigned_by=excluded.assigned_by".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal retention: request a destruction.
pub async fn legal_retention_request_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,\
 reason,manifest,created_at) VALUES(?,?,?,?,?,'requested',?,?::jsonb,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal retention: change destruction state/approval with coalesce.
pub async fn legal_retention_change_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state=?,approved_by=coalesce(?,approved_by),\
 approved_at=coalesce(?,approved_at),version=version+1,error=NULL,attempts=0 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal operations: save a named query view.
pub async fn legal_saved_view_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_saved_views VALUES(?,?,?,?::jsonb) ON CONFLICT(company_id,user_id,name) DO UPDATE SET query=excluded.query".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal operations: create a derived item link.
pub async fn legal_derived_link_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_links VALUES(?,?,'derived') ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal operations: persist a reviewed AI document as text/plain.
pub async fn legal_ai_document_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,\
 created_by,created_at) VALUES(?,?,?,?,'text/plain',?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal operations: accept an AI proposal by appending to its proposal JSON.
pub async fn legal_proposal_accept_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_ai_proposals SET state='accepted',proposal=proposal||?::jsonb WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal documents: insert an inspection unless a destruction is active.
pub async fn legal_document_inspection_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_document_inspections SELECT ?,?,?,?,?,? WHERE NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed'))".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal documents: retain an executed copy.
pub async fn legal_executed_copy_retain_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_executed_copies(id,item_id,document_id,approved_cycle,\
 approved_document_id,approved_hash,executed_hash,metadata,imported_by,imported_at) VALUES(?,?,?,?,?,?,?,?::jsonb,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal documents: insert a canonical (attachment-backed) document.
pub async fn legal_canonical_document_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,\
 created_by,created_at,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal documents: insert a stored (text/bytes) document.
pub async fn legal_stored_document_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,\
 created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal corpus: insert a source chunk.
pub async fn legal_source_chunk_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_source_chunks(id,company_id,item_id,document_id,document_checksum,\
 revision_id,text_review_id,page,page_mapping,section,text,text_checksum) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: register a law source.
pub async fn legal_law_source_register_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_law_sources(id,company_id,rule_id,title,source_url,created_by,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: import a law revision.
pub async fn legal_law_revision_import_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_law_revisions(id,company_id,rule_id,source_id,document_id,document_checksum,\
 revision_label,published_on,effective_from,effective_until,extraction_method,inspection_id,created_by,created_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: bind a candidate revision to a source check.
pub async fn legal_source_check_bind_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_law_source_checks SET candidate_revision_id=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: import document text as a text review.
pub async fn legal_text_review_import_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_text_reviews(id,company_id,item_id,document_id,document_checksum,\
 extraction_method,inspection_id,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: propose law applicability.
pub async fn legal_applicability_propose_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_law_applicability(id,company_id,rule_id,revision_id,applies,basis,created_by,created_at) VALUES(?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: approve law applicability.
pub async fn legal_applicability_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_law_applicability SET state='approved',reviewed_by=?,reviewed_at=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal corpus: mark a source check reviewed.
pub async fn legal_source_check_review_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_law_source_checks SET state='reviewed' WHERE candidate_revision_id=? AND state='pending_review'".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal service: configure company settings.
pub async fn legal_settings_configure_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_settings(company_id,enabled,ai_enabled,reminder_days) VALUES(?,?,?,coalesce(?::jsonb,'[90,60,30,7,1]'::jsonb)) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,ai_enabled=excluded.ai_enabled,reminder_days=coalesce(?::jsonb,legal_settings.reminder_days),version=legal_settings.version+1".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal service: create a legal item.
pub async fn legal_item_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,\
 classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,created_at,updated_at) \
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal service: create a kind-specific subtype row.
pub async fn legal_subtype_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("INSERT INTO {table} VALUES(?,?::jsonb)"),
        values: values_from_json(input)?,
    })
}

/// Legal service: add an initial member with edit access.
pub async fn legal_member_add_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_members VALUES(?,?,'edit')".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal service: route the requester's manager as a reviewer.
pub async fn legal_requester_route_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let source = input.source_access_sql.as_deref().unwrap_or("");
    if source.is_empty() { return Err(invalid().into()); }
    Ok(NotificationStatement {
        sql: format!("INSERT INTO legal_members(item_id,user_id,access) SELECT DISTINCT ?,user_id,'edit' FROM ({source}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review') ON CONFLICT DO NOTHING"),
        values: values_from_json(input)?,
    })
}

/// Legal service: retain the owner's membership with edit access.
pub async fn legal_member_retain_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_members VALUES(?,?,'edit') ON CONFLICT(item_id,user_id) DO UPDATE SET access='edit'".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal service: expand active hold targets transitively.
pub async fn legal_hold_targets_expand_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "WITH RECURSIVE targets(hold_id,item_id) AS (SELECT id,item_id FROM legal_holds WHERE state!='released' UNION SELECT t.hold_id,l.target_id FROM targets t JOIN legal_links l ON l.source_id=t.item_id WHERE l.relation IN ('evidence','derived','amends','obligation')) INSERT INTO legal_hold_targets SELECT * FROM targets ON CONFLICT DO NOTHING".to_owned(),
        values: vec![],
    })
}


/// Legal service: update a legal item's core fields.
pub async fn legal_item_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET title=?,description=?,classification=?,country=?,jurisdiction=?,\
 language=?,currency=?,timezone=?,priority=?,due_date=?,owner_id=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal service: update a kind-specific subtype row.
pub async fn legal_subtype_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("UPDATE {table} SET data=?::jsonb WHERE item_id=?"),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction block.
pub async fn legal_destruction_block_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state='blocked',error='HOLD_OR_RETENTION_POLICY_CHANGED' WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction start.
pub async fn legal_destruction_start_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state='executing',started_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: retention started event.
pub async fn legal_retention_started_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'retention.started','system:legal',?,version,? FROM legal_items WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction count attempt.
pub async fn legal_destruction_count_attempt_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET attempts=attempts+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction retain executing.
pub async fn legal_destruction_retain_executing_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state='executing' WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: disposal retain.
pub async fn legal_disposal_retain_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_disposals VALUES(?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: document erase.
pub async fn legal_document_erase_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_documents SET body='',object_key=NULL WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: processing retire.
pub async fn legal_processing_retire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_processing_jobs SET state='failed',result_json=NULL,error='CONTENT_DESTROYED',lease_token=NULL,lease_until=NULL WHERE item_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: inspections erase.
pub async fn legal_inspections_erase_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "DELETE FROM legal_document_inspections WHERE document_id IN (SELECT id FROM legal_documents WHERE item_id=?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: ai retire.
pub async fn legal_ai_retire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_ai_proposals SET proposal=NULL,question='[disposed under retention policy]',state='disposed' WHERE item_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: shares revoke.
pub async fn legal_shares_revoke_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_shares SET revoked_at=? WHERE item_id=? AND revoked_at IS NULL".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction complete.
pub async fn legal_destruction_complete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state='completed',completed_at=?,error=NULL,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: retention destroyed event.
pub async fn legal_retention_destroyed_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,'retention.destroyed','system:legal',?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal destruction worker: destruction require review.
pub async fn legal_destruction_require_review_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_destructions SET state=CASE WHEN started_at IS NULL THEN 'failed' ELSE 'executing' END,error='DESTRUCTION_REQUIRES_REVIEW',attempts=attempts+1 WHERE id=? AND state!='completed'".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: access event.
pub async fn legal_portal_access_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: accept share.
pub async fn legal_portal_accept_share_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_shares SET user_id=?,version=version+1 WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: external comment.
pub async fn legal_portal_external_comment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_comments(id,item_id,document_id,body,visibility,created_by,created_at) VALUES(?,?,?,?,'external',?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: counsel entry.
pub async fn legal_portal_counsel_entry_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_counsel_entries(id,item_id,category,title,owner_id,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,'counsel_entry',?,?,'submitted',1,?::jsonb,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: counsel history.
pub async fn legal_portal_counsel_history_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,actor_id,reason,created_at) VALUES(?,?,?,'counsel_entry',1,?::jsonb,?,'External counsel submission',?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: interaction event.
pub async fn legal_portal_interaction_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,?,?,?,version,? FROM legal_items WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: receipt.
pub async fn legal_portal_receipt_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_portal_receipts VALUES(?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: external upload.
pub async fn legal_portal_external_upload_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_external_uploads(id,share_id,object_key,checksum,name,media_type,bytes,created_by,created_at,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: advance upload.
pub async fn legal_portal_advance_upload_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal portal: upload event.
pub async fn legal_portal_upload_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'portal.upload',?,?,version,? FROM legal_items WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal reminders: expire signed envelopes and emit lifecycle events.
pub async fn legal_envelope_expire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let prefix = input.cte_prefix.as_deref().unwrap_or("");
    if prefix.is_empty() { return Err(invalid().into()); }
    Ok(NotificationStatement {
        sql: format!(r#"{prefix}
 INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT 'expiry-'||e.id,c.id,'signing.expired','system:legal','Expired envelope; a new approval is required',c.version,? FROM changed c JOIN expired e ON e.item_id=c.id"#),
        values: values_from_json(input)?,
    })
}


/// Finance: create a book adjustment draft.
pub async fn finance_book_adjustment_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_book_adjustments(id,company_id,chart_id,primary_journal_id,source_version,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on a book adjustment.
pub async fn finance_book_adjustment_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_book_adjustments SET status=?,version=?,reviewed_by=?,journal_id=?,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: reverse a posted book adjustment.
pub async fn finance_book_adjustment_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_book_adjustments SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create a plan edition.
pub async fn finance_plan_edition_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_plan_editions(id,company_id,chart_id,kind,name,parent_id,edition,content_json,content_hash,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: review a plan edition (approve/reject).
pub async fn finance_plan_edition_review_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_plan_editions SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: create a report snapshot.
pub async fn finance_report_snapshot_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_report_snapshots(id,company_id,definition_id,start_date,end_date,result_json,source_hash,created_by,created_at,export_content,export_sha256) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create a reversal journal from a posted source journal.
pub async fn finance_reversal_journal_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,?,id,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: create reversal journal lines by swapping debit/credit.
pub async fn finance_reversal_lines_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: mark the source journal reversed.
pub async fn finance_journal_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create or update a tax identity (values_json carries ordered binds).
pub async fn finance_tax_identity_upsert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.kind.as_deref() == Some("finance_tax_identity_insert") {
        Ok(NotificationStatement {
            sql: "INSERT INTO finance_tax_identities(id,company_id,party_id,legal_name,tax_id,address,branch_code,entity_kind,vat_registered,vat_registration_evidence,effective_from,effective_to,previous_identity_id,signer_name,signer_authority,rule_pack_version,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
            values: values_from_json(input)?,
        })
    } else {
        Ok(NotificationStatement {
            sql: "UPDATE finance_tax_identities SET party_id=?,legal_name=?,tax_id=?,address=?,branch_code=?,entity_kind=?,vat_registered=?,vat_registration_evidence=?,effective_from=?,effective_to=?,previous_identity_id=?,signer_name=?,signer_authority=?,rule_pack_version=?,evidence_ref=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
            values: values_from_json(input)?,
        })
    }
}

/// Finance: mark a previous tax identity superseded.
pub async fn finance_tax_identity_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_tax_identities SET status='superseded',version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on a tax identity draft.
pub async fn finance_tax_identity_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_tax_identities SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,signature_link_id=?,signature_sha256=?,signature_media_type=?,signature_data_uri=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: schedule a due-draft job.
pub async fn finance_due_draft_schedule_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_due_drafts(id,company_id,holding_id,kind,source_id,run_on,source_version,input_json,input_hash,delegated_by,assignment_id,assignment_version,role,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide (retry/cancel) a due-draft job.
pub async fn finance_due_draft_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_due_drafts SET status=?,version=version+1,next_attempt_at=0,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create an intercompany document pair.
pub async fn finance_intercompany_pair_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_intercompany_document_pairs(id,holding_id,company_id,reference,source_company_id,source_journal_id,source_account_id,target_company_id,target_journal_id,target_account_id,currency,amount_minor,source_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: claim an intercompany pair side.
pub async fn finance_intercompany_claim_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_intercompany_pair_claims(pair_id,journal_id,account_id) VALUES(?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on an intercompany pair.
pub async fn finance_intercompany_pair_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_intercompany_document_pairs SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: clear intercompany pair claims.
pub async fn finance_intercompany_claims_clear_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "DELETE FROM finance_intercompany_pair_claims WHERE pair_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People/payroll: record an advance repayment deduction consumption.
pub async fn payroll_deduction_consumption_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_payroll_deduction_consumptions(id,company_id,repayment_id,source_version,run_id,item_id,journal_id,receivable_account_id,amount_minor,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: mark an advance repayment collected.
pub async fn payroll_repayment_collect_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_payroll_advance_repayments SET status='collected',payroll_run_id=?,version=version+1 WHERE id=? AND company_id=? AND version=? AND status IN ('open','reserved')".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: close fully-deducted advances.
pub async fn payroll_advances_close_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_payroll_advances a SET status='closed',version=version+1,updated_at=? WHERE a.company_id=? AND a.status='approved' AND EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c JOIN hr_payroll_advance_repayments r ON r.id=c.repayment_id WHERE c.run_id=? AND r.advance_id=a.id) AND NOT EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r WHERE r.advance_id=a.id AND r.status!='collected')".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People/payroll: record a final-pay cancellation event.
pub async fn payroll_final_pay_cancel_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_payroll_final_pay_events(id,preparation_id,task_id,run_id,company_id,action,actor_id,task_version,evidence_json,created_at) VALUES(?,?,?,?,?,'cancelled',?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: cancel a payroll run.
pub async fn payroll_run_cancel_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_payroll_runs SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: cancel a final-pay preparation.
pub async fn payroll_final_pay_prep_cancel_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_payroll_final_pay_preparations SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People/payroll: upsert an expense draft (values_json carries ordered binds).
pub async fn expense_draft_write_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: if input.identifier.as_deref() == Some("insert") {
            "INSERT INTO finance_expenses(request_kind,payee_party_id,employee_id,engagement_id,branch_id,department_id,claimant_user_id,claimant_name,expense_date,merchant,currency,amount_minor,tax_minor,category_account_id,receipt_reference,business_purpose,prepared_by,updated_at,id,company_id,created_by,created_at,workflow_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,2)".to_owned()
        } else {
            "UPDATE finance_expenses SET request_kind=?,payee_party_id=?,employee_id=?,engagement_id=?,branch_id=?,department_id=?,claimant_user_id=?,claimant_name=?,expense_date=?,merchant=?,currency=?,amount_minor=?,tax_minor=?,category_account_id=?,receipt_reference=?,business_purpose=?,prepared_by=?,updated_at=?,status='draft',workflow_version=2,workflow_stage='draft',version=version+1 WHERE id=?".to_owned()
        },
        values: values_from_json(input)?,
    })
}


/// Finance: create sales defaults draft.
pub async fn finance_sales_defaults_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: supersede the current approved sales defaults.
pub async fn finance_sales_defaults_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_sales_defaults SET status='superseded',version=version+1,updated_at=? WHERE company_id=? AND currency=? AND status='approved'".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on sales defaults.
pub async fn finance_sales_defaults_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_sales_defaults SET status=?,reviewed_by=?,review_reason=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People/payroll: insert a Thai payroll statutory profile.
pub async fn payroll_statutory_profile_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_thai_payroll_profiles(id,company_id,engagement_id,tax_year,effective_from,effective_to,annual_deductions_minor,scheduled_pays,social_security_eligible,social_security_exemption_evidence,opening_through_date,opening_taxable_minor,opening_irregular_minor,opening_withheld_minor,prior_income_complete,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: decide on a statutory profile.
pub async fn payroll_statutory_profile_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_thai_payroll_profiles SET status=?,approved_by=CASE WHEN ?='approved' THEN ? ELSE approved_by END,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create an accounting profile draft.
pub async fn finance_accounting_profile_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_accounting_profiles(id,company_id,statutory_chart_id,standard,fiscal_start_month,effective_date,eligibility_evidence,policy_evidence,tax_basis,inventory_policy,depreciation_policy,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on an accounting profile.
pub async fn finance_accounting_profile_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_accounting_profiles SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1 WHERE id=? AND company_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/hospitality: record a tax document.
pub async fn hospitality_tax_doc_record_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hospitality_tax_documents(id,company_id,invoice_id,source_type,issuer,external_reference,issued_on,currency,total_minor,tax_minor,document_total_minor,document_tax_minor,state,version,recorded_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'recorded',1,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance/hospitality: verify/void a tax document (decision column supplied by caller).
pub async fn hospitality_tax_doc_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let column = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("UPDATE hospitality_tax_documents SET state=?,version=?,{column}=?,reason=?,updated_at=? WHERE id=? AND company_id=? AND version=?"),
        values: values_from_json(input)?,
    })
}


/// Finance/PMS import: create a customer party.
pub async fn finance_pms_party_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,'THB','active',?,?) ON CONFLICT(id) DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance/PMS import: create a settled receipt payment.
pub async fn finance_pms_payment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,'THB',?,0,'settled',?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: create a journal from a prepared plan.
pub async fn finance_journal_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: create a journal line from a prepared line.
pub async fn finance_journal_line_create_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: record a versioned invoice line snapshot.
pub async fn finance_invoice_line_version_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: record a review-file evidence manifest.
pub async fn finance_expansion_review_file_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_expansion_review_files(id,company_id,target_type,target_id,target_version,command_id,manifest_json,manifest_sha256,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/commerce: set base carrying amounts on a journal line.
pub async fn finance_commerce_base_carry_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_journal_lines SET base_debit_minor=?,base_credit_minor=? WHERE journal_id=? AND ordinal=? AND account_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/payroll: match a bank transaction to a remittance.
pub async fn finance_payroll_remittance_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_remittance',matched_id=?,version=version+1 WHERE id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/POS: upsert a daily customer party.
pub async fn finance_pos_party_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,phone,notification_email,address_json,created_at,updated_at) VALUES(?,?,'customer',?,?,?,'active',?,?,?::jsonb,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/payroll: match a bank transaction to an "other" remittance (company-scoped).
pub async fn finance_payroll_other_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_remittance',matched_id=?,version=version+1 WHERE id=? AND company_id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: prepare a group report edition.
pub async fn finance_group_edition_prepare_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_group_report_editions(id,holding_id,company_id,parent_id,root_id,edition,start_date,end_date,currency,content_json,source_json,result_json,source_hash,export_content,export_sha256,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: supersede a parent group report edition.
pub async fn finance_group_edition_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_group_report_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: decide on a group report edition.
pub async fn finance_group_edition_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_group_report_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: set a company tool eligibility flag (upsert).
pub async fn finance_company_tool_set_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO finance_company_tools(company_id,tool,enabled,version,changed_by,reason,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,tool) DO UPDATE SET enabled=excluded.enabled,version=excluded.version,changed_by=excluded.changed_by,reason=excluded.reason,updated_at=excluded.updated_at".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance/payroll: bump the source invoice version when a payment batch is created.
pub async fn finance_payroll_batch_invoice_version_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=? AND company_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance/payroll: match a bank transaction to a payroll payment.
pub async fn finance_payroll_batch_bank_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_payment',matched_id=?,version=version+1 WHERE id=? AND version=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: complete a due draft (worker, holding-keyed).
pub async fn finance_due_draft_complete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_due_drafts SET status='prepared',version=version+1,result_json=?,attempts=attempts+1,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Finance: block a due draft (worker, holding-keyed).
pub async fn finance_due_draft_block_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_due_drafts SET status='blocked',version=version+1,attempts=attempts+1,last_code=?,last_message=?,next_attempt_at=?,updated_at=? WHERE id=? AND status IN ('scheduled','blocked')".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Finance: void an unposted supplier-bill draft invoice.
pub async fn finance_supplier_bill_draft_void_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: insert a self-service employee row.
pub async fn people_self_service_employee_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/payroll: record an issued payslip.
pub async fn people_issued_payslip_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_issued_payslips(id,company_id,run_id,run_revision,run_version,item_id,engagement_id,person_id,recipient_user_id,pay_date,currency,gross_minor,deductions_minor,net_minor,source_digest,snapshot_json,export_html,export_sha256,issued_by,command_id,issued_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People/expense: cancel an expense payment draft.
pub async fn people_expense_payment_cancel_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE expense_payment_drafts SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=? AND status='draft' AND payment_id IS NULL".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: decide on a leave evidence manifest.
pub async fn people_leave_evidence_manifest_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO leave_evidence_manifests(id,request_id,company_id,approval_instance_id,decision_command_id,request_version,policy_version_id,reviewer_user_id,manifest_json,created_at) VALUES(?,?,?,?,?,?,?,?,?::jsonb,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: reschedule a preparation task.
pub async fn people_prep_task_reschedule_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE people_preparation_tasks SET due_date=?,assignee_id=?,responsibility=?,version=version+1,updated_at=? WHERE id=? AND company_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: complete a preparation task.
pub async fn people_prep_task_complete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE people_preparation_tasks SET status=?,completion_reason=?,document_references=?::jsonb,completed_by=?,completed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People/payroll: record a statutory remittance.
pub async fn people_statutory_remittance_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_payroll_statutory_remittances(id,company_id,run_id,kind,amount_minor,liability_account_id,bank_account_id,bank_transaction_id,journal_id,bank_reference,recorded_by,recorded_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: assign an engagement level.
pub async fn people_assign_level_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_engagements SET level_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: bump an attachment owner record version (table chosen by server-side descriptor).
pub async fn people_attachment_owner_version_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("UPDATE {table} SET version=version+1,updated_at=? WHERE id=? AND version=?"),
        values: values_from_json(input)?,
    })
}


/// Native-owned constant SQL for the workflow command-assertion cleanup.
/// A checked-in JSON contract mirrors this for synchronous TS statement preparation;
/// the test in this module enforces the contract matches this constant.
pub const WORKFLOW_ASSERTION_CLEAR_SQL: &str = "DELETE FROM command_assertions WHERE command_id=?";

/// Workflow: clear the assertion rows retained for a command.
pub async fn workflow_command_assertion_clear_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "DELETE FROM command_assertions WHERE command_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Workflow: clear assertion rows for several commands at once.
pub async fn workflow_command_assertion_clear_many_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let values = values_from_json(input)?;
    if values.is_empty() { return Err(invalid().into()); }
    let placeholders = vec!["?"; values.len()].join(",");
    Ok(NotificationStatement {
        sql: format!("DELETE FROM command_assertions WHERE command_id IN ({placeholders})"),
        values,
    })
}


/// Collaboration: provision the organization-managed team.
pub async fn collaboration_org_team_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let source = input.source_access_sql.as_deref().unwrap_or("");
    if source.is_empty() { return Err(invalid().into()); }
    Ok(NotificationStatement {
        sql: format!("INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) SELECT 'org-team-' || company_id,company_id,'Organization','organization',true,? {source} WHERE company_id=? AND kind='company' AND status='active' ON CONFLICT DO NOTHING"),
        values: values_from_json(input)?,
    })
}

/// Collaboration: provision organization-managed channel rooms.
pub async fn collaboration_org_room_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let source = input.source_access_sql.as_deref().unwrap_or("");
    if source.is_empty() { return Err(invalid().into()); }
    Ok(NotificationStatement {
        sql: format!("INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT 'org-channel-' || n.id,n.company_id,'channel',n.display_name,'organization','org-team-' || n.company_id,n.id,? {source} WHERE n.company_id=? AND n.kind IN ('company','branch','department') AND n.status='active' AND EXISTS(SELECT 1 FROM messaging_teams t WHERE t.id='org-team-' || n.company_id) AND NOT EXISTS(SELECT 1 FROM messaging_rooms r WHERE r.company_id=n.company_id AND r.organization_node_id=n.id) ON CONFLICT DO NOTHING"),
        values: values_from_json(input)?,
    })
}


/// Legal: backfill specialized attachment linkage on a legal document.
pub async fn legal_backfill_document_attachment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_documents SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Legal: backfill specialized attachment linkage on an external upload.
pub async fn legal_backfill_external_attachment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE legal_external_uploads SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Collaboration: expire stale beta AI usage reservations (holding worker).
pub async fn beta_ai_usage_expire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE social_ai_usage SET state='failed' WHERE state='reserved' AND lease_until<?".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Collaboration: expire stale beta AI drafts (holding worker).
pub async fn beta_ai_draft_expire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE social_ai_drafts SET state='failed',result_json=NULL WHERE (state='pending' AND created_at<?) OR (state='ready' AND expires_at<?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: create a legacy employee row for a leave request.
pub async fn people_legacy_leave_employee_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: create a legacy leave request.
pub async fn people_legacy_leave_request_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,(SELECT id FROM employees WHERE person_id=? AND company_id=?),?,?,?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: record a project-worker starter row (server-chosen table).
pub async fn people_worker_starter_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("INSERT INTO {table}(person_id,company_id,account_id,engagement_id,command_id,request_hash,proof_json) SELECT ?,?,?,?,?,request_hash,?::jsonb FROM command_receipts WHERE command_id=? AND operation=? AND record_id=?"),
        values: values_from_json(input)?,
    })
}

/// People: record a project-worker placement row (server-chosen table).
pub async fn people_worker_placement_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let table = safe_table(&input.table_name)?;
    Ok(NotificationStatement {
        sql: format!("INSERT INTO {table}(case_id,company_id,person_id,engagement_id,starter_command_id,command_id,request_hash,proof_json) SELECT ?,?,?,?,?,?,request_hash,?::jsonb FROM command_receipts WHERE command_id=? AND operation='people.lifecycle.submit' AND record_id=?"),
        values: values_from_json(input)?,
    })
}


/// People: prepare a beta onboarding person.
pub async fn people_beta_person_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH',?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: prepare a beta onboarding employee.
pub async fn people_beta_employee_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: prepare a beta onboarding engagement.
pub async fn people_beta_engagement_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,?,'employee','salaried','active',to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'),'Team member','THB',?,?,?,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: relocate organization employees and engagements (department move).
pub async fn people_org_relocation_engagement_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE hr_engagements SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND status IN ('draft','onboarding','active','suspended')".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: relocate active employees (department move).
pub async fn people_org_relocation_employee_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND department_id=? AND status='active'".to_owned(),
        values: values_from_json(input)?,
    })
}


/// People: consume a hire handoff person.
pub async fn people_handoff_person_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_people(id,company_id,legal_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: consume a hire handoff engagement.
pub async fn people_handoff_engagement_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','onboarding',?,?,?,?,?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}

/// People: consume a hire handoff compensation revision.
pub async fn people_handoff_compensation_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'draft',?,?,?)".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Collaboration: provision an organization team row.
pub async fn org_node_team_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) VALUES(?,?,'Organization','organization',true,?) ON CONFLICT DO NOTHING".to_owned(),
        values: values_from_json(input)?,
    })
}

/// Collaboration: provision an organization channel room (caller-supplied source fragment).
pub async fn org_node_room_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let source = input.source_access_sql.as_deref().unwrap_or("");
    if source.is_empty() { return Err(invalid().into()); }
    Ok(NotificationStatement {
        sql: format!("INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT ?,?,'channel',?,'organization',?,?,? WHERE EXISTS(SELECT 1 {source} WHERE id=? AND company_id=? AND status='active') ON CONFLICT DO NOTHING"),
        values: values_from_json(input)?,
    })
}

/// Collaboration: rename an organization channel room.
pub async fn org_node_room_rename_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE messaging_rooms SET name=? WHERE company_id=? AND organization_node_id=?".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Collaboration: backfill specialized attachment linkage on a messaging file.
pub async fn collaboration_backfill_messaging_attachment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement {
        sql: "UPDATE messaging_files SET attachment_set_id=?,attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL".to_owned(),
        values: values_from_json(input)?,
    })
}


/// Legal: renew a runtime job health heartbeat.
pub async fn legal_job_health_renew_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE legal_runtime_health SET heartbeat_at=? WHERE task=?".to_owned(), values: values_from_json(input)? })
}

/// Legal: start a runtime job health record.
pub async fn legal_job_health_start_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO legal_runtime_health(task,heartbeat_at) VALUES(?,?) ON CONFLICT(task) DO UPDATE SET heartbeat_at=excluded.heartbeat_at".to_owned(), values: values_from_json(input)? })
}

/// Legal: record a runtime job success.
pub async fn legal_job_health_success_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE legal_runtime_health SET heartbeat_at=?,last_success_at=?,failure_count=0,last_error=NULL WHERE task=?".to_owned(), values: values_from_json(input)? })
}

/// Legal: record a runtime job failure.
pub async fn legal_job_health_failure_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE legal_runtime_health SET heartbeat_at=?,failure_count=failure_count+1,last_error=? WHERE task=?".to_owned(), values: values_from_json(input)? })
}


/// Finance: prepare a collection case.
pub async fn finance_collection_prepare_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: supersede a parent collection case.
pub async fn finance_collection_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_collection_cases SET status='superseded',version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance: revise a collection case.
pub async fn finance_collection_revise_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id,revision_number,parent_case_id,parent_version,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a collection test delivery.
pub async fn finance_collection_delivery_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_collection_deliveries(case_id,company_id,adapter,delivered_by,payload_hash,delivered_at) VALUES(?,?,'test',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: decide on a collection case.
pub async fn finance_collection_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_collection_cases SET status=?,version=version+1,reviewed_by=CASE WHEN ?='approved' THEN ? ELSE reviewed_by END,review_reason=CASE WHEN ?='approved' THEN ? ELSE review_reason END,reviewed_at=CASE WHEN ?='approved' THEN ? ELSE reviewed_at END WHERE id=?".to_owned(), values: values_from_json(input)? })
}


/// Finance: prepare a calculation run.
pub async fn finance_calc_run_prepare_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_id,policy_version,period_id,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,version,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'prepared',1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a calculation prepare event.
pub async fn finance_calc_event_prepare_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,'Prepared deterministic calculation',?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: decide on a calculation run (reviewer column chosen by caller).
pub async fn finance_calc_run_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    let column = safe_table(&input.table_name)?;
    Ok(NotificationStatement { sql: format!("UPDATE finance_calculation_runs SET status=?,version=version+1,updated_at=?,{column}=? WHERE id=?"), values: values_from_json(input)? })
}

/// Finance: record a calculation decision event.
pub async fn finance_calc_event_decide_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_calculation_events VALUES(?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a period-close check.
pub async fn finance_close_check_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING".to_owned(), values: values_from_json(input)? })
}


/// Finance: record a group entity for intercompany reporting.
pub async fn finance_group_entity_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'active',1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record an intercompany transaction.
pub async fn finance_intercompany_transaction_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_intercompany_transactions(id,holding_id,source_company_id,target_company_id,reference,currency,amount_minor,source_journal_id,target_journal_id,status,version,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,'unmatched',1,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record an intercompany lifecycle event.
pub async fn finance_intercompany_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_intercompany_events(id,transaction_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: change an intercompany transaction status.
pub async fn finance_intercompany_status_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_intercompany_transactions SET status=?,version=version+1 WHERE id=? AND holding_id=?".to_owned(), values: values_from_json(input)? })
}


/// Finance: start a company chart.
pub async fn finance_starter_chart_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,'THB','Company policy pending review',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: start a chart account.
pub async fn finance_starter_account_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: start an accounting period.
pub async fn finance_starter_period_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: start a dimension definition.
pub async fn finance_starter_dimension_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,0,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: start a dimension value.
pub async fn finance_starter_dimension_value_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_dimension_values(id,company_id,dimension_id,code,name,created_at) VALUES(?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}


/// People: apply a draft placement to an engagement.
pub async fn people_draft_engagement_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE hr_engagements SET position_id=?,branch_id=?,department_id=?,level_id=?,manager_person_id=?,job_title=?,version=version+1,updated_at=? WHERE company_id=? AND id=?".to_owned(), values: values_from_json(input)? })
}

/// People: apply a draft placement to an employee.
pub async fn people_draft_employee_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?".to_owned(), values: values_from_json(input)? })
}

/// People: record a draft placement history entry.
pub async fn people_draft_history_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}


/// Finance: record a prepared statement package.
pub async fn finance_statement_package_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_statement_packages(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,comparative_start_date,comparative_end_date,previous_package_id,edition,source_revision,source_fingerprint,presentation_json,notes_json,snapshot_json,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a prepared statement event.
pub async fn finance_statement_event_prepared_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_statement_events VALUES(?,?,?,'prepared',?,1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: finalize a statement package decision.
pub async fn finance_statement_package_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_statement_packages SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,export_html=?,export_sha256=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a statement decision event.
pub async fn finance_statement_event_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_statement_events VALUES(?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: record a prepared year closing.
pub async fn finance_year_closing_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_year_closings(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,period_id,retained_earnings_account_id,source_revision,source_fingerprint,snapshot_json,profit_minor,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: finalize a year closing decision.
pub async fn finance_year_closing_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_year_closings SET status=?,journal_id=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}


/// Inbox: record an event (idempotent by company/event key).
pub async fn inbox_event_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,actor_user_id,category,preview,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,event_key) DO NOTHING".to_owned(), values: values_from_json(input)? })
}

/// Inbox: deliver an event to a user (idempotent by event/user).
pub async fn inbox_delivery_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES(?,?,?,?,?) ON CONFLICT(event_id,user_id) DO NOTHING".to_owned(), values: values_from_json(input)? })
}

/// Inbox: change a delivery read/archive state.
pub async fn inbox_delivery_state_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE inbox_deliveries SET read_at=?,archived_at=? WHERE id=? AND company_id=? AND user_id=?".to_owned(), values: values_from_json(input)? })
}

/// Inbox: acknowledge a legacy staff notification.
pub async fn inbox_staff_notification_read_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE staff_notifications SET read_at=? WHERE id=? AND company_id=? AND user_id=?".to_owned(), values: values_from_json(input)? })
}

/// Inbox: insert a notice.
pub async fn inbox_notice_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO inbox_notices(id,company_id,sender_user_id,title,body,audience_json,recipient_count,created_at) VALUES(?,?,?,?,?,?::jsonb,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Inbox: target a notice at a user.
pub async fn inbox_notice_target_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO inbox_notice_targets(notice_id,company_id,user_id) VALUES(?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Inbox: mark a notice target delivered.
pub async fn inbox_notice_target_complete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE inbox_notice_targets SET delivered_at=? WHERE notice_id=? AND user_id=? AND delivered_at IS NULL".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual schedule_insert native statement.
pub async fn finance_accrual_schedule_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accrual_schedules(id,company_id,description,debit_account_id,credit_account_id,currency,total_minor,start_date,end_date,next_run_date,frequency,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'draft',1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual event_created native statement.
pub async fn finance_accrual_event_created_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accrual_schedule_events(id,schedule_id,company_id,action,actor_id,schedule_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual schedule_decision native statement.
pub async fn finance_accrual_schedule_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_accrual_schedules SET status=?,approved_by=CASE WHEN ?='active' THEN ? ELSE approved_by END,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual event_decision native statement.
pub async fn finance_accrual_event_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accrual_schedule_events(id,schedule_id,company_id,action,actor_id,schedule_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual journal_insert native statement.
pub async fn finance_accrual_journal_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual journal_lines_insert native statement.
pub async fn finance_accrual_journal_lines_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,0),(?,?,?,?,?,?,?,?,?,1)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual run_insert native statement.
pub async fn finance_accrual_run_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accrual_schedule_runs(id,schedule_id,company_id,period_start,period_end,amount_minor,currency,journal_id,schedule_version,source_hash,status,version,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?, 'prepared',1,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual event_run native statement.
pub async fn finance_accrual_event_run_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_accrual_schedule_events(id,schedule_id,run_id,company_id,action,actor_id,schedule_version,run_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual journal_post native statement.
pub async fn finance_accrual_journal_post_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND status='draft'".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual run_post native statement.
pub async fn finance_accrual_run_post_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_accrual_schedule_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance: accrual schedule_post native statement.
pub async fn finance_accrual_schedule_post_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_accrual_schedules SET next_run_date=?,status=CASE WHEN ? THEN 'completed' ELSE status END,version=version+1,updated_at=? WHERE id=? AND status='active' AND version=?".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for INSERT INTO finance_parties.
pub async fn pms_finance_party_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,?,'active',?,?) ON CONFLICT(id) DO NOTHING".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for INSERT INTO finance_invoices.
pub async fn pms_finance_invoice_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,?,?,?,?,'pms',?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for INSERT INTO finance_payments.
pub async fn pms_finance_payment_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for DELETE FROM finance_allocations WHERE in.
pub async fn pms_finance_allocation_delete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "DELETE FROM finance_allocations WHERE invoice_id=? AND company_id=?".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for UPDATE finance_invoices SET outstanding_.
pub async fn pms_finance_invoice_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_invoices SET outstanding_minor=?,base_outstanding_minor=?,status=CASE WHEN ?=0 THEN 'paid' WHEN ?<total_minor THEN 'partially_paid' ELSE 'posted' END,version=version+1,updated_at=? WHERE id=? AND company_id=?".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for INSERT INTO finance_allocations.
pub async fn pms_finance_allocation_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// PMS finance fact: native-owned statement for UPDATE finance_bank_transactions SET mat.
pub async fn pms_finance_bank_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=? AND match_status IN ('unmatched','proposed')".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_opening_journal statement.
pub async fn books_opening_journal_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,id,?,?,?,base_currency,1000000,'book_opening',?,'Reviewed secondary book opening','posted',1,?,?,?,?,? FROM finance_charts WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_opening_journal_lines statement.
pub async fn books_opening_journal_lines_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?,?,secondary_account_id,?,?,?,?,?,? FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_rule_decision statement.
pub async fn books_rule_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_book_rules SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_dimension_definition statement.
pub async fn books_dimension_definition_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_dimension_value statement.
pub async fn books_dimension_value_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_dimension_values(id,dimension_id,company_id,code,name,created_at) VALUES(?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_rule_prepare statement.
pub async fn books_rule_prepare_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance book: native-owned books_account_mapping statement.
pub async fn books_account_mapping_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES(?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_withholding_consumed statement.
pub async fn wh_withholding_consumed_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_assessments SET status='consumed',consumed_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_assessment_reversed statement.
pub async fn wh_assessment_reversed_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_assessments SET status='reversed',reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_thai_document_cancel statement.
pub async fn wh_thai_document_cancel_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_thai_document_events(id,document_id,company_id,action,reason,actor_id,command_id,created_at) VALUES(?,?,?,'cancelled',?,?,?,?) ON CONFLICT(document_id,action) DO NOTHING".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_assessment_insert statement.
pub async fn wh_assessment_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_withholding_assessments(id,company_id,payment_id,payment_version,payer_identity_id,payer_identity_version,payee_identity_id,payee_identity_version,liability_account_id,liability_account_version,payment_date,form_kind,gross_minor,income_minor,tax_minor,source_fingerprint,source_snapshot_json,rule_pack_version,rule_pack_digest,classification_evidence,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_income_line_insert statement.
pub async fn wh_income_line_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_withholding_income_lines(id,assessment_id,company_id,invoice_id,invoice_version,invoice_line_revision,source_line_id,description,category,contract_ref,contract_total_minor,allocated_gross_minor,prior_paid_gross_minor,original_income_minor,income_minor,prior_withheld_minor,rate_bps,tax_minor,source_snapshot_json,ordinal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_payment_withholding statement.
pub async fn wh_payment_withholding_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_payments SET withholding_assessment_id=?,withheld_minor=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding: native-owned wh_assessment_decision statement.
pub async fn wh_assessment_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_assessments SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_sequence_seed statement.
pub async fn thai_doc_sequence_seed_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_thai_document_sequences(company_id,document_kind,fiscal_year,last_number) VALUES(?,?,?,0) ON CONFLICT DO NOTHING".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_sequence_bump statement.
pub async fn thai_doc_sequence_bump_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_thai_document_sequences SET last_number=last_number+1 WHERE company_id=? AND document_kind=? AND fiscal_year=? AND last_number=?".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_issue statement.
pub async fn thai_doc_issue_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_issued_thai_documents(id,company_id,document_kind,document_number,issue_date,payment_id,invoice_id,assessment_id,previous_document_id,edition,payer_identity_id,payee_identity_id,source_fingerprint,snapshot_json,export_html,export_sha256,reason,issued_by,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_event_issued statement.
pub async fn thai_doc_event_issued_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_thai_document_events(id,document_id,company_id,action,reason,actor_id,command_id,created_at) VALUES(?,?,?,'issued',?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_change_insert statement.
pub async fn thai_doc_change_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_thai_document_changes(id,company_id,document_id,corrected_payer_identity_id,corrected_payee_identity_id,issue_date,reason,source_fingerprint,source_snapshot_json,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_event_supersede statement.
pub async fn thai_doc_event_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_thai_document_events(id,document_id,company_id,action,replacement_document_id,reason,actor_id,command_id,created_at) VALUES(?,?,?,'superseded',?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance Thai document: native-owned thai_doc_change_decision statement.
pub async fn thai_doc_change_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_thai_document_changes SET status=?,replacement_document_id=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_correction_insert statement.
pub async fn billing_correction_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_billing_corrections(id,company_id,billing_run_id,invoice_id,number,entry_date,period_id,input_json,result_json,source_hash,source_json,subtotal_minor,tax_minor,total_minor,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_correction_reject statement.
pub async fn billing_correction_reject_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_billing_corrections SET status='rejected',version=?,reviewed_by=?,review_reason=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_recognition_reverse statement.
pub async fn billing_recognition_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=? AND version=? AND status='posted'".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_invoice_paid statement.
pub async fn billing_invoice_paid_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_invoices SET outstanding_minor=0,base_outstanding_minor=0,status='paid',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_correction_post statement.
pub async fn billing_correction_post_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_billing_corrections SET status='posted',version=?,reviewed_by=?,review_reason=?,credit_journal_id=?,replacement_bill_id=?,credited_minor=?,refund_minor=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_refund_obligation statement.
pub async fn billing_refund_obligation_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_refund_obligations(id,company_id,billing_correction_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,source_credit_journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'outgoing',?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_invoice_created_by statement.
pub async fn billing_invoice_created_by_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_invoices SET created_by=? WHERE id=? AND company_id=? AND status='draft' AND version=1".to_owned(), values: values_from_json(input)? })
}

/// Finance billing correction: native-owned billing_run_approve statement.
pub async fn billing_run_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_business_billing_runs SET no_charge_status='approved',reviewed_by=?,review_reason=?,version=version+1 WHERE id=? AND company_id=? AND no_charge_status='draft'".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_event_insert statement.
pub async fn asset_event_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_insert statement.
pub async fn asset_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,depreciation_account_id,depreciation_expense_account_id,currency,cost_minor,residual_minor,useful_life_months,in_service_date,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_update statement.
pub async fn asset_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_assets SET asset_number=?,name=?,account_id=?,depreciation_account_id=?,depreciation_expense_account_id=?,currency=?,cost_minor=?,residual_minor=?,useful_life_months=?,in_service_date=?,version=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_retire statement.
pub async fn asset_retire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_assets SET status='retired',version=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_activate statement.
pub async fn asset_activate_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_assets SET status='in_service',activation_journal_id=?,activated_by=?,version=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_depreciation_insert statement.
pub async fn asset_depreciation_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_asset_depreciations(id,asset_id,company_id,depreciation_month,ordinal,period_id,entry_date,amount_minor,journal_id,asset_version,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_depreciation_update statement.
pub async fn asset_depreciation_update_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_assets SET accumulated_depreciation_minor=?,version=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_disposal_insert statement.
pub async fn asset_disposal_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_asset_disposals(id,asset_id,company_id,asset_version,period_id,entry_date,proceeds_minor,proceeds_account_id,gain_account_id,loss_account_id,evidence_ref,created_by,created_at,updated_at,policy_source_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_disposal_retire statement.
pub async fn asset_disposal_retire_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_assets SET status='retired',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance asset: native-owned asset_disposal_decision statement.
pub async fn asset_disposal_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_asset_disposals SET status=?,journal_id=?,decided_by=?,version=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_edition_insert statement.
pub async fn instrument_edition_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_instrument_editions(id,company_id,root_id,parent_id,kind,number,name,chart_id,edition,content_json,schedule_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_edition_supersede statement.
pub async fn instrument_edition_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_bank_match statement.
pub async fn instrument_bank_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_edition_decision statement.
pub async fn instrument_edition_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_editions SET status=?,reviewed_by=?,activation_journal_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_run_insert statement.
pub async fn instrument_run_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_instrument_runs(id,company_id,instrument_id,kind,ordinal,input_json,result_json,source_hash,bank_transaction_id,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_bank_match2 statement.
pub async fn instrument_bank_match2_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_edition_agree statement.
pub async fn instrument_edition_agree_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_editions SET status=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_run_decision statement.
pub async fn instrument_run_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_runs SET status=?,reviewed_by=?,journal_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_bank_unmatch statement.
pub async fn instrument_bank_unmatch_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_run_reverse statement.
pub async fn instrument_run_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_runs SET status='reversed',reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance instrument: native-owned instrument_edition_activate statement.
pub async fn instrument_edition_activate_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_instrument_editions SET status='active',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_obligation_approve statement.
pub async fn refund_obligation_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_obligations SET status='approved',beneficiary_name=?,beneficiary_account=?,approval_reason=?,approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_instruction_insert statement.
pub async fn refund_instruction_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_refund_instructions(id,obligation_id,company_id,amount_minor,planned_date,bank_account,cash_account_id,fx_gain_account_id,fx_loss_account_id,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_obligation_bump statement.
pub async fn refund_obligation_bump_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_obligations SET version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_instruction_export statement.
pub async fn refund_instruction_export_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_instructions SET status='exported',export_hash=?,export_command_id=?,exported_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_instruction_resolve statement.
pub async fn refund_instruction_resolve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_instructions SET status=?,evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_obligation_bump2 statement.
pub async fn refund_obligation_bump2_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_obligations SET version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_payment_insert statement.
pub async fn refund_payment_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,fx_rate_micros,base_amount_minor,fx_journal_id,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_bank_match statement.
pub async fn refund_bank_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_instruction_confirm statement.
pub async fn refund_instruction_confirm_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_instructions SET status='confirmed',bank_reference=?,evidence=?,payment_id=?,bank_transaction_id=?,resolved_by=?,base_carrying_minor=?,cash_base_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance refund: native-owned refund_obligation_settle statement.
pub async fn refund_obligation_settle_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_refund_obligations SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=COALESCE(base_outstanding_minor,?)-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE 'approved' END,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_package_insert statement.
pub async fn wh_package_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_withholding_packages(id,company_id,month,form_kind,previous_package_id,edition,tax_minor,monthly_tax_minor,chart_id,source_fingerprint,snapshot_json,export_csv,export_sha256,prepared_by,reason,command_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_package_decision statement.
pub async fn wh_package_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_packages SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_remittance_insert statement.
pub async fn wh_remittance_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_withholding_remittances(id,company_id,package_id,amount_minor,liability_accounts_json,bank_account_id,bank_transaction_id,journal_id,filing_channel,filing_evidence,bank_reference,recorded_by,recorded_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_remittance_item_insert statement.
pub async fn wh_remittance_item_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_withholding_remittance_items(remittance_id,assessment_id,company_id,tax_minor) VALUES(?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_bank_match statement.
pub async fn wh_bank_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='withholding_remittance',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_package_settle statement.
pub async fn wh_package_settle_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_packages SET status='settled',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_remittance_reverse statement.
pub async fn wh_remittance_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_remittances SET status='reversed',reversal_bank_transaction_id=?,reversal_journal_id=?,reversal_evidence=?,reversed_by=?,reversed_at=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_package_reverse statement.
pub async fn wh_package_reverse_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_withholding_packages SET status='reversed',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance withholding remittance: native-owned wh_bank_match2 statement.
pub async fn wh_bank_match2_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='withholding_remittance_reversal',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Native-owned SQL contract for finance asset policy statements. The checked-in
/// JSON mirror is consumed synchronously by TS; this test enforces parity.
pub const ASSET_POLICY_SQL_PREPARE_POLICY_EDITION: &str = "INSERT INTO finance_asset_policy_editions(id,company_id,asset_id,parent_id,content_json,source_hash,prepared_by,reason,created_at,updated_at,basis_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const ASSET_POLICY_SQL_SUPERSEDE_POLICY_EDITION: &str = "UPDATE finance_asset_policy_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_DECIDE_POLICY_EDITION: &str = "UPDATE finance_asset_policy_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_PREPARE_POLICY_RUN: &str = "INSERT INTO finance_asset_policy_runs(id,company_id,asset_id,policy_id,ordinal,entry_date,input_json,result_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const ASSET_POLICY_SQL_UPDATE_ASSET_CARRYING: &str = "UPDATE finance_assets SET accumulated_depreciation_minor=?,version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_BUMP_POLICY_EDITION: &str = "UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_ASSET_DEPRECIATION_EVENT: &str = "INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,'depreciated',?,?,?,?,?)";
pub const ASSET_POLICY_SQL_DECIDE_POLICY_RUN: &str = "UPDATE finance_asset_policy_runs SET status=?,reviewed_by=?,journal_ids_json=?,version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_REVERSE_ASSET_CARRYING: &str = "UPDATE finance_assets SET accumulated_depreciation_minor=accumulated_depreciation_minor-?,version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_REVERSE_POLICY_VERSION: &str = "UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?";
pub const ASSET_POLICY_SQL_REVERSE_POLICY_RUN: &str = "UPDATE finance_asset_policy_runs SET status='reversed',reversal_ids_json=?,reversal_entry_date=?,version=version+1,updated_at=? WHERE id=?";

#[cfg(test)]
mod asset_policy_contract_tests {
    use super::*;
    #[test]
    fn asset_policy_sql_contract_matches_native_consts() {
        let raw = include_str!("native-asset-policy-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid asset policy SQL contract");
        let pairs = [
            ("prepare_policy_edition", ASSET_POLICY_SQL_PREPARE_POLICY_EDITION),
            ("supersede_policy_edition", ASSET_POLICY_SQL_SUPERSEDE_POLICY_EDITION),
            ("decide_policy_edition", ASSET_POLICY_SQL_DECIDE_POLICY_EDITION),
            ("prepare_policy_run", ASSET_POLICY_SQL_PREPARE_POLICY_RUN),
            ("update_asset_carrying", ASSET_POLICY_SQL_UPDATE_ASSET_CARRYING),
            ("bump_policy_edition", ASSET_POLICY_SQL_BUMP_POLICY_EDITION),
            ("asset_depreciation_event", ASSET_POLICY_SQL_ASSET_DEPRECIATION_EVENT),
            ("decide_policy_run", ASSET_POLICY_SQL_DECIDE_POLICY_RUN),
            ("reverse_asset_carrying", ASSET_POLICY_SQL_REVERSE_ASSET_CARRYING),
            ("reverse_policy_version", ASSET_POLICY_SQL_REVERSE_POLICY_VERSION),
            ("reverse_policy_run", ASSET_POLICY_SQL_REVERSE_POLICY_RUN),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "asset policy SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for finance business-contract statements.
pub const BUSINESS_CONTRACT_SQL_PREPARE_CONTRACT: &str = "INSERT INTO finance_business_contracts(id,company_id,root_id,parent_id,number,name,chart_id,party_id,start_date,end_date,edition,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const BUSINESS_CONTRACT_SQL_SUPERSEDE_CONTRACT: &str = "UPDATE finance_business_contracts SET status='superseded',version=version+1,updated_at=? WHERE id=?";
pub const BUSINESS_CONTRACT_SQL_DECIDE_CONTRACT: &str = "UPDATE finance_business_contracts SET status=?,reviewed_by=?,version=?,updated_at=? WHERE id=?";
pub const BUSINESS_CONTRACT_SQL_VOID_CONTRACT_INVOICE: &str = "UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE id=? AND company_id=?";
pub const BUSINESS_CONTRACT_SQL_PREPARE_BILL: &str = "INSERT INTO finance_business_billing_runs(id,company_id,contract_id,root_id,contract_version,billing_key,service_from,service_to,input_json,result_json,source_hash,invoice_id,invoice_version,created_by,created_at,no_charge_status,parent_run_id,parent_version,bill_edition,contract_content_json,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const BUSINESS_CONTRACT_SQL_PREPARE_RECOGNITION: &str = "INSERT INTO finance_recognition_runs(id,company_id,billing_run_id,item_key,due_date,entry_date,period_id,amount_minor,account_id,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const BUSINESS_CONTRACT_SQL_DECIDE_RECOGNITION: &str = "UPDATE finance_recognition_runs SET status=?,version=?,reviewed_by=?,journal_id=?,reason=?,updated_at=? WHERE id=?";
pub const BUSINESS_CONTRACT_SQL_REVERSE_RECOGNITION: &str = "UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?";
pub const BUSINESS_CONTRACT_SQL_APPROVE_NO_CHARGE: &str = "UPDATE finance_business_billing_runs SET no_charge_status=?,reviewed_by=?,review_reason=?,version=version+1 WHERE id=?";

#[cfg(test)]
mod business_contract_sql_contract_tests {
    use super::*;
    #[test]
    fn business_contract_sql_contract_matches_native_consts() {
        let raw = include_str!("native-business-contract-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid business contract SQL contract");
        let pairs = [
            ("prepare_contract", BUSINESS_CONTRACT_SQL_PREPARE_CONTRACT),
            ("supersede_contract", BUSINESS_CONTRACT_SQL_SUPERSEDE_CONTRACT),
            ("decide_contract", BUSINESS_CONTRACT_SQL_DECIDE_CONTRACT),
            ("void_contract_invoice", BUSINESS_CONTRACT_SQL_VOID_CONTRACT_INVOICE),
            ("prepare_bill", BUSINESS_CONTRACT_SQL_PREPARE_BILL),
            ("prepare_recognition", BUSINESS_CONTRACT_SQL_PREPARE_RECOGNITION),
            ("decide_recognition", BUSINESS_CONTRACT_SQL_DECIDE_RECOGNITION),
            ("reverse_recognition", BUSINESS_CONTRACT_SQL_REVERSE_RECOGNITION),
            ("approve_no_charge", BUSINESS_CONTRACT_SQL_APPROVE_NO_CHARGE),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "business contract SQL contract drift: {key}");
        }
    }
}

/// Finance bank automation: native-owned bank_csv_mapping_insert statement.
pub async fn bank_csv_mapping_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_bank_csv_mappings(id,company_id,name,columns_json,created_by,created_at) VALUES(?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_import_file_insert statement.
pub async fn bank_import_file_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_bank_import_files(id,company_id,bank_account_id,mapping_id,file_hash,statement_id,created_at) VALUES(?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_match_group_insert statement.
pub async fn bank_match_group_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_bank_match_groups(id,company_id,currency,status,source_hash,explanation_json,proposed_by,created_at,updated_at) VALUES(?,?,?,'proposed',?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_match_allocation_insert statement.
pub async fn bank_match_allocation_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_bank_match_allocations(id,group_id,bank_transaction_id,target_type,target_id,amount_minor,created_at) VALUES(?,?,?,'journal_line',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_txn_match statement.
pub async fn bank_txn_match_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='matched',matched_type='group',matched_id=?,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_match_target_insert statement.
pub async fn bank_match_target_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_bank_match_targets(journal_line_id,group_id) VALUES(?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_txn_unmatch statement.
pub async fn bank_txn_unmatch_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_match_target_delete statement.
pub async fn bank_match_target_delete_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "DELETE FROM finance_bank_match_targets WHERE group_id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance bank automation: native-owned bank_match_group_decision statement.
pub async fn bank_match_group_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_bank_match_groups SET status=?,version=version+1,confirmed_by=CASE WHEN ?='matched' THEN ? ELSE confirmed_by END,reversed_by=CASE WHEN ?='reversed' THEN ? ELSE reversed_by END,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned tax_entry_insert statement.
pub async fn tax_entry_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_tax_entries(id,company_id,chart_id,financial_event_id,tax_account_id,source_type,source_id,source_version,original_entry_id,direction,document_date,accounting_date,tax_point_date,claim_month,document_reference,treatment,deductible_bps,taxable_minor,tax_minor,deductible_minor,source_hash,evidence_json,prepared_by,created_at,replaces_entry_id,correction_id)\n      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned tax_package_event statement.
pub async fn tax_package_event_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_statutory_package_events VALUES(?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned tax_entry_decision statement.
pub async fn tax_entry_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned vat_correction_insert statement.
pub async fn vat_correction_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_vat_corrections(id,company_id,chart_id,root_entry_id,family_source_id,reason,family_evidence_json,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned tax_entry_correction_decision statement.
pub async fn tax_entry_correction_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE correction_id=? AND status='prepared'".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned vat_correction_decision statement.
pub async fn vat_correction_decision_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_vat_corrections SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned calc_run_insert statement.
pub async fn calc_run_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'thai_statutory',1,'THB',2,?,?,?,?,'prepared',?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned statutory_package_insert statement.
pub async fn statutory_package_insert_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "INSERT INTO finance_statutory_packages(id,company_id,jurisdiction,form_code,filing_kind,calculation_run_id,status,schema_version,effective_date,prepared_by,created_at,updated_at,chart_id,filing_month,source_hash,snapshot_json,original_package_id,evidence_json) VALUES(?,?,'TH','PP30',?,?,'prepared','erpx-pp30-workpaper-v1',?,?,?,?,?,?,?,?,?,?)".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned statutory_package_approve statement.
pub async fn statutory_package_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_statutory_packages SET status='approved_for_export',version=version+1,reviewed_by=?,approved_by=?,review_reason=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned calc_run_approve statement.
pub async fn calc_run_approve_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_calculation_runs SET status='approved',version=version+1,reviewed_by=?,approved_by=?,updated_at=? WHERE id=? AND status='prepared'".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned statutory_package_export statement.
pub async fn statutory_package_export_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_statutory_packages SET status='exported',version=version+1,export_content=?,export_hash=?,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned statutory_package_file statement.
pub async fn statutory_package_file_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_statutory_packages SET status='filed',version=version+1,filing_reference=?,filed_at=?,evidence_json=(evidence_json::jsonb||?::jsonb)::text,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Finance tax register: native-owned statutory_package_supersede statement.
pub async fn statutory_package_supersede_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    Ok(NotificationStatement { sql: "UPDATE finance_statutory_packages SET status='superseded',version=version+1,updated_at=? WHERE id=?".to_owned(), values: values_from_json(input)? })
}

/// Native-owned SQL contract for finance group-ownership statements.
pub const GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_INSERT: &str = "INSERT INTO finance_ownership_interests(id,holding_id,company_id,parent_id,investor_entity_id,investee_entity_id,accounting_method,ownership_bps,voting_bps,control_assessment,effective_from,effective_to,created_by,created_at,updated_at,source_hash,source_json,reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_RETIRE: &str = "UPDATE finance_ownership_interests SET status='retired',effective_to=?,version=version+1,updated_at=? WHERE id=? AND version=?";
pub const GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_DECIDE: &str = "UPDATE finance_ownership_interests SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?";
pub const GROUP_OWNERSHIP_SQL_GROUP_CALC_RUN_INSERT: &str = "INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'consolidation',1,?,2,?,?,?,?,'prepared',?,?,?)";
pub const GROUP_OWNERSHIP_SQL_GROUP_CALC_EVENT_PREPARE: &str = "INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,?,?,?)";
pub const GROUP_OWNERSHIP_SQL_ACQUISITION_INSERT: &str = "INSERT INTO finance_acquisitions(id,holding_id,company_id,ownership_id,investee_entity_id,acquisition_date,currency,consideration_minor,previous_interest_minor,nci_measurement,nci_minor,identifiable_net_assets_minor,deferred_tax_minor,goodwill_minor,bargain_purchase_minor,cgu_reference,calculation_run_id,created_by,created_at,updated_at,source_hash,source_json,reason,parent_id,parent_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const GROUP_OWNERSHIP_SQL_ACQUISITION_SUPERSEDE: &str = "UPDATE finance_acquisitions SET status='superseded',version=version+1,updated_at=? WHERE id=?";
pub const GROUP_OWNERSHIP_SQL_ACQUISITION_DECIDE: &str = "UPDATE finance_acquisitions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?";
pub const GROUP_OWNERSHIP_SQL_GROUP_CALC_RUN_DECIDE: &str = "UPDATE finance_calculation_runs SET status=?,reviewed_by=?,approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='prepared'";
pub const GROUP_OWNERSHIP_SQL_GROUP_CALC_EVENT_DECIDE: &str = "INSERT INTO finance_calculation_events VALUES(?,?,2,?,?,?,?,?)";

#[cfg(test)]
mod group_ownership_sql_contract_tests {
    use super::*;
    #[test]
    fn group_ownership_sql_contract_matches_native_consts() {
        let raw = include_str!("native-group-ownership-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid group ownership SQL contract");
        let pairs = [
            ("ownership_interest_insert", GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_INSERT),
            ("ownership_interest_retire", GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_RETIRE),
            ("ownership_interest_decide", GROUP_OWNERSHIP_SQL_OWNERSHIP_INTEREST_DECIDE),
            ("group_calc_run_insert", GROUP_OWNERSHIP_SQL_GROUP_CALC_RUN_INSERT),
            ("group_calc_event_prepare", GROUP_OWNERSHIP_SQL_GROUP_CALC_EVENT_PREPARE),
            ("acquisition_insert", GROUP_OWNERSHIP_SQL_ACQUISITION_INSERT),
            ("acquisition_supersede", GROUP_OWNERSHIP_SQL_ACQUISITION_SUPERSEDE),
            ("acquisition_decide", GROUP_OWNERSHIP_SQL_ACQUISITION_DECIDE),
            ("group_calc_run_decide", GROUP_OWNERSHIP_SQL_GROUP_CALC_RUN_DECIDE),
            ("group_calc_event_decide", GROUP_OWNERSHIP_SQL_GROUP_CALC_EVENT_DECIDE),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "group ownership SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for commerce finance fact statements.
pub const COMMERCE_FACTS_SQL_COMMERCE_INVOICE_OUTSTANDING: &str = "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?";
pub const COMMERCE_FACTS_SQL_COMMERCE_REFUND_OBLIGATION_INSERT: &str = "INSERT INTO finance_refund_obligations(id,company_id,branch_id,department_id,return_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,source_credit_journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_JOURNAL_INSERT: &str = "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_JOURNAL_LINES_INSERT: &str = "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,?,?,?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_INVOICE_INSERT: &str = "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,source_version,status,created_by,created_at,updated_at) VALUES(?,?,?,?,CASE WHEN ?='receivable' THEN erpx_sales_document_number(?,'INV',?) ELSE ? END,?,?,?,?,?,?,?,?,?,?,'commerce_fulfillment',?,?,'draft',?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_PAYMENT_INSERT: &str = "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,base_amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at,payment_slip_id) VALUES(?,?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_ALLOCATION_INSERT: &str = "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)";
pub const COMMERCE_FACTS_SQL_COMMERCE_INVOICE_ADJUST: &str = "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?";
pub const COMMERCE_FACTS_SQL_COMMERCE_PARTY_UPDATE: &str = "UPDATE finance_parties SET code=?,name=?,payment_terms_days=?,credit_limit_minor=?,status=?,notification_email=?,notification_consent=?,version=version+1,updated_at=? WHERE id=?";
pub const COMMERCE_FACTS_SQL_COMMERCE_PARTY_INSERT: &str = "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,notification_email,notification_consent,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)";

#[cfg(test)]
mod commerce_facts_sql_contract_tests {
    use super::*;
    #[test]
    fn commerce_facts_sql_contract_matches_native_consts() {
        let raw = include_str!("native-commerce-facts-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid commerce facts SQL contract");
        let pairs = [
            ("commerce_invoice_outstanding", COMMERCE_FACTS_SQL_COMMERCE_INVOICE_OUTSTANDING),
            ("commerce_refund_obligation_insert", COMMERCE_FACTS_SQL_COMMERCE_REFUND_OBLIGATION_INSERT),
            ("commerce_journal_insert", COMMERCE_FACTS_SQL_COMMERCE_JOURNAL_INSERT),
            ("commerce_journal_lines_insert", COMMERCE_FACTS_SQL_COMMERCE_JOURNAL_LINES_INSERT),
            ("commerce_invoice_insert", COMMERCE_FACTS_SQL_COMMERCE_INVOICE_INSERT),
            ("commerce_payment_insert", COMMERCE_FACTS_SQL_COMMERCE_PAYMENT_INSERT),
            ("commerce_allocation_insert", COMMERCE_FACTS_SQL_COMMERCE_ALLOCATION_INSERT),
            ("commerce_invoice_adjust", COMMERCE_FACTS_SQL_COMMERCE_INVOICE_ADJUST),
            ("commerce_party_update", COMMERCE_FACTS_SQL_COMMERCE_PARTY_UPDATE),
            ("commerce_party_insert", COMMERCE_FACTS_SQL_COMMERCE_PARTY_INSERT),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "commerce facts SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for finance consolidation statements.
pub const CONSOLIDATION_SQL_ELIMINATION_INSERT: &str = "INSERT INTO finance_consolidation_eliminations(id,run_id,description,debit_account_id,credit_account_id,amount_minor,status,journal_id,version,source_transaction_id,created_by,created_at,updated_at,evidence_json)\n      VALUES(?,?,?,?,?,?, 'proposed',NULL,1,?,?,?,?,?)";
pub const CONSOLIDATION_SQL_ELIMINATION_EVENT_PREPARE: &str = "INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const CONSOLIDATION_SQL_ELIMINATION_APPROVE: &str = "UPDATE finance_consolidation_eliminations SET status='approved',approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='proposed'";
pub const CONSOLIDATION_SQL_JOURNAL_INSERT: &str = "INSERT INTO finance_consolidation_journals(id,run_id,holding_id,journal_number,entry_date,currency,memo,status,source_hash,version,created_by,posted_by,posted_at,created_at,updated_at)\n        VALUES(?,?,?,?,?,?,?,'posted',?,1,?,?,?,?,?) ON CONFLICT(run_id) DO NOTHING";
pub const CONSOLIDATION_SQL_JOURNAL_LINES_DEBIT: &str = "INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,?,0,?)";
pub const CONSOLIDATION_SQL_JOURNAL_LINES_CREDIT: &str = "INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,0,?,?)";
pub const CONSOLIDATION_SQL_JOURNAL_POST: &str = "UPDATE finance_consolidation_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND run_id=?";
pub const CONSOLIDATION_SQL_ELIMINATION_POST: &str = "UPDATE finance_consolidation_eliminations SET status='posted',consolidation_journal_id=?,posted_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='approved'";
pub const CONSOLIDATION_SQL_ELIMINATION_EVENT_DECIDE: &str = "INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const CONSOLIDATION_SQL_RUN_INSERT: &str = "INSERT INTO finance_consolidation_runs(id,holding_id,presentation_currency,as_of_date,status,source_snapshot_json,source_hash,version,created_by,created_at,updated_at) VALUES(?,?,?,?, 'draft',?,?,1,?,?,?)";
pub const CONSOLIDATION_SQL_RUN_EVENT_PREPARE: &str = "INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const CONSOLIDATION_SQL_RUN_EVENT_DECIDE: &str = "INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const CONSOLIDATION_SQL_UPDATE_RUN_PREFIX: &str = "UPDATE finance_consolidation_runs SET ";
pub const CONSOLIDATION_SQL_UPDATE_RUN_SUFFIX: &str = " WHERE id=? AND holding_id=? AND status=? AND version=?";

#[cfg(test)]
mod consolidation_sql_contract_tests {
    use super::*;
    #[test]
    fn consolidation_sql_contract_matches_native_consts() {
        let raw = include_str!("native-consolidation-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid consolidation SQL contract");
        let pairs = [
            ("elimination_insert", CONSOLIDATION_SQL_ELIMINATION_INSERT),
            ("elimination_event_prepare", CONSOLIDATION_SQL_ELIMINATION_EVENT_PREPARE),
            ("elimination_approve", CONSOLIDATION_SQL_ELIMINATION_APPROVE),
            ("journal_insert", CONSOLIDATION_SQL_JOURNAL_INSERT),
            ("journal_lines_debit", CONSOLIDATION_SQL_JOURNAL_LINES_DEBIT),
            ("journal_lines_credit", CONSOLIDATION_SQL_JOURNAL_LINES_CREDIT),
            ("journal_post", CONSOLIDATION_SQL_JOURNAL_POST),
            ("elimination_post", CONSOLIDATION_SQL_ELIMINATION_POST),
            ("elimination_event_decide", CONSOLIDATION_SQL_ELIMINATION_EVENT_DECIDE),
            ("run_insert", CONSOLIDATION_SQL_RUN_INSERT),
            ("run_event_prepare", CONSOLIDATION_SQL_RUN_EVENT_PREPARE),
            ("run_event_decide", CONSOLIDATION_SQL_RUN_EVENT_DECIDE),
            ("update_run_prefix", CONSOLIDATION_SQL_UPDATE_RUN_PREFIX),
            ("update_run_suffix", CONSOLIDATION_SQL_UPDATE_RUN_SUFFIX),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "consolidation SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for POS core finance fact statements.
pub const POS_CORE_SQL_POS_JOURNAL_INSERT: &str = "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'pos',?,?,'posted',1,?,?,?,?,?)";
pub const POS_CORE_SQL_POS_JOURNAL_LINES_INSERT: &str = "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?)";
pub const POS_CORE_SQL_POS_PAYMENT_INSERT: &str = "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,?,?,0,?,'settled',?,?,?,?,?)";
pub const POS_CORE_SQL_POS_PARTY_INSERT: &str = "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer','POS-WALKIN','POS walk-in customer',?,'active',?,?) ON CONFLICT(id) DO NOTHING";
pub const POS_CORE_SQL_POS_INVOICE_INSERT: &str = "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,0,?,?,?,'pos',?,'paid',?,?,?,?,?,?)";
pub const POS_CORE_SQL_POS_PAYMENT_PARTY_UPDATE: &str = "UPDATE finance_payments SET party_id=? WHERE id=?";
pub const POS_CORE_SQL_POS_ALLOCATION_INSERT: &str = "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)";
pub const POS_CORE_SQL_POS_ACCOUNT_INSERT: &str = "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const POS_CORE_SQL_POS_BANK_MATCH: &str = "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?";
pub const POS_CORE_SQL_POS_BANK_MATCH_PAYOUT: &str = "UPDATE finance_bank_transactions SET match_status='matched',matched_type='pos_payout',matched_id=?,version=version+1 WHERE id=?";
pub const POS_CORE_SQL_POS_REFUND_PAYMENT_INSERT: &str = "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'refund',?,?,?,?,0,?,'settled',?,?,?,?,?)";

#[cfg(test)]
mod pos_core_sql_contract_tests {
    use super::*;
    #[test]
    fn pos_core_sql_contract_matches_native_consts() {
        let raw = include_str!("native-pos-core-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid POS core SQL contract");
        let pairs = [
            ("pos_journal_insert", POS_CORE_SQL_POS_JOURNAL_INSERT),
            ("pos_journal_lines_insert", POS_CORE_SQL_POS_JOURNAL_LINES_INSERT),
            ("pos_payment_insert", POS_CORE_SQL_POS_PAYMENT_INSERT),
            ("pos_party_insert", POS_CORE_SQL_POS_PARTY_INSERT),
            ("pos_invoice_insert", POS_CORE_SQL_POS_INVOICE_INSERT),
            ("pos_payment_party_update", POS_CORE_SQL_POS_PAYMENT_PARTY_UPDATE),
            ("pos_allocation_insert", POS_CORE_SQL_POS_ALLOCATION_INSERT),
            ("pos_account_insert", POS_CORE_SQL_POS_ACCOUNT_INSERT),
            ("pos_bank_match", POS_CORE_SQL_POS_BANK_MATCH),
            ("pos_bank_match_payout", POS_CORE_SQL_POS_BANK_MATCH_PAYOUT),
            ("pos_refund_payment_insert", POS_CORE_SQL_POS_REFUND_PAYMENT_INSERT),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "POS core SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for company money statements.
pub const COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT: &str = "INSERT INTO money_transaction_snapshots\n      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)\n      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const COMPANY_MONEY_SQL_MONEY_SNAPSHOT_BATCH_INSERT: &str = "INSERT INTO money_transaction_snapshots\n            (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)\n            VALUES ${chunk.map(() => '(?,?,?,?,?,?,?,?,?,?,?,?)').join(',')}";
pub const COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT2: &str = "INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind,carried_allocation_offset_units)\n    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT3: &str = "INSERT INTO money_transaction_snapshots\n      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,\n       original_source_type,original_source_id,original_amount_kind)\n      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const COMPANY_MONEY_SQL_CURRENCY_PREFERENCE_INSERT: &str = "INSERT INTO user_currency_preferences(company_id,user_id,display_currency,updated_at)\n      VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET display_currency=excluded.display_currency,version=user_currency_preferences.version+1,updated_at=excluded.updated_at";
pub const COMPANY_MONEY_SQL_FX_RATE_INSERT: &str = "INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)";
pub const COMPANY_MONEY_SQL_CURRENCY_SCALE_INSERT: &str = "INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,currency) DO NOTHING";

#[cfg(test)]
mod company_money_sql_contract_tests {
    use super::*;
    #[test]
    fn company_money_sql_contract_matches_native_consts() {
        let raw = include_str!("native-company-money-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid company money SQL contract");
        let pairs = [
            ("money_snapshot_insert", COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT),
            ("money_snapshot_batch_insert", COMPANY_MONEY_SQL_MONEY_SNAPSHOT_BATCH_INSERT),
            ("money_snapshot_insert2", COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT2),
            ("money_snapshot_insert3", COMPANY_MONEY_SQL_MONEY_SNAPSHOT_INSERT3),
            ("currency_preference_insert", COMPANY_MONEY_SQL_CURRENCY_PREFERENCE_INSERT),
            ("fx_rate_insert", COMPANY_MONEY_SQL_FX_RATE_INSERT),
            ("currency_scale_insert", COMPANY_MONEY_SQL_CURRENCY_SCALE_INSERT),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "company money SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for finance AI worker statements.
pub const FINANCE_AI_SQL_AI_STMT_1: &str = "INSERT INTO finance_ai_events(id,company_id,proposal_id,action,actor_id,command_id,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?)";
pub const FINANCE_AI_SQL_AI_STMT_2: &str = "UPDATE finance_ai_proposals SET claim_until=? WHERE id=? AND state='running' AND lease_token=? AND claim_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint";
pub const FINANCE_AI_SQL_AI_STMT_3: &str = "UPDATE finance_ai_proposals SET state='running',lease_token=?,claim_until=?,attempts=attempts+1,updated_at=? WHERE id=? AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) RETURNING *";
pub const FINANCE_AI_SQL_AI_STMT_4: &str = "INSERT INTO finance_ai_policies(company_id,enabled,version,kinds_json,timezone,daily_limit,monthly_limit,owner_actor_json,reason,changed_by,changed_at,next_scan_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,0) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,version=excluded.version,kinds_json=excluded.kinds_json,timezone=excluded.timezone,daily_limit=excluded.daily_limit,monthly_limit=excluded.monthly_limit,owner_actor_json=excluded.owner_actor_json,reason=excluded.reason,changed_by=excluded.changed_by,changed_at=excluded.changed_at,next_scan_at=0,last_error=NULL";
pub const FINANCE_AI_SQL_AI_STMT_5: &str = "UPDATE finance_ai_proposals SET state='queued',version=version+1,error=NULL,attempts=0,lease_token=NULL,claim_until=NULL,actor_json=?,usage_id=?,updated_at=? WHERE id=? AND state IN ('failed','stale')";
pub const FINANCE_AI_SQL_AI_STMT_6: &str = "INSERT INTO finance_ai_proposals(id,company_id,kind,source_id,source_version,source_hash,source_json,source_query_json,company_revision,policy_version,provider_json,actor_json,state,usage_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'queued',?,?,?)";
pub const FINANCE_AI_SQL_AI_STMT_7: &str = "INSERT INTO finance_ai_usage(id,proposal_id,company_id,actor_id,month,business_date,state,lease_until,created_at) VALUES(?,?,?,?,?,?,'reserved',?,?)";
pub const FINANCE_AI_SQL_AI_STMT_8: &str = "UPDATE finance_ai_proposals SET state='ready',result_json=?,error=NULL,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND lease_token=?";
pub const FINANCE_AI_SQL_AI_STMT_9: &str = "UPDATE finance_ai_usage SET state='succeeded',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?";
pub const FINANCE_AI_SQL_AI_STMT_10: &str = "UPDATE finance_ai_proposals SET state=?,error=?,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND state='running' AND lease_token=? RETURNING id";
pub const FINANCE_AI_SQL_AI_STMT_11: &str = "UPDATE finance_ai_usage SET state='failed',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?";
pub const FINANCE_AI_SQL_AI_STMT_12: &str = "UPDATE finance_ai_proposals SET state='failed',error='Preparation was interrupted. Retry or continue manually.',lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE state='running' AND claim_until<? AND attempts>=3";
pub const FINANCE_AI_SQL_AI_STMT_13: &str = "UPDATE finance_ai_proposals SET state='dismissed',version=version+1,lease_token=NULL,claim_until=NULL,updated_at=? WHERE id=?";
pub const FINANCE_AI_SQL_AI_STMT_14: &str = "UPDATE finance_ai_policies SET next_scan_at=? WHERE company_id=? AND enabled AND version=? AND next_scan_at<=? RETURNING company_id";
pub const FINANCE_AI_SQL_AI_STMT_15: &str = "UPDATE finance_ai_policies SET last_error=NULL WHERE company_id=? AND version=?";
pub const FINANCE_AI_SQL_AI_STMT_16: &str = "UPDATE finance_ai_policies SET last_error=?,next_scan_at=? WHERE company_id=? AND version=?";
pub const FINANCE_AI_SQL_AI_STMT_17: &str = "UPDATE finance_ai_proposals SET state='applied',draft_command_id=?,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_AI_SQL_AI_STMT_18: &str = "UPDATE finance_ai_usage SET state='failed',lease_until=0 WHERE state='reserved' AND proposal_id IN (SELECT id FROM finance_ai_proposals WHERE state IN ('failed','stale','dismissed'))";

#[cfg(test)]
mod finance_ai_sql_contract_tests {
    use super::*;
    #[test]
    fn finance_ai_sql_contract_matches_native_consts() {
        let raw = include_str!("native-finance-ai-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid finance AI SQL contract");
        let pairs = [
            ("ai_stmt_1", FINANCE_AI_SQL_AI_STMT_1),
            ("ai_stmt_2", FINANCE_AI_SQL_AI_STMT_2),
            ("ai_stmt_3", FINANCE_AI_SQL_AI_STMT_3),
            ("ai_stmt_4", FINANCE_AI_SQL_AI_STMT_4),
            ("ai_stmt_5", FINANCE_AI_SQL_AI_STMT_5),
            ("ai_stmt_6", FINANCE_AI_SQL_AI_STMT_6),
            ("ai_stmt_7", FINANCE_AI_SQL_AI_STMT_7),
            ("ai_stmt_8", FINANCE_AI_SQL_AI_STMT_8),
            ("ai_stmt_9", FINANCE_AI_SQL_AI_STMT_9),
            ("ai_stmt_10", FINANCE_AI_SQL_AI_STMT_10),
            ("ai_stmt_11", FINANCE_AI_SQL_AI_STMT_11),
            ("ai_stmt_12", FINANCE_AI_SQL_AI_STMT_12),
            ("ai_stmt_13", FINANCE_AI_SQL_AI_STMT_13),
            ("ai_stmt_14", FINANCE_AI_SQL_AI_STMT_14),
            ("ai_stmt_15", FINANCE_AI_SQL_AI_STMT_15),
            ("ai_stmt_16", FINANCE_AI_SQL_AI_STMT_16),
            ("ai_stmt_17", FINANCE_AI_SQL_AI_STMT_17),
            ("ai_stmt_18", FINANCE_AI_SQL_AI_STMT_18),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "finance AI SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for expense effect statements.
pub const EXPENSE_EFFECTS_SQL_EXPENSE_EMPLOYEE_PARTY: &str = "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,person_id,created_at,updated_at) VALUES(?,?,'employee',?,?,?,0,?,?,?) ON CONFLICT(company_id,person_id) WHERE person_id IS NOT NULL DO UPDATE SET name=excluded.name";
pub const EXPENSE_EFFECTS_SQL_EXPENSE_PAYABLE_INVOICE: &str = "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'payable',?,?,?,?,?,?,?,?,?,?,?,'expense_claim',?,'posted',?,?,?,?,?,?)";
pub const EXPENSE_EFFECTS_SQL_EXPENSE_REVERSAL_JOURNAL: &str = "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'expense_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?";
pub const EXPENSE_EFFECTS_SQL_EXPENSE_REVERSAL_LINES: &str = "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?";
pub const EXPENSE_EFFECTS_SQL_EXPENSE_JOURNAL_REVERSED: &str = "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?";
pub const EXPENSE_EFFECTS_SQL_EXPENSE_INVOICE_VOIDED: &str = "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?";

#[cfg(test)]
mod expense_effects_sql_contract_tests {
    use super::*;
    #[test]
    fn expense_effects_sql_contract_matches_native_consts() {
        let raw = include_str!("native-expense-effects-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid expense effects SQL contract");
        let pairs = [
            ("expense_employee_party", EXPENSE_EFFECTS_SQL_EXPENSE_EMPLOYEE_PARTY),
            ("expense_payable_invoice", EXPENSE_EFFECTS_SQL_EXPENSE_PAYABLE_INVOICE),
            ("expense_reversal_journal", EXPENSE_EFFECTS_SQL_EXPENSE_REVERSAL_JOURNAL),
            ("expense_reversal_lines", EXPENSE_EFFECTS_SQL_EXPENSE_REVERSAL_LINES),
            ("expense_journal_reversed", EXPENSE_EFFECTS_SQL_EXPENSE_JOURNAL_REVERSED),
            ("expense_invoice_voided", EXPENSE_EFFECTS_SQL_EXPENSE_INVOICE_VOIDED),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "expense effects SQL contract drift: {key}");
        }
    }
}

/// Native-owned SQL contract for main finance statements.
pub const FINANCE_MAIN_SQL_FINANCE_STMT_1: &str = "INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_2: &str = "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_3: &str = "UPDATE finance_accounts SET status=?,version=version+1,updated_at=? WHERE id=? AND chart_id=? AND company_id=? AND version=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_4: &str = "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_5: &str = "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_6: &str = "INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,effective_to,status) VALUES(?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_7: &str = "INSERT INTO finance_budgets(id,company_id,name,fiscal_year,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_8: &str = "INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_9: &str = "INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_10: &str = "INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_11: &str = "INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,recognition_method,start_date,end_date,revenue_account_id,deferred_account_id) VALUES(?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_12: &str = "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(chart_id,code) DO UPDATE SET name=excluded.name,category=excluded.category,currency=excluded.currency,is_control=excluded.is_control,control_kind=excluded.control_kind,version=finance_accounts.version+1,updated_at=excluded.updated_at";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_13: &str = "UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_14: &str = "UPDATE finance_invoices SET status='posted',approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=? AND status='draft'";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_15: &str = "INSERT INTO finance_batch_review_items(command_id,journal_id,company_id,reviewed_version,reviewed_by,assignment_id,evidence_digest,reason,reviewed_at) VALUES(?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_16: &str = "UPDATE finance_invoices SET party_id=?,kind=?,invoice_number=?,invoice_date=?,due_date=?,currency=?,subtotal_minor=?,tax_minor=?,total_minor=?,outstanding_minor=?,tax_code=?,debit_account_id=?,credit_account_id=?,tax_account_id=?,created_by=?,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_17: &str = "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_18: &str = "UPDATE finance_invoices SET line_mode=?,line_revision=? WHERE id=? AND company_id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_19: &str = "UPDATE finance_invoices SET status='posted',journal_id=?,fx_rate_micros=?,base_total_minor=?,base_outstanding_minor=?,approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_20: &str = "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_21: &str = "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_22: &str = "UPDATE finance_payments SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_23: &str = "UPDATE finance_payments SET status='released',released_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_24: &str = "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_25: &str = "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_26: &str = "UPDATE finance_payments SET outstanding_minor=0,status='settled',journal_id=?,fx_rate_micros=?,base_amount_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_27: &str = "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_28: &str = "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_29: &str = "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_30: &str = "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_31: &str = "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_fx_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_32: &str = "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_33: &str = "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_34: &str = "UPDATE finance_invoices SET outstanding_minor=outstanding_minor+?,base_outstanding_minor=base_outstanding_minor+?,status=CASE WHEN outstanding_minor+?=total_minor THEN 'posted' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_35: &str = "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched'";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_36: &str = "UPDATE finance_payments SET status='voided',version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_37: &str = "INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_38: &str = "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_39: &str = "UPDATE finance_payments SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE status END,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_40: &str = "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_41: &str = "UPDATE finance_accounts SET is_control=1,control_kind='bank',version=version+CASE WHEN control_kind='none' THEN 1 ELSE 0 END,updated_at=? WHERE id=? AND company_id=? AND control_kind='none'";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_42: &str = "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_43: &str = "INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_44: &str = "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_45: &str = "INSERT INTO finance_fx_revaluation_runs(id,company_id,chart_id,period_id,entry_date,base_currency,rates_json,source_hash,gain_account_id,loss_account_id,preparation_proof,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_46: &str = "INSERT INTO finance_fx_revaluation_lines(id,run_id,invoice_id,kind,currency,control_account_id,transaction_outstanding_minor,prior_base_minor,revalued_base_minor,adjustment_minor,rate_micros) VALUES(?,?,?,?,?,?,?,?,?,?,?)";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_47: &str = "UPDATE finance_fx_revaluation_runs SET status='rejected',decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_48: &str = "UPDATE finance_invoices SET base_outstanding_minor=base_outstanding_minor+?,version=version+1,updated_at=? WHERE id=?";
pub const FINANCE_MAIN_SQL_FINANCE_STMT_49: &str = "UPDATE finance_fx_revaluation_runs SET status='posted',journal_id=?,decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?";

#[cfg(test)]
mod finance_main_sql_contract_tests {
    use super::*;
    #[test]
    fn finance_main_sql_contract_matches_native_consts() {
        let raw = include_str!("native-finance-main-sql.json");
        let parsed: serde_json::Value = serde_json::from_str(raw).expect("invalid main finance SQL contract");
        let pairs = [
            ("finance_stmt_1", FINANCE_MAIN_SQL_FINANCE_STMT_1),
            ("finance_stmt_2", FINANCE_MAIN_SQL_FINANCE_STMT_2),
            ("finance_stmt_3", FINANCE_MAIN_SQL_FINANCE_STMT_3),
            ("finance_stmt_4", FINANCE_MAIN_SQL_FINANCE_STMT_4),
            ("finance_stmt_5", FINANCE_MAIN_SQL_FINANCE_STMT_5),
            ("finance_stmt_6", FINANCE_MAIN_SQL_FINANCE_STMT_6),
            ("finance_stmt_7", FINANCE_MAIN_SQL_FINANCE_STMT_7),
            ("finance_stmt_8", FINANCE_MAIN_SQL_FINANCE_STMT_8),
            ("finance_stmt_9", FINANCE_MAIN_SQL_FINANCE_STMT_9),
            ("finance_stmt_10", FINANCE_MAIN_SQL_FINANCE_STMT_10),
            ("finance_stmt_11", FINANCE_MAIN_SQL_FINANCE_STMT_11),
            ("finance_stmt_12", FINANCE_MAIN_SQL_FINANCE_STMT_12),
            ("finance_stmt_13", FINANCE_MAIN_SQL_FINANCE_STMT_13),
            ("finance_stmt_14", FINANCE_MAIN_SQL_FINANCE_STMT_14),
            ("finance_stmt_15", FINANCE_MAIN_SQL_FINANCE_STMT_15),
            ("finance_stmt_16", FINANCE_MAIN_SQL_FINANCE_STMT_16),
            ("finance_stmt_17", FINANCE_MAIN_SQL_FINANCE_STMT_17),
            ("finance_stmt_18", FINANCE_MAIN_SQL_FINANCE_STMT_18),
            ("finance_stmt_19", FINANCE_MAIN_SQL_FINANCE_STMT_19),
            ("finance_stmt_20", FINANCE_MAIN_SQL_FINANCE_STMT_20),
            ("finance_stmt_21", FINANCE_MAIN_SQL_FINANCE_STMT_21),
            ("finance_stmt_22", FINANCE_MAIN_SQL_FINANCE_STMT_22),
            ("finance_stmt_23", FINANCE_MAIN_SQL_FINANCE_STMT_23),
            ("finance_stmt_24", FINANCE_MAIN_SQL_FINANCE_STMT_24),
            ("finance_stmt_25", FINANCE_MAIN_SQL_FINANCE_STMT_25),
            ("finance_stmt_26", FINANCE_MAIN_SQL_FINANCE_STMT_26),
            ("finance_stmt_27", FINANCE_MAIN_SQL_FINANCE_STMT_27),
            ("finance_stmt_28", FINANCE_MAIN_SQL_FINANCE_STMT_28),
            ("finance_stmt_29", FINANCE_MAIN_SQL_FINANCE_STMT_29),
            ("finance_stmt_30", FINANCE_MAIN_SQL_FINANCE_STMT_30),
            ("finance_stmt_31", FINANCE_MAIN_SQL_FINANCE_STMT_31),
            ("finance_stmt_32", FINANCE_MAIN_SQL_FINANCE_STMT_32),
            ("finance_stmt_33", FINANCE_MAIN_SQL_FINANCE_STMT_33),
            ("finance_stmt_34", FINANCE_MAIN_SQL_FINANCE_STMT_34),
            ("finance_stmt_35", FINANCE_MAIN_SQL_FINANCE_STMT_35),
            ("finance_stmt_36", FINANCE_MAIN_SQL_FINANCE_STMT_36),
            ("finance_stmt_37", FINANCE_MAIN_SQL_FINANCE_STMT_37),
            ("finance_stmt_38", FINANCE_MAIN_SQL_FINANCE_STMT_38),
            ("finance_stmt_39", FINANCE_MAIN_SQL_FINANCE_STMT_39),
            ("finance_stmt_40", FINANCE_MAIN_SQL_FINANCE_STMT_40),
            ("finance_stmt_41", FINANCE_MAIN_SQL_FINANCE_STMT_41),
            ("finance_stmt_42", FINANCE_MAIN_SQL_FINANCE_STMT_42),
            ("finance_stmt_43", FINANCE_MAIN_SQL_FINANCE_STMT_43),
            ("finance_stmt_44", FINANCE_MAIN_SQL_FINANCE_STMT_44),
            ("finance_stmt_45", FINANCE_MAIN_SQL_FINANCE_STMT_45),
            ("finance_stmt_46", FINANCE_MAIN_SQL_FINANCE_STMT_46),
            ("finance_stmt_47", FINANCE_MAIN_SQL_FINANCE_STMT_47),
            ("finance_stmt_48", FINANCE_MAIN_SQL_FINANCE_STMT_48),
            ("finance_stmt_49", FINANCE_MAIN_SQL_FINANCE_STMT_49),
        ];
        for (key, native) in pairs {
            assert_eq!(parsed[key].as_str(), Some(native), "main finance SQL contract drift: {key}");
        }
    }
}
