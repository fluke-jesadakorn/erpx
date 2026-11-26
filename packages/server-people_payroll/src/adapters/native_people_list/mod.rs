//! Native owner slice for the `GET .../people` directory list served by the
//! `packages/feature-people/src/routes/api/v1/holdings/[holdingSlug]/people/route.ts`
//! family. The axum owner executes the Company-scoped statement with sqlx,
//! validates and shapes the response in Rust, and no TypeScript prepare/batch
//! fallback is part of this slice.
//!
//! Composition and the Next route deletion are lead-owned; this module owns
//! only the slice and its public constructors.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{ListFuture, ListPeopleQuery, PeopleListRead, list_people};
pub use domain::{
    MAX_ROWS, PersonListRow, filter, invalid, missing, unavailable, valid_id,
};
pub use http::{
    PeopleListTarget, router_for_holdings, router_with_reader,
};
pub use postgres::{PgPeopleListRead, SQL_ORACLE_SHA256, SQL_PEOPLE_LIST};
