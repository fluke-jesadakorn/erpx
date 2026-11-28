//! Transport-neutral composition port and Axum ingress for the native
//! CRM/marketing report read.
pub use crate::adapters::crm_marketing_report_http::{
    CRM_MARKETING_REPORT_PATH, CrmMarketingReportBackend, CrmMarketingReportRequest,
    CrmMarketingReportTarget, crm_marketing_report_router,
};
pub use crate::application::crm_marketing_report::{
    CrmMarketingReportFuture, CrmMarketingReportPayload,
};
pub use crate::domain::crm_marketing_report::{
    CrmMarketingReportKind, CrmMarketingReportQuery,
};
