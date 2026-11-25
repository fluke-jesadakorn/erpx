//! Pure domain types for lane 1: Finance expense posting projection reads.
//!
//! These are the wire shapes of the two TypeScript reads this lane replaces.
//! No SQL, pool or HTTP concern belongs here. Every field is a projection of an
//! owner-owned column; the response shape is pinned by unit tests so the
//! People Payroll consumer sees the exact row objects it used to receive.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};

/// The two registered reads. The discriminator crosses the wire; no SQL and no
/// caller-supplied predicate does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionKind {
    JournalLines,
    PostingAccounts,
}

impl ProjectionKind {
    pub const JOURNAL_LINES: &'static str = "journal-lines";
    pub const POSTING_ACCOUNTS: &'static str = "posting-accounts";

    pub fn parse(raw: &str) -> Result<Self, BusinessError> {
        match raw {
            Self::JOURNAL_LINES => Ok(Self::JournalLines),
            Self::POSTING_ACCOUNTS => Ok(Self::PostingAccounts),
            _ => Err(invalid(
                "LANE_01_READ_KIND_INVALID",
                "Use a registered lane 1 read.",
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JournalLines => Self::JOURNAL_LINES,
            Self::PostingAccounts => Self::POSTING_ACCOUNTS,
        }
    }

    pub const fn needs_journal(self) -> bool {
        matches!(self, Self::JournalLines)
    }
}

/// One `finance_journal_lines` row as exposed by `expenseJournalProjection`.
/// Both money columns are owner-owned non-null bigints; a missing or coerced
/// column fails closed in the adapter rather than silently changing a number.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JournalLineRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

/// One `finance_accounts` row as exposed by `expensePostingAccounts`.
/// `currency` is nullable in the owner schema, so null is preserved exactly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PostingAccountRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
    pub chart_id: String,
    pub currency: Option<String>,
}

/// Native result of one registered read. The row arrays are the complete
/// payload; People Payroll receives them as its unchanged `ExpenseLedgerRow[]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpenseProjection {
    JournalLines(Vec<JournalLineRow>),
    PostingAccounts(Vec<PostingAccountRow>),
}

impl ExpenseProjection {
    pub fn into_rows(self) -> Value {
        match self {
            Self::JournalLines(rows) => json!(rows),
            Self::PostingAccounts(rows) => json!(rows),
        }
    }
}

/// Routing-scoped Company. The HTTP layer derives it from the registered target
/// and the vendor-supplied context header; a caller cannot choose another
/// Company through the request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyScope {
    pub company_id: String,
}

pub fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

/// The identifier fence used for Company, journal and Holding values. Keeps
/// caller input out of SQL text and rejects control/path characters.
pub fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '?' | '#' | '\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_kinds_round_trip() {
        for kind in [
            ProjectionKind::JournalLines,
            ProjectionKind::PostingAccounts,
        ] {
            assert_eq!(ProjectionKind::parse(kind.as_str()), Ok(kind));
        }
        assert_eq!(
            ProjectionKind::parse("SELECT 1").unwrap_err().code,
            "LANE_01_READ_KIND_INVALID"
        );
        assert!(ProjectionKind::JournalLines.needs_journal());
        assert!(!ProjectionKind::PostingAccounts.needs_journal());
    }

    #[test]
    fn identifier_fence_rejects_scope_escape() {
        assert!(valid_identifier("journal-1"));
        let long = "x".repeat(201);
        for bad in ["", "a/b", "a?b", "a#b", "a\\b", "a\nb", long.as_str()] {
            assert!(!valid_identifier(bad), "accepted {bad:?}");
        }
    }

    #[test]
    fn response_shape_is_pinned() {
        let journal = ExpenseProjection::JournalLines(vec![JournalLineRow {
            id: "line-1".into(),
            code: "1210".into(),
            name: "Receivable".into(),
            debit_minor: 500,
            credit_minor: 0,
        }]);
        assert_eq!(
            journal.into_rows(),
            json!([{"id":"line-1","code":"1210","name":"Receivable","debit_minor":500,"credit_minor":0}])
        );

        let accounts = ExpenseProjection::PostingAccounts(vec![
            PostingAccountRow {
                id: "acc-1".into(),
                code: "1210".into(),
                name: "Receivable".into(),
                status: "active".into(),
                chart_id: "chart-1".into(),
                currency: None,
            },
            PostingAccountRow {
                id: "acc-2".into(),
                code: "5400".into(),
                name: "Expense".into(),
                status: "active".into(),
                chart_id: "chart-1".into(),
                currency: Some("THB".into()),
            },
        ]);
        assert_eq!(
            accounts.into_rows(),
            json!([
                {"id":"acc-1","code":"1210","name":"Receivable","status":"active","chart_id":"chart-1","currency":null},
                {"id":"acc-2","code":"5400","name":"Expense","status":"active","chart_id":"chart-1","currency":"THB"}
            ])
        );
    }
}
