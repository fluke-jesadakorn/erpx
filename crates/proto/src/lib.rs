#![doc = "Versioned ERPX Tonic contracts."]

/// The descriptor set generated from the checked-in protobuf sources. The
/// build script compares it with the approved development baseline.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/erpx_descriptor_set.bin"));

pub const APPROVED_DESCRIPTOR_SHA256: &str =
    include_str!("../descriptor/erpx_descriptor_set.sha256");

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
