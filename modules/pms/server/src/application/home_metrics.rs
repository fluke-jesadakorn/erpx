//! Native PMS home hotel-metrics read use case.
//!
//! The repository is a bounded, read-only PMS port. No serializable unit, no
//! lock, no journal and no cross-owner value is involved: every count is read
//! from PMS-owned `pms_properties` / `pms_records`, exactly as the retained
//! `hotelMetrics` query does. The use case only applies the retained ordering
//! guard before handing the rows to a caller.
use crate::domain::home_metrics::{PmsHotelMetric, PmsHotelMetricsFacts, ensure_ordered};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PmsHotelMetricsFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Read-only PMS hotel-metrics repository. One method, no authority of its own.
pub trait PmsHotelMetricsRepository: Send {
    type Error: From<BusinessError> + Send;

    fn hotel_metrics<'a>(
        &'a mut self,
        facts: &'a PmsHotelMetricsFacts,
    ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, Self::Error>;
}

/// Retained `hotelMetrics` read. The selection comes from the current Company/
/// branch context supplied by composition; this use case never trusts a caller
/// with cross-scope values and fails closed on an unordered read.
pub async fn pms_hotel_metrics<R: PmsHotelMetricsRepository>(
    facts: &PmsHotelMetricsFacts,
    repository: &mut R,
) -> Result<Vec<PmsHotelMetric>, R::Error> {
    let rows = repository.hotel_metrics(facts).await?;
    ensure_ordered(&rows)?;
    Ok(rows)
}
