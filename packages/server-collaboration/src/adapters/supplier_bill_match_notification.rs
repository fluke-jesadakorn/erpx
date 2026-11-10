//! Collaboration-owned supplier-bill match notification port. Procurement and
//! Finance pass the reviewed match facts; this owner writes its own notification
//! table and selects the current Finance recipients from IAM assignments.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_BILL_NOTIFICATION_UNAVAILABLE",
        "The supplier-bill notification source is unavailable.",
    )
}

pub async fn stage_supplier_bill_match_notification(
    c: &mut PgConnection,
    company_id: &str,
    match_id: &str,
    version: i64,
    status: &str,
    branch_id: &str,
    department_id: Option<&str>,
    command_id: &str,
    now: i64,
) -> Result<(), BusinessError> {
    if company_id.is_empty()
        || match_id.is_empty()
        || command_id.is_empty()
        || version < 1
        || !matches!(status, "posted" | "returned")
    {
        return Err(unavailable());
    }
    let message = match status {
        "posted" => "Matched supplier invoice posted to GL",
        "returned" => {
            "Supplier invoice returned: obtain corrected evidence and resubmit from the receipt"
        }
        _ => return Err(unavailable()),
    };
    sqlx::query(
        "LOCK TABLE staff_notifications,inbox_events,inbox_deliveries IN ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    sqlx::query(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
         record_version,message,created_at) \
         SELECT DISTINCT $1 || '-' || a.user_id,$2,a.user_id,'supplier_bill',$3,$4,$5,$6 \
         FROM assignments a \
         WHERE a.company_id=$2 \
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_role_allows(a.role,a.company_id,'finance.write') \
           AND (a.scope_kind='holding' \
             OR (a.scope_kind='branch' AND a.scope_node_id=$7) \
             OR (a.scope_kind='department' AND a.scope_node_id=$8)) \
         ON CONFLICT DO NOTHING",
    )
    .bind(command_id)
    .bind(company_id)
    .bind(match_id)
    .bind(version)
    .bind(message)
    .bind(now)
    .bind(branch_id)
    .bind(department_id)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}
