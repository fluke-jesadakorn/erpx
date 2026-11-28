//! Native Customer 360 timeline activity read ports and PostgreSQL adapter.
//!
//! Read-only native replacement for the retained TypeScript
//! `customerTimelineEvents` projection over the same federated sources.
pub use crate::adapters::timeline::FinalRootEventSources;
pub use crate::adapters::timeline_activity::PgCustomerTimelineActivityRead;
pub use crate::application::timeline::{
    CustomerTimelineFuture, CustomerTimelineSupportEntitlement,
};
pub use crate::application::timeline_activity::{
    CustomerTimelineActivityRepository, customer_timeline_activity,
};
pub use crate::domain::timeline::{CustomerTimelineActor, CustomerTimelineView};
pub use crate::domain::timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow, MAX_ACTIVITY_ROWS,
};
