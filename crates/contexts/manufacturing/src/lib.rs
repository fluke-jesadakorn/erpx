#![doc = "Manufacturing bounded context for discrete and process execution."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod ports;
pub mod record;

pub use application::{ManufacturingCommandService, ManufacturingError};
pub use domain::*;
pub use ports::*;
pub use record::*;
