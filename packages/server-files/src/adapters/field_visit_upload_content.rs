//! Two-phase Field content read: fresh authority, bounded private I/O, then a
//! newly authorized same-source check before bytes may leave the owner boundary.
use crate::application::attachment_scan::{QuarantinedOriginalInput, QuarantinedOriginalStore};
use crate::public::field_visit_intake::{FieldIntakeAuthority, prepare_field_intake};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
const SOURCE_SQL: &str = "SELECT CASE WHEN octet_length(image::text)<=65536 THEN image END FROM (SELECT jsonb_build_object('linkId',l.id,'linkState',l.state,'creator',l.created_by,'setId',s.id,'targetType',s.target_type,'targetId',s.target_id,'componentKind',s.component_kind,'purposeKey',s.purpose_key,'sourceId',a.id,'sourceState',a.source_state,'originalKey',a.object_key,'originalBytes',a.bytes,'originalSha',a.checksum_sha256,'originalType',a.media_type,'filename',a.original_name,'previewKey',a.preview_object_key,'previewBytes',a.preview_bytes,'previewSha',a.preview_checksum_sha256,'previewType',a.preview_media_type) image FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2) bounded";
fn err(kind: FailureKind, code: &'static str, msg: &'static str) -> Error {
    BusinessError::new(kind, code, msg).into()
}
fn missing() -> Error {
    err(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}
fn unavailable() -> Error {
    err(
        FailureKind::Unavailable,
        "FILE_UNAVAILABLE",
        "The private file is temporarily unavailable.",
    )
}
fn changed() -> Error {
    err(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldContentVariant {
    Original,
    Preview,
}
/// Private fields prevent callers from manufacturing an authorized source.
pub struct FieldContentWitness {
    image: serde_json::Value,
    company: String,
    user: String,
    session: String,
    holding: String,
    assignment: String,
    assignment_version: i64,
    selection_revision: i64,
    role_revision: i64,
    principal: String,
    person: Option<String>,
    identity_version: i64,
    hr_version: Option<i64>,
    branch: Option<String>,
    department: Option<String>,
    variant: FieldContentVariant,
    input: QuarantinedOriginalInput,
}
impl Clone for FieldContentWitness {
    fn clone(&self) -> Self {
        Self {
            image: self.image.clone(),
            company: self.company.clone(),
            user: self.user.clone(),
            session: self.session.clone(),
            holding: self.holding.clone(),
            assignment: self.assignment.clone(),
            assignment_version: self.assignment_version,
            selection_revision: self.selection_revision,
            role_revision: self.role_revision,
            principal: self.principal.clone(),
            person: self.person.clone(),
            identity_version: self.identity_version,
            hr_version: self.hr_version,
            branch: self.branch.clone(),
            department: self.department.clone(),
            variant: self.variant,
            input: QuarantinedOriginalInput {
                key: self.input.key.clone(),
                bytes: self.input.bytes,
                checksum_sha256: self.input.checksum_sha256.clone(),
            },
        }
    }
}
#[derive(Clone)]
pub struct FieldContentBytes {
    witness: FieldContentWitness,
    bytes: std::sync::Arc<[u8]>,
}
pub struct FieldContentResponse {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub filename: String,
    pub inline: bool,
}
async fn source(
    c: &mut PgConnection,
    company: &str,
    link: &str,
) -> Result<serde_json::Value, Error> {
    let value: Option<serde_json::Value> = sqlx::query_scalar(SOURCE_SQL)
        .bind(link)
        .bind(company)
        .fetch_optional(c)
        .await?
        .flatten();
    value.ok_or_else(missing)
}
fn validate(
    image: &serde_json::Value,
    user: &str,
    variant: FieldContentVariant,
    read_only: bool,
) -> Result<(String, QuarantinedOriginalInput), Error> {
    let string = |k: &str| {
        image
            .get(k)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(changed)
    };
    let state = string("linkState")?;
    if !crate::domain::field_visit_link::valid_field_purpose(
        string("targetType")?,
        string("componentKind")?,
        string("purposeKey")?,
    ) || matches!(state, "withdrawn" | "expired")
        || (state == "draft" && string("creator")? != user)
        || (read_only && (state != "bound" || image["sourceState"] != "available"))
    {
        return Err(missing());
    }
    if image["sourceState"] != "available" {
        return Err(err(
            FailureKind::Conflict,
            "ATTACHMENT_NOT_AVAILABLE",
            "The file is still quarantined or was rejected.",
        ));
    }
    let (key_field, size_field, sha_field, type_field) = match variant {
        FieldContentVariant::Original => (
            "originalKey",
            "originalBytes",
            "originalSha",
            "originalType",
        ),
        FieldContentVariant::Preview => ("previewKey", "previewBytes", "previewSha", "previewType"),
    };
    if image[key_field].is_null() {
        return Err(err(
            FailureKind::NotFound,
            "ATTACHMENT_PREVIEW_NOT_FOUND",
            "A safe preview is not available.",
        ));
    }
    let key = string(key_field)?.to_owned();
    let bytes = image[size_field]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 10 * 1024 * 1024)
        .ok_or_else(unavailable)? as usize;
    let checksum = string(sha_field)?.to_owned();
    let media = string(type_field)?.to_owned();
    if !crate::domain::attachment_scan::valid_sha(&checksum) {
        return Err(unavailable());
    }
    Ok((
        media,
        QuarantinedOriginalInput {
            key,
            bytes,
            checksum_sha256: checksum,
        },
    ))
}
/// Initial authority check; caller must commit this unit before private I/O.
pub async fn prepare_field_content<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    link: &str,
    variant: FieldContentVariant,
    read_only: bool,
    now: i64,
) -> Result<FieldContentWitness, Error> {
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    let image = source(c, company, link).await?;
    let (_, input) = validate(&image, user, variant, read_only)?;
    let target = image["targetId"].as_str().ok_or_else(changed)?;
    let guard = prepare_field_intake(c, authority, actor, target, false, now).await?;
    guard.require_current(c, now).await?;
    if source(c, company, link).await? != image {
        return Err(changed());
    }
    Ok(FieldContentWitness {
        image,
        company: company.into(),
        user: user.into(),
        session: actor.session_id().as_str().into(),
        holding: actor.holding_id().as_str().into(),
        assignment: actor.selection().assignment().id.as_str().into(),
        assignment_version: actor.selection().assignment().version,
        selection_revision: actor.selection().revision(),
        role_revision: actor.selection().role_revision(),
        principal: actor.principal_id().into(),
        person: actor.person_id().map(|v| v.as_str().into()),
        identity_version: actor.identity_version(),
        hr_version: actor.hr_appointment_version(),
        branch: actor
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map(|v| v.as_str().into()),
        department: actor
            .selection()
            .workspace()
            .department_id
            .as_ref()
            .map(|v| v.as_str().into()),
        variant,
        input,
    })
}
/// Outside a database transaction; the sealed witness supplies the physical key.
pub async fn read_field_content_bytes<S: QuarantinedOriginalStore>(
    store: &S,
    witness: FieldContentWitness,
) -> Result<FieldContentBytes, Error> {
    let bytes = store
        .read(&witness.input)
        .await
        .map_err(|_| unavailable())?;
    if bytes.len() != witness.input.bytes
        || hex::encode(Sha256::digest(&bytes)) != witness.input.checksum_sha256
    {
        return Err(unavailable());
    }
    Ok(FieldContentBytes {
        witness,
        bytes: bytes.into(),
    })
}
/// Re-check current actor, source and policy after I/O before returning bytes.
pub async fn finish_field_content<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    content: FieldContentBytes,
    read_only: bool,
    now: i64,
) -> Result<FieldContentResponse, Error> {
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    if company != content.witness.company
        || user != content.witness.user
        || actor.session_id().as_str() != content.witness.session
        || actor.holding_id().as_str() != content.witness.holding
        || actor.selection().assignment().id.as_str() != content.witness.assignment
        || actor.selection().assignment().version != content.witness.assignment_version
        || actor.selection().revision() != content.witness.selection_revision
        || actor.selection().role_revision() != content.witness.role_revision
        || actor.principal_id() != content.witness.principal
        || actor.person_id().map(|v| v.as_str()) != content.witness.person.as_deref()
        || actor.identity_version() != content.witness.identity_version
        || actor.hr_appointment_version() != content.witness.hr_version
        || actor
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map(|v| v.as_str())
            != content.witness.branch.as_deref()
        || actor
            .selection()
            .workspace()
            .department_id
            .as_ref()
            .map(|v| v.as_str())
            != content.witness.department.as_deref()
    {
        return Err(changed());
    }
    let link = content.witness.image["linkId"]
        .as_str()
        .ok_or_else(changed)?;
    let image = source(c, company, link).await?;
    if image != content.witness.image {
        return Err(changed());
    }
    let (media, _) = validate(&image, user, content.witness.variant, read_only)?;
    let target = image["targetId"].as_str().ok_or_else(changed)?;
    let guard = prepare_field_intake(c, authority, actor, target, false, now).await?;
    let source_id = image["sourceId"].as_str().ok_or_else(changed)?;
    let variant = match content.witness.variant {
        FieldContentVariant::Original => "original",
        FieldContentVariant::Preview => "preview",
    };
    let event_id = format!("attachment-access-{}", uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,$6,'allowed',$7)").bind(&event_id).bind(company).bind(source_id).bind(link).bind(user).bind(variant).bind(now).execute(&mut *c).await?;
    // Deferred effects of the access audit must run before the last source and
    // authority checks. The outer transaction's IAM check cannot validate this
    // Field content witness after a deferred source mutation.
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    guard.require_current(c, now).await?;
    if source(c, company, link).await? != image {
        return Err(changed());
    }
    let evidence:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_access_events WHERE id=$1 AND company_id=$2 AND attachment_id=$3 AND link_id=$4 AND actor_id=$5 AND variant=$6 AND outcome='allowed' AND created_at=$7)").bind(&event_id).bind(company).bind(source_id).bind(link).bind(user).bind(variant).bind(now).fetch_one(&mut *c).await?;
    if !evidence {
        return Err(changed());
    }
    Ok(FieldContentResponse {
        bytes: content.bytes.to_vec(),
        media_type: media,
        filename: image["filename"].as_str().ok_or_else(changed)?.into(),
        inline: content.witness.variant == FieldContentVariant::Preview,
    })
}
