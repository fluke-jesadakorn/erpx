//! Native migration lane 11 for identity_access.
//!
//! The owner now serves the person-identity read family that used to run as
//! inline TypeScript SQL in `adapters/iam-person.ts`: alias expansion and
//! active-successor resolution over the Identity Access-owned `iam_members`
//! table. The TypeScript adapter is a strict, fail-closed transport only.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::PersonIdentityQuery;
pub use domain::{
    IdentityIdsRequest, IdentityIdsResponse, PrincipalPair, PrincipalsRequest, PrincipalsResponse,
    SuccessorRow,
};
pub use http::{
    LaneTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgPersonIdentityQuery, identity_ids_sql, principals_snapshot_sql, principals_sql};
