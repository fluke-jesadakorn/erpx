//! Compatibility module for the G7 Operations completion slice.
//!
//! The implementation is kept in the adjacent completion file so the
//! existing untracked Folio/ERPX bridge can remain intact while Rust's module
//! tree exposes the public `operations::completion` namespace.

include!("../operations_completion.rs");
