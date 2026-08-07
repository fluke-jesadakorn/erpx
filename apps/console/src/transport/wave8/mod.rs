//! Production typed-v1 transports for the Wave 8 Finance, Legal and Support
//! bounded contexts.
//!
//! This is the only Wave 8 Console module that knows about generated protobuf
//! or Tonic. Feature gateways continue to depend on their context-core types.

// The integrator registers this transport before the Console composition root
// switches its Wave 8 gateways, so the crate-private constructors are allowed
// to remain dormant at that barrier.
#![allow(dead_code, unused_imports)]

mod common;
mod finance;
mod legal;
mod support;

pub(crate) use common::Wave8Route;
pub(crate) use finance::FinanceGrpcTransport;
pub(crate) use legal::LegalGrpcTransport;
pub(crate) use support::SupportGrpcTransport;
