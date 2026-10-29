use crate::domain::packages::{
    CapabilityPurpose, CompanyPackageGrant, CompanyPackageHistoryRight, PackageDecision,
    PackageEnforcementMode,
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use serde::Serialize;
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
