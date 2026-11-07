//! Native Maintenance work-order creation insert. Source admission, purchase,
//! workflow receipt and notification remain coordinator obligations.
use super::{Error, transaction};
use erpx_server_shared::public::RecordId;
use sqlx::PgConnection;

pub struct MaintenanceCreationPlan<'a> {
    pub id: &'a RecordId,
    pub key: &'a str,
    pub company_id: &'a str,
    pub branch_id: &'a RecordId,
    pub department_id: Option<&'a RecordId>,
    pub asset_id: &'a RecordId,
    pub asset_version: i64,
    pub title: &'a str,
    pub priority: &'a str,
    pub status: &'a str,
    pub owner_id: &'a str,
    pub scheduled_at: Option<i64>,
    pub reason: &'a str,
    pub actor_id: &'a str,
    pub now: i64,
}

/// Confirm the asset has no active work after the caller locks the work table.
pub async fn maintenance_creation_vacancy(
    c: &mut PgConnection,
    asset_id: &RecordId,
) -> Result<bool, Error> {
    let _ = transaction(c).await?;
    Ok(sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))",
    )
    .bind(asset_id.as_str())
    .fetch_one(c)
    .await?)
}

/// Insert the work order and its version-1 create event on the caller's open
/// serializable transaction. Never commits.
pub async fn insert_maintenance_creation(
    c: &mut PgConnection,
    plan: &MaintenanceCreationPlan<'_>,
) -> Result<(), Error> {
    let _ = transaction(c).await?;
    let inserted = sqlx::query(
        "INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,scheduled_at,version,created_by,created_at,updated_at) \
         SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11,$12,$12 \
         WHERE EXISTS(SELECT 1 FROM operational_assets WHERE id=$5 AND company_id=$2 AND version=$13)",
    )
    .bind(plan.id.as_str())
    .bind(plan.company_id)
    .bind(plan.branch_id.as_str())
    .bind(plan.department_id.map(RecordId::as_str))
    .bind(plan.asset_id.as_str())
    .bind(plan.title)
    .bind(plan.priority)
    .bind(plan.status)
    .bind(plan.owner_id)
    .bind(plan.scheduled_at)
    .bind(plan.actor_id)
    .bind(plan.now)
    .bind(plan.asset_version)
    .execute(&mut *c)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(super::changed().into());
    }
    sqlx::query(
        "INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,created_at) \
         VALUES($1,$2,1,'create',$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("maintenance-event-{}", plan.key))
    .bind(plan.id.as_str())
    .bind(plan.actor_id)
    .bind(plan.reason)
    .bind(plan.status)
    .bind(plan.owner_id)
    .bind(plan.scheduled_at)
    .bind(plan.now)
    .execute(&mut *c)
    .await?;
    Ok(())
}
