use crate::domain::packages::{
    CapabilityPurpose, CompanyPackageGrant, CompanyPackageHistoryRight, PackageDecision,
    PackageDefinition,
};
use serde::Serialize;

/// One current Company package view. `revision` is the same Company revision
/// used by package command CAS; it is not a person's authority or a release.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanyPackageOverview {
    pub company_id: String,
    pub revision: String,
    pub packages: Vec<PackageDefinition>,
    pub billing_mode: String,
    pub observed_at: i64,
    pub grants: Vec<CompanyPackageGrant>,
    pub history: Vec<CompanyPackageHistoryRight>,
}

/// One observed admission decision. It never carries the transaction witness
/// that an owner command must revalidate inside its own owner unit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanyCapabilityAdmission {
    pub capability: String,
    pub purpose: CapabilityPurpose,
    pub decision: PackageDecision,
    pub allowed_by_mode: bool,
}

/// A bounded read of the current Company purchase source. The caller still
/// owns current Account/IAM admission and any same-transaction owner effect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CompanyPurchaseAdmission {
    pub company_id: String,
    pub revision: String,
    pub observed_at: i64,
    pub access: Vec<CompanyCapabilityAdmission>,
}
