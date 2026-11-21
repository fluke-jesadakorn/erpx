//! Current Ecommerce command/read actor policy.
//!
//! Composition supplies the normal Account/Holding session, current assignment
//! authority and package admission. This owner policy keeps the current
//! Company, optional branch/department, work-context expiry, the retained
//! `commerce.read`/`commerce.sales`/`commerce.approve` permissions and the
//! exact `ecommerce` module requirement.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use serde::Serialize;

#[derive(Serialize)]
struct Authority<'a> {
    #[serde(rename = "assignmentId")]
    assignment_id: &'a str,
    #[serde(rename = "companyId")]
    company_id: &'a str,
    #[serde(rename = "branchId")]
    branch_id: Option<&'a str>,
    #[serde(rename = "departmentId")]
    department_id: Option<&'a str>,
    modules: &'a [String],
    permissions: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<i64>,
    #[serde(rename = "assignmentVersion", skip_serializing_if = "Option::is_none")]
    assignment_version: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct EcommerceActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl EcommerceActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "ecommerce")
    }

    fn require_context(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has_module() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ECOMMERCE_MODULE_FORBIDDEN",
                "Your current role cannot access E-commerce.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ECOMMERCE_PERMISSION_REQUIRED",
                "Your selected role does not grant E-commerce read access.",
            ));
        }
        Ok(())
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_context(now)?;
        if !self.has("commerce.sales") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ECOMMERCE_PERMISSION_REQUIRED",
                "Your selected role does not grant E-commerce sales access.",
            ));
        }
        Ok(())
    }

    /// Channel and listing terms are shared Company records. A branch or
    /// department assignment cannot review them even when it holds
    /// `commerce.approve`.
    pub fn require_company_review(&self) -> Result<(), BusinessError> {
        if self.branch_id.is_some() || self.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMPANY_ECOMMERCE_REQUIRED",
                "Review shared channel and listing terms through a Company assignment.",
            ));
        }
        if !self.has("commerce.approve") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ECOMMERCE_APPROVAL_REQUIRED",
                "Your selected role does not grant E-commerce approval.",
            ));
        }
        Ok(())
    }

    /// Retention-compatible authority projection. It records the selected
    /// assignment and scope for audit/outbox readers; it grants no authority.
    pub fn authority_json(&self) -> String {
        serde_json::to_string(&Authority {
            assignment_id: self.assignment_id.as_str(),
            company_id: self.company_id.as_str(),
            branch_id: self.branch_id.as_ref().map(|value| value.as_str()),
            department_id: self.department_id.as_ref().map(|value| value.as_str()),
            modules: &self.modules,
            permissions: &self.permissions,
            role: self.role.as_deref(),
            revision: self.authority_revision,
            assignment_version: self.assignment_version,
        })
        .expect("the Ecommerce authority projection always serializes")
    }
}
