//! Native consumer of the Field final-root projection sources.
//!
//! The fixed source statements in `final_root_projection` are consumed by
//! Customer 360 timeline and Customer Authority duplicate review, which embed
//! the Field agreements/orders in their own cross-owner joins. This port serves
//! the Field-owned part of that composition: the `scoped_service` agreement
//! projection, scoped to one Company (and optional branch/department), built
//! only from a closed `FinalRootProjectionSource`. A caller supplies a scope,
//! never SQL or a relation name, so a late effect cannot change the source of
//! a composed read. Removal condition for the retained TS consumer export:
//! both cross-owner imports are switched to this port and the TS export is
//! deleted by lead integration.

use crate::adapters::final_root_projection::FinalRootProjectionSource;
use erpx_server_shared::public::CompanyId;

/// A Field-owned Company scope for the shared customer projections.
///
/// Branch and department are presentation scope only; they never grant access.
/// Their length is bounded so a caller cannot smuggle a large payload into a
/// prepared statement, but their text is otherwise preserved exactly so the
/// native statement matches the retained TS consumer binding semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalRootConsumerScope {
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl FinalRootConsumerScope {
    pub fn new(
        company_id: CompanyId,
        branch_id: Option<String>,
        department_id: Option<String>,
    ) -> Option<Self> {
        let bounded = |value: &Option<String>| match value {
            None => true,
            Some(s) => !s.is_empty()
                && s.len() <= 200
                && !s.chars().any(|c| c.is_control()),
        };
        if !bounded(&branch_id) || !bounded(&department_id) {
            return None;
        }
        Some(Self { company_id, branch_id, department_id })
    }

    /// The exact Field-owned `scoped_service` composition the shared customer
    /// projections embed. Placeholders are positional (`?`) to match the
    /// retained TS consumer binding order. Only fixed Field SQL is interpolated;
    /// no caller text reaches the statement.
    pub fn scoped_service_sql(&self) -> String {
        format!(
            "SELECT g.id,g.customer_id,g.branch_id,g.department_id FROM ({}) g WHERE g.company_id=? AND (?::text IS NULL OR g.branch_id=?) AND (?::text IS NULL OR g.department_id=?)",
            FinalRootProjectionSource::Agreements.sql()
        )
    }

    /// Binding order for `scoped_service_sql`: company, branch, branch,
    /// department, department. A NULL branch/department widens the scope
    /// without changing the prepared statement shape.
    pub fn scoped_service_args(&self) -> [Option<&str>; 5] {
        let branch = self.branch_id.as_deref();
        let department = self.department_id.as_deref();
        [
            Some(self.company_id.as_str()),
            branch,
            branch,
            department,
            department,
        ]
    }
}
