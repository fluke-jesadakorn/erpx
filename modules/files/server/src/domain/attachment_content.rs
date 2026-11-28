//! Pure facts and gates for one bounded private attachment content read.
//!
//! No SQL, no I/O and no HTTP request lives here. The composition root supplies
//! the current Company/actor and the owning-record authority; the adapter reads
//! the exact Files rows and the verified byte store. Every gate that decides
//! whether a variant may be served is a pure function of these facts.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Ten megabyte bound shared by the retained Next content routes and the native
/// Project content reader.
pub const MAX_CONTENT_BYTES: i64 = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentVariant {
    Original,
    Preview,
}

impl ContentVariant {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "original" => Ok(Self::Original),
            "preview" => Ok(Self::Preview),
            _ => Err(BusinessError::invalid(
                "ATTACHMENT_VARIANT",
                "Choose original or preview.",
            )),
        }
    }

    /// A page request always selects the sanitized preview, exactly as the
    /// retained Next attachment-id content route did.
    pub fn from_page(page: Option<u32>) -> Self {
        if page.is_some() {
            Self::Preview
        } else {
            Self::Original
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Preview => "preview",
        }
    }
}

/// The exact Files-owned row projection used for a content decision. Values
/// come only from the company-scoped attachment read; a caller cannot mint one.
#[derive(Clone, Debug)]
pub struct ContentFacts {
    pub link_id: String,
    pub set_id: String,
    pub company_id: String,
    pub target_type: String,
    pub target_id: String,
    pub purpose_key: Option<String>,
    pub component_kind: Option<String>,
    pub created_by: String,
    pub link_state: String,
    pub source_id: Option<String>,
    pub source_state: Option<String>,
    pub scanned_at: Option<i64>,
    pub scanner_definition_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub destroyed_at: Option<i64>,
    pub original_name: Option<String>,
    pub media_type: Option<String>,
    pub bytes: Option<i64>,
    pub checksum_sha256: Option<String>,
    pub object_key: Option<String>,
    pub preview_object_key: Option<String>,
    pub preview_bytes: Option<i64>,
    pub preview_checksum_sha256: Option<String>,
    pub preview_media_type: Option<String>,
}

/// The verified selection ready for the byte store and the HTTP response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentSelection {
    pub object_key: String,
    pub bytes: usize,
    pub checksum_sha256: String,
    pub media_type: String,
    pub filename: String,
    pub inline: bool,
    pub variant: &'static str,
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn validate_identifier(id: &str, code: &'static str) -> Result<(), BusinessError> {
    if id.is_empty()
        || id.len() > 200
        || id.chars().any(char::is_control)
        || id.contains('/')
        || id.contains('\\')
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            code,
            "File not found.",
        ));
    }
    Ok(())
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
}

fn not_available() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_CONTENT_NOT_AVAILABLE",
        "The file is not available.",
    )
}

fn preview_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_PREVIEW_NOT_AVAILABLE",
        "The file preview is not available.",
    )
}

/// Apply every retained content gate and return the exact variant selection.
/// Quarantine/scan state, revocation, destruction, checksum, media type and
/// size are all rechecked here before the byte store is ever called.
pub fn select(
    facts: &ContentFacts,
    variant: ContentVariant,
    user_id: &str,
) -> Result<ContentSelection, BusinessError> {
    if facts.link_state == "withdrawn" || facts.link_state == "expired" {
        return Err(not_found());
    }
    if facts.link_state == "draft" && facts.created_by != user_id {
        return Err(not_found());
    }
    if facts.source_state.as_deref() != Some("available")
        || facts.scanned_at.is_none()
        || facts.scanner_definition_at.is_none()
        || facts.revoked_at.is_some()
        || facts.destroyed_at.is_some()
        || facts
            .checksum_sha256
            .as_deref()
            .is_none_or(|value| !valid_hash(value))
        || facts.media_type.as_deref().is_none_or(str::is_empty)
        || facts
            .bytes
            .is_none_or(|value| !(1..=MAX_CONTENT_BYTES).contains(&value))
    {
        return Err(not_available());
    }
    let (object_key, bytes, checksum, media_type, filename, inline) = match variant {
        ContentVariant::Original => (
            facts.object_key.clone().ok_or_else(not_available)?,
            facts.bytes.ok_or_else(not_available)?,
            facts.checksum_sha256.clone().ok_or_else(not_available)?,
            facts.media_type.clone().ok_or_else(not_available)?,
            facts
                .original_name
                .clone()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "file".into()),
            false,
        ),
        ContentVariant::Preview => {
            let checksum = facts
                .preview_checksum_sha256
                .clone()
                .filter(|value| valid_hash(value));
            (
                facts
                    .preview_object_key
                    .clone()
                    .ok_or_else(preview_not_found)?,
                facts.preview_bytes.ok_or_else(preview_not_found)?,
                checksum.ok_or_else(preview_not_found)?,
                facts
                    .preview_media_type
                    .clone()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(preview_not_found)?,
                format!(
                    "preview-{}",
                    facts
                        .original_name
                        .clone()
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "file".into())
                ),
                true,
            )
        }
    };
    if !(1..=MAX_CONTENT_BYTES).contains(&bytes) {
        return Err(not_available());
    }
    Ok(ContentSelection {
        object_key,
        bytes: bytes as usize,
        checksum_sha256: checksum,
        media_type,
        filename,
        inline,
        variant: variant.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ContentFacts {
        ContentFacts {
            link_id: "link-1".into(),
            set_id: "set-1".into(),
            company_id: "co-1".into(),
            target_type: "staff_task".into(),
            target_id: "task-1".into(),
            purpose_key: None,
            component_kind: None,
            created_by: "actor".into(),
            link_state: "bound".into(),
            source_id: Some("attachment-1".into()),
            source_state: Some("available".into()),
            scanned_at: Some(1),
            scanner_definition_at: Some(1),
            revoked_at: None,
            destroyed_at: None,
            original_name: Some("evidence.pdf".into()),
            media_type: Some("application/pdf".into()),
            bytes: Some(26),
            checksum_sha256: Some("a".repeat(64)),
            object_key: Some("objects/original".into()),
            preview_object_key: Some("objects/preview".into()),
            preview_bytes: Some(21),
            preview_checksum_sha256: Some("b".repeat(64)),
            preview_media_type: Some("image/webp".into()),
        }
    }

    #[test]
    fn page_always_selects_the_sanitized_preview() {
        assert_eq!(ContentVariant::from_page(None), ContentVariant::Original);
        assert_eq!(ContentVariant::from_page(Some(2)), ContentVariant::Preview);
        assert_eq!(
            ContentVariant::parse("original").unwrap().as_str(),
            "original"
        );
        assert!(ContentVariant::parse("thumb").is_err());
    }

    #[test]
    fn original_and_preview_select_exact_stored_facts() {
        let original = select(&facts(), ContentVariant::Original, "actor").unwrap();
        assert_eq!(original.object_key, "objects/original");
        assert_eq!(original.bytes, 26);
        assert_eq!(original.checksum_sha256, "a".repeat(64));
        assert_eq!(original.media_type, "application/pdf");
        assert_eq!(original.filename, "evidence.pdf");
        assert!(!original.inline);
        let preview = select(&facts(), ContentVariant::Preview, "actor").unwrap();
        assert_eq!(preview.object_key, "objects/preview");
        assert_eq!(preview.bytes, 21);
        assert_eq!(preview.checksum_sha256, "b".repeat(64));
        assert_eq!(preview.media_type, "image/webp");
        assert_eq!(preview.filename, "preview-evidence.pdf");
        assert!(preview.inline);
    }

    #[test]
    fn quarantine_draft_revocation_and_bad_checksum_fail_closed() {
        let mut row = facts();
        row.scanned_at = None;
        assert!(select(&row, ContentVariant::Original, "actor").is_err());
        let mut row = facts();
        row.revoked_at = Some(1);
        assert!(select(&row, ContentVariant::Original, "actor").is_err());
        let mut row = facts();
        row.link_state = "draft".into();
        assert!(select(&row, ContentVariant::Original, "other").is_err());
        assert!(select(&row, ContentVariant::Original, "actor").is_ok());
        let mut row = facts();
        row.checksum_sha256 = Some("bad".into());
        assert!(select(&row, ContentVariant::Original, "actor").is_err());
        let mut row = facts();
        row.preview_checksum_sha256 = None;
        assert!(select(&row, ContentVariant::Preview, "actor").is_err());
        let mut row = facts();
        row.bytes = Some(MAX_CONTENT_BYTES + 1);
        assert!(select(&row, ContentVariant::Original, "actor").is_err());
    }
}
