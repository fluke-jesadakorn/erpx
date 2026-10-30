//! Same-unit Field visit reservation planning and Files-owned effects.
use crate::public::{
    field_visit_intake::{FieldIntakeAuthority, prepare_field_intake},
    field_visit_upload::{
        FIELD_RESERVATION_MILLIS, FieldFileReservation, NormalizedFieldFileReservation,
        field_reservation_replay_matches, field_upload_id, field_upload_link_id,
        normalize_fresh_field_file,
    },
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;
type RecordedReservation = (
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    i64,
);
/// Trusted ManageERPX composition port; this uses the same database transaction.
pub trait FieldStorageQuota {
    fn storage_limit<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company: &'a str,
    ) -> PgWorkFuture<'a, Option<i64>, BusinessError>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldReservationResult {
    pub upload_intent_id: String,
    pub attachment_link_id: String,
    pub expires_at: i64,
    pub replay: bool,
}
fn problem(kind: FailureKind, code: &'static str, message: &'static str) -> Error {
    BusinessError::new(kind, code, message).into()
}
fn conflict() -> Error {
    problem(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key was already used for another file reservation.",
    )
}
fn changed() -> Error {
    problem(
        FailureKind::Conflict,
        "ATTACHMENT_TARGET_CHANGED",
        "The record changed. Create a new file set for its current version.",
    )
}
/// Caller owns the SERIALIZABLE unit and source/receipt lock order. This port
/// never begins or commits a transaction and cannot accept HTTP claims as proof.
#[expect(
    clippy::too_many_arguments,
    reason = "Owner unit and exact authority, scope, input and clock are all required"
)]
pub async fn reserve_field_file<A: FieldIntakeAuthority, Q: FieldStorageQuota>(
    c: &mut PgConnection,
    authority: &A,
    quota: &Q,
    actor: &AuthorizedWorkContext,
    holding: &HoldingId,
    set_id: &str,
    request: &FieldFileReservation,
    key: &str,
    now: i64,
) -> Result<FieldReservationResult, Error> {
    if !(8..=180).contains(&key.len())
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
    {
        return Err(problem(
            FailureKind::InvalidInput,
            "COMMAND_KEY_REQUIRED",
            "A valid Idempotency-Key is required.",
        ));
    }
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    let upload = field_upload_id(company, user, key);
    let link = field_upload_link_id(&upload).ok_or_else(changed)?;
    // The retained lock name and order serialize storage accounting with TS peers.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-reserve:{company}"))
        .execute(&mut *c)
        .await?;
    let set:Option<(String,String,String,String,i64,String,i64)> = sqlx::query_as("SELECT target_type,target_id,purpose_key,component_kind,expected_target_version,state,max_files FROM attachment_sets WHERE id=$1 AND company_id=$2 FOR UPDATE").bind(set_id).bind(company).fetch_optional(&mut *c).await?;
    let (target_type, target_id, purpose, component, expected, state, max_files) =
        set.ok_or_else(|| {
            problem(
                FailureKind::NotFound,
                "ATTACHMENT_SET_NOT_FOUND",
                "File set not found.",
            )
        })?;
    if !crate::domain::field_visit_link::valid_field_purpose(&target_type, &component, &purpose) {
        return Err(problem(
            FailureKind::InvalidInput,
            "ATTACHMENT_POLICY_UNKNOWN",
            "This file purpose is not registered for the selected record and context.",
        ));
    }
    let recorded:Option<RecordedReservation>=sqlx::query_as("SELECT u.attachment_set_id,u.attachment_link_id,u.original_name,u.media_type,u.declared_bytes,l.caption,l.supersedes_link_id,u.expires_at FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3").bind(&upload).bind(company).bind(user).fetch_optional(&mut *c).await?;
    let guard =
        prepare_field_intake(c, authority, actor, &target_id, recorded.is_none(), now).await?;
    if let Some((
        recorded_set,
        recorded_link,
        filename,
        media_type,
        bytes,
        caption,
        supersedes_link_id,
        expires_at,
    )) = recorded
    {
        let stored = NormalizedFieldFileReservation {
            filename,
            media_type,
            bytes,
            caption,
            supersedes_link_id,
        };
        if recorded_set != set_id
            || recorded_link != link
            || !field_reservation_replay_matches(&stored, request)
        {
            return Err(conflict());
        }
        guard.require_current(c, now).await?;
        return Ok(FieldReservationResult {
            upload_intent_id: upload,
            attachment_link_id: link,
            expires_at,
            replay: true,
        });
    }
    if state != "draft" {
        return Err(problem(
            FailureKind::Conflict,
            "ATTACHMENT_SET_LOCKED",
            "This file set is already bound or closed.",
        ));
    }
    if guard.scope().version != expected {
        return Err(changed());
    }
    let input = normalize_fresh_field_file(&purpose, request)?;
    if let Some(id) = input.supersedes_link_id.as_deref() {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=$1 AND s.company_id=$2 AND s.target_type=$3 AND s.target_id=$4 AND s.purpose_key=$5 AND l.state='bound')").bind(id).bind(company).bind(&target_type).bind(&target_id).bind(&purpose).fetch_one(&mut *c).await?;
        if !valid {
            return Err(problem(
                FailureKind::InvalidInput,
                "ATTACHMENT_SUPERSEDES_INVALID",
                "A correction may supersede only bound evidence on the same record and purpose.",
            ));
        }
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attachment_links WHERE set_id=$1 AND state IN ('draft','bound')",
    )
    .bind(set_id)
    .fetch_one(&mut *c)
    .await?;
    if count >= max_files {
        return Err(problem(
            FailureKind::InvalidInput,
            "ATTACHMENT_LIMIT",
            "This file set has reached its file limit.",
        ));
    }
    let used:i64=sqlx::query_scalar("SELECT COALESCE(sum(bytes),0)::bigint FROM attachment_storage_ledger WHERE company_id=$1 AND state IN ('reserved','stored')").bind(company).fetch_one(&mut *c).await?;
    let pending:i64=sqlx::query_scalar("SELECT COALESCE(sum(declared_bytes),0)::bigint FROM upload_intents WHERE company_id=$1 AND status='pending' AND expires_at>$2").bind(company).bind(now).fetch_one(&mut *c).await?;
    let limit = quota
        .storage_limit(c, company)
        .await?
        .unwrap_or(9_007_199_254_740_991);
    if used
        .checked_add(pending)
        .and_then(|v| v.checked_add(input.bytes))
        .is_none_or(|v| v > limit)
    {
        return Err(problem(
            FailureKind::InvalidInput,
            "STORAGE_QUOTA",
            "Company file storage is full.",
        ));
    }
    let expires = now
        .checked_add(FIELD_RESERVATION_MILLIS)
        .ok_or_else(changed)?;
    let object_key = format!("{}/quarantine/{upload}/original", holding.as_str());
    sqlx::query("INSERT INTO attachment_links(id,set_id,caption,state,supersedes_link_id,created_by,created_at,expires_at) VALUES($1,$2,$3,'draft',$4,$5,$6,$7)").bind(&link).bind(set_id).bind(&input.caption).bind(&input.supersedes_link_id).bind(user).bind(now).bind(expires).execute(&mut *c).await?;
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9,$10,$11,$11,$12,$13,$14,$15,$8)").bind(&upload).bind(&target_type).bind(&target_id).bind(&purpose).bind(&input.filename).bind(&input.media_type).bind(input.bytes).bind(&object_key).bind(user).bind(expires).bind(now).bind(expected).bind(set_id).bind(&link).bind(company).execute(&mut *c).await?;
    sqlx::query("UPDATE attachment_links SET upload_intent_id=$1 WHERE id=$2 AND set_id=$3")
        .bind(&upload)
        .bind(&link)
        .bind(set_id)
        .execute(&mut *c)
        .await?;
    let event_id = format!("attachment-event-{}", uuid::Uuid::new_v4());
    let details=serde_json::json!({"filename":input.filename,"mediaType":input.media_type,"bytes":input.bytes}).to_string().replace('<',"\\u003c");
    sqlx::query("INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,'file.reserved',$6,$7)").bind(&event_id).bind(company).bind(&link).bind(set_id).bind(user).bind(details).bind(now).execute(&mut *c).await?;
    guard.require_current(c, now).await?;
    let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id JOIN attachment_sets s ON s.id=l.set_id WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3 AND u.status='pending' AND u.attachment_set_id=$4 AND u.attachment_link_id=$5 AND u.owner_type=$6 AND u.owner_id=$7 AND u.purpose=$8 AND u.owner_version=$9 AND u.original_name=$10 AND u.media_type=$11 AND u.declared_bytes=$12 AND u.quarantine_key=$13 AND u.staging_key=$13 AND u.expires_at=$14 AND l.upload_intent_id=u.id AND l.attachment_id IS NULL AND l.created_by=$3 AND l.state='draft' AND l.caption IS NOT DISTINCT FROM $15 AND l.supersedes_link_id IS NOT DISTINCT FROM $16 AND s.company_id=$2 AND s.state='draft' AND s.expected_target_version=$9) AND (SELECT count(*) FROM attachment_links WHERE set_id=$4 AND state IN ('draft','bound'))=$17 AND EXISTS(SELECT 1 FROM attachment_events WHERE id=$18 AND company_id=$2 AND link_id=$5 AND set_id=$4 AND actor_id=$3 AND action='file.reserved' AND details_json::jsonb=jsonb_build_object('filename',$10::text,'mediaType',$11::text,'bytes',$12::bigint))")
        .bind(&upload).bind(company).bind(user).bind(set_id).bind(&link).bind(&target_type).bind(&target_id).bind(&purpose).bind(expected).bind(&input.filename).bind(&input.media_type).bind(input.bytes).bind(&object_key).bind(expires).bind(&input.caption).bind(&input.supersedes_link_id).bind(count+1).bind(&event_id).fetch_one(&mut *c).await?;
    if !exact {
        return Err(changed());
    }
    Ok(FieldReservationResult {
        upload_intent_id: upload,
        attachment_link_id: link,
        expires_at: expires,
        replay: false,
    })
}
