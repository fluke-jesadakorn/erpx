//! Public surface for the native R3 Manufacturing posting-setup owner slice.
//!
//! Composition mounts the router, supplies the backend that resolves the
//! Account session, Company scope, Finance authority, purchased-package
//! admission and workflow command, and runs the owner SQL on the caller's
//! Holding transaction. Consumers never import adapter internals.
pub use crate::adapters::native_r3_posting_setup_http::{
    posting_setup_router, PostingSetupHttpBackend, PostingSetupHttpFuture, PostingSetupHttpRequest,
    PostingSetupHttpResponse, PostingSetupHttpTarget,
};
pub use crate::adapters::native_r3_posting_setup_read::{
    posting_setup_read_in_unit, PgPostingSetupRead, POSTING_SETUP_SQL,
};
pub use crate::application::native_r3_posting_setup::{
    manufacturing_posting_setup, PostingSetupFuture, PostingSetupRepository,
};
pub use crate::domain::native_r3_posting_setup::{validate_rows, PostingSetupActor, Row};
