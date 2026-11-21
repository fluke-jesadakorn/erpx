//! Inbound JSON numeric representation helpers, not business positivity or monetary rules.
//! URL query strings need a separate typed query adapter; JSON strings remain rejected.
pub use crate::adapters::json_numbers::{
    MAX_SAFE_INTEGER, deserialize_optional_safe_i64, deserialize_optional_safe_u64,
    deserialize_optional_u32, deserialize_safe_i64, deserialize_safe_u64, deserialize_u32,
};
