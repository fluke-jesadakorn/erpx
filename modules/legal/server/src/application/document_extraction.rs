use crate::domain::document_extraction::{
    DocumentComputeBudget, DocumentExtractionFailure, DocumentExtractionInput,
    DocumentExtractionKind, DocumentExtractionReport, EXTRACTION_PAGE_MAP_LIMIT,
    EXTRACTION_TEXT_LIMIT, ExtractedDocument, ExtractedPage, ExtractionMethod, ExtractionStatus,
    LegalDocumentMediaType, SCANNED_PDF_PAGE_LIMIT,
};
use std::future::Future;

pub trait DocumentExtractor: Sync {
    fn extract(
        &self,
        input: DocumentExtractionInput,
    ) -> impl Future<Output = Result<DocumentExtractionReport, DocumentExtractionFailure>> + Send
    {
        self.extract_with_budget(input, DocumentComputeBudget::maximum())
    }
    fn extract_with_budget(
        &self,
        input: DocumentExtractionInput,
        budget: DocumentComputeBudget,
    ) -> impl Future<Output = Result<DocumentExtractionReport, DocumentExtractionFailure>> + Send;
}

pub(crate) fn require_extraction_report(
    checksum: &str,
    kind: DocumentExtractionKind,
    report: &DocumentExtractionReport,
) -> Result<(), DocumentExtractionFailure> {
    let expected = finish_document_extraction(
        checksum,
        kind,
        ExtractedDocument {
            method: report
                .method
                .ok_or(DocumentExtractionFailure::WorkerProtocol)?,
            text: report.text.clone(),
            pages: report.pages.clone(),
        },
    )?;
    if report.status != expected.status
        || report.checksum != expected.checksum
        || !report.requires_review
        || report.retryable
        || !report.reasons.is_empty()
        || report.page_mapping != expected.page_mapping
        || report.text != expected.text
        || report.pages != expected.pages
    {
        return Err(DocumentExtractionFailure::WorkerProtocol);
    }
    Ok(())
}

/// OCR remains draft evidence requiring human review. The business text budget
/// uses JavaScript-compatible UTF16 units while preserving valid Unicode scalars.
pub fn finish_document_extraction(
    checksum: &str,
    kind: DocumentExtractionKind,
    raw: ExtractedDocument,
) -> Result<DocumentExtractionReport, DocumentExtractionFailure> {
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DocumentExtractionFailure::InputChecksum);
    }
    let expected = match kind {
        DocumentExtractionKind::InlineText => vec![ExtractionMethod::Inline],
        DocumentExtractionKind::Original(LegalDocumentMediaType::Pdf) => {
            vec![ExtractionMethod::PdfText, ExtractionMethod::PdfOcr]
        }
        DocumentExtractionKind::Original(
            LegalDocumentMediaType::Png | LegalDocumentMediaType::Jpeg,
        ) => {
            vec![ExtractionMethod::ImageOcr]
        }
        DocumentExtractionKind::Original(LegalDocumentMediaType::Docx) => {
            vec![ExtractionMethod::DocxText]
        }
    };
    if !expected.contains(&raw.method) {
        return Err(DocumentExtractionFailure::WorkerProtocol);
    }
    if raw.pages.len() > EXTRACTION_PAGE_MAP_LIMIT {
        return Err(DocumentExtractionFailure::PageMapLimit);
    }
    if raw.method == ExtractionMethod::PdfOcr
        && (raw.pages.is_empty() || raw.pages.len() > SCANNED_PDF_PAGE_LIMIT)
    {
        return Err(DocumentExtractionFailure::ScannedPageLimit);
    }
    let units = raw.text.encode_utf16().count();
    if raw.method == ExtractionMethod::DocxText && units > EXTRACTION_TEXT_LIMIT {
        return Err(DocumentExtractionFailure::DocxTextLimit);
    }
    if raw
        .pages
        .iter()
        .enumerate()
        .any(|(index, page)| page.page as usize != index + 1)
    {
        return Err(DocumentExtractionFailure::WorkerProtocol);
    }
    let mut pages = raw.pages;
    if pages.is_empty() {
        pages.push(ExtractedPage {
            page: 1,
            text: raw.text.clone(),
        });
    }
    let mut remaining = EXTRACTION_TEXT_LIMIT;
    for page in &mut pages {
        page.text = truncate_utf16(&page.text, remaining);
        remaining -= page.text.encode_utf16().count();
    }
    Ok(DocumentExtractionReport {
        status: ExtractionStatus::Extracted,
        method: Some(raw.method),
        text: truncate_utf16(&raw.text, EXTRACTION_TEXT_LIMIT),
        pages,
        page_mapping: matches!(
            raw.method,
            ExtractionMethod::PdfText | ExtractionMethod::PdfOcr | ExtractionMethod::ImageOcr
        ),
        truncated: units > EXTRACTION_TEXT_LIMIT,
        requires_review: true,
        checksum: checksum.to_ascii_lowercase(),
        retryable: false,
        reasons: Vec::new(),
    })
}

/// Failure never retains text from pages completed before timeout/cancellation.
pub fn unresolved_document_extraction(
    checksum: &str,
    failure: DocumentExtractionFailure,
) -> DocumentExtractionReport {
    DocumentExtractionReport {
        status: ExtractionStatus::Indeterminate,
        method: None,
        text: String::new(),
        pages: Vec::new(),
        page_mapping: false,
        truncated: false,
        requires_review: true,
        checksum: checksum.to_owned(),
        retryable: failure.retryable(),
        reasons: vec![failure.code().to_owned()],
    }
}

fn truncate_utf16(text: &str, limit: usize) -> String {
    let mut used = 0;
    let mut end = 0;
    for (index, character) in text.char_indices() {
        if used + character.len_utf16() > limit {
            break;
        }
        used += character.len_utf16();
        end = index + character.len_utf8();
    }
    text[..end].to_owned()
}
