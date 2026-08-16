#![doc = "E-commerce bounded context for governed headless commerce orchestration."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod fulfillment;
pub mod ports;

pub use application::*;
pub use domain::*;
pub use fulfillment::*;
pub use ports::*;
