//! Native PMS dashboard/report read ports and the PostgreSQL owner adapter.
//! Composition supplies the current Account session, Company/branch scope, the
//! PMS actor and the owner connection; this module mints no authority, writes
//! nothing and reads no other owner's tables.
pub use crate::adapters::report_http::{
    PgPmsReportRead, PmsReportEntriesRequest, PmsReportHttpBackend, PmsReportHttpFuture,
    PmsReportHttpResponse, PmsReportHttpTarget, PmsReportRangeRequest, pms_report_router,
};
pub use crate::application::report_projection::{
    PmsReportFuture, PmsReportNightRead, PmsReportRepository, pms_range_report, pms_report_entries,
};
pub use crate::domain::report_projection::{
    MAX_DAYS, MAX_ID, MAX_PAGE, MAX_RANGE_DAYS, PAGE_SIZE, PmsChannelReport, PmsFinanceBalanceRow,
    PmsFinanceBalances, PmsNetPickup, PmsPacePoint, PmsReportBalances, PmsReportData, PmsReportDay,
    PmsReportEntries, PmsReportEntriesData, PmsReportEntry, PmsReportFlow, PmsReportKind,
    PmsReportNightAudit, PmsReportPace, PmsReportRange, PmsReportTotals, assemble_pace,
    date_invalid, finance_balances, in_scope, invalid as report_invalid, js_round, not_found,
    pace_range_invalid, range_invalid, read_invalid, read_limit, require_report, summarize,
    valid_observation, validate_days, validate_range,
};
