//! Public surface for the native R3 RFQ owner slice.
//!
//! Composition mounts the router, supplies the backend that resolves the
//! Account session, Company scope, purchased-package admission, workflow write
//! and workflow command, and runs the owner SQL on the caller's Holding unit.
//! Consumers never import adapter internals.
pub use crate::adapters::native_r3_rfq_http::{
    rfq_router, RfqHttpBackend, RfqHttpFuture, RfqHttpRequest, RfqHttpResponse, RfqHttpTarget,
};
pub use crate::adapters::native_r3_rfq_read::{rfq_read_in_unit, PgRfqRead, DETAIL_SQL, WORKSPACE_SQL};
pub use crate::application::native_r3_rfq::{rfq_detail, rfq_workspace, RfqFuture, RfqReadRepository};
pub use crate::domain::native_r3_rfq::{
    current_offers, source_blocked, validate_detail, validate_workspace, RfqActor, Row,
};
