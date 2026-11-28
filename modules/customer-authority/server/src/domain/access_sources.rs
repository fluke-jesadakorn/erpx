//! Pure domain for the Customer Authority customer-access sources read.
//!
//! This module owns the bounded scope value and the invariants the payload must
//! satisfy. It has no SQL, no HTTP and no I/O. Row facts reuse the owner's
//! existing `AccessSourceRow` value type so the wire shape stays identical to
//! the retired TypeScript projection.
use crate::domain::access::{AccessSourceRow, MAX_SOURCE_ROWS};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

/// Closed holding scope for the approved customer document source list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessSourcesQuery {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

pub fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_ACCESS_SOURCES_INVALID",
        "Review the customer document source scope.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_ACCESS_SOURCES_UNAVAILABLE",
        "The approved customer document list is not available.",
    )
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.contains('\0')
}

impl AccessSourcesQuery {
    /// Branch and department are optional scope fences; an absent value means
    /// "all", matching the `?::text IS NULL` predicate of the owner SQL.
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

/// Validate the complete read result: bounded, ordered, unique source rows.
/// Every field the TypeScript projection returned is checked here.
pub fn validate_rows(rows: Vec<AccessSourceRow>) -> Result<Vec<AccessSourceRow>, BusinessError> {
    if rows.len() > MAX_SOURCE_ROWS {
        return Err(unavailable());
    }
    let mut seen = BTreeSet::new();
    for row in &rows {
        if row.id.is_empty()
            || row.id.len() > 200
            || row.number.is_empty()
            || row.number.len() > 200
            || row.name.is_empty()
            || row.name.len() > 300
            || row.version < 1
            || !seen.insert(row.id.as_str())
        {
            return Err(unavailable());
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> AccessSourcesQuery {
        AccessSourcesQuery {
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
        }
    }

    fn row(id: &str) -> AccessSourceRow {
        AccessSourceRow {
            id: id.into(),
            number: "SO-1".into(),
            name: "Acme".into(),
            version: 1,
        }
    }

    #[test]
    fn bounded_scope_validates() {
        assert!(query().validate().is_ok());
        let mut bad = query();
        bad.company_id.clear();
        assert_eq!(bad.validate().unwrap_err().code, "CUSTOMER_ACCESS_SOURCES_INVALID");
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
    fn rows_reject_duplicates_empty_fields_and_bad_versions() {
        assert!(validate_rows(vec![row("a"), row("b")]).is_ok());
        assert!(validate_rows(vec![row("a"), row("a")]).is_err());
        assert!(validate_rows(vec![AccessSourceRow { version: 0, ..row("a") }]).is_err());
        assert!(validate_rows(vec![AccessSourceRow { name: String::new(), ..row("a") }]).is_err());
    }

    #[test]
    fn row_count_is_capped() {
        let rows = (0..=MAX_SOURCE_ROWS)
            .map(|n| row(&format!("source-{n}")))
            .collect();
        assert_eq!(validate_rows(rows).unwrap_err().code, "CUSTOMER_ACCESS_SOURCES_UNAVAILABLE");
    }
}
