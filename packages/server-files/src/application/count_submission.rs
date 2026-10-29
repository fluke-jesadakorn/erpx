//! Portable policy boundary; caller retains real Records, Inventory and IAM sources.
pub use crate::domain::count_submission::{
    CountBindingFeatures, CountBindingLink, CountBindingSet, CountManifestItem,
    prepare_count_binding, unique_set_ids,
};
