#![doc = "Framework-free Collaboration bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod ports;

pub use application::*;
pub use domain::*;
pub use ports::*;
