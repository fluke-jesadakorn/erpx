#![doc = "Recruiting and Talent bounded context."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod ports;
pub mod record;

pub use application::*;
pub use domain::*;
pub use ports::*;
pub use record::*;
