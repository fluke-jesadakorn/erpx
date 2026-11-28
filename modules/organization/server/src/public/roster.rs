//! Native Organization roster read contract.
//!
//! Consumers use these named exports; no SQL relation or private adapter type
//! escapes the crate. The roster pair is bounded to the same transaction that
//! authorizes the workspace read.
pub use crate::adapters::postgres_roster::PgOrganizationRoster;
pub use crate::application::roster::{
    OrganizationRosterRead, OrganizationRosterRepository, read_organization_roster,
};
