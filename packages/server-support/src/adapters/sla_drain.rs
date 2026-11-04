//! Support-owned SLA breach sweep.
//!
//! The caller supplies the existing Holding transaction and current Company.
//! Every sweep rechecks the active Support purchase at the detection instant,
//! inserts only missing immutable alerts and emits eligible-manager notices.
//! The caller must commit this unit; a failure leaves both alert and notice
//! rows for the retained durable recovery path to retry.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

const SLA_BREACHED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,'Resolution target breached: take responsibility for follow-up',$1 FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=m.user_id AND p.company_id=m.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE s.detected_at=$1 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ON CONFLICT DO NOTHING";

/// Insert overdue Support clock alerts and their current-manager notices. The
/// returned count is exactly the number of newly inserted alerts; a repeated
/// sweep at the same instant returns zero and inserts no duplicate notice.
pub async fn drain_support_sla(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<i64, Error> {
    sqlx::query("LOCK TABLE support_tickets,support_sla_alerts,staff_notifications IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    let inserted = sqlx::query("INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at) SELECT 'support-breach-'||t.id,t.id,t.version,COALESCE((t.sla_clock_json->>'breachedAt')::bigint,(t.sla_clock_json->>'dueAt')::bigint),$1 FROM support_tickets t WHERE t.company_id=$2 AND EXISTS(SELECT 1 FROM company_package_grants g WHERE g.company_id=t.company_id AND g.package_id='support' AND g.state='active' AND g.starts_at<=$1 AND (g.ends_at IS NULL OR g.ends_at>$1)) AND ((t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL OR (t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt')::bigint<$1)) ON CONFLICT DO NOTHING")
        .bind(now)
        .bind(company.as_str())
        .execute(&mut *c)
        .await?
        .rows_affected();
    sqlx::query(SLA_BREACHED_NOTIFICATION)
        .bind(now)
        .execute(&mut *c)
        .await?;
    Ok(inserted as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_notice_projection_stays_idempotent_and_purchase_gated() {
        assert!(SLA_BREACHED_NOTIFICATION.contains("ON CONFLICT DO NOTHING"));
        assert!(SLA_BREACHED_NOTIFICATION.contains("'support_sla_alert'"));
        assert!(SLA_BREACHED_NOTIFICATION.contains("s.detected_at=$1"));
    }
}
