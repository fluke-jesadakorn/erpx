//! Pure query vocabulary and owner-owned SQL for the native follow-up workspace.
//! Every fragment is a fixed owner literal; only scalar identities are bound.
use erpx_platform_shared_server::public::BusinessError;

pub const WORKSPACE_PATH: &str = "/api/v1/holdings/{holding_slug}/customer-followups";

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_FOLLOWUP_REQUEST_INVALID",
        "Use one bounded customer follow-up workspace read.",
    )
}

/// Matches URLSearchParams.get: ignore unrelated keys, take the first value,
/// and retain an explicitly empty value.
#[derive(Clone, Debug, Default)]
pub struct WorkspaceQuery {
    pub order_id: Option<String>,
}

impl WorkspaceQuery {
    pub fn from_query(query: Option<&str>) -> Self {
        fn decode(value: &str) -> String {
            let bytes = value.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    let hex = |b: u8| (b as char).to_digit(16).filter(|_| b.is_ascii());
                    if let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                        out.push((a * 16 + b) as u8); i += 3; continue;
                    }
                }
                out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
                i += 1;
            }
            String::from_utf8_lossy(&out).into_owned()
        }
        let order_id = query.unwrap_or("").split('&').filter(|part| !part.is_empty()).find_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode(key) == "orderId").then(|| decode(value))
        });
        Self { order_id }
    }
    pub fn validate(&self) -> Result<(), BusinessError> {
        Ok(())
    }
}

/// Full retired workspace row projection: fixed columns, fixed join, bounded binds.
pub fn items_sql() -> &'static str {
    "SELECT to_jsonb(x) value FROM (SELECT c.*, \
     (SELECT legal_name FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name, \
     o.number order_number,o.branch_id,o.department_id,p.name customer_name, \
     CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=o.company_id AND a.user_id=c.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible \
     FROM customer_followups c JOIN (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) o ON o.id=c.order_id AND o.company_id=c.company_id \
     JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
     WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) AND ($3::text IS NULL OR o.department_id=$3) AND ($4::text IS NULL OR c.order_id=$4) AND ((o.kind='quote' AND $5) OR (o.kind='sales' AND $6)) \
     ORDER BY c.due_date,c.id) x"
}

/// The selected customer quotation/order; a purchase source is rejected by
/// the application projection exactly as the retired `source()` guard did.
pub fn order_sql() -> &'static str {
    "SELECT to_jsonb(result) value FROM (SELECT o.*,p.name party_name,p.code party_code,p.notification_email,p.notification_consent,p.version party_version,w.name warehouse_name FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id LEFT JOIN commerce_warehouses w ON w.id=o.warehouse_id WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4)) result"
}

/// Sales-qualified recipients for the selected order.
pub fn recipients_sql() -> &'static str {
    "SELECT to_jsonb(r) value FROM (SELECT DISTINCT c.owner_id id,p.legal_name name FROM (SELECT DISTINCT user_id owner_id FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) assignments WHERE company_id=$1) c JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=c.owner_id AND p.company_id=$1 JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=$4 WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) AND ($3::text IS NULL OR o.department_id=$3) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=o.company_id AND a.user_id=c.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ORDER BY p.legal_name) r"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_the_shared_handler_route() {
        assert_eq!(WORKSPACE_PATH, "/api/v1/holdings/{holding_slug}/customer-followups");
    }

    #[test]
    fn query_matches_url_search_params_first_value_and_decoding() {
        assert!(WorkspaceQuery::default().validate().is_ok());
        assert_eq!(WorkspaceQuery::from_query(Some("other=1&orderId=one&orderId=two")).order_id.as_deref(), Some("one"));
        assert_eq!(WorkspaceQuery::from_query(Some("orderId=&orderId=two")).order_id.as_deref(), Some(""));
        assert_eq!(WorkspaceQuery::from_query(Some("order%49d=%E0%B9%84%E0%B8%97%E0%B8%A2+%ZZ")).order_id.as_deref(), Some("ไทย %ZZ"));
    }

    #[test]
    fn statements_are_fixed_owner_sql_with_bounded_placeholders() {
        for sql in [items_sql(), order_sql(), recipients_sql()] {
            assert!(!sql.contains('?'), "no caller SQL placeholders");
        }
        assert!(items_sql().contains("customer_followups c"));
        assert!(items_sql().contains("owner_eligible"));
        assert!(items_sql().contains("o.kind='quote' AND $5"));
        assert!(items_sql().contains("o.kind='sales' AND $6"));
        assert!(order_sql().contains("o.branch_id=$3"));
        assert!(order_sql().contains("o.department_id=$4"));
        for field in ["party_name", "party_code", "notification_email", "notification_consent", "party_version", "warehouse_name"] {
            assert!(order_sql().contains(field));
        }
        assert!(recipients_sql().contains("iam_assignment_active"));
        assert!(recipients_sql().contains("iam_principal_active"));
        assert!(recipients_sql().contains("iam_role_allows"));
        assert!(recipients_sql().contains("hr_engagements"));
    }
}
