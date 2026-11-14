//! Field source-record version contract.
use erpx_server_shared::public::{BusinessError, FailureKind};
pub fn check_version(current: i64, expected: Option<f64>) -> Result<(), BusinessError> {
    if expected.is_some_and(|v| v != current as f64) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SOURCE_RECORD_CHANGED",
            "The source version changed. Reload its owning workspace before handing it off.",
        ));
    }
    Ok(())
}
