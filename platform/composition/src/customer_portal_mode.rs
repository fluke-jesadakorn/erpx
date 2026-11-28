//! Unmounted Portal switch policy. The caller must obtain both inputs from
//! trusted runtime configuration; neither this function nor its result grants
//! purchase, release, session, or other authority.
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Matches the TS switch's exact raw values and resolves the runtime
/// environment only for valid enabled modes. Environment resolution errors
/// propagate unchanged, as they do from `runtimeEnv()`.
pub(crate) fn resolve_portal_mode<F>(
    raw: Option<&str>,
    resolved_environment: F,
) -> Result<PackageEnforcementMode, BusinessError>
where
    F: FnOnce() -> Result<String, BusinessError>,
{
    let mode = match raw.unwrap_or("disabled") {
        "disabled" => return Ok(PackageEnforcementMode::Disabled),
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => return Err(BusinessError::new(FailureKind::Unavailable, "PACKAGE_ENFORCEMENT_CONFIGURATION", "Package enforcement is unavailable.")),
    };
    if resolved_environment()? != "local" {
        return Err(BusinessError::new(FailureKind::Unavailable, "PACKAGE_ENFORCEMENT_ENVIRONMENT", "Package enforcement is unavailable in this environment."));
    }
    Ok(mode)
}
