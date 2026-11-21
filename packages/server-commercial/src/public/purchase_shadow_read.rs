//! Staged read-phase telemetry only; checked-phase telemetry is not implemented.
pub use crate::adapters::purchase_shadow_read::{
    PurchaseShadowReadObservation, inspect_purchase_shadow_read,
    write_purchase_shadow_read_observation,
};
