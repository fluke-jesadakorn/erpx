//! Calculation only; cross-owner transaction-bound source composition is required.
pub use crate::adapters::day_report_source::{PgPosDayReportSources, RetainedPosDaySources};
pub use crate::application::day_report::{
    PosDayForeignFacts, PosDayPmsSaleFact, assemble_day_report,
};
pub use crate::domain::day_report::{
    DayReport, DayReportInput, PosDayOwnedSources, PosDaySaleSource, calculate_day_report,
};
