//! Current Field visit link status under a same-unit owner authority witness.
use crate::public::field_visit_intake::{FieldIntakeAuthority, prepare_field_intake};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
const STATUS_SQL: &str = "SELECT CASE WHEN octet_length(image::text)<=65536 THEN image END FROM (SELECT jsonb_build_object('id',l.id,'setId',l.set_id,'creator',l.created_by,'linkState',l.state,'expiresAt',l.expires_at,'boundAt',l.bound_at,'withdrawnAt',l.withdrawn_at,'targetType',s.target_type,'targetId',s.target_id,'componentKind',s.component_kind,'purposeKey',s.purpose_key,'expectedVersion',s.expected_target_version,'sourceId',a.id,'sourceState',a.source_state,'previewAvailable',a.preview_object_key IS NOT NULL,'attempts',COALESCE(j.attempt,0),'lastResult',j.state,'lastError',j.error_code,'signature',j.scanner_signature,'definitionUpdatedAt',j.scanner_definition_at,'scannedAt',j.completed_at) image FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN LATERAL (SELECT attempt,state,error_code,scanner_signature,scanner_definition_at,completed_at FROM attachment_scan_attempts WHERE attachment_id=a.id ORDER BY attempt DESC LIMIT 1) j ON true WHERE l.id=$1 AND s.company_id=$2 AND (l.attachment_id IS NULL OR a.id IS NOT NULL)) bounded";
type Error = PgTransactionError<BusinessError>;
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
    .into()
}
/// Caller supplies an authorized actor and a SERIALIZABLE unit, never identity hints.
pub async fn read_field_link_status<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    link_id: &str,
    read_only: bool,
    now: i64,
) -> Result<serde_json::Value, Error> {
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    let raw: Option<serde_json::Value> = sqlx::query_scalar(STATUS_SQL)
        .bind(link_id)
        .bind(company)
        .fetch_optional(&mut *c)
        .await?
        .flatten();
    let row = raw.ok_or_else(missing)?;
    let text = |key: &str| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(changed)
    };
    let target = text("targetId")?;
    let target_type = text("targetType")?;
    let kind = text("componentKind")?;
    let purpose = text("purposeKey")?;
    let state = text("linkState")?;
    let creator = text("creator")?;
    if !crate::domain::field_visit_link::valid_field_purpose(target_type, kind, purpose)
        || (read_only && (state != "bound" || row["sourceState"] != "available"))
        || (state == "draft" && creator != user)
    {
        return Err(missing());
    }
    let guard = prepare_field_intake(c, authority, actor, target, false, now).await?;
    let expected = row["expectedVersion"].as_i64().ok_or_else(changed)?;
    let event_id = row["sourceId"]
        .as_str()
        .map(|_| format!("attachment-access-{}", uuid::Uuid::new_v4()));
    if let Some(source) = row["sourceId"].as_str() {
        sqlx::query("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,'status','allowed',$6)").bind(event_id.as_deref().ok_or_else(changed)?).bind(company).bind(source).bind(link_id).bind(user).bind(now).execute(&mut *c).await?;
    }
    guard.require_current(c, now).await?;
    let after: Option<serde_json::Value> = sqlx::query_scalar(STATUS_SQL)
        .bind(link_id)
        .bind(company)
        .fetch_optional(&mut *c)
        .await?
        .flatten();
    if after.as_ref() != Some(&row) {
        return Err(changed());
    }
    if let (Some(event), Some(source)) = (event_id.as_deref(), row["sourceId"].as_str()) {
        let evidence:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_access_events WHERE id=$1 AND company_id=$2 AND attachment_id=$3 AND link_id=$4 AND actor_id=$5 AND variant='status' AND outcome='allowed' AND created_at=$6)").bind(event).bind(company).bind(source).bind(link_id).bind(user).bind(now).fetch_one(&mut *c).await?;
        if !evidence {
            return Err(changed());
        }
    }
    Ok(
        serde_json::json!({"linkId":row["id"],"setId":row["setId"],"sourceId":row["sourceId"],"sourceState":row["sourceState"],"linkState":row["linkState"],"scan":{"attempts":row["attempts"],"lastResult":row["lastResult"],"lastError":row["lastError"],"signature":row["signature"],"definitionUpdatedAt":row["definitionUpdatedAt"],"scannedAt":row["scannedAt"]},"previewAvailable":row["previewAvailable"],"bindable":row["sourceState"]=="available"&&state=="draft"&&guard.scope().version==expected,"expiresAt":row["expiresAt"],"boundAt":row["boundAt"],"withdrawnAt":row["withdrawnAt"]}),
    )
}
