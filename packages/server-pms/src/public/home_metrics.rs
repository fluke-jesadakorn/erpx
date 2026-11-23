//! Native PMS home hotel-metrics read port and its PostgreSQL owner adapter.
//!
//! Composition supplies the current Account session, Company/branch scope and
//! owner connection. Nothing here mints authority, writes data, reads another
//! owner's table or makes a money/KPI claim. HTTP and composition binding are
//! deliberately outside this slice.
pub use crate::adapters::home_metrics_postgres::PgPmsHotelMetricsRead;
pub use crate::application::home_metrics::{
    PmsHotelMetricsFuture, PmsHotelMetricsRepository, pms_hotel_metrics,
};
pub use crate::domain::home_metrics::{
    PmsHotelMetric, PmsHotelMetricsFacts, PmsMetricPropertyRow, PmsMetricRecordRow, ensure_ordered,
    project_hotel_metrics, read_invalid,
};
