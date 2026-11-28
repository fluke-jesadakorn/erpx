//! Public handle for the mounted Manage owner read-plan route.
//!
//! The router stays authoritative in `adapters::owner_write_http`; this module
//! only exposes it to composition so the `manage/command-plan` kind resolver is
//! mounted through the same target boundary as owner writes.
pub use crate::adapters::owner_write_http::owner_write_router as native_owner_read_router;
