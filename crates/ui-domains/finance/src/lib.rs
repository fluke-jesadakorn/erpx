//! Target-neutral Finance UI models.
mod finance;
mod mentions;
mod workspace;
pub use finance::*;
pub use mentions::{
    FinanceMentionField, FinanceMentionKind, finance_field_clipboard_token,
    finance_record_clipboard_token, is_finance_record_type,
};
pub use workspace::{
    BankingImportJobRow, FinanceKpi, FinanceKpiTone, FinanceWorkspaceModel, PeriodCloseRow,
};
