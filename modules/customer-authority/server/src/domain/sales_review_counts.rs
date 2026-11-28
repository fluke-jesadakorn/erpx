//! Pure domain for the native sales customer review counts read.
//!
//! This module owns the bounded holding scope, the closed review-kind set and
//! the complete-payload invariant. It has no SQL, no HTTP and no I/O. The wire
//! shape is the owner's retired TypeScript `{kind,count}` projection.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

/// Closed discriminators the retired owner SQL emitted, in UNION ALL order.
pub const REVIEW_KINDS: [&str; 3] = ["identity", "duplicates", "access"];

/// Closed holding scope for the pending-review counts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesReviewCountsQuery {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// One `{kind,count}` fact. `kind` is one of `REVIEW_KINDS`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCount {
    pub kind: String,
    pub count: i64,
}

impl ReviewCount {
    pub fn to_json(&self) -> Value {
        json!({"kind": self.kind, "count": self.count})
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_SALES_REVIEW_COUNTS_INVALID",
        "Review the sales customer review scope.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE",
        "The sales customer review counts are not available.",
    )
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.contains('\0')
}

impl SalesReviewCountsQuery {
    /// Branch and department are optional fences; absence means "all",
    /// matching the `?::text IS NULL` predicates of the owner SQL.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.company_id)
            || self.branch_id.as_deref().is_some_and(|v| !bounded(v))
            || self.department_id.as_deref().is_some_and(|v| !bounded(v))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Pin the complete read result: exactly the three closed kinds, once each, in
/// the SQL's UNION ALL order, with non-negative counts. A partial or reordered
/// projection fails closed instead of masquerading as parity.
pub fn validate_counts(rows: Vec<ReviewCount>) -> Result<Vec<ReviewCount>, BusinessError> {
    if rows.len() != REVIEW_KINDS.len()
        || rows
            .iter()
            .zip(REVIEW_KINDS.iter())
            .any(|(row, expected)| row.kind != *expected || row.count < 0)
    {
        return Err(unavailable());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> SalesReviewCountsQuery {
        SalesReviewCountsQuery {
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
        }
    }

    fn counts() -> Vec<ReviewCount> {
        REVIEW_KINDS
            .iter()
            .enumerate()
            .map(|(n, kind)| ReviewCount {
                kind: (*kind).into(),
                count: n as i64,
            })
            .collect()
    }

    #[test]
    fn bounded_scope_validates() {
        assert!(query().validate().is_ok());
        let mut bad = query();
        bad.company_id.clear();
        assert_eq!(
            bad.validate().unwrap_err().code,
            "CUSTOMER_SALES_REVIEW_COUNTS_INVALID"
        );
    }

    #[test]
    fn optional_scope_fences_are_bounded() {
        let mut q = query();
        q.branch_id = Some("branch-1".into());
        q.department_id = Some("department-1".into());
        assert!(q.validate().is_ok());
        q.department_id = Some("\0".into());
        assert!(q.validate().is_err());
    }

    #[test]
    fn counts_require_all_closed_kinds_in_order() {
        assert!(validate_counts(counts()).is_ok());
        let mut reordered = counts();
        reordered.swap(0, 1);
        assert_eq!(
            validate_counts(reordered).unwrap_err().code,
            "CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE"
        );
        let mut missing = counts();
        missing.pop();
        assert!(validate_counts(missing).is_err());
        let mut negative = counts();
        negative[0].count = -1;
        assert!(validate_counts(negative).is_err());
    }

    #[test]
    fn row_json_is_the_exact_two_field_projection() {
        let value = counts()[1].to_json();
        assert_eq!(value.as_object().map_or(0, |o| o.len()), 2);
        assert_eq!(value["kind"], "duplicates");
        assert_eq!(value["count"], 1);
    }
}
