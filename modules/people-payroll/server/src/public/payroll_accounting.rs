//! Inactive pure Payroll projection; it carries no posting authority.
pub use crate::adapters::payroll_accounting_input::read_payroll_projection_inputs;
pub use crate::domain::payroll_accounting::{
    PayrollAccountingProjection, PayrollCostShare, PayrollDimensions, PayrollJournalLine,
    PayrollProjectionAccounts, PayrollProjectionItem, PayrollProjectionOutcome,
    PayrollSourceFailure, PayrollTotals, payroll_accounting_lines, payroll_net_cost_shares,
};
pub use crate::domain::payroll_accounting_operand::Text as PayrollText;
/// Compatibility gates describe unqualified source semantics, not product validation.
pub struct PayrollProjectionCoverage {
    pub inactive: bool,
    pub non_authoritative: bool,
    pub full_source_api_parity: bool,
    pub p1_open: bool,
    pub p2_open: bool,
    pub p3_open: bool,
    pub qualified_grammar: &'static [&'static str],
}
pub const COVERAGE: PayrollProjectionCoverage = PayrollProjectionCoverage {
    inactive: true,
    non_authoritative: true,
    full_source_api_parity: false,
    p1_open: true,
    p2_open: true,
    p3_open: true,
    qualified_grammar: &[
        "lazy JSON scalar operands",
        "calendar-valid ISO date and explicit offsets",
        "exact UTF-16 JSON strings with pinned en-US collation",
    ],
};
