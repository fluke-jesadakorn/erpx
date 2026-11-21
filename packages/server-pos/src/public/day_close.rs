//! POS day-close decision facts; the composed report and writer remain pending.
pub use crate::adapters::day_close::{NewDayClose, PgPosDayClose};
pub use crate::domain::day_close::{
    DayReportFacts, PreparedDayClose, validate_confirm, validate_prepare,
};
