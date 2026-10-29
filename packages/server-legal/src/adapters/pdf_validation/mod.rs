mod crl;
mod crypto;
mod document;
mod seed;
mod trust;

use crate::application::pdf_validation::{finish_report, finish_signature};
use crate::domain::pdf_validation::{PdfValidationReport, SignatureValidation, ValidationState};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) use trust::material_digest;
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
    if document.signatures.iter().any(|field| {
        matches!(
            field
                .dictionary
                .get(b"Type")
                .and_then(lopdf::Object::as_name),
            Ok(b"DocTimeStamp")
        ) || matches!(
            field
                .dictionary
                .get(b"SubFilter")
                .and_then(lopdf::Object::as_name),
            Ok(b"ETSI.RFC3161")
        )
    }) {
        // B0's ordinary-signature entrypoint rejects document timestamps and
        // returns one indeterminate document, without partial signature output.
        // Do not invent document-timestamp validity from a CMS signature alone.
        report.reasons = vec![
            "VALIDATION_FAILED".to_owned(),
            "DOCUMENT_TIMESTAMP_POLICY_UNRESOLVED".to_owned(),
        ];
        return report;
    }
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
        if let Err(reason) =
            seed::satisfied(pdf, &document, field, signature.timestamp, trust, moment)
        {
            signature
                .reasons
                .push(if reason == "SEED_VALUE_CONSTRAINTS_FAILURE" {
                    reason.to_owned()
                } else {
                    "SEED_VALUE_CONSTRAINTS_UNRESOLVED".to_owned()
                });
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
