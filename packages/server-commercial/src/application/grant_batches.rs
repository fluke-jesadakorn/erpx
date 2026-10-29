use super::grant_commands::{
    GrantChange, PackageGrantUnit, apply_package_grant, preview_package_grant,
};
use crate::domain::collation::package_id_collator;
use crate::domain::packages::{CommercialCatalogue, CompanyPackageGrant};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, UserId, json_numbers::deserialize_safe_i64,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, future::Future};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchChange {
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub expected_version: i64,
    pub grant: CompanyPackageGrant,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantBatch {
    pub company_id: String,
    pub expected_revision: String,
    pub changes: Vec<BatchChange>,
    pub idempotency_key: String,
}
/// Field order is the original normalized JSON request, excluding its key.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizedGrantBatch {
    pub company_id: String,
    pub expected_revision: String,
    pub changes: Vec<BatchChange>,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantBatchPreview {
    pub company_id: String,
    pub expected_revision: String,
    pub changes: Vec<BatchChange>,
    pub before: Vec<CompanyPackageGrant>,
    pub after: Vec<CompanyPackageGrant>,
}
pub struct BatchReceipt {
    pub hash: String,
    pub result: GrantBatchPreview,
}

pub trait PackageBatchUnit: PackageGrantUnit + Send {
    fn all(
        &mut self,
        company: &CompanyId,
        writing: bool,
    ) -> impl Future<Output = Result<Vec<CompanyPackageGrant>, Self::Error>> + Send;
    fn batch_receipt(
        &mut self,
        key: &str,
    ) -> impl Future<Output = Result<Option<BatchReceipt>, Self::Error>> + Send;
    fn review(
        &mut self,
        preview: &GrantBatchPreview,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Runs final owner/deferred-source effects and rechecks review time after
    /// those potentially waiting reads, while the whole unit can roll back.
    fn retain_review(
        &mut self,
        preview: &GrantBatchPreview,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn prospective_dependencies(
        &mut self,
        grant: &CompanyPackageGrant,
        prospective: &[CompanyPackageGrant],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn save_batch_receipt(
        &mut self,
        key: &str,
        hash: &str,
        actor: &UserId,
        result: &GrantBatchPreview,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub fn normalize_grant_batch(
    catalogue: &CommercialCatalogue,
    input: &GrantBatch,
) -> Result<NormalizedGrantBatch, BusinessError> {
    if input.company_id.is_empty()
        || input.expected_revision.len() != 64
        || !input
            .expected_revision
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || !(16..=128).contains(&input.idempotency_key.len())
        || !input
            .idempotency_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        || input.changes.is_empty()
        || input.changes.len() > 100
    {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for change in &input.changes {
        if change.grant.company_id != input.company_id || !seen.insert(&change.grant.package_id) {
            return Err(invalid());
        }
    }
    let mut changes = input.changes.clone();
    let collator = package_id_collator()?;
    changes.sort_by(|a, b| {
        let addon = |change: &BatchChange| {
            catalogue
                .definition(&change.grant.package_id, change.grant.package_version)
                .is_some_and(|p| p.kind == "addon")
        };
        addon(a)
            .cmp(&addon(b))
            .then_with(|| collator.compare(&a.grant.package_id, &b.grant.package_id))
    });
    Ok(NormalizedGrantBatch {
        company_id: input.company_id.clone(),
        expected_revision: input.expected_revision.clone(),
        changes,
    })
}
pub fn grant_batch_digest(input: &NormalizedGrantBatch) -> Result<String, BusinessError> {
    digest(input)
}
pub fn package_grant_revision(
    company: &str,
    grants: &[CompanyPackageGrant],
) -> Result<String, BusinessError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Revision<'a> {
        company_id: &'a str,
        grants: Vec<&'a CompanyPackageGrant>,
    }
    let mut scoped: Vec<_> = grants.iter().filter(|g| g.company_id == company).collect();
    let collator = package_id_collator()?;
    scoped.sort_by(|a, b| collator.compare(&a.package_id, &b.package_id));
    digest(&Revision {
        company_id: company,
        grants: scoped,
    })
}
fn digest(value: &impl Serialize) -> Result<String, BusinessError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| invalid())?,
    )))
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVALID_PACKAGE_BATCH",
        "Provide a bounded Company package change set.",
    )
}
fn conflict(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "The Commercial batch or current authority changed.",
    )
}
fn child_key(key: &str, package: &str) -> String {
    hex::encode(Sha256::digest(format!("{key}:{package}")))
}

pub async fn preview_package_batch<U: PackageBatchUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &GrantBatch,
) -> Result<GrantBatchPreview, U::Error> {
    let normalized = normalize_grant_batch(catalogue, input)?;
    prepare(unit, &normalized).await?;
    let company = CompanyId::new(&normalized.company_id)?;
    let actor = unit.authorize(&company).await?;
    let preview = inspect(unit, catalogue, &normalized, false).await?;
    if unit.authorize(&company).await? != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    Ok(preview)
}
pub async fn apply_package_batch<U: PackageBatchUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &GrantBatch,
) -> Result<GrantBatchPreview, U::Error> {
    let normalized = normalize_grant_batch(catalogue, input)?;
    let hash = grant_batch_digest(&normalized)?;
    prepare(unit, &normalized).await?;
    // Acquire every receipt before Company. This includes the per-package keys
    // used by the normal grant application, preventing opposite lock ordering.
    let mut keys: BTreeSet<String> = normalized
        .changes
        .iter()
        .map(|c| child_key(&input.idempotency_key, &c.grant.package_id))
        .collect();
    keys.insert(input.idempotency_key.clone());
    for key in keys {
        unit.lock(&format!("receipt:{key}")).await?;
    }
    unit.lock(&format!("company:{}", normalized.company_id))
        .await?;
    let company = CompanyId::new(&normalized.company_id)?;
    let actor = unit.authorize(&company).await?;
    if let Some(receipt) = unit.batch_receipt(&input.idempotency_key).await? {
        if receipt.hash != hash {
            return Err(conflict("IDEMPOTENCY_CONFLICT").into());
        }
        if unit.authorize(&company).await? != actor {
            return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
        }
        return Ok(receipt.result);
    }
    let preview = inspect(unit, catalogue, &normalized, true).await?;
    unit.review(&preview).await?;
    {
        let mut prospective = ProspectiveUnit {
            inner: unit,
            after: &preview.after,
        };
        for change in &normalized.changes {
            apply_package_grant(
                &mut prospective,
                catalogue,
                &GrantChange {
                    grant: change.grant.clone(),
                    expected_version: change.expected_version,
                    idempotency_key: child_key(&input.idempotency_key, &change.grant.package_id),
                },
            )
            .await?;
        }
    }
    if unit.authorize(&company).await? != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    unit.save_batch_receipt(&input.idempotency_key, &hash, &actor, &preview)
        .await?;
    unit.retain_review(&preview).await?;
    if unit.authorize(&company).await? != actor {
        return Err(conflict("COMMERCIAL_AUTHORITY_CHANGED").into());
    }
    Ok(preview)
}
async fn prepare<U: PackageBatchUnit>(
    unit: &mut U,
    input: &NormalizedGrantBatch,
) -> Result<(), U::Error> {
    // Every owner source lease is acquired before any authority/query snapshot.
    for change in &input.changes {
        unit.prepare_sources(&GrantChange {
            grant: change.grant.clone(),
            expected_version: change.expected_version,
            idempotency_key: "preview-only-no-write".into(),
        })
        .await?;
    }
    Ok(())
}
async fn inspect<U: PackageBatchUnit>(
    unit: &mut U,
    catalogue: &CommercialCatalogue,
    input: &NormalizedGrantBatch,
    writing: bool,
) -> Result<GrantBatchPreview, U::Error> {
    let company = CompanyId::new(&input.company_id)?;
    let before = unit.all(&company, writing).await?;
    if package_grant_revision(&input.company_id, &before)? != input.expected_revision {
        return Err(conflict("STALE_PACKAGE_BATCH").into());
    }
    let after = prospective_package_grants(&before, input)?;
    let mut preview = ProspectiveUnit {
        inner: unit,
        after: &after,
    };
    for change in &input.changes {
        preview_package_grant(
            &mut preview,
            catalogue,
            &GrantChange {
                grant: change.grant.clone(),
                expected_version: change.expected_version,
                idempotency_key: "preview-only-no-write".into(),
            },
        )
        .await?;
    }
    Ok(GrantBatchPreview {
        company_id: input.company_id.clone(),
        expected_revision: input.expected_revision.clone(),
        changes: input.changes.clone(),
        before,
        after,
    })
}
/// Private adapter overrides only dependencies. Source locks and all receipt
/// locks were obtained by the enclosing batch before its first snapshot.
struct ProspectiveUnit<'a, U> {
    inner: &'a mut U,
    after: &'a [CompanyPackageGrant],
}
impl<U: PackageBatchUnit> PackageGrantUnit for ProspectiveUnit<'_, U> {
    type Error = U::Error;
    async fn prepare_sources(&mut self, _: &GrantChange) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn lock(&mut self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn authorize(&mut self, c: &CompanyId) -> Result<UserId, Self::Error> {
        self.inner.authorize(c).await
    }
    async fn current(
        &mut self,
        c: &CompanyId,
        p: &str,
    ) -> Result<Option<CompanyPackageGrant>, Self::Error> {
        self.inner.current(c, p).await
    }
    async fn receipt(
        &mut self,
        k: &str,
    ) -> Result<Option<super::grant_commands::GrantReceipt>, Self::Error> {
        self.inner.receipt(k).await
    }
    async fn obligations(&mut self, c: &CompanyId, p: &str) -> Result<i64, Self::Error> {
        self.inner.obligations(c, p).await
    }
    async fn dependencies(&mut self, g: &CompanyPackageGrant) -> Result<(), Self::Error> {
        self.inner.prospective_dependencies(g, self.after).await
    }
    async fn save(
        &mut self,
        i: &GrantChange,
        c: Option<&CompanyPackageGrant>,
        a: &UserId,
        h: &str,
    ) -> Result<(), Self::Error> {
        self.inner.save(i, c, a, h).await
    }
}

/// Build data for the same stable Map replacement/insertion preview as legacy.
/// This pure projection never supplies permission or a reviewed command proof.
pub fn prospective_package_grants(
    before: &[CompanyPackageGrant],
    input: &NormalizedGrantBatch,
) -> Result<Vec<CompanyPackageGrant>, BusinessError> {
    let mut after = before.to_vec();
    for change in &input.changes {
        if let Some(existing) = after
            .iter_mut()
            .find(|g| g.package_id == change.grant.package_id)
        {
            *existing = change.grant.clone();
        } else {
            after.push(change.grant.clone());
        }
    }
    let collator = package_id_collator()?;
    after.sort_by(|a, b| collator.compare(&a.package_id, &b.package_id));
    Ok(after)
}
