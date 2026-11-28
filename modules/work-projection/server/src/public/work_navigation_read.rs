//! Native Work Projection navigation read contract.
pub use crate::adapters::work_navigation_read::{
    NativeWorkNavigationRequest, read_native_work_navigation, read_work_navigation_snapshot,
};
pub use crate::domain::work_navigation::{
    WorkAttentionFact, WorkNavigationFact, WorkViewAvailabilityFact,
};
