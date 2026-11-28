//! Stable public surface for the native Commerce return-line read.
//!
//! Composition mounts [`router_for_holdings`] with the registered Holding
//! targets; no SQL, fallback or payload shaping stays in TypeScript.
pub use crate::adapters::native_return_lines::*;
