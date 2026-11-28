//! Native PMS home hotel-metrics read port and its PostgreSQL owner adapter.
//!
//! Composition supplies the current Account session, Company/branch scope and
//! owner connection. Nothing here mints authority, writes data, reads another
//! owner's table or makes a money/KPI claim. The owner-local axum router
//! executes the retained SQL and returns the full payload; TypeScript is a
//! strict transport binding only. Composition mounts the router with the
//! registered Holding targets (lead-owned).
pub use crate::adapters::home_metrics_http::{
    HomeMetricsRead, HomeMetricsTarget, PgHomeMetricsRead, router, router_for_holdings,
    router_with_reader, router_with_readers,
};
pub use crate::adapters::home_metrics_postgres::PgPmsHotelMetricsRead;
pub use crate::application::home_metrics::{
    PmsHotelMetricsFuture, PmsHotelMetricsRepository, pms_hotel_metrics,
};
pub use crate::domain::home_metrics::{
    PmsHotelMetric, PmsHotelMetricsFacts, PmsMetricPropertyRow, PmsMetricRecordRow, ensure_ordered,
    project_hotel_metrics, read_invalid,
};
