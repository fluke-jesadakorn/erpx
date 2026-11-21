//! Explicit Company ownership for selected Service capabilities. Registered
//! scope accepts only the opaque Company selected by the Holding authorizer;
//! a header or bootstrap Company never grants access.
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, SelectedContextRead},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
#[derive(Clone)]
pub enum ServiceCompanyOwnership {
    Fixed(CompanyId),
    RegisteredHolding,
}
impl From<CompanyId> for ServiceCompanyOwnership {
    fn from(value: CompanyId) -> Self {
        Self::Fixed(value)
    }
}
fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SERVICE_COMPANY_SCOPE_REQUIRED",
        "Use the current registered Holding Company.",
    )
}
impl ServiceCompanyOwnership {
    pub fn request(
        &self,
        value: HoldingWorkspaceRequest,
    ) -> Result<HoldingWorkspaceRequest, BusinessError> {
        match self {
            Self::Fixed(company) if &value.company_id != company => Err(denied()),
            Self::RegisteredHolding if value.expected.is_none() => Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Use the complete rendered Company and work selection.",
            )),
            _ => Ok(value),
        }
    }
    pub fn selected(
        &self,
        holding: &HoldingId,
        selected: &SelectedContextRead,
    ) -> Result<CompanyId, BusinessError> {
        if selected.holding_id() != holding {
            return Err(denied());
        }
        let company = &selected.selection().workspace().company_id;
        if matches!(self, Self::Fixed(expected) if company != expected) {
            return Err(denied());
        }
        Ok(company.clone())
    }
    pub fn current(
        &self,
        holding: &HoldingId,
        current: &AuthorizedWorkContext,
    ) -> Result<(), BusinessError> {
        if current.holding_id() != holding {
            return Err(denied());
        }
        if matches!(self, Self::Fixed(expected) if &current.selection().workspace().company_id != expected)
        {
            return Err(denied());
        }
        Ok(())
    }
}
