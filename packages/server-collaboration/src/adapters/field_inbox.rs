//! Only Collaboration's own notification tables are read here. Source ownership,
//! current IAM and Field Records witnesses are composed outside this owner.
use crate::domain::field_inbox::*;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, UserId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
/// Utility preparation precedes the selected snapshot. Read leases must remain
/// compatible with unrelated delivery writers; the enclosing unit is SERIALIZABLE.
pub async fn lock_field_inbox_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INBOX_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE inbox_events,inbox_deliveries IN ROW SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_field_inbox_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &UserId,
    query: &FieldInboxQuery,
) -> Result<Vec<FieldInboxCandidate>, Error> {
    query.validate()?;
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INBOX_TRANSACTION_REQUIRED").into());
    }
    // Staff retains its original row data and archived visibility. Its page
    // cursor orders the historical text key, while the top list uses id ASC.
    let rows=sqlx::query(r#"
WITH source AS (
 SELECT d.id,d.event_id,d.company_id,d.user_id,
  CASE WHEN $9='inbox' THEN d.delivered_at ELSE n.created_at END delivered_at,
  CASE WHEN $9='inbox' THEN d.read_at ELSE n.read_at END read_at,d.archived_at,
  CASE WHEN $9='inbox' THEN e.source_type ELSE n.record_type END source_type,
  CASE WHEN $9='inbox' THEN e.source_id ELSE n.record_id END source_id,
  CASE WHEN $9='inbox' THEN e.source_version ELSE n.record_version END source_version,
  e.category,CASE WHEN $9='inbox' THEN e.preview ELSE n.message END preview
 FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id
 LEFT JOIN staff_notifications n ON n.id=d.id AND n.company_id=d.company_id AND n.user_id=d.user_id AND e.id='legacy:'||n.id
 WHERE d.company_id=$1 AND d.user_id=$2
  AND (($9='inbox' AND d.archived_at IS NULL) OR ($9<>'inbox' AND n.id IS NOT NULL))
), candidates AS (
 SELECT * FROM source WHERE source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal')
 AND erpx_field_push_source(source_type,source_id,company_id) AND ($3 OR read_at IS NULL)
 AND ($4::text IS NULL OR category=$4) AND preview ILIKE '%'||$5||'%'
)
SELECT * FROM candidates c WHERE $6::bigint IS NULL
 OR ($9='inbox' AND (c.delivered_at,c.id)<($6,$7))
 OR ($9='staff-list' AND (c.delivered_at<$6 OR (c.delivered_at=$6 AND c.id>$7)))
 OR ($9='staff-page' AND ((NOT $10 AND (c.delivered_at::text,c.id)<($11,$7)) OR ($10 AND (c.delivered_at::text,c.id)>($11,$7))))
ORDER BY CASE WHEN $9='staff-page' AND NOT $10 THEN delivered_at::text END DESC,
 CASE WHEN $9='staff-page' AND $10 THEN delivered_at::text END ASC,
 CASE WHEN $9<>'staff-page' THEN delivered_at END DESC,
 CASE WHEN $9='staff-list' OR $10 THEN id END ASC,
 CASE WHEN $9<>'staff-list' AND NOT $10 THEN id END DESC LIMIT $8
"#).bind(company.as_str()).bind(user.as_str()).bind(query.all).bind(&query.category).bind(&query.search)
 .bind(query.before.as_ref().map(|v|v.at)).bind(query.before.as_ref().map(|v|v.id.as_str())).bind(query.limit as i64)
 .bind(query.projection.as_str()).bind(query.reverse).bind(query.before.as_ref().map(|v|v.key.clone().unwrap_or_else(||v.at.to_string())))
 .fetch_all(c).await?;
    let mut result = Vec::with_capacity(rows.len());
    for r in rows {
        let item = FieldInboxCandidate {
            id: r.try_get("id")?,
            event_id: r.try_get("event_id")?,
            company_id: r.try_get("company_id")?,
            user_id: r.try_get("user_id")?,
            delivered_at: r.try_get("delivered_at")?,
            read_at: r.try_get("read_at")?,
            archived_at: r.try_get("archived_at")?,
            source_type: r.try_get("source_type")?,
            source_id: r.try_get("source_id")?,
            source_version: r.try_get("source_version")?,
            category: r.try_get("category")?,
            preview: r.try_get("preview")?,
        };
        item.validate()?;
        result.push(item);
    }
    Ok(result)
}

fn changed() -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "INBOX_SOURCE_CHANGED",
        "Refresh the Inbox and retry.",
    )
    .into()
}
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("INBOX_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn delivery_image(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &UserId,
    id: &str,
) -> Result<Option<String>, Error> {
    let image:Option<String>=sqlx::query_scalar("SELECT jsonb_build_object('delivery',to_jsonb(d),'event',to_jsonb(e),'staff',(SELECT to_jsonb(n) FROM staff_notifications n WHERE n.id=d.id AND n.company_id=d.company_id AND n.user_id=d.user_id AND e.id='legacy:'||n.id))::text FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id WHERE d.id=$1 AND d.company_id=$2 AND d.user_id=$3 AND e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id)").bind(id).bind(company.as_str()).bind(user.as_str()).fetch_optional(c).await?;
    if image.as_ref().is_some_and(|v| v.len() > 2 * 1024 * 1024) {
        return Err(changed());
    }
    Ok(image)
}
/// Opaque exact row witness bound to one SERIALIZABLE transaction.
pub struct CurrentFieldInboxDelivery {
    unit: Unit,
    company: CompanyId,
    user: UserId,
    id: String,
    image: String,
    candidate: FieldInboxCandidate,
}
impl CurrentFieldInboxDelivery {
    pub fn candidate(&self) -> &FieldInboxCandidate {
        &self.candidate
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || delivery_image(c, &self.company, &self.user, &self.id)
                .await?
                .as_deref()
                != Some(self.image.as_str())
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_inbox_delivery(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &UserId,
    id: &str,
) -> Result<CurrentFieldInboxDelivery, Error> {
    if id.is_empty() || id.len() > 512 {
        return Err(BusinessError::invalid("INVALID_INBOX_STATE", "Choose an Inbox item.").into());
    }
    let token = unit(c).await?;
    let image = delivery_image(c, company, user, id).await?.ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "INBOX_ITEM_NOT_FOUND",
            "This update is unavailable.",
        )
    })?;
    let v: serde_json::Value = serde_json::from_str(&image).map_err(|_| changed())?;
    let mut value = v["delivery"].clone();
    for key in [
        "source_type",
        "source_id",
        "source_version",
        "category",
        "preview",
    ] {
        value[key] = v["event"][key].clone();
    }
    let candidate = FieldInboxCandidate {
        id: id.into(),
        event_id: value["event_id"].as_str().ok_or_else(changed)?.into(),
        company_id: company.as_str().into(),
        user_id: user.as_str().into(),
        delivered_at: value["delivered_at"].as_i64().ok_or_else(changed)?,
        read_at: optional_time(&value, "read_at")?,
        archived_at: optional_time(&value, "archived_at")?,
        source_type: value["source_type"].as_str().ok_or_else(changed)?.into(),
        source_id: value["source_id"].as_str().ok_or_else(changed)?.into(),
        source_version: value["source_version"].as_i64().ok_or_else(changed)?,
        category: value["category"].as_str().ok_or_else(changed)?.into(),
        preview: value["preview"].as_str().ok_or_else(changed)?.into(),
    };
    candidate.validate()?;
    Ok(CurrentFieldInboxDelivery {
        unit: token,
        company: company.clone(),
        user: user.clone(),
        id: id.into(),
        image,
        candidate,
    })
}
fn optional_time(v: &serde_json::Value, key: &str) -> Result<Option<i64>, Error> {
    match &v[key] {
        serde_json::Value::Null => Ok(None),
        v => v.as_i64().map(Some).ok_or_else(changed),
    }
}
pub async fn apply_field_inbox_state(
    c: &mut PgConnection,
    before: &CurrentFieldInboxDelivery,
    state: &FieldInboxState,
) -> Result<CurrentFieldInboxDelivery, Error> {
    before.require_current(c).await?;
    if state.id != before.id {
        return Err(changed());
    }
    let old: serde_json::Value = serde_json::from_str(&before.image).map_err(|_| changed())?;
    let mut expected = old.clone();
    expected["delivery"]["read_at"] = serde_json::json!(state.read_at);
    expected["delivery"]["archived_at"] = serde_json::json!(state.archived_at);
    if !old["staff"].is_null() {
        expected["staff"]["read_at"] = serde_json::json!(state.read_at);
    }
    let marker: Option<String> =
        sqlx::query_scalar("SELECT current_setting('erpx.inbox_state_runtime',true)")
            .fetch_one(&mut *c)
            .await?;
    sqlx::query("SELECT set_config('erpx.inbox_state_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    let changed_count=sqlx::query("UPDATE inbox_deliveries SET read_at=$1,archived_at=$2 WHERE id=$3 AND company_id=$4 AND user_id=$5").bind(state.read_at).bind(state.archived_at).bind(&before.id).bind(before.company.as_str()).bind(before.user.as_str()).execute(&mut *c).await?.rows_affected();
    if changed_count != 1 {
        return Err(changed());
    }
    if !old["staff"].is_null() {
        let n=sqlx::query("UPDATE staff_notifications SET read_at=$1 WHERE id=$2 AND company_id=$3 AND user_id=$4 AND record_type=$5").bind(state.read_at).bind(&before.id).bind(before.company.as_str()).bind(before.user.as_str())
            .bind(&before.candidate.source_type).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
    }
    sqlx::query("SELECT set_config('erpx.inbox_state_runtime',$1,true)")
        .bind(marker.unwrap_or_default())
        .execute(&mut *c)
        .await?;
    let after = read_field_inbox_delivery(c, &before.company, &before.user, &before.id).await?;
    let observed: serde_json::Value = serde_json::from_str(&after.image).map_err(|_| changed())?;
    if observed != expected {
        return Err(changed());
    }
    Ok(after)
}
