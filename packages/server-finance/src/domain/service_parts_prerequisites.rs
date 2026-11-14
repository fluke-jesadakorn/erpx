//! Ordered Finance-owned constituents of the retained Service prerequisite image.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServicePartsPeriodReference {
    pub id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
    pub state: String,
    pub version: i64,
    pub source: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServicePartsRateReference {
    pub id: String,
    pub currency: String,
    pub effective_date: String,
    pub currency_per_usd: String,
    pub confirmed_at: i64,
    pub source: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServicePartsMoneyReference {
    pub kind: String,
    pub source: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServicePartsPrerequisiteReferences {
    pub periods: Vec<ServicePartsPeriodReference>,
    pub rates: Vec<ServicePartsRateReference>,
    pub money: Vec<ServicePartsMoneyReference>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServicePrerequisiteKind {
    Chart,
    Account,
    Period,
    Rate,
}
/// Already normalized input. This value is not authority or a receipt.
#[derive(Clone, Debug)]
pub enum ServicePrerequisiteInput {
    Chart {
        name: String,
        base_currency: String,
        accounting_basis: String,
    },
    Account {
        chart_id: String,
        code: String,
        name: String,
        category: String,
        parent_id: Option<String>,
        currency: Option<String>,
        is_control: bool,
        control_kind: String,
    },
    Period {
        name: String,
        start_date: String,
        end_date: String,
    },
    Rate {
        currency: String,
        effective_date: String,
        currency_per_usd: String,
        note: String,
    },
}
impl ServicePrerequisiteInput {
    pub fn variant(&self) -> ServicePrerequisiteKind {
        match self {
            Self::Chart { .. } => ServicePrerequisiteKind::Chart,
            Self::Account { .. } => ServicePrerequisiteKind::Account,
            Self::Period { .. } => ServicePrerequisiteKind::Period,
            Self::Rate { .. } => ServicePrerequisiteKind::Rate,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Chart { .. } => "chart",
            Self::Account { .. } => "account",
            Self::Period { .. } => "period",
            Self::Rate { .. } => "rate",
        }
    }
}
