//! Native owner port for the customer-access sources read.
//!
//! The PostgreSQL adapter executes the SQL; this HTTP router is the mount
//! point composition binds for the customer-access route family. No TypeScript
//! statement mirror and no fallback remain for this operation.
pub use crate::adapters::access_sources_http::{
    CUSTOMER_ACCESS_SOURCES_PATH, AccessSourcesBackend, AccessSourcesParams, AccessSourcesRequest,
    AccessSourcesTarget, customer_access_sources_router,
};
pub use crate::adapters::access_sources_postgres::PgAccessSources;
pub use crate::application::access_sources::{
    AccessSourcesSource, SourcesFuture, customer_access_sources,
};
pub use crate::domain::access::{AccessSourceRow, MAX_SOURCE_ROWS};
pub use crate::domain::access_sources::{
    AccessSourcesQuery, invalid, unavailable, validate_rows,
};
