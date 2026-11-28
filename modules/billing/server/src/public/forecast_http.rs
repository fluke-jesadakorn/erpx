//! Native Billing seat-forecast route port. Composition mounts the router on
//! the private owner gateway and implements the backend by validating the
//! current session/authority, acquiring a Control connection and calling
//! [`crate::public::forecast_read::read_seat_forecast_baseline`]. No TypeScript
//! reader is an alternative authority for this route.
pub use crate::adapters::forecast_http::{
    BillingForecastHttpBackend, BillingForecastHttpFuture, BillingForecastHttpRequest,
    BillingForecastHttpResponse, BillingForecastHttpState, BillingForecastHttpTarget,
    billing_forecast_router,
};
pub use crate::application::forecast_read::ReadSeatForecast;
pub use crate::domain::forecast_read::SeatForecastBaseline;
