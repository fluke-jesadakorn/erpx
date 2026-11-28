//! Native Manufacturing production accounting-profiles owner read.
//! Stable owner surface for composition: domain facts, the query/handler port,
//! the real sqlx reader and the axum router constructor.
pub use crate::adapters::production_profiles_postgres::PgProductionProfilesRead;
pub use crate::application::production_profiles::{
    AccountingProfilesHandler, AccountingProfilesQuery, AccountingProfilesRead,
    AccountingProfilesReadFuture,
};
pub use crate::domain::production_profiles::{
    ACCOUNTING_PROFILES_SQL, AccountingProfileRow, MAX_IDENTIFIER, invalid, validate_company,
};
pub use crate::adapters::production_profiles_http::{
    ProductionProfilesTarget, router, router_for_holdings, router_with_reader,
};
