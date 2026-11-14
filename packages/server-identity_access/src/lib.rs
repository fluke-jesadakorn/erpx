mod adapters;
mod application;
mod domain;
pub mod public;

/// Isolated local PostgreSQL fixture shared by crate-level native adapter
/// proofs. Only compiled for tests; database lanes run it explicitly.
#[cfg(test)]
#[path = "../../../tests/support/rust-postgres.rs"]
pub(crate) mod people_fixture;
