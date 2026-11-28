//! Public surface for the native R3 Manufacturing posting-setup prerequisites
//! branch. Composition mounts the router and supplies the Account session,
//! Company scope, Finance authority, purchased admission, locks, `contextKey`
//! and the complete retained view; consumers never import adapter internals.
pub use crate::adapters::native_r3_posting_prereq_http::{
    prerequisites_router, PrereqHttpBackend, PrereqHttpFuture, PrereqHttpRequest,
    PrereqHttpResponse, PrereqHttpTarget,
};
pub use crate::adapters::native_r3_posting_prereq_read::{
    posting_prereq_in_unit, PgPostingPrereqRead, PERIODS_SQL, RATES_SQL,
};
pub use crate::application::native_r3_posting_prereq::{
    posting_prerequisites, PrereqFuture, PrereqRepository,
};
pub use crate::domain::native_r3_posting_prereq::project_prerequisites;
