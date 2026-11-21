//! Independent vectors from the TS development-package-enforcement policy.
//! These do not establish trusted configuration propagation or release/purchase authority.
#[path = "../../../src/composition/src/customer_portal_mode.rs"]
mod mode;
use erpx_server_commercial::public::purchases::PackageEnforcementMode as Mode;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::cell::Cell;

#[test]
fn raw_values_and_lazy_environment_match_switch() {
    let cases = [
        (None, Some(Mode::Disabled)), (Some("disabled"), Some(Mode::Disabled)),
        (Some("shadow"), Some(Mode::Shadow)), (Some("enforced"), Some(Mode::Enforced)),
        (Some(""), None), (Some("SHADOW"), None), (Some(" shadow"), None),
        (Some("enforced\n"), None), (Some("unknown"), None),
    ];
    for (raw, expected) in cases {
        let calls = Cell::new(0);
        let actual = mode::resolve_portal_mode(raw, || { calls.set(calls.get()+1); Ok("local".into()) });
        match expected {
            Some(value) => assert_eq!(actual.unwrap(), value, "{raw:?}"),
            None => assert_eq!(actual.unwrap_err(), BusinessError::new(FailureKind::Unavailable,"PACKAGE_ENFORCEMENT_CONFIGURATION","Package enforcement is unavailable."), "{raw:?}"),
        }
        assert_eq!(calls.get(), usize::from(matches!(raw, Some("shadow" | "enforced"))), "{raw:?}");
    }
}

#[test]
fn enabled_requires_local_and_propagates_environment_failure() {
    for raw in ["shadow", "enforced"] {
        for environment in ["production", "staging", "", "LOCAL"] {
            assert_eq!(mode::resolve_portal_mode(Some(raw), || Ok(environment.into())).unwrap_err(), BusinessError::new(FailureKind::Unavailable,"PACKAGE_ENFORCEMENT_ENVIRONMENT","Package enforcement is unavailable in this environment."));
        }
        let failure = BusinessError::new(FailureKind::Unavailable,"RUNTIME_FAILURE","Runtime resolution failed.");
        assert_eq!(mode::resolve_portal_mode(Some(raw), || Err(failure.clone())).unwrap_err(), failure);
    }
    for raw in [None, Some("disabled"), Some("invalid")] {
        let result = mode::resolve_portal_mode(raw, || -> Result<String, BusinessError> { panic!("environment must not be resolved") });
        if raw == Some("invalid") { assert_eq!(result.unwrap_err().code,"PACKAGE_ENFORCEMENT_CONFIGURATION"); } else { assert_eq!(result.unwrap(),Mode::Disabled); }
    }
}
