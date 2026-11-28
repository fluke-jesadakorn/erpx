//! Native Work Projection report read HTTP ingress contract.
pub use crate::adapters::work_report_read_http::{
    WorkReportHttpBackend, WorkReportHttpFuture, WorkReportHttpProjection, WorkReportHttpRequest,
    WorkReportHttpResponse, WorkReportHttpState, WorkReportHttpTarget, native_work_report_json,
    parse_work_report_projection, work_report_router,
};
