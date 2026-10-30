//! Pure validation and stable identity for Field visit file reservations.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};

pub const FIELD_UPLOAD_MAX_BYTES: i64 = 10 * 1024 * 1024;
pub const FIELD_RESERVATION_MILLIS: i64 = 15 * 60_000;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FieldFileReservation {
    pub filename: String,
    pub media_type: String,
    pub bytes: i64,
    pub caption: Option<String>,
    pub supersedes_link_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedFieldFileReservation {
    pub filename: String,
    pub media_type: String,
    pub bytes: i64,
    pub caption: Option<String>,
    pub supersedes_link_id: Option<String>,
}
fn error(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
fn trim_js(value: &str) -> &str {
    value.trim_matches(|c: char| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}
/// Call after replay selection: retained replays compare original input before
/// applying fresh media and metadata policy.
pub fn normalize_fresh_field_file(
    purpose: &str,
    value: &FieldFileReservation,
) -> Result<NormalizedFieldFileReservation, BusinessError> {
    let image = matches!(
        value.media_type.as_str(),
        "image/jpeg" | "image/png" | "image/webp" | "image/heic" | "image/heif"
    );
    if !matches!(
        purpose,
        "site_photo" | "technician_report" | "signed_job_sheet" | "exception_evidence"
    ) {
        return Err(error(
            "ATTACHMENT_POLICY_UNKNOWN",
            "This file purpose is not registered for the selected record and context.",
        ));
    }
    if !image && (purpose == "site_photo" || value.media_type != "application/pdf") {
        return Err(error(
            "ATTACHMENT_MEDIA_TYPE",
            "This file type is not allowed for the selected purpose.",
        ));
    }
    if !(1..=FIELD_UPLOAD_MAX_BYTES).contains(&value.bytes) {
        return Err(error(
            "ATTACHMENT_SIZE",
            "Files for this purpose must be at most 10 MiB.",
        ));
    }
    let filename = trim_js(&value.filename);
    let caption = value
        .caption
        .as_deref()
        .map(trim_js)
        .filter(|s| !s.is_empty());
    if filename.is_empty()
        || filename.encode_utf16().count() > 255
        || caption.is_some_and(|s| s.encode_utf16().count() > 1000)
    {
        return Err(error(
            "ATTACHMENT_METADATA",
            "Check the filename and caption.",
        ));
    }
    Ok(NormalizedFieldFileReservation {
        filename: filename.into(),
        media_type: value.media_type.clone(),
        bytes: value.bytes,
        caption: caption.map(str::to_owned),
        supersedes_link_id: value.supersedes_link_id.clone(),
    })
}
pub fn field_upload_id(company: &str, user: &str, wire_key: &str) -> String {
    let hash = hex::encode(Sha256::digest(
        format!("{company}\0{user}\0{wire_key}").as_bytes(),
    ));
    format!("attachment-upload-{}", &hash[..32])
}
pub fn field_upload_link_id(upload_id: &str) -> Option<String> {
    let suffix = upload_id.strip_prefix("attachment-upload-")?;
    if suffix.len() != 32
        || !suffix
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return None;
    }
    Some(format!("attachment-link-{suffix}"))
}

/// Validate complete bytes before any store write; matching a signature is not
/// a scan result and must never make the source available.
pub fn validate_field_upload_bytes(
    media_type: &str,
    declared_bytes: i64,
    bytes: &[u8],
) -> Result<String, BusinessError> {
    if !(1..=FIELD_UPLOAD_MAX_BYTES).contains(&declared_bytes)
        || bytes.len() as u64 != declared_bytes as u64
    {
        return Err(error(
            "UPLOAD_SIZE",
            "The uploaded bytes must exactly match the reserved size.",
        ));
    }
    if super::media_signature::detected_attachment_media_type(bytes, media_type) != Some(media_type)
    {
        return Err(error(
            "UPLOAD_TYPE",
            "The file contents do not match the selected type.",
        ));
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// A retained reservation may be replayed after its source changes, but its
/// stored request must still match the caller's original normalized metadata.
pub fn field_reservation_replay_matches(
    stored: &NormalizedFieldFileReservation,
    request: &FieldFileReservation,
) -> bool {
    stored.filename == trim_js(&request.filename)
        && stored.media_type == request.media_type
        && stored.bytes == request.bytes
        && stored.caption.as_deref()
            == request
                .caption
                .as_deref()
                .map(trim_js)
                .filter(|s| !s.is_empty())
        && stored.supersedes_link_id == request.supersedes_link_id
}
