//! Validated Runtime roster data, not a sealed attestation or database grant.
//! JavaScript accepts distinct array tokens that stringify identically; returned
//! token strings therefore must not be assumed unique.
pub use crate::domain::native_engineering_roster::{NativeEngineeringEntry,NativeEngineeringRoster,validate_native_engineering_roster};
