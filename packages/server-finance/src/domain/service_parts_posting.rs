//! Pure Finance rules for Service parts posting references.
use super::company_money::CurrencyRules;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServicePartsPostingMapping {
    pub chart_id: String,
    pub currency: String,
    pub receivable_account_id: String,
    pub payable_account_id: String,
    pub revenue_account_id: String,
    pub inventory_account_id: String,
    pub clearing_account_id: String,
    pub cogs_account_id: String,
    pub input_tax_account_id: String,
    pub output_tax_account_id: String,
    pub cash_account_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SalesDefaultsVersion {
    pub id: String,
    pub version: i64,
}
/// PostgreSQL jsonb array text, or SQL NULL when the owner has no rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServicePartsPostingFinanceHistory {
    pub invoices: Option<String>,
    pub payments: Option<String>,
    pub journals: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ServicePartsPostingReferences {
    pub charts: Vec<Value>,
    pub accounts: Vec<Value>,
    pub approved_sales_defaults: Option<Value>,
    pub expected_sales_defaults: Option<SalesDefaultsVersion>,
    pub defaults_blocked: bool,
    pub finance_history: bool,
    pub history_image: ServicePartsPostingFinanceHistory,
    pub reference_image: String,
}
pub(crate) fn validate_defaults(
    data: &ServicePartsPostingReferences,
    mapping: &ServicePartsPostingMapping,
    expected: Option<&SalesDefaultsVersion>,
) -> Result<(), BusinessError> {
    if data.defaults_blocked {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SALES_DEFAULTS_REQUIRED",
            "Restore the independently approved Sales defaults.",
        ));
    }
    if data.expected_sales_defaults.as_ref() != expected
        || data
            .approved_sales_defaults
            .as_ref()
            .is_some_and(|d| !defaults_match(d, mapping))
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_PARTS_POSTING_DEFAULTS_CHANGED",
            "Retain the current independently approved Sales defaults.",
        ));
    }
    Ok(())
}
pub(crate) fn select_defaults(
    defaults: &[Value],
    charts: &[Value],
    accounts: &[Value],
    existing: Option<&ServicePartsPostingMapping>,
) -> Result<(Option<Value>, Option<SalesDefaultsVersion>, bool), BusinessError> {
    // An existing mapping only wins when its chart joins as active; otherwise
    // the retained reader selects the latest valid Company Sales default.
    let existing = existing.filter(|m| {
        charts
            .iter()
            .any(|c| text(c, "id") == Some(&m.chart_id) && text(c, "status") == Some("active"))
    });
    // Input order is the originating PostgreSQL ORDER BY, including its collation.
    let selected = defaults
        .iter()
        .find(|d| {
            text(d, "status") == Some("approved")
                && valid_default(d, charts, accounts)
                && existing.is_none_or(|m| {
                    text(d, "chart_id") == Some(&m.chart_id)
                        && text(d, "currency") == Some(&m.currency)
                })
        })
        .cloned();
    let expected = selected
        .as_ref()
        .map(|d| match (text(d, "id"), number(d, "version")) {
            (Some(id), Some(version)) => Ok(SalesDefaultsVersion {
                id: id.to_owned(),
                version,
            }),
            _ => Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
                "The Finance posting source changed.",
            )),
        })
        .transpose()?;
    let blocked = selected.is_none()
        && defaults
            .iter()
            .any(|d| text(d, "status") == Some("approved"));
    Ok((selected, expected, blocked))
}
pub(crate) fn values(rows: &[(String, Value)], kind: &str) -> Vec<Value> {
    rows.iter()
        .filter(|(k, _)| k == kind)
        .map(|(_, v)| v.clone())
        .collect()
}
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}
pub(crate) fn number(v: &Value, key: &str) -> Option<i64> {
    v.get(key)?.as_i64()
}
pub(crate) fn valid_default(d: &Value, charts: &[Value], accounts: &[Value]) -> bool {
    let Some(chart) = text(d, "chart_id") else {
        return false;
    };
    let Some(currency) = text(d, "currency") else {
        return false;
    };
    if !charts
        .iter()
        .any(|c| text(c, "id") == Some(chart) && text(c, "status") == Some("active"))
    {
        return false;
    }
    let account = |field: &str, category: Option<&str>, control: Option<&str>| {
        let Some(id) = text(d, field) else {
            return false;
        };
        accounts.iter().any(|a| {
            text(a, "id") == Some(id)
                && text(a, "chart_id") == Some(chart)
                && text(a, "currency") == Some(currency)
                && text(a, "status") == Some("active")
                && category.is_none_or(|x| text(a, "category") == Some(x))
                && control.is_none_or(|x| text(a, "control_kind") == Some(x))
        })
    };
    account("receivable_account_id", None, Some("receivable"))
        && account("revenue_account_id", Some("revenue"), None)
        && (d.get("tax_account_id").is_none_or(Value::is_null)
            || account("tax_account_id", Some("liability"), Some("none")))
}
pub(crate) fn defaults_match(d: &Value, m: &ServicePartsPostingMapping) -> bool {
    text(d, "chart_id") == Some(&m.chart_id)
        && text(d, "currency") == Some(&m.currency)
        && text(d, "receivable_account_id") == Some(&m.receivable_account_id)
        && text(d, "revenue_account_id") == Some(&m.revenue_account_id)
        && text(d, "tax_account_id").is_none_or(|tax| tax == m.output_tax_account_id)
}
pub(crate) fn validate_mapping(
    rows: &[(String, Value)],
    m: &ServicePartsPostingMapping,
    rules: &CurrencyRules,
) -> Result<(), BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "COMMERCE_POSTING_ACCOUNTS_INVALID",
            "Choose active Company accounts with their original posting roles.",
        )
    };
    let charts = values(rows, "chart");
    let accounts = values(rows, "account");
    let chart = charts
        .iter()
        .find(|c| text(c, "id") == Some(&m.chart_id) && text(c, "status") == Some("active"))
        .ok_or_else(invalid)?;
    let base = text(chart, "base_currency").ok_or_else(invalid)?;
    for (id, category, control) in [
        (&m.receivable_account_id, "asset", "receivable"),
        (&m.payable_account_id, "liability", "payable"),
        (&m.revenue_account_id, "revenue", "none"),
        (&m.inventory_account_id, "asset", "none"),
        (&m.clearing_account_id, "liability", "none"),
        (&m.cogs_account_id, "expense", "none"),
        (&m.input_tax_account_id, "asset", "none"),
        (&m.output_tax_account_id, "liability", "none"),
        (&m.cash_account_id, "asset", "bank"),
    ] {
        if !accounts.iter().any(|a| {
            text(a, "id") == Some(id)
                && text(a, "status") == Some("active")
                && text(a, "chart_id") == Some(&m.chart_id)
                && text(a, "category") == Some(category)
                && text(a, "control_kind") == Some(control)
                && text(a, "currency").is_some_and(|c| c == m.currency || c == base)
        }) {
            return Err(invalid());
        }
    }
    for currency in [m.currency.as_str(), base] {
        let configured = rows
            .iter()
            .find(|(k, v)| k == "scale" && text(v, "currency") == Some(currency))
            .and_then(|(_, v)| number(v, "scale"));
        let scale = rules.scale(currency, configured).map_err(|_| {
            BusinessError::invalid("CURRENCY_SCALE_REQUIRED", "Configure the currency scale.")
        })?;
        if scale != 2 {
            return Err(BusinessError::invalid(
                "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
                "Use a currency with two minor-unit decimals.",
            ));
        }
    }
    if base != m.currency {
        return Err(BusinessError::invalid(
            "SERVICE_PARTS_FUNCTIONAL_CURRENCY_REQUIRED",
            "Use the chart's functional currency.",
        ));
    }
    Ok(())
}
