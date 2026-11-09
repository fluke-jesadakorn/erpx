//! Pure Assets switch policy. No runtime/environment discovery is performed.
use crate::domain::packages::PackageEnforcementMode;
use erpx_server_shared::public::{BusinessError,FailureKind};

/// An environment value supplied by trusted composition after resolving the
/// runtime's actual environment semantics. Constructing this wrapper does not
/// itself authenticate the caller, deployment, process or configuration.
pub struct ResolvedPackageEnvironment(String);
impl ResolvedPackageEnvironment {
    pub fn from_trusted_runtime(value: impl Into<String>) -> Self { Self(value.into()) }
}
/// `raw` must come from the runtime-owned Assets switch, not a request or a
/// purchased grant. For a valid disabled/missing value, do not resolve the
/// environment at all. Other values resolve it exactly once, on demand.
pub fn resolve_assets_package_mode<F>(raw:Option<&str>, resolved_environment:F)->Result<PackageEnforcementMode,BusinessError>
where F:FnOnce()->Result<ResolvedPackageEnvironment,BusinessError> {
    let mode=match raw.unwrap_or("disabled") {
        "disabled"=>return Ok(PackageEnforcementMode::Disabled),
        "shadow"=>PackageEnforcementMode::Shadow,
        "enforced"=>PackageEnforcementMode::Enforced,
        _=>return Err(BusinessError::new(FailureKind::Unavailable,"PACKAGE_ENFORCEMENT_CONFIGURATION","Package enforcement is unavailable.")),
    };
    if resolved_environment()?.0!="local" {return Err(BusinessError::new(FailureKind::Unavailable,"PACKAGE_ENFORCEMENT_ENVIRONMENT","Package enforcement is unavailable in this environment."));}
    Ok(mode)
}
