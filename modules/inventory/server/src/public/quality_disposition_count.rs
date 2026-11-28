//! Posted Inventory Count identity for Quality provenance, not a grant.
//!
//! This is a pre-effect witness: it only proves that an Inventory Count is
//! posted and points back to the requested Quality hold. It is not retained
//! post-effect proof and does not attest the Quality parent, reciprocal parent
//! Count ID, disposition event, child arithmetic, source authority, or purchase
//! decisions; callers must retain those separately.
pub use crate::adapters::quality_disposition_count::{
    CurrentQualityDispositionCount, QualityDispositionCountRequest,
    inspect_quality_disposition_count, lock_quality_disposition_count_sources,
};
