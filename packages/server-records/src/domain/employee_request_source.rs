//! Records-owned employee-request purchase sources for Procurement consumers.
//!
//! `employee_requests` is a Records-owned table. This module is the retained
//! pure policy behind the same-transaction witness consumed by Procurement
//! RFQ creation/actions and the request-order handoff:
//!
//! - RFQ create reads the quotation source with
//!   `packages/server-records/src/application/employee-requests.ts`
//!   `requestQuotationSource`: a `purchase` request whose approval decision
//!   is `approved` and whose detail actions include `link-order` (the
//!   assigned handler is working the request `in_progress`).
//! - RFQ actions (non-cancel) assert the retained exact
//!   `(id, company, evidence_version, items_json, approval decision)`
//!   tuple, mirroring `rfq-request-approval` in
//!   `packages/server-procurement/src/adapters/rfqs.ts`.
//! - RFQ order handoff asserts
//!   `rfq-source-evidence` (request row plus the Procurement-owned
//!   `procurement_request_orders` link) and
//!   `prepareRequestOrderHandoff` requires an approved `purchase` request
//!   assigned to the actor.
//! - `requestVersionGuard` asserts the exact
//!   `(id, company, version, state, evidence_version)` tuple.
//!
//! The witness retains only Records-owned facts. It does not grant
//! `commerce.purchase` authority, does not prove the Support/People
//! workspace, service-member or assignee-team membership facts checked by
//! `requestAccess`, and does not read Procurement allocations or order
//! links. Callers (Procurement writers, composed in `src/composition`)
//! must supply those facts through their own owners in the same unit.
//!
//! Scope follows the exact retained selection used by
//! `RecordContextActor::permits_scope`: a department selection sees rows
//! with the selected department, a branch selection rows with the
//! selected branch, and a holding selection every row in the Company.
//! Callers whose retained contract is company-only (for example the RFQ
//! `sourceState` read) pass a holding scope; callers with a selected
//! branch/department pass that selection rather than widening scope.
//! Foreign or out-of-selection rows fail as `REQUEST_NOT_FOUND`, never
//! as hints that the row exists.
//!
//! The `id` bound mirrors the existing Records source-record precedent
//! (`validate_reference`): non-empty, no NUL, UTF-16 code-unit count at
//! most 180. JSON facts are validated by parsing; malformed retained
//! JSON fails closed with `REQUEST_SOURCE_INVALID` rather than with a
//! panic. No size cap is imposed beyond parsing: the RFQ guards compare
//! `items_json`/`approval_json` exactly, so truncation would change
//! semantics.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const CONTRACT_VERSION: &str = "records.employee-request-source.v1";

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "REQUEST_NOT_FOUND",
        "Request not found.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "REQUEST_SOURCE_INVALID",
        "The request source is not available.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "The request changed. Reopen it before preparing the order.",
    )
}

fn purchase_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PURCHASE_REQUEST_REQUIRED",
        "An approved purchase request assigned to you and in progress is required.",
    )
}

pub fn validate_id(id: &str) -> Result<(), BusinessError> {
    if id.is_empty() || id.encode_utf16().count() > 180 || id.contains('\0') {
        return Err(BusinessError::invalid(
            "REQUEST_SOURCE_INVALID",
            "Provide a valid request source.",
        ));
    }
    Ok(())
}

/// Retained `employee_requests` facts needed by Procurement consumers.
/// `title`/`currency` are carried so callers can render or validate the
/// retained row without a second read; they do not affect the guards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeeRequestRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub version: i64,
    pub evidence_version: i64,
    pub items_json: String,
    pub approval_json: String,
    pub title: String,
    pub currency: String,
    pub requester_user_id: String,
    pub created_by: String,
    pub assignee_user_id: Option<String>,
    pub purchase_order_id: Option<String>,
}

/// Actor selection facts. These describe which workspace the caller was
/// selected into; they are not authority and confer no permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeeRequestScope {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl EmployeeRequestScope {
    /// Exact retained scope selection, mirroring
    /// `RecordContextActor::permits_scope`.
    pub fn permits_scope(
        &self,
        company_id: &str,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> bool {
        if company_id != self.company_id {
            return false;
        }
        if let Some(department) = self.department_id.as_deref() {
            return department_id == Some(department);
        }
        if let Some(branch) = self.branch_id.as_deref() {
            return branch_id == Some(branch);
        }
        true
    }
}

/// Retained exact tuple revalidated after deferred effects. `items_json`
/// is compared byte-for-byte because the retained RFQ assertions compare
/// the stored JSON text exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmployeeRequestGuard {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
    pub state: String,
    pub evidence_version: i64,
    pub items_json: String,
}

pub fn guard_for(row: &EmployeeRequestRow) -> EmployeeRequestGuard {
    EmployeeRequestGuard {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        branch_id: row.branch_id.clone(),
        department_id: row.department_id.clone(),
        version: row.version,
        state: row.state.clone(),
        evidence_version: row.evidence_version,
        items_json: row.items_json.clone(),
    }
}

/// The approval decision retained in `approval_json`, parsed exactly as
/// the retained callers do (`JSON.parse(approval_json).decision`).
/// Anything other than a JSON object with a string `decision` fails
/// closed.
pub fn approval_decision(row: &EmployeeRequestRow) -> Result<String, BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(&row.approval_json).map_err(|_| source_invalid())?;
    let Some(decision) = value
        .as_object()
        .and_then(|object| object.get("decision"))
        .and_then(serde_json::Value::as_str)
    else {
        return Err(source_invalid());
    };
    Ok(decision.to_owned())
}

/// Validate the retained row shape and its scope. Missing, foreign and
/// out-of-selection rows all fail as `REQUEST_NOT_FOUND`.
pub fn validate_row(
    row: &EmployeeRequestRow,
    scope: &EmployeeRequestScope,
) -> Result<(), BusinessError> {
    if !scope.permits_scope(
        &row.company_id,
        row.branch_id.as_deref(),
        row.department_id.as_deref(),
    ) {
        return Err(not_found());
    }
    if !matches!(row.kind.as_str(), "service" | "equipment" | "purchase")
        || !matches!(
            row.state.as_str(),
            "draft"
                | "returned"
                | "blocked"
                | "awaiting_approval"
                | "queued"
                | "in_progress"
                | "waiting_requester"
                | "resolved"
                | "fulfilled"
                | "closed"
                | "rejected"
                | "cancelled"
        )
        || row.version <= 0
        || row.evidence_version <= 0
        || row.items_json.contains('\0')
        || row.approval_json.contains('\0')
    {
        return Err(source_invalid());
    }
    let items: serde_json::Value =
        serde_json::from_str(&row.items_json).map_err(|_| source_invalid())?;
    if !items.is_array() {
        return Err(source_invalid());
    }
    approval_decision(row).map(|_| ())
}

/// An approved employee purchase request. Used by the order handoff,
/// where the retained contract requires kind `purchase` and approval
/// decision `approved` without the `in_progress`/assignee condition.
pub fn require_approved_purchase(row: &EmployeeRequestRow) -> Result<(), BusinessError> {
    if row.kind != "purchase" || approval_decision(row)? != "approved" {
        return Err(purchase_required());
    }
    Ok(())
}

/// The quotation source for RFQ creation and the non-editing order
/// handoff: an approved `purchase` request whose assigned handler is
/// the actor and whose state is `in_progress`. The retained
/// `link-order` action additionally requires team membership
/// (`access.member`) and `commerce.purchase` permission; those stay
/// with the caller.
pub fn require_quotation_source(
    row: &EmployeeRequestRow,
    actor_user_id: &str,
) -> Result<(), BusinessError> {
    require_approved_purchase(row)?;
    if row.state != "in_progress"
        || row.assignee_user_id.as_deref() != Some(actor_user_id)
        || actor_user_id.is_empty()
    {
        return Err(purchase_required());
    }
    Ok(())
}

/// Revalidate the retained guard after deferred effects: every retained
/// field of the current row must equal the guard exactly, including
/// the `items_json` text, scope and versions. Any drift fails with
/// `VERSION_CONFLICT`.
pub fn require_current(
    guard: &EmployeeRequestGuard,
    row: &EmployeeRequestRow,
    scope: &EmployeeRequestScope,
) -> Result<(), BusinessError> {
    validate_row(row, scope)?;
    if guard.id != row.id
        || guard.company_id != row.company_id
        || guard.branch_id != row.branch_id
        || guard.department_id != row.department_id
        || guard.version != row.version
        || guard.state != row.state
        || guard.evidence_version != row.evidence_version
        || guard.items_json != row.items_json
    {
        return Err(changed());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> EmployeeRequestRow {
        EmployeeRequestRow {
            id: "request-1".into(),
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
            kind: "purchase".into(),
            state: "in_progress".into(),
            version: 3,
            evidence_version: 2,
            items_json: "[{\"description\":\"Cable\"}]".into(),
            approval_json: "{\"decision\":\"approved\"}".into(),
            title: "Cables".into(),
            currency: "THB".into(),
            requester_user_id: "requester".into(),
            created_by: "creator".into(),
            assignee_user_id: Some("handler".into()),
            purchase_order_id: None,
        }
    }

    fn scope() -> EmployeeRequestScope {
        EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
        }
    }

    #[test]
    fn holding_scope_permits_any_row_in_the_company() {
        let scope = scope();
        assert!(scope.permits_scope("company", Some("branch"), Some("dept")));
        assert!(scope.permits_scope("company", None, None));
        assert!(!scope.permits_scope("foreign", None, None));
    }

    #[test]
    fn branch_and_department_scopes_keep_the_retained_selection() {
        let branch = EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: None,
        };
        assert!(branch.permits_scope("company", Some("branch"), Some("dept")));
        assert!(!branch.permits_scope("company", Some("other"), None));
        let department = EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: Some("branch".into()),
            department_id: Some("dept".into()),
        };
        assert!(department.permits_scope("company", Some("branch"), Some("dept")));
        assert!(department.permits_scope("company", None, Some("dept")));
        assert!(!department.permits_scope("company", Some("branch"), Some("other")));
        assert!(!department.permits_scope("company", None, None));
    }

    #[test]
    fn quotation_source_requires_approved_assigned_in_progress_purchase() {
        let source = row();
        assert!(require_quotation_source(&source, "handler").is_ok());
        assert!(require_quotation_source(&source, "other").is_err());
        let mut wrong_state = row();
        wrong_state.state = "queued".into();
        assert!(require_quotation_source(&wrong_state, "handler").is_err());
        let mut wrong_kind = row();
        wrong_kind.kind = "service".into();
        assert!(require_quotation_source(&wrong_kind, "handler").is_err());
        let mut wrong_approval = row();
        wrong_approval.approval_json = "{\"decision\":\"pending\"}".into();
        assert!(require_quotation_source(&wrong_approval, "handler").is_err());
    }

    #[test]
    fn approved_purchase_allows_handoff_without_assignment() {
        let source = row();
        assert!(require_approved_purchase(&source).is_ok());
        let mut unassigned = row();
        unassigned.assignee_user_id = None;
        unassigned.state = "queued".into();
        assert!(require_approved_purchase(&unassigned).is_ok());
        assert!(require_quotation_source(&unassigned, "handler").is_err());
    }

    #[test]
    fn malformed_json_fails_closed() {
        let mut bad_items = row();
        bad_items.items_json = "not json".into();
        assert_eq!(
            validate_row(&bad_items, &scope()).unwrap_err().code,
            "REQUEST_SOURCE_INVALID"
        );
        let mut bad_approval = row();
        bad_approval.approval_json = "[]".into();
        assert_eq!(
            validate_row(&bad_approval, &scope()).unwrap_err().code,
            "REQUEST_SOURCE_INVALID"
        );
        let mut bad_version = row();
        bad_version.version = 0;
        assert!(validate_row(&bad_version, &scope()).is_err());
    }

    #[test]
    fn foreign_and_out_of_scope_rows_are_not_found() {
        let source = row();
        let foreign = EmployeeRequestScope {
            company_id: "foreign".into(),
            branch_id: None,
            department_id: None,
        };
        assert_eq!(
            validate_row(&source, &foreign).unwrap_err().code,
            "REQUEST_NOT_FOUND"
        );
        let branch = EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: Some("other".into()),
            department_id: None,
        };
        assert_eq!(
            validate_row(&source, &branch).unwrap_err().code,
            "REQUEST_NOT_FOUND"
        );
    }

    #[test]
    fn guard_revalidation_detects_every_retained_field_drift() {
        let source = row();
        let guard = guard_for(&source);
        assert!(require_current(&guard, &source, &scope()).is_ok());
        for (label, drifted) in [
            ("version", {
                let mut value = row();
                value.version += 1;
                value
            }),
            ("state", {
                let mut value = row();
                value.state = "resolved".into();
                value
            }),
            ("evidence", {
                let mut value = row();
                value.evidence_version += 1;
                value
            }),
            ("items", {
                let mut value = row();
                value.items_json = "[{\"description\":\"Other\"}]".into();
                value
            }),
            ("branch", {
                let mut value = row();
                value.branch_id = Some("other".into());
                value
            }),
        ] {
            let error = require_current(&guard, &drifted, &scope()).unwrap_err();
            assert_eq!(error.code, "VERSION_CONFLICT", "{label}");
        }
    }

    #[test]
    fn id_bound_matches_the_records_source_record_precedent() {
        assert!(validate_id("request-1").is_ok());
        assert!(validate_id("").is_err());
        assert!(validate_id("bad\0id").is_err());
        assert!(validate_id(&"x".repeat(180)).is_ok());
        assert!(validate_id(&"x".repeat(181)).is_err());
    }
}
