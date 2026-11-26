//! Native owner port for the verified-customer portal support-thread read.
//!
//! The PostgreSQL adapter executes the SQL; the HTTP router is the mount point
//! composition binds for the Customer Portal tickets family.

pub use crate::adapters::portal_support_threads_http::{
    CUSTOMER_THREADS_PATH, CustomerThreadBackend, CustomerThreadTarget, CustomerThreadsRequest,
    portal_support_threads_router,
};
pub use crate::adapters::portal_support_threads_postgres::PgPortalSupportThreads;
pub use crate::application::portal_support_threads::{
    PortalThreadSource, ThreadsFuture, customer_support_threads,
};
pub use crate::domain::portal_support_threads::*;
