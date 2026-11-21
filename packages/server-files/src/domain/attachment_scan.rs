//! Files/system sanitization facts; these types never confer a Person's read
//! grant. Native provider evidence is retained only through a fresh owner lease.
use crate::domain::preview::PreviewSourceMedia;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use serde::{Deserialize, Serialize};

pub const SCAN_LEASE_MILLIS: i64 = 60_000;
pub const SCAN_CLEANUP_HEADROOM_MILLIS: i64 = 5_000;
pub const DRAFT_EXTENSION_MILLIS: i64 = 7 * 24 * 60 * 60_000;
pub const ORIGINAL_BYTES_LIMIT: i64 = 10 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentScanScope {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
}

/// Only the exact alias normalization performed by B0's explicit legacy
/// migration. This is not a generic purpose/owner permission dispatch.
pub fn legacy_purpose<'a>(owner: &str, purpose: &'a str) -> Option<&'a str> {
    match (owner, purpose) {
        ("support_ticket", "support_evidence" | "internal_evidence") => Some("internal_evidence"),
        ("support_ticket", "customer_shareable") => Some("customer_shareable"),
        ("employee_request", "request_evidence" | "it_request_evidence" | "fulfillment") => {
            Some(purpose)
        }
        ("leave_request", "medical_certificate") => Some(purpose),
        ("leave_request", "leave_supporting_photo" | "evidence") => Some("leave_supporting_photo"),
        ("expense_claim", "receipt" | "merchant_receipt" | "supporting_document") => Some(purpose),
        ("expense_payment", "payment_slip") => Some(purpose),
        ("invoice_payment", "payment_slip" | "bank_payment_slip") => Some("payment_slip"),
        ("staff_task", "task_evidence" | "evidence") => Some("task_evidence"),
        ("social_draft", "post_image" | "post-image") => Some("post_image"),
        ("quality_hold", "quality_inspection_evidence") => Some(purpose),
        ("commerce_order_delivery", "proof_of_delivery") => Some(purpose),
        ("holding_brand" | "branch_brand", "primary" | "inverse") => Some(purpose),
        ("company_storefront_brand", "logo") => Some(purpose),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ScanSourceFacts {
    pub id: String,
    pub upload_id: String,
    pub company_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub purpose: String,
    pub object_key: String,
    pub revision: i64,
    pub original_name: String,
    pub media_type: String,
    pub bytes: i64,
    pub checksum: String,
    pub created_by: String,
    pub created_at: i64,
    pub detected_media_type: Option<String>,
    pub state: String,
    pub revoked_at: Option<i64>,
    pub destroyed_at: Option<i64>,
    pub has_preview: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ScanUploadFacts {
    pub id: String,
    pub company_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub purpose: String,
    pub original_name: String,
    pub media_type: String,
    pub declared_bytes: i64,
    pub actual_bytes: Option<i64>,
    pub checksum: Option<String>,
    pub quarantine_key: Option<String>,
    pub final_key: Option<String>,
    pub staging_key: String,
    pub revision: i64,
    pub status: String,
    pub created_by: String,
    pub set_id: String,
    pub link_id: String,
    pub detected_media_type: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ScanRelationshipFacts {
    pub set_id: String,
    pub company_id: String,
    pub target_type: String,
    pub target_id: String,
    pub purpose: String,
    pub link_id: String,
    pub link_source_id: Option<String>,
    pub link_upload_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct OriginalStorageFacts {
    pub company_id: String,
    pub source_id: String,
    pub object_key: String,
    pub bytes: i64,
    pub variant: String,
    pub state: String,
}

/// Selected by an exact Files event/relationship reader, not from caller input.
/// The event itself is retained/rechecked by the PostgreSQL owner adapter.
pub(crate) enum ScanOrigin {
    Managed,
    Legacy,
    Specialized,
}

pub(crate) fn require_system_scan_source(
    scope: &AttachmentScanScope,
    source: &ScanSourceFacts,
    upload: &ScanUploadFacts,
    relationship: &ScanRelationshipFacts,
    original: &OriginalStorageFacts,
    origin: ScanOrigin,
) -> Result<PreviewSourceMedia, BusinessError> {
    let media = PreviewSourceMedia::try_from(source.media_type.as_str()).map_err(|_| changed())?;
    let (expected_purpose, managed) = match origin {
        ScanOrigin::Managed => {
            if source.id != format!("attachment-{}", upload.id) {
                return Err(changed());
            }
            (source.purpose.as_str(), true)
        }
        ScanOrigin::Legacy => (
            legacy_purpose(&source.owner_type, &source.purpose).ok_or_else(changed)?,
            false,
        ),
        ScanOrigin::Specialized => (source.purpose.as_str(), false),
    };
    let effective_key = upload
        .quarantine_key
        .as_deref()
        .or(upload.final_key.as_deref())
        .unwrap_or(&upload.staging_key);
    if source.company_id != scope.company_id.as_str()
        || source.upload_id != upload.id
        || upload.company_id != source.company_id
        || source.owner_type != upload.owner_type
        || source.owner_id != upload.owner_id
        || source.purpose != upload.purpose
        || source.original_name != upload.original_name
        || source.media_type != upload.media_type
        || source.revision < 1
        || source.revision != upload.revision
        || source.created_by != upload.created_by
        || source.bytes < 1
        || source.bytes > ORIGINAL_BYTES_LIMIT
        || source.bytes != upload.declared_bytes
        || upload.actual_bytes != Some(source.bytes)
        || upload.checksum.as_deref() != Some(source.checksum.as_str())
        || effective_key != source.object_key
        || source
            .detected_media_type
            .as_deref()
            .is_some_and(|v| v != source.media_type)
        || upload
            .detected_media_type
            .as_deref()
            .is_some_and(|v| v != source.media_type)
        || (managed
            && (source.detected_media_type.is_none() || upload.detected_media_type.is_none()))
        || upload.status != "uploaded"
        || !matches!(
            source.state.as_str(),
            "quarantined" | "scanning" | "scan_error"
        )
        || source.destroyed_at.is_some()
        || source.has_preview
        || !valid_sha(&source.checksum)
        || relationship.company_id != source.company_id
        || relationship.set_id != upload.set_id
        || relationship.link_id != upload.link_id
        || relationship.target_type != source.owner_type
        || relationship.target_id != source.owner_id
        || relationship.purpose != expected_purpose
        || relationship.link_source_id.as_deref() != Some(source.id.as_str())
        || relationship.link_upload_id.as_deref() != Some(source.upload_id.as_str())
        || original.company_id != source.company_id
        || original.source_id != source.id
        || original.variant != "original"
        || original.state != "stored"
        || original.bytes != source.bytes
        || original.object_key != source.object_key
    {
        return Err(changed());
    }
    // No Person, purchase, saved-parent, draft/bound/withdrawn link/set state or
    // uploaded-intent expiry gate: B0 sanitizes accepted uploaded drafts. A
    // revoked source can still be sanitized/preserved; read policy is separate.
    Ok(media)
}

/// Existing interrupted running work is reclaimed as the same attempt. Only a
/// settled scan ERROR creates the next attempt, through six; schema accepts
/// historical attempts up to twelve and none above five schedules another.
pub fn next_scan_attempt(
    attempt: i64,
    completed: i64,
) -> Result<Option<(i64, i64)>, BusinessError> {
    if !(1..=12).contains(&attempt) {
        return Err(changed());
    }
    if attempt >= 6 {
        return Ok(None);
    }
    let delay = (1000_i64 << attempt).min(60_000);
    Ok(Some((
        attempt + 1,
        completed.checked_add(delay).ok_or_else(changed)?,
    )))
}
pub fn preview_object_key(
    original: &str,
    checksum: &str,
    media: &str,
) -> Result<String, BusinessError> {
    if original.is_empty()
        || original.len() > 4096
        || original.contains('\0')
        || !valid_sha(checksum)
    {
        return Err(changed());
    }
    let extension = match media {
        "image/webp" => "webp",
        "image/jpeg" => "jpg",
        _ => return Err(changed()),
    };
    let prefix = original.strip_suffix("/original").unwrap_or(original);
    let key = format!("{prefix}/preview-{checksum}.{extension}");
    if key.len() > 4096 {
        return Err(changed());
    }
    Ok(key)
}
pub fn remaining_compute_millis(claim_until: i64, now: i64) -> Result<u64, BusinessError> {
    let value = claim_until
        .checked_sub(now)
        .and_then(|v| v.checked_sub(SCAN_CLEANUP_HEADROOM_MILLIS))
        .filter(|v| *v > 0)
        .ok_or_else(deadline)?;
    u64::try_from(value.min(SCAN_LEASE_MILLIS - SCAN_CLEANUP_HEADROOM_MILLIS))
        .map_err(|_| deadline())
}
pub(crate) fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub(crate) fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SCAN_SOURCE_CHANGED",
        "The current file scan source or lease changed.",
    )
}
pub(crate) fn deadline() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_SCAN_DEADLINE",
        "The owned file scan lease has insufficient remaining time.",
    )
}
