//! Finance workspace shell presentation models.

use crate::finance::{FINANCE_ROUTES, FinanceLabel, FinanceRoute, FinanceSurface};
use erpx_ui_core::Locale;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceKpi {
    pub label: FinanceLabel,
    pub value: String,
    pub delta: Option<String>,
    pub tone: FinanceKpiTone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceKpiTone {
    Neutral,
    Positive,
    Negative,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceWorkspaceModel {
    pub active_surface: FinanceSurface,
    pub locale: Locale,
    pub kpis: Vec<FinanceKpi>,
    pub scope_label: Option<String>,
    pub freshness_label: Option<String>,
}

impl FinanceWorkspaceModel {
    pub fn nav_items(&self) -> Vec<(FinanceRoute, &'static str, bool)> {
        FINANCE_ROUTES
            .iter()
            .map(|def| {
                let route = FinanceRoute::parse(def.template).unwrap_or(FinanceRoute::Finance);
                let active = route.surface() == self.active_surface;
                (route, def.label.text(self.locale), active)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankingImportJobRow {
    pub id: String,
    pub file_name: String,
    pub status: String,
    pub created_at: String,
    pub applied_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodCloseRow {
    pub period_code: String,
    pub status: String,
    pub trial_balance_delta: String,
    pub can_close: bool,
    pub can_reopen: bool,
}
