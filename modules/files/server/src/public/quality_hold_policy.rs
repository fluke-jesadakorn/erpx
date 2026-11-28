//! Pure Quality-hold target policy; callers must retain owner and purchase witnesses.
pub use crate::domain::quality_hold_policy::{
    QualityHoldDescriptor, QualityHoldPolicyError, QualityHoldScope,
    quality_hold_required_permission, quality_hold_target,
};
