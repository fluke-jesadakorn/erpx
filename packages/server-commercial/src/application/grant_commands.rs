use crate::domain::packages::{CommercialCatalogue, CompanyPackageGrant};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, UserId, json_numbers::deserialize_safe_i64,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::future::Future;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantChange {
    pub grant: CompanyPackageGrant,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub expected_version: i64,
    pub idempotency_key: String,
}
pub struct GrantReceipt {
    pub hash: String,
    pub grant: CompanyPackageGrant,
}
pub struct GrantPreview {
    pub before: Option<CompanyPackageGrant>,
    pub after: CompanyPackageGrant,
    pub expected_version: i64,
}

/// One concrete Commercial command unit. Composition must supply actual
/// current authority and owner-obligation sources; no grant or zero-backlog
/// implementation is implicit. All participating writes use this same unit.
pub trait PackageGrantUnit {
    type Error: From<BusinessError> + Send;
    /// Complete source locks must precede the first owner snapshot. This port
    /// is mandatory even when an earlier projection observed no obligations.
    fn prepare_sources(
        &mut self,
        input: &GrantChange,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn lock(&mut self, key: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn authorize(
        &mut self,
        company: &CompanyId,
    ) -> impl Future<Output = Result<UserId, Self::Error>> + Send;
    fn current(
        &mut self,
        company: &CompanyId,
        package: &str,
    ) -> impl Future<Output = Result<Option<CompanyPackageGrant>, Self::Error>> + Send;
    fn receipt(
        &mut self,
        key: &str,
    ) -> impl Future<Output = Result<Option<GrantReceipt>, Self::Error>> + Send;
    fn obligations(
        &mut self,
        company: &CompanyId,
        package: &str,
    ) -> impl Future<Output = Result<i64, Self::Error>> + Send;
    fn dependencies(
        &mut self,
        grant: &CompanyPackageGrant,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn save(
        &mut self,
        input: &GrantChange,
        current: Option<&CompanyPackageGrant>,
        actor: &UserId,
        hash: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Receipt replay requires current authorization before the receipt is read.
/// Caller owns commit/rollback, receipts, audit and any additional owner effects.
pub async fn apply_package_grant<U: PackageGrantUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &GrantChange,
) -> Result<CompanyPackageGrant, U::Error> {
    if !(16..=128).contains(&input.idempotency_key.len())
        || !input
            .idempotency_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(BusinessError::invalid(
            "INVALID_IDEMPOTENCY_KEY",
            "Provide an owner command key.",
        )
        .into());
    }
    let company = CompanyId::new(&input.grant.company_id).map_err(U::Error::from)?;
    let hash = grant_change_digest(input).map_err(U::Error::from)?;
    unit.prepare_sources(input).await?;
    unit.lock(&format!("receipt:{}", input.idempotency_key))
        .await?;
    unit.lock(&format!("company:{}", input.grant.company_id))
        .await?;
    let actor = unit.authorize(&company).await?;
    if let Some(replay) = unit.receipt(&input.idempotency_key).await? {
        if replay.hash != hash {
            return Err(conflict("IDEMPOTENCY_CONFLICT").into());
        }
        if unit.authorize(&company).await? != actor {
            return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
        }
        return Ok(replay.grant);
    }
    let (checked_actor, current) = inspect(unit, catalogue, input, &company).await?;
    if checked_actor != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    unit.save(input, current.as_ref(), &actor, &hash).await?;
    if unit.authorize(&company).await? != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    Ok(input.grant.clone())
}

pub async fn preview_package_grant<U: PackageGrantUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &GrantChange,
) -> Result<GrantPreview, U::Error> {
    let company = CompanyId::new(&input.grant.company_id).map_err(U::Error::from)?;
    unit.prepare_sources(input).await?;
    let (actor, current) = inspect(unit, catalogue, input, &company).await?;
    if unit.authorize(&company).await? != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    Ok(GrantPreview {
        before: current,
        after: input.grant.clone(),
        expected_version: input.expected_version,
    })
}
async fn inspect<U: PackageGrantUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &GrantChange,
    company: &CompanyId,
) -> Result<(UserId, Option<CompanyPackageGrant>), U::Error> {
    let actor = unit.authorize(company).await?;
    let current = unit.current(company, &input.grant.package_id).await?;
    catalogue
        .validate_grant_change(current.as_ref(), &input.grant, input.expected_version, 0)
        .map_err(U::Error::from)?;
    let outstanding = if catalogue.requires_settlement_review(current.as_ref(), &input.grant) {
        unit.obligations(company, &input.grant.package_id).await?
    } else {
        0
    };
    catalogue
        .validate_grant_change(
            current.as_ref(),
            &input.grant,
            input.expected_version,
            outstanding,
        )
        .map_err(U::Error::from)?;
    unit.dependencies(&input.grant).await?;
    Ok((actor, current))
}

/// Keep the exact legacy flattened property order for existing receipt hashes.
pub fn grant_change_digest(input: &GrantChange) -> Result<String, BusinessError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload<'a> {
        company_id: &'a str,
        package_id: &'a str,
        package_version: i64,
        state: &'a str,
        starts_at: i64,
        ends_at: Option<i64>,
        version: i64,
        reason: &'a str,
        evidence: &'a str,
        expected_version: i64,
    }
    let g = &input.grant;
    let payload = Payload {
        company_id: &g.company_id,
        package_id: &g.package_id,
        package_version: g.package_version,
        state: &g.state,
        starts_at: g.starts_at,
        ends_at: g.ends_at,
        version: g.version,
        reason: &g.reason,
        evidence: &g.evidence,
        expected_version: input.expected_version,
    };
    let bytes = serde_json::to_vec(&payload).map_err(|_| {
        BusinessError::invalid(
            "COMMERCIAL_REQUEST_ENCODING",
            "The command payload cannot be encoded.",
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
fn conflict(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "The Commercial command or current source changed.",
    )
}
