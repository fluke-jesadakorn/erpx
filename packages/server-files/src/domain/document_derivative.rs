//! Pure value types and gates for one bounded native document-page derivative.
//!
//! No SQL, no I/O and no HTTP request lives here. The composition root supplies
//! the current actor, the owning expense authority and the byte store; the
//! adapter reads only Files-owned rows and writes the exact Files-owned effects.
//! Every gate that decides whether a page may be prepared is a pure function of
//! these facts.
use erpx_server_shared::public::BusinessError;
use serde::Serialize;

pub const MAX_PAGES: u32 = 20;
pub const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
pub const DERIVATIVE_OPERATION: &str = "expense.document.render";

/// The exact page payload accepted by the retained Next derivatives route.
/// `bytes` is the raw rendered JPEG body; it never travels in command JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentPageInput {
    pub page: u32,
    pub page_count: u32,
    pub original_checksum: String,
    pub bytes: Vec<u8>,
}

/// The exact Files-owned attachment projection used for a page decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentPageSource {
    pub owner_type: String,
    pub owner_id: String,
    pub checksum_sha256: String,
    pub media_type: String,
}

/// The complete native reply. `canonicalDestinationLinks` leaves `{id,page}`
/// unchanged because neither field is a destination href.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DocumentDerivativeResult {
    pub id: String,
    pub page: u32,
}

/// Cross-owner facts resolved by the composition authority, never minted by a
/// request. `allowed` is the People Payroll preparation decision for the current
/// claim/stage; `version` is the claim version fenced at commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivativeAuthority {
    pub expense_id: String,
    pub version: i64,
    pub allowed: bool,
}

/// Validate page bounds and the JPEG signature exactly as the retained route.
pub fn validate_page(input: &DocumentPageInput) -> Result<(), BusinessError> {
    if input.page == 0
        || input.page > input.page_count
        || input.page_count > MAX_PAGES
        || input.bytes.len() > MAX_PAGE_BYTES
    {
        return Err(BusinessError::invalid(
            "DOCUMENT_PAGE_LIMIT",
            "Use up to 20 pages, each at most 2 MB.",
        ));
    }
    if input.bytes.len() < 2 || input.bytes[0] != 0xff || input.bytes[1] != 0xd8 {
        return Err(BusinessError::invalid(
            "PAGE_TYPE",
            "Rendered pages must be JPEG images.",
        ));
    }
    Ok(())
}

/// Only PDF, JPEG, PNG and WebP originals may be rendered. Non-PDF originals
/// are strictly single page, matching the retained application gate.
pub fn validate_type(
    source: &DocumentPageSource,
    input: &DocumentPageInput,
) -> Result<(), BusinessError> {
    let renderable = matches!(
        source.media_type.as_str(),
        "application/pdf" | "image/jpeg" | "image/png" | "image/webp"
    );
    if !renderable || (source.media_type != "application/pdf" && input.page_count != 1) {
        return Err(BusinessError::invalid(
            "DOCUMENT_TYPE",
            "This document cannot be rendered for image preparation.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg() -> Vec<u8> {
        vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10]
    }

    fn page() -> DocumentPageInput {
        DocumentPageInput {
            page: 1,
            page_count: 1,
            original_checksum: "a".repeat(64),
            bytes: jpeg(),
        }
    }

    fn source(media_type: &str) -> DocumentPageSource {
        DocumentPageSource {
            owner_type: "expense_claim".into(),
            owner_id: "claim-1".into(),
            checksum_sha256: "a".repeat(64),
            media_type: media_type.into(),
        }
    }

    #[test]
    fn accepts_one_valid_pdf_page() {
        assert!(validate_page(&page()).is_ok());
        assert!(validate_type(&source("application/pdf"), &page()).is_ok());
    }

    #[test]
    fn rejects_out_of_range_and_oversized_pages() {
        let mut input = page();
        input.page = 0;
        assert_eq!(
            validate_page(&input).unwrap_err().code,
            "DOCUMENT_PAGE_LIMIT"
        );
        let mut input = page();
        input.page = 2;
        assert_eq!(
            validate_page(&input).unwrap_err().code,
            "DOCUMENT_PAGE_LIMIT"
        );
        let mut input = page();
        input.page_count = MAX_PAGES + 1;
        assert_eq!(
            validate_page(&input).unwrap_err().code,
            "DOCUMENT_PAGE_LIMIT"
        );
        let mut input = page();
        input.bytes = vec![0xff, 0xd8];
        input.bytes.extend(std::iter::repeat_n(0u8, MAX_PAGE_BYTES));
        assert_eq!(
            validate_page(&input).unwrap_err().code,
            "DOCUMENT_PAGE_LIMIT"
        );
    }

    #[test]
    fn rejects_non_jpeg_bytes() {
        let mut input = page();
        input.bytes = b"%PDF-1.4 not a jpeg".to_vec();
        assert_eq!(validate_page(&input).unwrap_err().code, "PAGE_TYPE");
    }

    #[test]
    fn non_pdf_originals_must_be_single_page() {
        let mut input = page();
        input.page_count = 2;
        input.page = 1;
        assert_eq!(
            validate_type(&source("image/png"), &input).unwrap_err().code,
            "DOCUMENT_TYPE"
        );
        assert!(validate_type(&source("application/pdf"), &input).is_ok());
        assert_eq!(
            validate_type(&source("text/plain"), &page())
                .unwrap_err()
                .code,
            "DOCUMENT_TYPE"
        );
    }
}
