use crate::platform::{CapabilityStatus, ClientTarget, NativeCapability, NativePlatformBindings};

/// macOS binding descriptor. Passkeys remain blocked until the shared payload
/// contract can carry a complete AuthenticationServices ceremony.
#[derive(Debug, Clone, Copy, Default)]
pub struct MacOsBindings;

impl NativePlatformBindings for MacOsBindings {
    fn target(&self) -> ClientTarget {
        ClientTarget::MacOs
    }

    fn capability(&self, service: NativeCapability) -> CapabilityStatus {
        match service {
            NativeCapability::Http
            | NativeCapability::SecureStorage
            | NativeCapability::Navigation
            | NativeCapability::Preferences
            | NativeCapability::ExternalAction => CapabilityStatus::Available,
            NativeCapability::Passkey => {
                CapabilityStatus::Blocked("macOS passkey contract and ceremony are not certified")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_integrated_services_as_available() {
        let bindings = MacOsBindings;
        assert_eq!(
            bindings.capability(NativeCapability::ExternalAction),
            CapabilityStatus::Available
        );
        assert!(matches!(
            bindings.capability(NativeCapability::Passkey),
            CapabilityStatus::Blocked(_)
        ));
    }
}
