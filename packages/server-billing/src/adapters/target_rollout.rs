//! PostgreSQL read adapter for Billing target-rollout readiness. The caller
//! owns the Control connection and transaction; this adapter never writes,
//! commits, calls a provider or approves a rollout.
use crate::application::target_rollout::TargetRolloutRepository;
use crate::domain::target_rollout::{
    AvailableOperatingCostsRow, CapacityReviewRow, EffectivePriceRow, RatedShadowPeriodRow,
    RolloutUsageSnapshotRow, TargetRolloutReadiness,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::{PgConnection, Row};
use std::future::Future;

fn source_invalid() -> BusinessError {
    crate::domain::target_rollout::source_invalid()
}

pub struct PgTargetRolloutRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTargetRolloutRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl TargetRolloutRepository for PgTargetRolloutRepository<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn effective_price(
        &mut self,
        effective_month: &str,
    ) -> impl Future<Output = Result<Option<EffectivePriceRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT id,included_original_bytes::text included_original_bytes,\
                 seat_terms_status,rating_enabled FROM billing_target_prices \
                 WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",
            )
            .bind(effective_month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(EffectivePriceRow {
                    id: row.try_get("id")?,
                    included_original_bytes: row
                        .try_get("included_original_bytes")
                        .map_err(|_| source_invalid())?,
                    seat_terms_status: row.try_get("seat_terms_status")?,
                    rating_enabled: row.try_get("rating_enabled")?,
                })
            })
            .transpose()
        }
    }

    fn rated_shadow_period(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<RatedShadowPeriodRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT state,mode,total_satang::text total_satang,price_id,\
                 meter_snapshot_id,lines_json FROM billing_target_rated_periods \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(RatedShadowPeriodRow {
                    state: row.try_get("state")?,
                    mode: row.try_get("mode")?,
                    total_satang: row.try_get("total_satang")?,
                    price_id: row.try_get("price_id")?,
                    meter_snapshot_id: row.try_get("meter_snapshot_id")?,
                    lines_json: row.try_get("lines_json")?,
                })
            })
            .transpose()
        }
    }

    fn usage_snapshot(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Option<RolloutUsageSnapshotRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT snapshot_id FROM billing_usage_snapshots \
                 WHERE holding_id=$1 AND month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(RolloutUsageSnapshotRow {
                    snapshot_id: row.try_get("snapshot_id")?,
                })
            })
            .transpose()
        }
    }

    fn available_operating_costs(
        &mut self,
        holding: &HoldingId,
        month: &str,
    ) -> impl Future<Output = Result<Vec<AvailableOperatingCostsRow>, Self::Error>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT o.category,o.amount_satang::text amount_satang,e.source_state \
                 FROM billing_target_operating_costs o \
                 JOIN billing_evidence e ON e.id=o.evidence_id \
                 WHERE o.holding_id=$1 AND o.month=$2",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(AvailableOperatingCostsRow {
                        category: row.try_get("category")?,
                        amount_satang: row
                            .try_get("amount_satang")
                            .map_err(|_| source_invalid())?,
                        source_state: row.try_get("source_state")?,
                    })
                })
                .collect()
        }
    }

    fn capacity_review(
        &mut self,
        holding: &HoldingId,
        effective_month: &str,
    ) -> impl Future<Output = Result<Option<CapacityReviewRow>, Self::Error>> + Send {
        async move {
            let row = sqlx::query(
                "SELECT r.projected_original_bytes::text projected_original_bytes,\
                 r.primary_available_bytes::text primary_available_bytes,\
                 r.backup_available_bytes::text backup_available_bytes,\
                 r.reserved_bytes::text reserved_bytes,\
                 r.company_limit_bytes::text company_limit_bytes,e.source_state \
                 FROM billing_target_capacity_reviews r \
                 JOIN billing_evidence e ON e.id=r.evidence_id \
                 WHERE r.holding_id=$1 AND r.effective_month=$2",
            )
            .bind(holding.as_str())
            .bind(effective_month)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(CapacityReviewRow {
                    projected_original_bytes: row
                        .try_get("projected_original_bytes")
                        .map_err(|_| source_invalid())?,
                    primary_available_bytes: row
                        .try_get("primary_available_bytes")
                        .map_err(|_| source_invalid())?,
                    backup_available_bytes: row
                        .try_get("backup_available_bytes")
                        .map_err(|_| source_invalid())?,
                    reserved_bytes: row
                        .try_get("reserved_bytes")
                        .map_err(|_| source_invalid())?,
                    company_limit_bytes: row
                        .try_get("company_limit_bytes")
                        .map_err(|_| source_invalid())?,
                    source_state: row.try_get("source_state")?,
                })
            })
            .transpose()
        }
    }
}

pub async fn read_target_rollout_readiness(
    connection: &mut PgConnection,
    holding: &HoldingId,
    effective_month: &str,
    minimum_margin_bps: i64,
    now: TimestampMillis,
    storage_limit: i64,
) -> Result<TargetRolloutReadiness, PgTransactionError<BusinessError>> {
    crate::application::target_rollout::read_target_rollout_readiness(
        &mut PgTargetRolloutRepository::new(connection),
        holding,
        effective_month,
        minimum_margin_bps,
        now,
        storage_limit,
    )
    .await
}
