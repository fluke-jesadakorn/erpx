//! Read-only selected context, never an AuthorizedWorkContext or queue grant.
pub const CONTRACT_VERSION: &str = "identity.account-work-presentation.v1";
pub use crate::adapters::account_work_presentation::{
    AccountWorkPresentation, lock_account_work_presentation_sources, read_account_work_presentation,
};
