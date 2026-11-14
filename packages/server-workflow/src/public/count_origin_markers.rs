//! Workflow receipt markers; these do not establish Count or Finance authority.
pub use crate::adapters::count_origin_markers::{
    CountOriginMarkers, CurrentCountOriginMarkers, inspect_count_origin_markers,
    lock_count_origin_marker_sources,
};
