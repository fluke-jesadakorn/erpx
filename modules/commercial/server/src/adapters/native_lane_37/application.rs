//! Query port and handler. No SQL lives here; the owner postgres adapter owns
//! the statements and the domain module owns the projection and calculation.

use std::{future::Future, pin::Pin, sync::Arc};

use erpx_platform_shared_server::public::BusinessError;

use crate::adapters::native_lane_37::domain::{
    response, unavailable_meter, valid_month, validate_window, ProjectionFacts,
    SeatHistoryResponse, SeatHistoryRows,
};

pub type SeatHistoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;
pub type ProjectionFactsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner owns the
/// four statements, the ordered bind and the typed row projection.
pub trait SeatHistoryRead: Send + Sync {
    fn rows<'a>(&'a self, end: i64) -> SeatHistoryFuture<'a, SeatHistoryRows>;
}

pub struct ReadRequest {
    pub start: i64,
    pub end: i64,
    pub has_cutoff_dependency: bool,
}

pub fn handler(
    reader: Arc<dyn SeatHistoryRead>,
    request: ReadRequest,
) -> SeatHistoryFuture<'static, SeatHistoryResponse> {
    Box::pin(async move {
        validate_window(request.start, request.end)?;
        let rows = reader.rows(request.end).await?;
        Ok(response(
            &rows,
            request.has_cutoff_dependency,
            request.start,
            request.end,
        ))
    })
}

/// A reader used by unit tests and by fail-closed readiness checks: it returns
/// no rows, so the derived meter is `unavailable` and no SQL is issued.
pub struct EmptySeatHistoryRead;

impl SeatHistoryRead for EmptySeatHistoryRead {
    fn rows<'a>(&'a self, _end: i64) -> SeatHistoryFuture<'a, SeatHistoryRows> {
        Box::pin(async { Ok(SeatHistoryRows::default()) })
    }
}

pub fn unavailable_response() -> SeatHistoryResponse {
    SeatHistoryResponse {
        meter: unavailable_meter(),
        rows: SeatHistoryRows::default().to_json(),
    }
}

/// Wave 2 control-DB port: the owner executes the three byte-identical
/// projection statements and projects typed rows; this layer only validates
/// the month and transports the facts to the retained TypeScript calculation.
pub trait ProjectionFactsRead: Send + Sync {
    fn facts<'a>(&'a self, month: &'a str) -> ProjectionFactsFuture<'a, ProjectionFacts>;
}

pub fn projection_facts_handler(
    reader: Arc<dyn ProjectionFactsRead>,
    month: &str,
) -> ProjectionFactsFuture<'static, ProjectionFacts> {
    if !valid_month(month) {
        return Box::pin(async {
            Err(BusinessError::invalid(
                "LANE37_MONTH_INVALID",
                "Use a bounded billing month.",
            ))
        });
    }
    let month = month.to_owned();
    Box::pin(async move { reader.facts(&month).await })
}

/// A no-op projection reader for the seat-only constructors and tests.
pub struct EmptyProjectionFactsRead;

impl ProjectionFactsRead for EmptyProjectionFactsRead {
    fn facts<'a>(&'a self, _month: &'a str) -> ProjectionFactsFuture<'a, ProjectionFacts> {
        Box::pin(async { Ok(ProjectionFacts::default()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_reader_yields_unavailable_meter() {
        let value = handler(
            Arc::new(EmptySeatHistoryRead),
            ReadRequest {
                start: 0,
                end: 10,
                has_cutoff_dependency: false,
            },
        )
        .await
        .expect("handler");
        assert_eq!(value.meter, unavailable_meter());
    }

    #[tokio::test]
    async fn projection_facts_validate_the_month() {
        let reader: Arc<dyn ProjectionFactsRead> = Arc::new(EmptyProjectionFactsRead);
        assert_eq!(
            projection_facts_handler(reader.clone(), "2026-11")
                .await
                .expect("facts"),
            ProjectionFacts::default()
        );
        assert_eq!(
            projection_facts_handler(reader, "2026-13")
                .await
                .unwrap_err()
                .code,
            "LANE37_MONTH_INVALID"
        );
    }

    #[tokio::test]
    async fn invalid_window_is_a_validation_error() {
        let error = handler(
            Arc::new(EmptySeatHistoryRead),
            ReadRequest {
                start: 10,
                end: 10,
                has_cutoff_dependency: false,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "LANE37_WINDOW_INVALID");
    }
}
