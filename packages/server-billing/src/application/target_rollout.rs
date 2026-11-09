//! Billing target-rollout readiness read. The repository supplies only the
//! Control evidence rows; the domain evaluates the two shadow cycles, margin
//! and capacity. No rollout approval or price assignment is performed here.
use crate::domain::target_rollout::{
    AvailableOperatingCostsRow, CapacityReviewRow, EffectivePriceRow, RatedShadowPeriodRow,
    RolloutUsageSnapshotRow, TargetRolloutFacts, TargetRolloutMonthFacts, TargetRolloutReadiness,
    evaluate_target_rollout_readiness, previous_month,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use std::future::Future;

pub trait TargetRolloutRepository {
    type Error: From<BusinessError> + Send;

    fn effective_price(
        &mut self,
        effective_month: &str,
    ) -> impl Future<Output = Result<Option<EffectivePriceRow>, Self::Error>> + Send;

    fn rated_shadow_period(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<RatedShadowPeriodRow>, Self::Error>> + Send;

    fn usage_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<RolloutUsageSnapshotRow>, Self::Error>> + Send;

    fn available_operating_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<AvailableOperatingCostsRow>, Self::Error>> + Send;

    fn capacity_review(
        &mut self,
        holding: &HoldingId,
        effective_month: &str,
    ) -> impl Future<Output = Result<Option<CapacityReviewRow>, Self::Error>> + Send;
}

/// Read and evaluate a target rollout without approving it. `storage_limit` is
/// the trusted Beta storage limit resolved by composition.
pub async fn read_target_rollout_readiness<R: TargetRolloutRepository>(
    repository: &mut R,
    holding: &HoldingId,
    effective_month: &str,
    minimum_margin_bps: i64,
    now: TimestampMillis,
    storage_limit: i64,
) -> Result<TargetRolloutReadiness, R::Error> {
    let second = previous_month(effective_month, 1)?;
    let first = previous_month(effective_month, 32)?;
    read_target_rollout_readiness_for_months(
        repository,
        holding,
        effective_month,
        &first,
        &second,
        minimum_margin_bps,
        now,
        storage_limit,
    )
    .await
}

/// Evaluate a rollout for an exact pair of completed shadow months. The
/// migration command uses the two months that ended before the current month
/// so a future effective month can be approved without an unclosed shadow
/// cycle. The read-only installed parity above keeps effective-month arithmetic.
pub async fn read_target_rollout_readiness_for_months<R: TargetRolloutRepository>(
    repository: &mut R,
    holding: &HoldingId,
    effective_month: &str,
    shadow_month_1: &str,
    shadow_month_2: &str,
    minimum_margin_bps: i64,
    now: TimestampMillis,
    storage_limit: i64,
) -> Result<TargetRolloutReadiness, R::Error> {
    crate::domain::target_pricing::month_bounds(effective_month)?;
    crate::domain::target_pricing::month_bounds(shadow_month_1)?;
    crate::domain::target_pricing::month_bounds(shadow_month_2)?;
    if shadow_month_1 >= shadow_month_2 || shadow_month_2 >= effective_month {
        return Err(BusinessError::invalid(
            "INVALID_BILLING_MONTH",
            "Shadow months must be completed before the effective month.",
        )
        .into());
    }
    if !(0..=10_000).contains(&minimum_margin_bps) {
        return Err(BusinessError::invalid(
            "INVALID_MARGIN_THRESHOLD",
            "The Billing target rollout request is invalid.",
        )
        .into());
    }
    let first = shadow_month_1.to_owned();
    let second = shadow_month_2.to_owned();
    let price = repository.effective_price(effective_month).await?;
    let mut months = Vec::with_capacity(2);
    for month in [first, second] {
        let rated = repository.rated_shadow_period(holding, &month).await?;
        let snapshot = repository.usage_snapshot(holding, &month).await?;
        let operating_costs = repository
            .available_operating_costs(holding, &month)
            .await?;
        months.push(TargetRolloutMonthFacts {
            month,
            rated,
            snapshot,
            operating_costs,
        });
    }
    let capacity = repository.capacity_review(holding, effective_month).await?;
    let months: [TargetRolloutMonthFacts; 2] = months
        .try_into()
        .map_err(|_| BusinessError::invalid("INVALID_BILLING_MONTH", "Invalid Billing month."))?;
    evaluate_target_rollout_readiness(
        holding.as_str(),
        effective_month,
        minimum_margin_bps,
        now.get(),
        storage_limit,
        &TargetRolloutFacts {
            price,
            months,
            capacity,
        },
    )
    .map_err(Into::into)
}
