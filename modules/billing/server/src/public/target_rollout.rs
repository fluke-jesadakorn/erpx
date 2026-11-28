//! Billing target-rollout readiness read over Control evidence.
pub use crate::adapters::target_rollout::{
    PgTargetRolloutRepository, read_target_rollout_readiness,
};
pub use crate::application::target_rollout::{
    TargetRolloutRepository, read_target_rollout_readiness as read_target_rollout_readiness_with,
};
pub use crate::domain::target_rollout::{
    AvailableOperatingCostsRow, CapacityReviewRow, EffectivePriceRow, RatedShadowPeriodRow,
    RolloutUsageSnapshotRow, TargetMarginReport, TargetRolloutCapacity, TargetRolloutFacts,
    TargetRolloutMonthFacts, TargetRolloutReadiness, evaluate_target_rollout_readiness,
    previous_month,
};
