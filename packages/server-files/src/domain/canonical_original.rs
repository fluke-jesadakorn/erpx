use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};

pub const ORIGINAL_LIMIT: usize = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalOriginalMediaType {
    Pdf,
    Png,
    Jpeg,
    Docx,
}
impl LegalOriginalMediaType {
    pub fn as_mime(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        }
    }
}
impl TryFrom<&str> for LegalOriginalMediaType {
    type Error = BusinessError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "application/pdf" => Ok(Self::Pdf),
            "image/png" => Ok(Self::Png),
            "image/jpeg" => Ok(Self::Jpeg),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Ok(Self::Docx)
            }
            _ => Err(BusinessError::invalid(
                "FILE_ORIGINAL_MEDIA",
                "Unsupported canonical Legal original media type.",
            )),
        }
    }
}

/// Selection facts, never an authorization grant. The current owner policy is
/// mandatory and must correlate these facts with its retained Control lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalReadActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub selection_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalOriginalQuery {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub item_id: RecordId,
    pub document_id: RecordId,
    pub link_id: RecordId,
    pub expected_checksum_sha256: String,
    pub expected_media_type: LegalOriginalMediaType,
    pub max_bytes: usize,
}

impl LegalOriginalQuery {
    pub fn validate(&self, actor: &OriginalReadActor) -> Result<(), BusinessError> {
        if actor.holding_id != self.holding_id || actor.company_id != self.company_id {
            return Err(unavailable());
        }
        if self.max_bytes == 0
            || self.max_bytes > ORIGINAL_LIMIT
            || !is_checksum(&self.expected_checksum_sha256)
            || actor.selection_revision.is_empty()
            || actor.selection_revision.len() > 200
        {
            return Err(BusinessError::invalid(
                "FILE_ORIGINAL_INPUT",
                "Invalid original file request.",
            ));
        }
        Ok(())
    }
}

/// Complete private Files source projection for the injected current owner
/// policy. It contains no bytes and is never a browser DTO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalOriginalFacts {
    pub company_id: CompanyId,
    pub link_id: RecordId,
    pub source_id: RecordId,
    pub set_id: RecordId,
    pub target_type: String,
    pub target_id: RecordId,
    pub purpose_key: String,
    pub component_kind: String,
    pub component_ref: Option<String>,
    pub policy_version: i64,
    pub classification: String,
    pub audience_json: String,
    pub retention_policy_key: Option<String>,
    pub set_state: String,
    pub link_state: String,
    pub binding_type: Option<String>,
    pub binding_id: Option<String>,
    pub source_state: String,
    pub source_company_id: Option<String>,
    pub source_owner_type: String,
    pub source_owner_id: String,
    pub source_purpose: String,
    pub object_key: String,
    pub media_type: String,
    pub detected_media_type: Option<String>,
    pub bytes: i64,
    pub checksum_sha256: String,
    pub revoked_at: Option<i64>,
    pub destroyed_at: Option<i64>,
    pub scanned_at: Option<i64>,
    pub scanner_definition_at: Option<i64>,
}

impl CanonicalOriginalFacts {
    pub fn validate(&self, query: &LegalOriginalQuery) -> Result<(), BusinessError> {
        if self.company_id != query.company_id
            || self.link_id != query.link_id
            || self.source_company_id.as_deref() != Some(query.company_id.as_str())
            || self.target_type != "legal_item"
            || self.target_id != query.item_id
            || self.purpose_key != "legal_document"
            || self.component_kind != "record"
            || self.policy_version != 1
            || self.classification != "restricted"
            || self.retention_policy_key.as_deref() != Some("legal-managed")
            || self.set_state != "bound"
            || self.link_state != "bound"
            || self.binding_type.as_deref() != Some("legal_document")
            || self.binding_id.as_deref() != Some(query.document_id.as_str())
            || self.source_state != "available"
            || self.revoked_at.is_some()
            || self.destroyed_at.is_some()
            || self.media_type != query.expected_media_type.as_mime()
            || self.detected_media_type.as_deref() != Some(query.expected_media_type.as_mime())
            || self.scanned_at.is_none()
            || self.scanner_definition_at.is_none()
            || self.bytes <= 0
            || self.bytes as u64 > query.max_bytes as u64
            || self.checksum_sha256 != query.expected_checksum_sha256
            || self.object_key.is_empty()
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

pub(crate) fn is_checksum(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub(crate) fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "FILE_ORIGINAL_UNAVAILABLE",
        "The canonical original is unavailable.",
    )
}
pub(crate) fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FILE_ORIGINAL_CHANGED",
        "The original file or current authority changed.",
    )
}
