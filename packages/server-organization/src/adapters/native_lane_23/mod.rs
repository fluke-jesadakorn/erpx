//! Native migration lane 23 for organization.
//!
//! The directory hierarchy read is owned end-to-end here: owner-local SQL and
//! projection in `postgres`, invariants in `domain`, a SQL-free application
//! handler, and an axum ingress that only transports the validated response.
//! TypeScript keeps a strict, fail-closed client and no SQL or calculation.
mod application;
mod domain;
mod http;
mod postgres;

pub use application::{DirectoryHierarchyRead, read_directory_hierarchy};
pub use domain::{
    DirectoryHierarchy, DirectoryLevel, DirectoryNode, DirectoryPosition, MAX_ROWS,
};
pub use http::{LaneTarget, router};
pub use postgres::{PgDirectoryHierarchyRead, NODES_SQL, LEVELS_SQL, POSITIONS_SQL};
