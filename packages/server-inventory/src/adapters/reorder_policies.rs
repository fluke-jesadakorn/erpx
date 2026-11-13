//! PostgreSQL reorder-policy source/effect seam for the native reorder command.
//!
//! The caller (composition) owns current IAM authority and the Workflow ledger.
//! These functions read current warehouse/product/policy facts, revalidate the
//! planned proposal or decision and apply only policy/event effects inside the
//! caller's serializable unit.
use crate::domain::reorder_policies::{
    ReorderPolicyRecord, ReorderSavePlan, ReorderSaveSource, ReorderTransitionPlan,
    ReorderTransitionSource, ReorderWarehouse,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "REORDER_POLICY_CHANGED",
        "The reorder policy changed. Refresh and review it before continuing.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Lock reorder policy/event, warehouse and product relations before reading.
pub async fn lock_reorder_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_reorder_policies,inventory_reorder_policy_events,\
         commerce_warehouses,commerce_products IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn read_policy(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<Option<ReorderPolicyRecord>, Error> {
    let row = sqlx::query(
        "SELECT r.id,r.company_id,r.warehouse_id,r.product_id,w.branch_id,\
         r.current_reorder_point_milli,r.proposed_reorder_point_milli,r.requested_by,\
         r.approved_by,r.retired_at,r.version \
         FROM inventory_reorder_policies r JOIN commerce_warehouses w ON w.id=r.warehouse_id \
         WHERE r.id=$1 AND r.company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, sqlx::Error>(ReorderPolicyRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            warehouse_id: row.try_get("warehouse_id")?,
            product_id: row.try_get("product_id")?,
            branch_id: row.try_get("branch_id")?,
            current_reorder_point_milli: row.try_get("current_reorder_point_milli")?,
            proposed_reorder_point_milli: row.try_get("proposed_reorder_point_milli")?,
            requested_by: row.try_get("requested_by")?,
            approved_by: row.try_get("approved_by")?,
            retired_at: row.try_get("retired_at")?,
            version: row.try_get("version")?,
        })
    })
    .transpose()
    .map_err(Into::into)
}

/// Read the active warehouse/product and any existing policy for the pair.
pub async fn read_reorder_save_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
    warehouse_id: &str,
    product_id: &str,
) -> Result<ReorderSaveSource, Error> {
    serializable(c).await?;
    let warehouse = sqlx::query(
        "SELECT id,branch_id,version FROM commerce_warehouses \
         WHERE id=$1 AND company_id=$2 AND status='active' \
           AND ($3::text IS NULL OR branch_id=$3)",
    )
    .bind(warehouse_id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?
    .map(|row| {
        Ok::<_, sqlx::Error>(ReorderWarehouse {
            id: row.try_get("id")?,
            branch_id: row.try_get("branch_id")?,
            version: row.try_get("version")?,
        })
    })
    .transpose()?;
    let product = sqlx::query(
        "SELECT id,version FROM commerce_products WHERE id=$1 AND company_id=$2 AND status='active'",
    )
    .bind(product_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .map(|row| {
        Ok::<_, sqlx::Error>(crate::domain::reorder_policies::ReorderProduct {
            id: row.try_get("id")?,
            version: row.try_get("version")?,
        })
    })
    .transpose()?;
    let policy = read_policy(c, company, id).await?;
    Ok(ReorderSaveSource {
        policy,
        warehouse,
        product,
    })
}

/// Read the current policy and warehouse scope for a review decision.
pub async fn read_reorder_transition_source(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<ReorderTransitionSource, Error> {
    serializable(c).await?;
    let policy = read_policy(c, company, id).await?.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "REORDER_POLICY_NOT_FOUND",
            "The inventory reorder policy is unavailable in the selected scope.",
        )
    })?;
    if let Some(branch) = branch
        && policy.branch_id != branch
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "REORDER_POLICY_NOT_FOUND",
            "The inventory reorder policy is unavailable in the selected scope.",
        )
        .into());
    }
    Ok(ReorderTransitionSource { policy })
}

fn snapshot(reason: &str, fields: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("reason".into(), serde_json::Value::String(reason.to_owned()));
    for (key, value) in fields {
        object.insert((*key).to_owned(), value.clone());
    }
    serde_json::Value::Object(object)
}

fn milli_value(value: Option<i64>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |value| {
        serde_json::Value::String(value.to_string())
    })
}

/// Apply a proposal/revision with warehouse, product, uniqueness and editable
/// policy guards, then append the immutable proposal event.
pub async fn apply_reorder_save(
    c: &mut PgConnection,
    plan: &ReorderSavePlan,
    key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    let warehouse: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=$1 AND company_id=$2 \
         AND branch_id=$3 AND version=$4 AND status='active')",
    )
    .bind(&plan.warehouse_id)
    .bind(&plan.company_id)
    .bind(&plan.branch_id)
    .bind(plan.warehouse_version)
    .fetch_one(&mut *c)
    .await?;
    if !warehouse {
        return Err(changed());
    }
    let product: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM commerce_products WHERE id=$1 AND version=$2 AND status='active')",
    )
    .bind(&plan.product_id)
    .bind(plan.product_version)
    .fetch_one(&mut *c)
    .await?;
    if !product {
        return Err(changed());
    }
    if plan.revise {
        let expected = plan.expected_version.unwrap_or(0);
        let updated = sqlx::query(
            "UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=$1,\
             requested_by=$2,version=version+1,updated_at=$3 \
             WHERE id=$4 AND company_id=$5 AND version=$6 \
               AND proposed_reorder_point_milli IS NULL",
        )
        .bind(plan.point_milli)
        .bind(actor)
        .bind(now)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(expected)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed());
        }
    } else {
        let unique: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS(SELECT 1 FROM inventory_reorder_policies \
             WHERE id=$1 OR (warehouse_id=$2 AND product_id=$3))",
        )
        .bind(&plan.id)
        .bind(&plan.warehouse_id)
        .bind(&plan.product_id)
        .fetch_one(&mut *c)
        .await?;
        if !unique {
            return Err(changed());
        }
        sqlx::query(
            "INSERT INTO inventory_reorder_policies(id,company_id,warehouse_id,product_id,\
             proposed_reorder_point_milli,requested_by,version,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,1,$7,$7)",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(&plan.warehouse_id)
        .bind(&plan.product_id)
        .bind(plan.point_milli)
        .bind(actor)
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    let snapshot = snapshot(
        &plan.reason,
        &[
            ("warehouseId", serde_json::Value::String(plan.warehouse_id.clone())),
            ("productId", serde_json::Value::String(plan.product_id.clone())),
            ("proposedReorderPointMilli", milli_value(Some(plan.point_milli))),
            ("currentReorderPointMilli", milli_value(plan.current_reorder_point_milli)),
        ],
    );
    sqlx::query(
        "INSERT INTO inventory_reorder_policy_events(id,policy_id,policy_version,action,\
         actor_id,reason,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
    )
    .bind(format!("{}:{}", plan.id, plan.version))
    .bind(&plan.id)
    .bind(plan.version)
    .bind(if plan.revise { "revise" } else { "propose" })
    .bind(actor)
    .bind(&plan.reason)
    .bind(snapshot.to_string())
    .bind(now)
    .execute(&mut *c)
    .await?;
    let _ = key;
    Ok(())
}

/// Apply an approve/reject/withdraw/retire decision and its immutable event.
pub async fn apply_reorder_transition(
    c: &mut PgConnection,
    plan: &ReorderTransitionPlan,
    key: &str,
    actor: &str,
    now: i64,
) -> Result<(), Error> {
    let updated = match plan.action {
        crate::domain::reorder_policies::ReorderAction::Approve => sqlx::query(
            "UPDATE inventory_reorder_policies SET \
             current_reorder_point_milli=proposed_reorder_point_milli,\
             proposed_reorder_point_milli=NULL,approved_by=$1,approved_at=$2,retired_at=NULL,\
             last_decision='approved',last_reviewed_by=$1,last_review_reason=$3,\
             last_reviewed_at=$2,version=version+1,updated_at=$2 \
             WHERE id=$4 AND company_id=$5 AND version=$6",
        )
        .bind(actor)
        .bind(now)
        .bind(&plan.reason)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(plan.expected_version),
        crate::domain::reorder_policies::ReorderAction::Reject => sqlx::query(
            "UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=NULL,\
             last_decision='rejected',last_reviewed_by=$1,last_review_reason=$2,\
             last_reviewed_at=$3,version=version+1,updated_at=$3 \
             WHERE id=$4 AND company_id=$5 AND version=$6",
        )
        .bind(actor)
        .bind(&plan.reason)
        .bind(now)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(plan.expected_version),
        crate::domain::reorder_policies::ReorderAction::Withdraw => sqlx::query(
            "UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=NULL,\
             last_decision=NULL,last_reviewed_by=NULL,last_review_reason=NULL,\
             last_reviewed_at=NULL,version=version+1,updated_at=$1 \
             WHERE id=$2 AND company_id=$3 AND version=$4",
        )
        .bind(now)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(plan.expected_version),
        crate::domain::reorder_policies::ReorderAction::Retire => sqlx::query(
            "UPDATE inventory_reorder_policies SET retired_at=$1,last_decision='retired',\
             last_reviewed_by=$2,last_review_reason=$3,last_reviewed_at=$4,version=version+1,\
             updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7",
        )
        .bind(now)
        .bind(actor)
        .bind(&plan.reason)
        .bind(now)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(plan.expected_version),
    };
    if updated.execute(&mut *c).await?.rows_affected() != 1 {
        return Err(changed());
    }
    let snapshot = snapshot(
        &plan.reason,
        &[
            (
                "action",
                serde_json::Value::String(plan.action.as_str().to_owned()),
            ),
            ("currentReorderPointMilli", milli_value(plan.current_reorder_point_milli)),
            ("proposedReorderPointMilli", milli_value(plan.proposed_reorder_point_milli)),
        ],
    );
    sqlx::query(
        "INSERT INTO inventory_reorder_policy_events(id,policy_id,policy_version,action,\
         actor_id,reason,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
    )
    .bind(format!("{}:{}", plan.id, plan.version))
    .bind(&plan.id)
    .bind(plan.version)
    .bind(plan.action.as_str())
    .bind(actor)
    .bind(&plan.reason)
    .bind(snapshot.to_string())
    .bind(now)
    .execute(&mut *c)
    .await?;
    let _ = key;
    Ok(())
}
