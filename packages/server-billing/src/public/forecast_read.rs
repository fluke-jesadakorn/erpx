//! Billing seat-forecast baseline read over Control Billing projections.
pub use crate::adapters::forecast_read::{PgSeatForecastRepository, read_seat_forecast_baseline};
pub use crate::application::forecast_read::{
    SeatForecastRepository, read_seat_forecast_baseline as read_seat_forecast_baseline_with,
};
pub use crate::domain::forecast_read::{
    ForecastSeats, ForecastStorage, RetainedStorageTotalsRow, SeatForecastBaseline,
    SeatForecastPriceRow, TargetPrice, UsageSnapshotRow, billing_month, next_billing_month,
    safe_number, target_holding,
};
