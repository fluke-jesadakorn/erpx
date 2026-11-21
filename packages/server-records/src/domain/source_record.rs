//! Records source identities are owner facts, never cross-owner SQL selectors.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};

pub fn validate_reference(reference: &SourceReference) -> Result<(), BusinessError> {
    if reference.id.is_empty()
        || reference.id.encode_utf16().count() > 180
        || reference.id.contains('\0')
        || reference.expected_version.is_some_and(|v| v <= 0)
    {
        return Err(BusinessError::invalid(
            "SOURCE_RECORD_INVALID",
            "Provide a valid source record.",
        ));
    }
    Ok(())
}
pub fn require_matching_source(
    reference: &SourceReference,
    company_id: &str,
    source: &SourceRecord,
) -> Result<(), BusinessError> {
    if source.kind != reference.kind || source.id != reference.id || source.company_id != company_id
    {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SOURCE_RECORD_NOT_FOUND",
            "Source record is unavailable in this work scope.",
        ));
    }
    if reference
        .expected_version
        .is_some_and(|version| version != source.version)
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SOURCE_RECORD_CHANGED",
            "The source version changed. Reload its owning workspace before handing it off.",
        ));
    }
    Ok(())
}
