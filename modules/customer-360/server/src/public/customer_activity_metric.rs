//! Native Executive customer-activity metric source ports.
pub use crate::adapters::executive_timeline::PgCustomerActivityMetric;
pub use crate::application::executive_timeline::{
    CustomerActivityMetricFuture, CustomerActivityMetricRepository, customer_activity_metric,
};
pub use crate::domain::executive_timeline::{
    CustomerActivityBreakdown, CustomerActivityMetricGroup, CustomerActivityMetricQuery,
    MAX_GROUPS, MAX_RANGE_MS, SOURCE_TYPES,
};
