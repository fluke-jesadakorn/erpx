//! Pure domain types for the lane 1 document posting preview reads.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocPreviewKind {
    Accounts,
    JournalLines,
    StockConfiguration,
}

impl DocPreviewKind {
    pub const ACCOUNTS: &'static str = "doc-accounts";
    pub const JOURNAL_LINES: &'static str = "doc-journal-lines";
    pub const STOCK_CONFIGURATION: &'static str = "doc-stock-configuration";

    pub fn parse(raw: &str) -> Result<Self, BusinessError> {
        match raw {
            Self::ACCOUNTS => Ok(Self::Accounts),
            Self::JOURNAL_LINES => Ok(Self::JournalLines),
            Self::STOCK_CONFIGURATION => Ok(Self::StockConfiguration),
            _ => Err(BusinessError::new(
                FailureKind::InvalidInput,
                "LANE_01_DOC_PREVIEW_KIND_INVALID",
                "Use a registered document preview read.",
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accounts => Self::ACCOUNTS,
            Self::JournalLines => Self::JOURNAL_LINES,
            Self::StockConfiguration => Self::STOCK_CONFIGURATION,
        }
    }

    pub const fn needs_journal(self) -> bool {
        matches!(self, Self::JournalLines)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreviewAccountRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreviewJournalLineRow {
    pub id: String,
    pub debit: i64,
    pub credit: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreviewStockConfigurationRow {
    pub inventory_account_id: String,
    pub variance_account_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocPreviewProjection {
    Accounts(Vec<PreviewAccountRow>),
    JournalLines(Vec<PreviewJournalLineRow>),
    StockConfiguration(Option<PreviewStockConfigurationRow>),
}

impl DocPreviewProjection {
    pub fn into_rows(self) -> Value {
        match self {
            Self::Accounts(rows) => json!(rows),
            Self::JournalLines(rows) => json!(rows),
            Self::StockConfiguration(row) => json!(row),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for kind in [
            DocPreviewKind::Accounts,
            DocPreviewKind::JournalLines,
            DocPreviewKind::StockConfiguration,
        ] {
            assert_eq!(DocPreviewKind::parse(kind.as_str()), Ok(kind));
        }
        assert!(DocPreviewKind::parse("SELECT 1").is_err());
        assert!(DocPreviewKind::JournalLines.needs_journal());
        assert!(!DocPreviewKind::Accounts.needs_journal());
    }

    #[test]
    fn response_shapes_are_pinned() {
        let accounts = DocPreviewProjection::Accounts(vec![PreviewAccountRow {
            id: "acc-1".into(),
            code: "1210".into(),
            name: "Receivable".into(),
            status: "active".into(),
        }]);
        assert_eq!(
            accounts.into_rows(),
            json!([{"id":"acc-1","code":"1210","name":"Receivable","status":"active"}])
        );
        let lines = DocPreviewProjection::JournalLines(vec![PreviewJournalLineRow {
            id: "acc-1".into(),
            debit: 500,
            credit: 0,
        }]);
        assert_eq!(
            lines.into_rows(),
            json!([{"id":"acc-1","debit":500,"credit":0}])
        );
        let none = DocPreviewProjection::StockConfiguration(None);
        assert_eq!(none.into_rows(), Value::Null);
        let config = DocPreviewProjection::StockConfiguration(Some(PreviewStockConfigurationRow {
            inventory_account_id: "inv".into(),
            variance_account_id: "var".into(),
        }));
        assert_eq!(
            config.into_rows(),
            json!({"inventory_account_id":"inv","variance_account_id":"var"})
        );
    }
}
