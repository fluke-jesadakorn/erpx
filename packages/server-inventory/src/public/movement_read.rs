//! Current inventory movement-history projection and its source fence.
pub use crate::adapters::movement_read::{lock_movement_sources, read_movements};
pub use crate::domain::movement::{
    MAX_ROWS, MovementRow, encode_cursor, parse_cursor, validate_row,
};
