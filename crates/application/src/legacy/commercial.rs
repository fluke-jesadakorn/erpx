use super::super::*;

pub fn authorize_entitlement(
    billing: BillingState,
    entitlement: Option<&ModuleEntitlement>,
    rbac_allows: bool,
    at: DateTime<Utc>,
    is_mutation: bool,
) -> Result<(), ApplicationError> {
    match authorize_module(billing, entitlement, rbac_allows, at, is_mutation) {
        EntitlementDecision::Allowed => Ok(()),
        EntitlementDecision::Denied(reason) => Err(ApplicationError::Authorization(reason.into())),
    }
}
