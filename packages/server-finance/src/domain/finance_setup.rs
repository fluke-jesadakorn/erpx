//! Pure readiness checks for the native Finance setup workspace. The caller
//! supplies current Company-scoped counts; this module owns only the stable
//! labels, completion rules and detail text rendered by the setup page.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FinanceSetupCounts {
    pub charts: i64,
    pub active_accounts: i64,
    pub primary_control_books: i64,
    pub effective_profiles: i64,
    pub preparers: i64,
    pub approvers: i64,
    pub independent_approver: bool,
    pub periods: i64,
    pub active_tax_codes: i64,
    pub bank_statement_accounts: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSetupCheck {
    pub id: &'static str,
    pub label: &'static str,
    pub complete: bool,
    pub detail: String,
}

pub fn finance_setup_checks(counts: FinanceSetupCounts) -> Vec<FinanceSetupCheck> {
    vec![
        FinanceSetupCheck {
            id: "chart",
            label: "Chart of accounts and base currency",
            complete: counts.charts > 0,
            detail: format!("{} chart(s)", counts.charts),
        },
        FinanceSetupCheck {
            id: "accounts",
            label: "Posting accounts",
            complete: counts.active_accounts > 0,
            detail: format!("{} active account(s)", counts.active_accounts),
        },
        FinanceSetupCheck {
            id: "controls",
            label: "Bank, receivable and payable controls",
            complete: counts.primary_control_books > 0,
            detail: format!(
                "{} primary book(s) with all three control mappings",
                counts.primary_control_books
            ),
        },
        FinanceSetupCheck {
            id: "profile",
            label: "Reviewed Company accounting policy",
            complete: counts.effective_profiles > 0,
            detail: format!(
                "{} effective independently approved profile(s)",
                counts.effective_profiles
            ),
        },
        FinanceSetupCheck {
            id: "approvers",
            label: "Independent Finance preparer and approver",
            complete: counts.independent_approver,
            detail: format!(
                "{} preparer(s), {} approver(s) with current Company authority",
                counts.preparers, counts.approvers
            ),
        },
        FinanceSetupCheck {
            id: "periods",
            label: "Accounting periods",
            complete: counts.periods > 0,
            detail: format!("{} period(s)", counts.periods),
        },
        FinanceSetupCheck {
            id: "tax",
            label: "Tax codes",
            complete: counts.active_tax_codes > 0,
            detail: format!("{} active code(s)", counts.active_tax_codes),
        },
        FinanceSetupCheck {
            id: "bank",
            label: "Bank accounts",
            complete: counts.bank_statement_accounts > 0,
            detail: format!(
                "{} account(s) from statements",
                counts.bank_statement_accounts
            ),
        },
    ]
}
