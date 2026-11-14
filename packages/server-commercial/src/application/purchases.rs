use crate::application::grant_batches::package_grant_revision;
use crate::domain::company_access::{CompanyPackageOverview, CompanyPurchaseAdmission};
use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight,
    PackageDecision, PackageEnforcementMode,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::Serialize;
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanyPurchaseFacts {
    pub grants: Vec<CompanyPackageGrant>,
    pub history: Vec<CompanyPackageHistoryRight>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityAccess {
    pub capability: String,
    pub purpose: CapabilityPurpose,
    pub decision: PackageDecision,
    pub allowed_by_mode: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PurchaseRequest {
    pub capabilities: Vec<String>,
    pub purpose: CapabilityPurpose,
    pub mode: PackageEnforcementMode,
}
pub trait CompanyPurchaseReadPort {
    type Error: From<BusinessError> + Send;
    fn read(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<CompanyPurchaseFacts, Self::Error>> + Send;
}

/// A read of current admission decisions. Unlike the command witness, this
/// port has no token that can be moved into another transaction or owner unit.
pub trait CompanyPurchaseAdmissionPort {
    type Error: From<BusinessError> + Send;
    fn inspect_admission(
        &mut self,
        request: &PurchaseRequest,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<CompanyPurchaseAdmission, Self::Error>> + Send;
}

pub fn validate_purchase_request(request: &PurchaseRequest) -> Result<(), BusinessError> {
    if request.capabilities.is_empty()
        || request.capabilities.len() > 256
        || request
            .capabilities
            .iter()
            .any(|capability| capability.is_empty() || capability.len() > 128)
    {
        return Err(BusinessError::invalid(
            "COMMERCIAL_CAPABILITIES_REQUIRED",
            "Choose a bounded set of owner capabilities.",
        ));
    }
    Ok(())
}

/// Current catalogue and Company entitlements for the provider package view.
/// The caller supplies the current authority scope and serializable unit; this
/// function never creates a package definition, grant, or history right.
pub async fn company_package_overview<P: CompanyPurchaseReadPort>(
    port: &mut P,
    catalogue: &CommercialCatalogue,
    company: &CompanyId,
    now: TimestampMillis,
) -> Result<CompanyPackageOverview, P::Error> {
    let facts = port.read(company).await?;
    if facts
        .grants
        .iter()
        .any(|grant| grant.company_id != company.as_str())
        || facts
            .history
            .iter()
            .any(|history| history.company_id != company.as_str())
    {
        return Err(changed().into());
    }
    let revision = package_grant_revision(company.as_str(), &facts.grants)?;
    Ok(CompanyPackageOverview {
        company_id: company.as_str().to_owned(),
        revision,
        packages: catalogue.definitions().to_vec(),
        billing_mode: "sandbox".to_owned(),
        observed_at: now.get(),
        grants: facts.grants,
        history: facts.history,
    })
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCIAL_PURCHASE_CHANGED",
        "The current purchase source or transaction changed.",
    )
}
