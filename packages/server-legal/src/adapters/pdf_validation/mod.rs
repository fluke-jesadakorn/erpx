mod crypto;
mod document;
mod trust;

use crate::application::pdf_validation::{finish_report, finish_signature};
use crate::domain::pdf_validation::{PdfValidationReport, SignatureValidation, ValidationState};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub use trust::{OperatorTrustStore, TrustMaterial};

/// Verify one immutable PDF version using only approved, supplied evidence.
/// No network client or platform certificate store is available to this adapter.
pub fn validate_signed_pdf(
    pdf: &[u8],
    trust: &OperatorTrustStore,
    moment: OffsetDateTime,
) -> PdfValidationReport {
    let mut report = PdfValidationReport {
        status: ValidationState::Indeterminate,
        signatures: Vec::new(),
        reasons: Vec::new(),
        validated_at: moment.format(&Rfc3339).unwrap_or_default(),
        trust_store_digest: trust.digest.clone(),
        mode: "offline-strict-revocation".to_owned(),
        engine: "ERPX Rust / OpenSSL / lopdf".to_owned(),
    };
    let document = match document::ParsedPdf::parse(pdf) {
        Ok(document) => document,
        Err(reason) => {
            report.reasons = vec!["VALIDATION_FAILED".to_owned(), reason.to_owned()];
            return report;
        }
    };
    for field in &document.signatures {
        let mut signature = match document::signed_content(pdf, field) {
            Ok(content) => {
                crypto::verify_signature(&field.name, &content.cms, &content.signed, trust, moment)
            }
            Err(reason) => SignatureValidation::unresolved(field.name.clone(), reason),
        };
        if signature
            .reasons
            .iter()
            .any(|reason| reason == "UNSUPPORTED_DIGEST")
        {
            report.signatures.clear();
            report.reasons = vec![
                "VALIDATION_FAILED".to_owned(),
                "UNSUPPORTED_DIGEST".to_owned(),
            ];
            return report;
        }
        // Seed-value certificate/handler policies require additional conformance
        // work. Do not silently treat an unimplemented policy as satisfied.
        if field.field.has(b"SV") {
            signature
                .reasons
                .push("SEED_VALUE_CONSTRAINTS_UNRESOLVED".to_owned());
        }
        match document::modification_policy(pdf, &document, field) {
            document::Modification::Allowed => {}
            document::Modification::Forbidden => signature
                .reasons
                .push("DOCUMENT_MODIFICATION_POLICY_FAILURE".to_owned()),
            document::Modification::Unresolved => signature
                .reasons
                .push("DOCUMENT_MODIFICATION_POLICY_UNRESOLVED".to_owned()),
        }
        finish_signature(&mut signature);
        report.signatures.push(signature);
    }
    finish_report(&mut report);
    report
}
