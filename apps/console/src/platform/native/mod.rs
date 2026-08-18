//! Native platform binding selection.
//!
//! macOS is the only native release target in this cycle. The remaining
//! modules intentionally satisfy the shared interface while reporting every
//! service as deferred.

mod deferred;
pub mod macos;

pub mod windows {
    pub type Bindings = super::deferred::DeferredBindings<1>;
}

pub mod linux {
    pub type Bindings = super::deferred::DeferredBindings<2>;
}

pub mod ios {
    pub type Bindings = super::deferred::DeferredBindings<3>;
}

pub mod android {
    pub type Bindings = super::deferred::DeferredBindings<4>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{CapabilityStatus, NativeCapability, NativePlatformBindings};

    #[test]
    fn future_targets_are_explicitly_deferred() {
        let bindings: [&dyn NativePlatformBindings; 4] = [
            &windows::Bindings::default(),
            &linux::Bindings::default(),
            &ios::Bindings::default(),
            &android::Bindings::default(),
        ];
        for binding in bindings {
            assert_eq!(
                binding.capability(NativeCapability::Http),
                CapabilityStatus::Deferred
            );
            assert_eq!(
                binding.capability(NativeCapability::Passkey),
                CapabilityStatus::Deferred
            );
        }
    }
}
