//! Native public Marketing form source read.
//!
//! The owner now executes the active-form source statement itself and returns
//! the complete `PublicMarketingForm` payload. The retained TypeScript path is
//! transport/planned SQL only until composition mounts this router and the lead
//! deletes the shared route and its re-exports; there is no TS fallback here.
pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{PublicFormFuture, PublicFormSource, read};
pub use domain::{
    FormQuery, FormRow, MAX_ID_LEN, SOURCE_KEYS, invalid, missing, valid_id, valid_row, validate,
};
pub use http::{
    PublicFormTarget, router, router_for_holdings, router_with_reader, router_with_sources,
};
pub use postgres::{PgPublicFormSource, SOURCE_SQL, rendered_sql};
