use crate::platform::{CapabilityStatus, ClientTarget, NativeCapability, NativePlatformBindings};

#[derive(Debug, Clone, Copy, Default)]
pub struct DeferredBindings<const TARGET: u8>;

impl<const TARGET: u8> NativePlatformBindings for DeferredBindings<TARGET> {
    fn target(&self) -> ClientTarget {
        match TARGET {
            1 => ClientTarget::Windows,
            2 => ClientTarget::Linux,
            3 => ClientTarget::Ios,
            4 => ClientTarget::Android,
            _ => unreachable!("deferred target marker must be declared in native/mod.rs"),
        }
    }

    fn capability(&self, _service: NativeCapability) -> CapabilityStatus {
        CapabilityStatus::Deferred
    }
}
