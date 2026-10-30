//! Transaction-bound Field upload completion. Byte publication must have
//! completed outside this unit and minted the opaque Files store receipt.
use crate::adapters::local_field_quarantine_writer::FieldQuarantineReceipt;
use crate::public::field_visit_intake::{FieldIntakeAuthority, prepare_field_intake};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
fn problem(kind: FailureKind, code: &'static str, message: &'static str) -> Error {
    BusinessError::new(kind, code, message).into()
}
fn conflict() -> Error {
    problem(
        FailureKind::Conflict,
        "UPLOAD_CONFLICT",
        "This upload already contains different bytes or changed evidence.",
    )
}
fn changed() -> Error {
    problem(
        FailureKind::Conflict,
        "ATTACHMENT_TARGET_CHANGED",
        "The record changed after this file reservation. Create a file set for its current version.",
    )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldUploadCompletion {
    pub upload_intent_id: String,
    pub attachment_link_id: String,
    pub source_id: String,
    pub source_state: String,
    pub replay: bool,
}
async fn verify_closed_pin(
    c: &mut PgConnection,
    pin: &FieldPublicationPin,
    closed_at: i64,
) -> Result<(), Error> {
    let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE token=$1 AND company_id=$2 AND upload_intent_id=$3 AND object_key=$4 AND checksum_sha256=$5 AND bytes=$6 AND expires_at=$7 AND state='committed' AND closed_at=$8)").bind(pin.token).bind(&pin.company).bind(&pin.upload_id).bind(&pin.object_key).bind(&pin.checksum).bind(pin.bytes).bind(pin.expires_at).bind(closed_at).fetch_one(&mut *c).await?;
    if !exact {
        return Err(conflict());
    }
    Ok(())
}
/// The caller owns an existing SERIALIZABLE unit and prior owner-source lock
/// ordering. The store receipt carries actual immutable bytes, not a client claim.
pub async fn complete_field_upload<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    upload_id: &str,
    receipt: &FieldQuarantineReceipt,
    pin: &FieldPublicationPin,
    clock: &(dyn erpx_server_shared::public::Clock + Sync),
) -> Result<FieldUploadCompletion, Error> {
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-upload:{company}:{upload_id}"))
        .execute(&mut *c)
        .await?;
    let row=sqlx::query("SELECT u.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,s.state set_state,l.state link_state,l.created_by link_creator,l.upload_intent_id,l.attachment_id linked_source,CASE WHEN octet_length(to_jsonb(u)::text)+octet_length(to_jsonb(s)::text)+octet_length(to_jsonb(l)::text)<=2097152 THEN to_jsonb(s) END set_image,CASE WHEN octet_length(to_jsonb(u)::text)+octet_length(to_jsonb(s)::text)+octet_length(to_jsonb(l)::text)<=2097152 THEN to_jsonb(l)-'attachment_id' END link_image,CASE WHEN octet_length(to_jsonb(u)::text)+octet_length(to_jsonb(s)::text)+octet_length(to_jsonb(l)::text)<=2097152 THEN to_jsonb(u)-ARRAY['status','actual_bytes','checksum_sha256','detected_media_type','final_key','updated_at'] END upload_image FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3 FOR UPDATE OF u,s,l").bind(upload_id).bind(company).bind(user).fetch_optional(&mut *c).await?.ok_or_else(||problem(FailureKind::NotFound,"UPLOAD_NOT_FOUND","Upload intent not found or expired."))?;
    let target_type: String = row.try_get("target_type")?;
    let target: String = row.try_get("target_id")?;
    let component: String = row.try_get("component_kind")?;
    let purpose: String = row.try_get("purpose_key")?;
    if !crate::domain::field_visit_link::valid_field_purpose(&target_type, &component, &purpose) {
        return Err(conflict());
    }
    let status: String = row.try_get("status")?;
    let replay = matches!(status.as_str(), "uploaded" | "ready");
    let guard =
        prepare_field_intake(c, authority, actor, &target, !replay, clock.now().get()).await?;
    let link: String = row.try_get("attachment_link_id")?;
    let set: String = row.try_get("attachment_set_id")?;
    let object_key: String = row
        .try_get::<Option<String>, _>("quarantine_key")?
        .unwrap_or(row.try_get("staging_key")?);
    let media: String = row.try_get("media_type")?;
    let declared: i64 = row.try_get("declared_bytes")?;
    let pubn = &receipt.publication;
    if pin.company != company
        || pin.upload_id != upload_id
        || pin.object_key != object_key
        || pin.checksum != pubn.checksum_sha256
        || pin.bytes != declared
    {
        return Err(conflict());
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-object:{company}:{object_key}"))
        .execute(&mut *c)
        .await?;
    let now = clock.now().get();
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE token=$1 AND company_id=$2 AND upload_intent_id=$3 AND object_key=$4 AND checksum_sha256=$5 AND bytes=$6 AND expires_at>$7 AND expires_at=$8 AND state='active' FOR UPDATE)").bind(pin.token).bind(company).bind(upload_id).bind(&object_key).bind(&pin.checksum).bind(pin.bytes).bind(now).bind(pin.expires_at).fetch_one(&mut *c).await?;
    if !live {
        return Err(conflict());
    }
    if pubn.key != object_key || pubn.media_type != media || pubn.bytes.len() as i64 != declared {
        return Err(conflict());
    }
    if row.try_get::<String, _>("owner_type")? != target_type
        || row.try_get::<String, _>("owner_id")? != target
        || row.try_get::<String, _>("purpose")? != purpose
        || row.try_get::<String, _>("upload_intent_id")? != upload_id
        || row.try_get::<String, _>("link_creator")? != user
    {
        return Err(conflict());
    }
    let source = format!("attachment-{upload_id}");
    if replay {
        let stored: Option<(String, String)> = sqlx::query_as(
            "SELECT checksum_sha256,source_state FROM attachments WHERE id=$1 AND company_id=$2",
        )
        .bind(&source)
        .bind(company)
        .fetch_optional(&mut *c)
        .await?;
        let (checksum, source_state) = stored.ok_or_else(conflict)?;
        if checksum != pubn.checksum_sha256 {
            return Err(conflict());
        }
        let close_at = clock.now().get();
        guard.require_current(c, close_at).await?;
        let closed=sqlx::query("UPDATE field_upload_publication_pins SET state='committed',closed_at=$2 WHERE token=$1 AND state='active' AND expires_at>$2").bind(pin.token).bind(close_at).execute(&mut *c).await?.rows_affected();
        if closed != 1 {
            return Err(conflict());
        }
        verify_closed_pin(c, pin, close_at).await?;
        guard.require_current(c, clock.now().get()).await?;
        return Ok(FieldUploadCompletion {
            upload_intent_id: upload_id.into(),
            attachment_link_id: link,
            source_id: source,
            source_state,
            replay: true,
        });
    }
    if status != "pending" || row.try_get::<i64, _>("expires_at")? <= now {
        return Err(problem(
            FailureKind::NotFound,
            "UPLOAD_NOT_FOUND",
            "Upload intent not found or expired.",
        ));
    }
    if row.try_get::<String, _>("set_state")? != "draft"
        || row.try_get::<String, _>("link_state")? != "draft"
        || row.try_get::<Option<String>, _>("linked_source")?.is_some()
        || row.try_get::<i64, _>("owner_version")? != guard.scope().version
        || row.try_get::<i64, _>("expected_target_version")? != guard.scope().version
    {
        return Err(changed());
    }
    let set_image: serde_json::Value = row
        .try_get::<Option<serde_json::Value>, _>("set_image")?
        .ok_or_else(changed)?;
    let link_image: serde_json::Value = row
        .try_get::<Option<serde_json::Value>, _>("link_image")?
        .ok_or_else(changed)?;
    let upload_image: serde_json::Value = row
        .try_get::<Option<serde_json::Value>, _>("upload_image")?
        .ok_or_else(changed)?;
    let revision: i64 = row.try_get("revision")?;
    let filename: String = row.try_get("original_name")?;
    let bytes = pubn.bytes.len() as i64;
    let checksum = &pubn.checksum_sha256;
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$9,'quarantined')").bind(&source).bind(upload_id).bind(&target_type).bind(&target).bind(&purpose).bind(&object_key).bind(revision).bind(filename).bind(&media).bind(bytes).bind(checksum).bind(user).bind(now).bind(company).execute(&mut *c).await?;
    let updated=sqlx::query("UPDATE upload_intents SET status='uploaded',actual_bytes=$1,checksum_sha256=$2,detected_media_type=$3,final_key=$4,updated_at=$5 WHERE id=$6 AND company_id=$7 AND status='pending'").bind(bytes).bind(checksum).bind(&media).bind(&object_key).bind(now).bind(upload_id).bind(company).execute(&mut *c).await?.rows_affected();
    if updated != 1 {
        return Err(conflict());
    }
    let linked=sqlx::query("UPDATE attachment_links SET attachment_id=$1 WHERE id=$2 AND set_id=$3 AND state='draft' AND attachment_id IS NULL").bind(&source).bind(&link).bind(&set).execute(&mut *c).await?.rows_affected();
    if linked != 1 {
        return Err(conflict());
    }
    let scan_id = format!("attachment-scan-{}", uuid::Uuid::new_v4());
    let ledger_id = format!("attachment-storage-{}", uuid::Uuid::new_v4());
    let event_id = format!("attachment-event-{}", uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3)").bind(&scan_id).bind(&source).bind(now).execute(&mut *c).await?;
    sqlx::query("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES($1,$2,$3,'original',$4,$5,'stored',$6)").bind(&ledger_id).bind(company).bind(&source).bind(&object_key).bind(bytes).bind(now).execute(&mut *c).await?;
    let details = serde_json::json!({"checksum":checksum,"bytes":bytes,"detectedMediaType":media})
        .to_string()
        .replace('<', "\\u003c");
    sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,'file.quarantined',$7,$8)").bind(&event_id).bind(company).bind(&source).bind(&link).bind(&set).bind(user).bind(details).bind(now).execute(&mut *c).await?;
    guard.require_current(c, now).await?;
    let exact: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE u.id=$1 AND u.company_id=$2 AND u.status='uploaded' AND u.actual_bytes=$3 AND u.checksum_sha256=$4 AND u.detected_media_type=$5 AND u.final_key=$6 AND l.id=$7 AND l.state='draft' AND l.set_id=$8 AND l.upload_intent_id=u.id AND a.id=$9 AND a.upload_id=u.id AND a.company_id=$2 AND a.object_key=$6 AND a.bytes=$3 AND a.checksum_sha256=$4 AND a.detected_media_type=$5 AND a.source_state='quarantined' AND s.state='draft' AND s.target_id=$10 AND s.expected_target_version=$11 AND to_jsonb(s)=$16 AND to_jsonb(l)-'attachment_id'=$17 AND to_jsonb(u)-ARRAY['status','actual_bytes','checksum_sha256','detected_media_type','final_key','updated_at']=$18) AND (SELECT count(*) FROM attachment_scan_attempts WHERE attachment_id=$9)=1 AND EXISTS(SELECT 1 FROM attachment_scan_attempts WHERE id=$12 AND attachment_id=$9 AND attempt=1 AND state='pending') AND (SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=$9 AND variant='original')=1 AND EXISTS(SELECT 1 FROM attachment_storage_ledger WHERE id=$13 AND company_id=$2 AND attachment_id=$9 AND variant='original' AND object_key=$6 AND bytes=$3 AND state='stored') AND EXISTS(SELECT 1 FROM attachment_events WHERE id=$14 AND company_id=$2 AND attachment_id=$9 AND link_id=$7 AND set_id=$8 AND actor_id=$15 AND action='file.quarantined' AND details_json::jsonb=jsonb_build_object('checksum',$4::text,'bytes',$3::bigint,'detectedMediaType',$5::text))")
        .bind(upload_id).bind(company).bind(bytes).bind(checksum).bind(&media).bind(&object_key).bind(&link).bind(&set).bind(&source).bind(&target).bind(guard.scope().version).bind(&scan_id).bind(&ledger_id).bind(&event_id).bind(user).bind(&set_image).bind(&link_image).bind(&upload_image).fetch_one(&mut *c).await?;
    if !exact {
        return Err(conflict());
    }
    let close_at = clock.now().get();
    guard.require_current(c, close_at).await?;
    let closed=sqlx::query("UPDATE field_upload_publication_pins SET state='committed',closed_at=$2 WHERE token=$1 AND state='active' AND expires_at>$2").bind(pin.token).bind(close_at).execute(&mut *c).await?.rows_affected();
    if closed != 1 {
        return Err(conflict());
    }
    verify_closed_pin(c, pin, close_at).await?;
    guard.require_current(c, clock.now().get()).await?;
    Ok(FieldUploadCompletion {
        upload_intent_id: upload_id.into(),
        attachment_link_id: link,
        source_id: source,
        source_state: "quarantined".into(),
        replay: false,
    })
}

/// Result of the first short owner unit. Replay is returned without private I/O;
/// a pending upload carries a Files-owned publication request.
pub enum FieldUploadPreparation {
    Replay(FieldUploadCompletion),
    Pending {
        publication: crate::adapters::local_field_quarantine_writer::FieldQuarantinePublication,
        pin: FieldPublicationPin,
    },
}
/// Opaque token bound to one authorized publication; never deserializable.
#[derive(Clone)]
pub struct FieldPublicationPin {
    token: uuid::Uuid,
    company: String,
    upload_id: String,
    object_key: String,
    checksum: String,
    bytes: i64,
    expires_at: i64,
}
/// Resolve original Field lineage and current authority before publishing bytes.
/// Callers must not hold this transaction open during filesystem I/O.
pub async fn prepare_field_upload<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    upload_id: &str,
    request_media_type: &str,
    bytes: Vec<u8>,
    now: i64,
) -> Result<FieldUploadPreparation, Error> {
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    let row = sqlx::query("SELECT u.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,s.state set_state,l.state link_state,l.created_by link_creator,l.upload_intent_id,l.attachment_id linked_source FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=u.company_id JOIN attachment_links l ON l.id=u.attachment_link_id AND l.set_id=s.id WHERE u.id=$1 AND u.company_id=$2 AND u.created_by=$3").bind(upload_id).bind(company).bind(user).fetch_optional(&mut *c).await?.ok_or_else(||problem(FailureKind::NotFound,"UPLOAD_NOT_FOUND","Upload intent not found or expired."))?;
    let target_type: String = row.try_get("target_type")?;
    let target: String = row.try_get("target_id")?;
    let component: String = row.try_get("component_kind")?;
    let purpose: String = row.try_get("purpose_key")?;
    if !crate::domain::field_visit_link::valid_field_purpose(&target_type, &component, &purpose) {
        return Err(conflict());
    }
    let status: String = row.try_get("status")?;
    let replay = matches!(status.as_str(), "uploaded" | "ready");
    let guard = prepare_field_intake(c, authority, actor, &target, !replay, now).await?;
    let media: String = row.try_get("media_type")?;
    if request_media_type != media {
        return Err(BusinessError::invalid("UPLOAD_TYPE", "The file contents do not match the selected type.").into());
    }
    let declared: i64 = row.try_get("declared_bytes")?;
    let checksum =
        crate::domain::field_visit_upload::validate_field_upload_bytes(&media, declared, &bytes)?;
    let link: String = row.try_get("attachment_link_id")?;
    if row.try_get::<String, _>("owner_type")? != target_type
        || row.try_get::<String, _>("owner_id")? != target
        || row.try_get::<String, _>("purpose")? != purpose
        || row.try_get::<String, _>("upload_intent_id")? != upload_id
        || row.try_get::<String, _>("link_creator")? != user
    {
        return Err(conflict());
    }
    let source = format!("attachment-{upload_id}");
    if replay {
        let stored: Option<(String, String)> = sqlx::query_as(
            "SELECT checksum_sha256,source_state FROM attachments WHERE id=$1 AND company_id=$2",
        )
        .bind(&source)
        .bind(company)
        .fetch_optional(&mut *c)
        .await?;
        let (stored_checksum, source_state) = stored.ok_or_else(conflict)?;
        if stored_checksum != checksum {
            return Err(conflict());
        }
        guard.require_current(c, now).await?;
        return Ok(FieldUploadPreparation::Replay(FieldUploadCompletion {
            upload_intent_id: upload_id.into(),
            attachment_link_id: link,
            source_id: source,
            source_state,
            replay: true,
        }));
    }
    if status != "pending" || row.try_get::<i64, _>("expires_at")? <= now {
        return Err(problem(
            FailureKind::NotFound,
            "UPLOAD_NOT_FOUND",
            "Upload intent not found or expired.",
        ));
    }
    if row.try_get::<String, _>("set_state")? != "draft"
        || row.try_get::<String, _>("link_state")? != "draft"
        || row.try_get::<Option<String>, _>("linked_source")?.is_some()
        || row.try_get::<i64, _>("owner_version")? != guard.scope().version
        || row.try_get::<i64, _>("expected_target_version")? != guard.scope().version
    {
        return Err(changed());
    }
    let key: String = row
        .try_get::<Option<String>, _>("quarantine_key")?
        .unwrap_or(row.try_get("staging_key")?);
    guard.require_current(c, now).await?;
    let expires_at = now
        .checked_add(60_000)
        .ok_or_else(changed)?
        .min(row.try_get("expires_at")?);
    if expires_at <= now {
        return Err(changed());
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("attachment-object:{company}:{key}"))
        .execute(&mut *c)
        .await?;
    // Expiry never permits publication while an old cleanup worker may still
    // be deleting. Only an object-locked reconciler can clear this claim.
    let cleaning: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_object_cleanup_claims WHERE company_id=$1 AND object_key=$2)").bind(company).bind(&key).fetch_one(&mut *c).await?;
    if cleaning {
        return Err(problem(
            FailureKind::Unavailable,
            "ATTACHMENT_CLEANUP_BUSY",
            "File intake is briefly paused for storage reconciliation.",
        ));
    }
    let token = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO field_upload_publication_pins(token,company_id,upload_intent_id,object_key,checksum_sha256,bytes,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(token).bind(company).bind(upload_id).bind(&key).bind(&checksum).bind(declared).bind(now).bind(expires_at).execute(&mut *c).await?;
    guard.require_current(c, now).await?;
    Ok(FieldUploadPreparation::Pending {
        pin: FieldPublicationPin {
            token,
            company: company.into(),
            upload_id: upload_id.into(),
            object_key: key.clone(),
            checksum: checksum.clone(),
            bytes: declared,
            expires_at,
        },
        publication: crate::adapters::local_field_quarantine_writer::FieldQuarantinePublication {
            key,
            bytes,
            media_type: media,
            checksum_sha256: checksum,
            uploaded_at: now,
        },
    })
}

/// Acquire the physical object lock first, then prove the pin and absence of
/// a cleanup claim in a fresh short Holding unit before publication. A stale
/// publisher cannot run after a cleanup lease expires and is reclaimed.
pub async fn publish_pinned_field_upload(
    writer: &crate::adapters::local_field_quarantine_writer::LocalFieldQuarantineWriter,
    pool: &sqlx::PgPool,
    pin: &FieldPublicationPin,
    publication: crate::adapters::local_field_quarantine_writer::FieldQuarantinePublication,
    clock: &(dyn erpx_server_shared::public::Clock + Sync),
) -> Result<FieldQuarantineReceipt, Error> {
    if publication.key != pin.object_key
        || publication.checksum_sha256 != pin.checksum
        || publication.bytes.len() as i64 != pin.bytes
    {
        return Err(conflict());
    }
    let object_lock = writer.lock_object(&pin.object_key).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "attachment-object:{}:{}",
            pin.company, pin.object_key
        ))
        .execute(&mut *tx)
        .await?;
    let now = clock.now().get();
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE token=$1 AND company_id=$2 AND upload_intent_id=$3 AND object_key=$4 AND checksum_sha256=$5 AND bytes=$6 AND expires_at>$7 AND expires_at=$8 AND state='active') AND NOT EXISTS(SELECT 1 FROM field_upload_object_cleanup_claims WHERE company_id=$2 AND object_key=$4)").bind(pin.token).bind(&pin.company).bind(&pin.upload_id).bind(&pin.object_key).bind(&pin.checksum).bind(pin.bytes).bind(now).bind(pin.expires_at).fetch_one(&mut *tx).await?;
    if !live {
        return Err(conflict());
    }
    tx.commit().await?;
    Ok(writer.publish_locked(publication, object_lock).await?)
}

/// Close only the caller's opaque pin. This never deletes bytes; cleanup must
/// separately acquire the same object lock and establish no live pins or source.
pub async fn release_field_publication_pin(
    c: &mut PgConnection,
    pin: &FieldPublicationPin,
    now: i64,
) -> Result<(), Error> {
    use sqlx::Connection;
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_SERIALIZABLE_REQUIRED",
            )
            .into(),
        );
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "attachment-object:{}:{}",
            pin.company, pin.object_key
        ))
        .execute(&mut *c)
        .await?;
    let changed=sqlx::query("UPDATE field_upload_publication_pins SET state='released',closed_at=$2 WHERE token=$1 AND company_id=$3 AND upload_intent_id=$4 AND object_key=$5 AND checksum_sha256=$6 AND bytes=$7 AND state='active'").bind(pin.token).bind(now).bind(&pin.company).bind(&pin.upload_id).bind(&pin.object_key).bind(&pin.checksum).bind(pin.bytes).execute(&mut *c).await?.rows_affected();
    if changed != 1 {
        return Err(conflict());
    }
    Ok(())
}

/// Coordinator must call this at the end of the same SERIALIZABLE owner unit,
/// after its Workflow/audit effects, immediately before commit.
pub async fn verify_field_completion_pin(
    c: &mut PgConnection,
    pin: &FieldPublicationPin,
    clock: &(dyn erpx_server_shared::public::Clock + Sync),
) -> Result<(), Error> {
    use sqlx::Connection;
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "FILES_SERIALIZABLE_REQUIRED",
            )
            .into(),
        );
    }
    let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM field_upload_publication_pins WHERE token=$1 AND company_id=$2 AND upload_intent_id=$3 AND object_key=$4 AND checksum_sha256=$5 AND bytes=$6 AND expires_at=$7 AND expires_at>$8 AND state='committed' AND closed_at IS NOT NULL AND closed_at<expires_at)").bind(pin.token).bind(&pin.company).bind(&pin.upload_id).bind(&pin.object_key).bind(&pin.checksum).bind(pin.bytes).bind(pin.expires_at).bind(clock.now().get()).fetch_one(&mut *c).await?;
    if !exact {
        return Err(conflict());
    }
    Ok(())
}
