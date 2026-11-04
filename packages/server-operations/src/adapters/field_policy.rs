//! Operations-owned policy persistence and exact affected-area witnesses.
use crate::domain::field_policy::*;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_POLICY_UNAVAILABLE",
        "Review the current queue policy.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("OPERATIONS_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(c: &mut PgConnection, company: &CompanyId, area: &str) -> Result<String, Error> {
    let v:Option<String>=sqlx::query_scalar("WITH policies AS MATERIALIZED(SELECT * FROM operations_queue_policies WHERE company_id=$1 AND area=$2 ORDER BY revision,id FOR UPDATE),payload AS(SELECT coalesce(jsonb_agg(to_jsonb(policies) ORDER BY revision,id),'[]'::jsonb)::text value FROM policies)SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM payload").bind(company.as_str()).bind(area).fetch_one(c).await?;
    v.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_POLICY_LIMIT",
            "This queue exceeds the bounded policy view.",
        )
        .into()
    })
}
pub struct CurrentPolicyArea {
    company: CompanyId,
    area: String,
    unit: Unit,
    image: String,
    rows: Vec<QueuePolicy>,
}
impl CurrentPolicyArea {
    pub fn rows(&self) -> &[QueuePolicy] {
        &self.rows
    }
    pub fn policy(&self, id: &str) -> Option<&QueuePolicy> {
        self.rows.iter().find(|v| v.id == id)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.company, &self.area).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_policy_area(
    c: &mut PgConnection,
    company: &CompanyId,
    area: &str,
) -> Result<CurrentPolicyArea, Error> {
    let unit = unit(c).await?;
    let image = image(c, company, area).await?;
    let rows = serde_json::from_str(&image).map_err(|_| changed())?;
    Ok(CurrentPolicyArea {
        company: company.clone(),
        area: area.into(),
        unit,
        image,
        rows,
    })
}
pub async fn read_policy(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentPolicyArea>, Error> {
    let area: Option<String> = sqlx::query_scalar(
        "SELECT area FROM operations_queue_policies WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    match area {
        None => Ok(None),
        Some(area) => Ok(Some(read_policy_area(c, company, &area).await?)),
    }
}
fn field_only(rows: &[QueuePolicy]) -> Result<(), Error> {
    for row in rows {
        let kinds: Vec<String> =
            serde_json::from_str(&row.source_kinds_json).map_err(|_| changed())?;
        if kinds.iter().any(|v| v != "field_service_order") {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_POLICY_SHARED_AREA",
                "This policy area still has other owner consumers.",
            )
            .into());
        }
    }
    Ok(())
}
pub async fn draft_policy(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    input: &DraftPolicy,
    id: &str,
    user: &str,
    now: i64,
) -> Result<CurrentPolicyArea, Error> {
    draft_policy_inner(c, before, input, id, user, now, true).await
}

/// Draft any Operations-owned queue policy from the canonical owner area.
pub async fn draft_operations_policy(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    input: &DraftPolicy,
    id: &str,
    user: &str,
    now: i64,
) -> Result<CurrentPolicyArea, Error> {
    draft_policy_inner(c, before, input, id, user, now, false).await
}

async fn draft_policy_inner(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    input: &DraftPolicy,
    id: &str,
    user: &str,
    now: i64,
    field: bool,
) -> Result<CurrentPolicyArea, Error> {
    before.require_current(c).await?;
    if field {
        input.validate()?;
        field_only(&before.rows)?;
    } else {
        input.validate_native()?;
    }
    if crate::domain::field_exceptions::trimmed(&input.area) != before.area {
        return Err(changed());
    }
    let revision = before
        .rows
        .iter()
        .map(|v| v.revision)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(changed)?;
    let intended = if field {
        input.draft(id, before.company.as_str(), user, revision, now)?
    } else {
        input.draft_native(id, before.company.as_str(), user, revision, now)?
    };
    sqlx::query("INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,status,created_by,version,created_at,updated_at)VALUES($1,$2,$3,$4,$5,$6,$7,'draft',$8,1,$9,$9)").bind(id).bind(before.company.as_str()).bind(&intended.area).bind(&intended.name).bind(revision).bind(&intended.source_kinds_json).bind(intended.target_minutes).bind(user).bind(now).execute(&mut *c).await?;
    let after = read_policy_area(c, &before.company, &before.area).await?;
    let mut expected: Vec<Value> = serde_json::from_str(&before.image).map_err(|_| changed())?;
    expected.push(serde_json::to_value(intended).map_err(|_| changed())?);
    if serde_json::from_str::<Value>(&after.image).map_err(|_| changed())? != Value::Array(expected)
    {
        return Err(changed());
    }
    Ok(after)
}
// Fixed Field policy contract; callers supply the complete current proof set.
#[allow(clippy::too_many_arguments)]
pub async fn review_policy_area(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    id: &str,
    action: PolicyAction,
    expected_version: i64,
    same_creator: bool,
    reason: &str,
    user: &str,
    now: i64,
) -> Result<CurrentPolicyArea, Error> {
    review_policy_area_inner(
        c,
        before,
        id,
        action,
        expected_version,
        same_creator,
        reason,
        user,
        now,
        true,
    )
    .await
}

/// Review any Operations-owned queue policy from the canonical owner area.
#[allow(clippy::too_many_arguments)]
pub async fn review_operations_policy_area(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    id: &str,
    action: PolicyAction,
    expected_version: i64,
    same_creator: bool,
    reason: &str,
    user: &str,
    now: i64,
) -> Result<CurrentPolicyArea, Error> {
    review_policy_area_inner(
        c,
        before,
        id,
        action,
        expected_version,
        same_creator,
        reason,
        user,
        now,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn review_policy_area_inner(
    c: &mut PgConnection,
    before: &CurrentPolicyArea,
    id: &str,
    action: PolicyAction,
    expected_version: i64,
    same_creator: bool,
    reason: &str,
    user: &str,
    now: i64,
    field: bool,
) -> Result<CurrentPolicyArea, Error> {
    before.require_current(c).await?;
    if field {
        field_only(&before.rows)?;
    }
    let row = before.policy(id).ok_or_else(changed)?;
    let version = review_policy(row, action, expected_version, same_creator, reason)?;
    let mut expected: Vec<Value> = serde_json::from_str(&before.image).map_err(|_| changed())?;
    if action == PolicyAction::Publish {
        sqlx::query("UPDATE operations_queue_policies SET status='retired',version=version+1,updated_at=$1 WHERE company_id=$2 AND area=$3 AND status='published'").bind(now).bind(before.company.as_str()).bind(&before.area).execute(&mut *c).await?;
        for v in &mut expected {
            if v["status"] == "published" {
                let n = v["version"]
                    .as_i64()
                    .ok_or_else(changed)?
                    .checked_add(1)
                    .ok_or_else(changed)?;
                positive(n)?;
                v["version"] = n.into();
                v["status"] = "retired".into();
                v["updated_at"] = now.into();
            }
        }
    }
    let changed_rows=sqlx::query("UPDATE operations_queue_policies SET status=$1,reviewed_by=$2,version=$3,updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7 AND status=$8").bind(action.status()).bind(user).bind(version).bind(now).bind(id).bind(before.company.as_str()).bind(expected_version).bind(&row.status).execute(&mut *c).await?.rows_affected();
    if changed_rows != 1 {
        return Err(changed());
    }
    for v in &mut expected {
        if v["id"] == id {
            v["status"] = action.status().into();
            v["reviewed_by"] = user.into();
            v["version"] = version.into();
            v["updated_at"] = now.into();
        }
    }
    let after = read_policy_area(c, &before.company, &before.area).await?;
    if serde_json::from_str::<Value>(&after.image).map_err(|_| changed())? != Value::Array(expected)
    {
        return Err(changed());
    }
    Ok(after)
}
