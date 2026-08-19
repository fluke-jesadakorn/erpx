//! Production typed-v1 transport for the Quality pilot.
//!
//! Generated protobuf and Tonic remain behind the feature gateway boundary.

// The integrator registers this leaf before the Console composition root
// constructs it during the coordinated v1 cutover.
#![allow(dead_code, unused_imports)]

// Quality uses the same envelope/receipt protocol as the later holding
// contexts through the single shared transport implementation.
pub(super) use super::wave6_7::common;
mod transport;

pub(crate) use common::Wave67Route as QualityRoute;
pub(crate) use transport::QualityGrpcTransport;
