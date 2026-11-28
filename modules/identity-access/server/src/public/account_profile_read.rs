//! Stable owner surface for the native Account profile read.
//!
//! Composition mounts [`account_profile_read_router`] on the private service
//! with one `AccountProfileReadTarget` per registered Holding Host, and builds
//! each reader from [`PgAccountProfileReader`] with the Control pool.

pub use crate::adapters::account_profile_http::{
    AccountProfileReadTarget, account_profile_read_router,
};
pub use crate::adapters::account_profile_postgres::PgAccountProfileReader;
pub use crate::application::account_profile::{
    AccountProfileFuture, AccountProfileReader, ProfileQuery, read_profile,
};
pub use crate::domain::account_profile::{
    AccountPreferences, AccountProfile, ProfileRecord, project_profile, valid_user_id,
};
