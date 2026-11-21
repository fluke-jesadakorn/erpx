//! Billing seat-forecast read use case. The repository supplies only the
//! Control projection rows; this layer performs the exact JS-compatible
//! month/safe-number and current-vs-future price selection.
use crate::domain::forecast_read::{
    RetainedStorageTotalsRow, SeatForecastBaseline, SeatForecastPriceRow, UsageSnapshotRow,
    baseline,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use std::future::Future;

pub trait SeatForecastRepository {
    type Error: From<BusinessError> + Send;

    fn current_price(
        &mut self,
        forecast_month: &str,
    ) -> impl Future<Output = Result<Option<SeatForecastPriceRow>, Self::Error>> + Send;

    fn future_price(
        &mut self,
        forecast_month: &str,
    ) -> impl Future<Output = Result<Option<SeatForecastPriceRow>, Self::Error>> + Send;

    fn usage_snapshot(
        &mut self,
        holding: &HoldingId,
        source_month: &str,
    ) -> impl Future<Output = Result<Option<UsageSnapshotRow>, Self::Error>> + Send;

    fn retained_storage_totals(
        &mut self,
        holding: &HoldingId,
        source_month: &str,
        snapshot_id: &str,
    ) -> impl Future<Output = Result<Option<RetainedStorageTotalsRow>, Self::Error>> + Send;
}

pub async fn read_seat_forecast_baseline<R: SeatForecastRepository>(
    repository: &mut R,
    holding: &HoldingId,
    now: TimestampMillis,
) -> Result<SeatForecastBaseline, R::Error> {
    let source_month = crate::domain::forecast_read::billing_month(now.get())?;
    let forecast_month = crate::domain::forecast_read::next_billing_month(now.get())?;
    let current = repository.current_price(&forecast_month).await?;
    let price = match current {
        Some(price) => Some(price),
        None => repository.future_price(&forecast_month).await?,
    };
    let snapshot = repository.usage_snapshot(holding, &source_month).await?;
    let totals = match &snapshot {
        Some(snapshot) => {
            repository
                .retained_storage_totals(holding, &source_month, &snapshot.snapshot_id)
                .await?
        }
        None => None,
    };
    baseline(holding, now.get(), price, snapshot, totals).map_err(Into::into)
}
