//! Native Executive customer-activity metric HTTP source ingress.
pub use crate::adapters::activity_metric_http::{
    CustomerActivityMetricHttpBackend, CustomerActivityMetricHttpFuture,
    CustomerActivityMetricHttpReadRequest, CustomerActivityMetricHttpTarget,
    customer_activity_metric_http_router,
};
