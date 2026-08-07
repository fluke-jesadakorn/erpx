//! Production typed-v1 Console transports for Wave 9 composed reads and
//! orchestration.

// Registered before final Console composition switches to these adapters.
#![allow(dead_code, unused_imports)]

mod common;
mod customer_360;
mod operations;

pub(crate) use common::Wave9Route;
pub(crate) use customer_360::Customer360GrpcTransport;
pub(crate) use operations::OperationsGrpcTransport;
