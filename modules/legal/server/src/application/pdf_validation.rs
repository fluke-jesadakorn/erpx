use crate::domain::pdf_validation::{PdfValidationReport, SignatureValidation, aggregate};

/// The application applies the three-state policy to independently verified evidence.
pub(crate) fn finish_signature(signature: &mut SignatureValidation) {
    use crate::domain::pdf_validation::ValidationState;
    signature.policy = if signature.reasons.iter().any(|reason| {
        reason == "DOCUMENT_MODIFICATION_POLICY_FAILURE"
            || reason == "SEED_VALUE_CONSTRAINTS_FAILURE"
    }) {
        ValidationState::Invalid
    } else if signature.reasons.iter().any(|reason| {
        reason == "DOCUMENT_MODIFICATION_POLICY_UNRESOLVED"
            || reason == "SEED_VALUE_CONSTRAINTS_UNRESOLVED"
    }) {
        ValidationState::Indeterminate
    } else {
        ValidationState::Valid
    };
    signature.status = aggregate([
        signature.integrity,
        signature.required_certificate_chain,
        signature.certificate_validity,
        signature.revocation,
        signature.timestamp,
    ]);
    // A verified modification policy violation is independently fatal.
    if signature
        .reasons
        .iter()
        .any(|reason| reason == "DOCUMENT_MODIFICATION_POLICY_FAILURE")
    {
        signature.status = ValidationState::Invalid;
    }
    if signature.reasons.iter().any(|reason| {
        reason == "DOCUMENT_MODIFICATION_POLICY_UNRESOLVED"
            || reason == "SEED_VALUE_CONSTRAINTS_UNRESOLVED"
            || reason == "SEED_VALUE_CONSTRAINTS_FAILURE"
    }) && signature.status == ValidationState::Valid
    {
        signature.status = ValidationState::Indeterminate;
    }
}

pub(crate) fn finish_report(report: &mut PdfValidationReport) {
    report.status = aggregate(report.signatures.iter().map(|signature| signature.status));
    if report.signatures.is_empty() && report.reasons.is_empty() {
        report.reasons.push("NO_EMBEDDED_SIGNATURES".to_owned());
    }
}
