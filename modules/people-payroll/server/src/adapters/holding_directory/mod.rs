//! Native migration of the holding people-public-directory read.
//!
//! The axum owner owns the exact Holding target, transports the Account
//! authority port and executes every directory statement with sqlx in this
//! crate. TypeScript keeps a strict, SQL-free client and no fallback.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    HoldingDirectoryAuthority, HoldingDirectoryFuture, HoldingDirectoryRead,
    read_holding_directory,
};
pub use domain::{
    DirectoryCompany, DirectoryLevel, DirectoryNode, DirectoryPerson, DirectoryPosition,
    HoldingDirectoryFacts, HoldingDirectoryScope, HoldingPeopleDirectory,
};
pub use http::{
    HoldingDirectoryTarget, router, router_with_reader, unique_header,
};
pub use postgres::{
    ASSIGNMENT_ACTIVE_SQL, COMPANIES_SQL, LEVELS_SQL, NODES_SQL, PEOPLE_SQL, POSITIONS_SQL,
    PgHoldingDirectoryRead,
};
