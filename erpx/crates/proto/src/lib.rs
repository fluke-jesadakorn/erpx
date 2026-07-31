#![doc = "Versioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. G13
/// compares this against an approved external baseline when one is supplied;
/// absence of that baseline remains a queued acceptance blocker.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub mod erpx {
    pub mod common {
        pub mod v1 {
            tonic::include_proto!("erpx.common.v1");
        }
    }

    pub mod platform {
        pub mod v1 {
            tonic::include_proto!("erpx.platform.v1");
        }
    }
}

pub mod common {
    pub mod v1 {
        pub use crate::erpx::common::v1::*;
    }

    pub use v1::*;
}

pub use erpx::platform::v1 as platform;
