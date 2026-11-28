//! Current stock-availability projection and its source fence.
pub use crate::adapters::availability_read::{lock_availability_sources, read_availability};
pub use crate::domain::availability::{AvailabilityRow, MAX_ROWS, validate_row};
