#![doc = "Projects bounded context: governed work graphs and typed owner-context handoffs."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod ports;

pub use application::*;
pub use domain::*;
pub use ports::*;
