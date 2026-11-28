//! Public surface for the additive native Organization authority read slice.
pub use crate::adapters::native_r3_authority::{
    AuthorityOverviewRead, AuthoritySubpath, OverviewActor, OverviewFacts, OverviewFuture,
    DEPARTMENTS_SQL, HISTORY_SQL, POLICIES_SQL, SEATS_SQL, classify, envelope, handle_overview,
    project_overview,
};
