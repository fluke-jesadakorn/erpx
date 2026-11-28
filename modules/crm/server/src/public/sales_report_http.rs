//! Transport-neutral composition port and Axum report ingress.
pub use crate::adapters::sales_report_http::{
    CrmSalesReportHttpBackend, CrmSalesReportHttpFuture, CrmSalesReportHttpRequest,
    CrmSalesReportHttpTarget, crm_sales_report_router,
};
