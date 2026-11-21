use erpx_server_legal::public::{
    DocumentExtractionFailure as Failure, DocumentExtractionKind as Kind, ExtractedDocument,
    ExtractedPage, ExtractionMethod as Method, ExtractionStatus, LegalDocumentMediaType as Media,
    finish_document_extraction, unresolved_document_extraction,
};
fn checksum() -> String {
    "a".repeat(64)
}
fn raw(method: Method, text: &str, pages: usize) -> ExtractedDocument {
    ExtractedDocument {
        method,
        text: text.into(),
        pages: (1..=pages)
            .map(|page| ExtractedPage {
                page: page as u32,
                text: text.into(),
            })
            .collect(),
    }
}

#[test]
fn original_media_allowlist_is_exact() {
    for media in [Media::Pdf, Media::Png, Media::Jpeg, Media::Docx] {
        assert_eq!(Media::try_from(media.as_mime()).unwrap(), media);
    }
    for unsupported in [
        "image/webp",
        "text/html",
        "application/zip",
        "application/msword",
        "application/pdf; charset=utf-8",
        "text/plain",
    ] {
        assert_eq!(Media::try_from(unsupported), Err(Failure::UnsupportedInput));
    }
}

#[test]
fn draft_text_and_shared_page_budget_preserve_the_existing_business_limit() {
    let text = "x".repeat(200_001);
    let mut input = raw(Method::PdfText, &text, 2);
    input.pages[1].text = "later".into();
    let report =
        finish_document_extraction(&checksum(), Kind::Original(Media::Pdf), input).unwrap();
    assert_eq!(report.status, ExtractionStatus::Extracted);
    assert_eq!(report.text.len(), 200_000);
    assert_eq!(report.pages[0].text.len(), 200_000);
    assert!(report.pages[1].text.is_empty());
    assert!(report.truncated && report.requires_review && report.page_mapping);
    assert!(!report.retryable);
}

#[test]
fn utf16_limit_preserves_complete_unicode_scalars() {
    let text = format!("{}😀tail", "ก".repeat(199_999));
    let report = finish_document_extraction(
        &checksum(),
        Kind::Original(Media::Png),
        raw(Method::ImageOcr, &text, 0),
    )
    .unwrap();
    assert_eq!(report.text, "ก".repeat(199_999));
    assert_eq!(report.text.encode_utf16().count(), 199_999);
    assert_eq!(report.pages[0].text, report.text);
    assert!(report.truncated && report.requires_review);
}

#[test]
fn docx_original_rejects_excess_text_but_inline_body_preserves_legacy_truncation() {
    let text = "x".repeat(200_001);
    assert!(matches!(
        finish_document_extraction(
            &checksum(),
            Kind::Original(Media::Docx),
            raw(Method::DocxText, &text, 0)
        ),
        Err(Failure::DocxTextLimit)
    ));
    let report =
        finish_document_extraction(&checksum(), Kind::InlineText, raw(Method::Inline, &text, 0))
            .unwrap();
    assert!(report.truncated && report.requires_review);
    assert!(!report.page_mapping);
}

#[test]
fn scanned_page_cap_does_not_disable_longer_text_layer_pdfs() {
    assert!(
        finish_document_extraction(
            &checksum(),
            Kind::Original(Media::Pdf),
            raw(Method::PdfOcr, "page", 25)
        )
        .is_ok()
    );
    for pages in [0, 26] {
        assert!(matches!(
            finish_document_extraction(
                &checksum(),
                Kind::Original(Media::Pdf),
                raw(Method::PdfOcr, "page", pages)
            ),
            Err(Failure::ScannedPageLimit)
        ));
    }
    assert!(
        finish_document_extraction(
            &checksum(),
            Kind::Original(Media::Pdf),
            raw(Method::PdfText, "page", 26)
        )
        .is_ok()
    );
}

#[test]
fn wrong_method_or_page_mapping_cannot_be_promoted_as_extraction() {
    assert!(matches!(
        finish_document_extraction(
            &checksum(),
            Kind::Original(Media::Png),
            raw(Method::PdfText, "page", 1)
        ),
        Err(Failure::WorkerProtocol)
    ));
    let mut input = raw(Method::PdfText, "page", 2);
    input.pages[1].page = 1;
    assert!(matches!(
        finish_document_extraction(&checksum(), Kind::Original(Media::Pdf), input),
        Err(Failure::WorkerProtocol)
    ));
    assert!(matches!(
        finish_document_extraction(
            &checksum(),
            Kind::Original(Media::Pdf),
            raw(Method::PdfText, "", 10_001)
        ),
        Err(Failure::PageMapLimit)
    ));
}

#[test]
fn deadline_or_tool_failure_retains_no_partial_text_and_explicit_review_state() {
    for failure in [
        Failure::DeadlineExceeded,
        Failure::ToolTimeout,
        Failure::WorkerUnavailable,
    ] {
        let report = unresolved_document_extraction(&checksum(), failure);
        assert_eq!(report.status, ExtractionStatus::Indeterminate);
        assert!(report.text.is_empty() && report.pages.is_empty() && report.method.is_none());
        assert!(report.requires_review && report.retryable);
        assert_eq!(report.reasons, [failure.code()]);
    }
    for failure in [
        Failure::WorkerCancelled,
        Failure::WorkerCleanupFailed,
        Failure::DocxInvalid,
    ] {
        assert!(!unresolved_document_extraction(&checksum(), failure).retryable);
    }
}
