use serde::{Deserialize, Serialize};

pub const DOCUMENT_INPUT_LIMIT: usize = 10 * 1024 * 1024;
pub const EXTRACTION_TEXT_LIMIT: usize = 200_000;
pub const EXTRACTION_OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
pub const EXTRACTION_DEADLINE_SECONDS: u64 = 540;
pub const SCANNED_PDF_PAGE_LIMIT: usize = 25;
pub const EXTRACTION_PAGE_MAP_LIMIT: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LegalDocumentMediaType {
    #[serde(rename = "application/pdf")]
    Pdf,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "application/vnd.openxmlformats-officedocument.wordprocessingml.document")]
    Docx,
}
impl LegalDocumentMediaType {
    pub const fn as_mime(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        }
    }
}
impl TryFrom<&str> for LegalDocumentMediaType {
    type Error = DocumentExtractionFailure;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "application/pdf" => Ok(Self::Pdf),
            "image/png" => Ok(Self::Png),
            "image/jpeg" => Ok(Self::Jpeg),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Ok(Self::Docx)
            }
            _ => Err(DocumentExtractionFailure::UnsupportedInput),
        }
    }
}

/// Internal input data only. It carries no path, trust selector or permission.
pub struct DocumentExtractionInput {
    pub checksum_sha256: String,
    pub source: DocumentExtractionSource,
}
/// Authored text is explicit; it has no original-file MIME or byte object.
pub enum DocumentExtractionSource {
    InlineText {
        body: String,
    },
    Original {
        media_type: LegalDocumentMediaType,
        bytes: Vec<u8>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentExtractionKind {
    InlineText,
    Original(LegalDocumentMediaType),
}
impl DocumentExtractionInput {
    pub fn bytes(&self) -> &[u8] {
        match &self.source {
            DocumentExtractionSource::InlineText { body } => body.as_bytes(),
            DocumentExtractionSource::Original { bytes, .. } => bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtractionMethod {
    Inline,
    PdfText,
    PdfOcr,
    ImageOcr,
    DocxText,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedPage {
    pub page: u32,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractedDocument {
    pub method: ExtractionMethod,
    pub text: String,
    pub pages: Vec<ExtractedPage>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionStatus {
    Extracted,
    Indeterminate,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentExtractionReport {
    pub status: ExtractionStatus,
    pub method: Option<ExtractionMethod>,
    pub text: String,
    pub pages: Vec<ExtractedPage>,
    pub page_mapping: bool,
    pub truncated: bool,
    pub requires_review: bool,
    pub checksum: String,
    pub retryable: bool,
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentExtractionFailure {
    UnsupportedInput,
    InputSizeLimit,
    InputChecksum,
    InvalidToolConfiguration,
    InvalidTextEncoding,
    DocxInvalid,
    DocxExpansionLimit,
    DocxTextLimit,
    ScannedPageLimit,
    PageMapLimit,
    ToolFailed,
    ToolTimeout,
    DeadlineExceeded,
    WorkerUnavailable,
    WorkerOutputLimit,
    WorkerMemoryLimit,
    WorkerFailed,
    WorkerProtocol,
    WorkerCancelled,
    WorkerCleanupFailed,
    ResourceLimitUnavailable,
}
impl DocumentExtractionFailure {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedInput => "DOCUMENT_EXTRACT_UNSUPPORTED_INPUT",
            Self::InputSizeLimit => "DOCUMENT_EXTRACT_INPUT_LIMIT",
            Self::InputChecksum => "DOCUMENT_EXTRACT_INPUT_CHECKSUM",
            Self::InvalidToolConfiguration => "DOCUMENT_EXTRACT_CONFIGURATION_INVALID",
            Self::InvalidTextEncoding => "DOCUMENT_EXTRACT_TEXT_ENCODING",
            Self::DocxInvalid => "DOCX_INVALID",
            Self::DocxExpansionLimit => "DOCX_EXPANSION_LIMIT",
            Self::DocxTextLimit => "DOCX_TEXT_TOO_LARGE",
            Self::ScannedPageLimit => "SCANNED_PDF_PAGE_LIMIT",
            Self::PageMapLimit => "DOCUMENT_EXTRACT_PAGE_MAP_LIMIT",
            Self::ToolFailed => "DOCUMENT_EXTRACT_TOOL_FAILED",
            Self::ToolTimeout => "DOCUMENT_EXTRACT_TOOL_TIMEOUT",
            Self::DeadlineExceeded => "DOCUMENT_EXTRACT_DEADLINE_EXCEEDED",
            Self::WorkerUnavailable => "DOCUMENT_EXTRACT_UNAVAILABLE",
            Self::WorkerOutputLimit => "DOCUMENT_EXTRACT_OUTPUT_LIMIT",
            Self::WorkerMemoryLimit => "DOCUMENT_EXTRACT_MEMORY_LIMIT",
            Self::WorkerFailed => "DOCUMENT_EXTRACT_PROCESS_FAILED",
            Self::WorkerProtocol => "DOCUMENT_EXTRACT_PROTOCOL_INVALID",
            Self::WorkerCancelled => "DOCUMENT_EXTRACT_CANCELLED",
            Self::WorkerCleanupFailed => "DOCUMENT_EXTRACT_CLEANUP_FAILED",
            Self::ResourceLimitUnavailable => "DOCUMENT_EXTRACT_RESOURCE_LIMIT_UNAVAILABLE",
        }
    }
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::ToolFailed
                | Self::ToolTimeout
                | Self::DeadlineExceeded
                | Self::WorkerUnavailable
                | Self::WorkerOutputLimit
                | Self::WorkerMemoryLimit
                | Self::WorkerFailed
                | Self::WorkerProtocol
        )
    }
}
