//! Native migration lane 12 for identity_access.
//!
//! The owner-local People-template attribute read set (`attributes`,
//! `activeRole`, `groupReferences`) is executed by this axum/sqlx module. The
//! TypeScript adapter transports bounded company/role/value inputs and pins the
//! response; there is no TypeScript SQL fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{AttributeReadFuture, PeopleTemplateRead};
pub use domain::{AttributePage, AttributeRead, LaneTarget, ReadPage};
pub use http::{router, router_for_holdings, router_with_reader};
pub use postgres::{
    ACTIVE_ROLE_SQL, ATTRIBUTES_SQL, GROUP_REFERENCES_SQL, PgPeopleTemplateRead,
};
