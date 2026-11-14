//! Native entitlement decision application services.
//!
//! The retained TS boundary loaded Company purchase facts once, evaluated the
//! three capability purposes and either observed (shadow) or refused (enforced)
//! the result. Disabled mode has no purchase snapshot at all: it returns the
//! explicit disabled contract instead of manufacturing a hash.

use crate::application::purchases::CompanyPurchaseFacts;
use crate::domain::capability_decision::{
    CapabilityGate, EffectiveCapabilityDecision, EffectiveCapabilityInput, capability_revision,
    decision_revision, effective_capability_decision,
};
use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, PackageDecision, PackageDecisionCode,
    PackageEnforcementMode,
};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

pub const MAX_CAPABILITIES: usize = 256;
pub const MAX_CAPABILITY_BYTES: usize = 128;
pub const MAX_GRANTS: usize = 1024;
pub const MAX_HISTORY_RIGHTS: usize = 4096;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityScope {
    pub company_id: String,
    pub capability: String,
    pub purpose: CapabilityPurpose,
}

/// A purchase-only command observation. Disabled mode deliberately has no
/// decision because the retained boundary returns before reading facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CapabilityCheck {
    pub mode: PackageEnforcementMode,
    pub decision: Option<PackageDecision>,
    pub allowed_by_mode: bool,
    pub revision: Option<String>,
}

/// Effective capability presentation. The two revisions are deliberately
/// separate: `source_revision` is the SHA-256 of the retained canonical
/// purchase source, while `revision` additionally binds the capability,
/// purpose and owner gates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CapabilityDecisionObservation {
    pub company_id: String,
    pub capability: String,
    pub purpose: CapabilityPurpose,
    pub decision: EffectiveCapabilityDecision,
    pub source_revision: String,
    pub revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct PurchasedAccessCodes {
    pub new_work: PackageDecisionCode,
    pub settle_existing: PackageDecisionCode,
    pub history: PackageDecisionCode,
}

/// One request-local presentation observation. Owner release, personal
/// authority and record policy must still intersect these flags.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct PurchasedAccess {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
    pub codes: PurchasedAccessCodes,
}

/// Native entitlement port. Implementations read over the caller's existing
/// owner connection and never open another transaction or mint authority.
pub trait CapabilityDecisionPort {
    type Error: From<BusinessError> + Send;

    fn decide(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
        release: &CapabilityGate,
        authority: &CapabilityGate,
        record: &CapabilityGate,
    ) -> impl Future<Output = Result<CapabilityDecisionObservation, Self::Error>> + Send;

    fn check(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<CapabilityCheck, Self::Error>> + Send;

    fn require(
        &mut self,
        capability: &str,
        purpose: CapabilityPurpose,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<CapabilityCheck, Self::Error>> + Send;

    fn access(
        &mut self,
        capability: &str,
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<PurchasedAccess, Self::Error>> + Send;

    fn accesses(
        &mut self,
        capabilities: &[String],
        mode: PackageEnforcementMode,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<BTreeMap<String, PurchasedAccess>, Self::Error>> + Send;
}

impl CapabilityCheck {
    pub fn decision(&self) -> Option<&PackageDecision> {
        self.decision.as_ref()
    }

    pub fn allowed_by_mode(&self) -> bool {
        self.allowed_by_mode
    }
}

pub fn validate_capability(capability: &str) -> Result<(), BusinessError> {
    if capability.is_empty() || capability.len() > MAX_CAPABILITY_BYTES {
        return Err(BusinessError::invalid(
            "COMMERCIAL_CAPABILITY_REQUIRED",
            "Choose one bounded owner capability.",
        ));
    }
    Ok(())
}

/// Validate the exact facts a decision may consume. Unknown package versions
/// remain a retained denial rather than an error; malformed persisted facts are
/// an error so a caller can never treat partial data as entitlement.
pub fn validate_purchase_facts(
    company_id: &str,
    facts: &CompanyPurchaseFacts,
) -> Result<(), BusinessError> {
    if facts.grants.len() > MAX_GRANTS || facts.history.len() > MAX_HISTORY_RIGHTS {
        return Err(invalid_facts());
    }
    let mut packages = BTreeSet::new();
    for grant in &facts.grants {
        if grant.company_id != company_id
            || grant.package_id.is_empty()
            || grant.package_id.len() > 200
            || !(1..=MAX_SAFE).contains(&grant.package_version)
            || !(1..=MAX_SAFE).contains(&grant.version)
            || !matches!(grant.state.as_str(), "active" | "draining" | "inactive")
            || !(0..=MAX_SAFE).contains(&grant.starts_at)
            || grant
                .ends_at
                .is_some_and(|end| end <= grant.starts_at || end > MAX_SAFE)
            || !packages.insert(grant.package_id.as_str())
        {
            return Err(invalid_facts());
        }
    }
    let mut retained = BTreeSet::new();
    for right in &facts.history {
        if right.company_id != company_id
            || right.package_id.is_empty()
            || right.package_id.len() > 200
            || !(1..=MAX_SAFE).contains(&right.package_version)
            || !(1..=MAX_SAFE).contains(&right.source_grant_version)
            || right.source_event_id.is_empty()
            || right.source_event_id.len() > 200
            || !(0..=MAX_SAFE).contains(&right.retained_at)
            || !retained.insert((right.package_id.as_str(), right.package_version))
        {
            return Err(invalid_facts());
        }
    }
    Ok(())
}

pub fn check_purchased_capability(
    catalogue: &CommercialCatalogue,
    facts: &CompanyPurchaseFacts,
    scope: &CapabilityScope,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<CapabilityCheck, BusinessError> {
    validate_capability(&scope.capability)?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(CapabilityCheck {
            mode,
            decision: None,
            allowed_by_mode: true,
            revision: None,
        });
    }
    validate_purchase_facts(&scope.company_id, facts)?;
    let decision = catalogue.purchased_capability(
        &facts.grants,
        &scope.company_id,
        &scope.capability,
        scope.purpose,
        now.get(),
        &facts.history,
    );
    let revision = capability_revision(&scope.company_id, &facts.grants, &facts.history, now)?;
    Ok(CapabilityCheck {
        mode,
        decision: Some(decision),
        allowed_by_mode: mode.permits(decision),
        revision: Some(revision),
    })
}

/// Enforced mode refuses with the retained package code. Shadow mode observes
/// the same code and never blocks. Disabled mode has no purchase decision.
pub fn require_purchased_capability(
    catalogue: &CommercialCatalogue,
    facts: &CompanyPurchaseFacts,
    scope: &CapabilityScope,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<CapabilityCheck, BusinessError> {
    let check = check_purchased_capability(catalogue, facts, scope, mode, now)?;
    if mode == PackageEnforcementMode::Enforced
        && let Some(decision) = check.decision.as_ref().filter(|decision| !decision.allowed)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            decision.code.as_code(),
            "This package does not permit the requested work.",
        ));
    }
    Ok(check)
}

/// Effective decision with owner-supplied release, authority and record gates.
/// The Commercial owner never fabricates those gates.
#[allow(clippy::too_many_arguments)]
pub fn decide_effective_capability(
    catalogue: &CommercialCatalogue,
    facts: &CompanyPurchaseFacts,
    company_id: &str,
    capability: &str,
    purpose: CapabilityPurpose,
    now: TimestampMillis,
    release: &CapabilityGate,
    authority: &CapabilityGate,
    record: &CapabilityGate,
) -> Result<CapabilityDecisionObservation, BusinessError> {
    validate_capability(capability)?;
    validate_purchase_facts(company_id, facts)?;
    let decision = effective_capability_decision(
        catalogue,
        EffectiveCapabilityInput {
            company_id,
            capability,
            purpose,
            grants: &facts.grants,
            history: &facts.history,
            now,
            release,
            authority,
            record,
        },
    );
    let source_revision = capability_revision(company_id, &facts.grants, &facts.history, now)?;
    let revision = decision_revision(&source_revision, capability, purpose, &decision)?;
    Ok(CapabilityDecisionObservation {
        company_id: company_id.to_owned(),
        capability: capability.to_owned(),
        purpose,
        decision,
        source_revision,
        revision,
    })
}

pub fn purchased_access(
    catalogue: &CommercialCatalogue,
    facts: &CompanyPurchaseFacts,
    company_id: &str,
    capability: &str,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<PurchasedAccess, BusinessError> {
    validate_capability(capability)?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(disabled_access());
    }
    validate_purchase_facts(company_id, facts)?;
    let revision = capability_revision(company_id, &facts.grants, &facts.history, now)?;
    let decision = |purpose| {
        catalogue.purchased_capability(
            &facts.grants,
            company_id,
            capability,
            purpose,
            now.get(),
            &facts.history,
        )
    };
    let new_work = decision(CapabilityPurpose::NewWork);
    let settle_existing = decision(CapabilityPurpose::SettleExisting);
    let history = decision(CapabilityPurpose::History);
    let allowed =
        |decision: PackageDecision| mode == PackageEnforcementMode::Shadow || decision.allowed;
    Ok(PurchasedAccess {
        new_work: allowed(new_work),
        settle_existing: allowed(settle_existing),
        history: allowed(history),
        revision,
        codes: PurchasedAccessCodes {
            new_work: new_work.code,
            settle_existing: settle_existing.code,
            history: history.code,
        },
    })
}

/// Related owner controls share one Company purchase/history snapshot.
pub fn purchased_accesses(
    catalogue: &CommercialCatalogue,
    facts: &CompanyPurchaseFacts,
    company_id: &str,
    capabilities: &[String],
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<BTreeMap<String, PurchasedAccess>, BusinessError> {
    if capabilities.is_empty() || capabilities.len() > MAX_CAPABILITIES {
        return Err(BusinessError::invalid(
            "COMMERCIAL_CAPABILITIES_REQUIRED",
            "Choose a bounded set of owner capabilities.",
        ));
    }
    let unique = capabilities.iter().collect::<BTreeSet<_>>();
    for capability in &unique {
        validate_capability(capability)?;
    }
    if mode == PackageEnforcementMode::Disabled {
        return Ok(unique
            .into_iter()
            .map(|capability| (capability.clone(), disabled_access()))
            .collect());
    }
    validate_purchase_facts(company_id, facts)?;
    let revision = capability_revision(company_id, &facts.grants, &facts.history, now)?;
    Ok(unique
        .into_iter()
        .map(|capability| {
            let decisions = [
                CapabilityPurpose::NewWork,
                CapabilityPurpose::SettleExisting,
                CapabilityPurpose::History,
            ]
            .map(|purpose| {
                catalogue.purchased_capability(
                    &facts.grants,
                    company_id,
                    capability,
                    purpose,
                    now.get(),
                    &facts.history,
                )
            });
            let allowed =
                |index: usize| mode == PackageEnforcementMode::Shadow || decisions[index].allowed;
            (
                capability.clone(),
                PurchasedAccess {
                    new_work: allowed(0),
                    settle_existing: allowed(1),
                    history: allowed(2),
                    revision: revision.clone(),
                    codes: PurchasedAccessCodes {
                        new_work: decisions[0].code,
                        settle_existing: decisions[1].code,
                        history: decisions[2].code,
                    },
                },
            )
        })
        .collect())
}

/// The retained Disabled contract. It is an explicit bypass, never a
/// fabricated purchase decision or hash.
fn disabled_access() -> PurchasedAccess {
    PurchasedAccess {
        new_work: true,
        settle_existing: true,
        history: true,
        revision: "disabled".to_owned(),
        codes: PurchasedAccessCodes {
            new_work: PackageDecisionCode::Granted,
            settle_existing: PackageDecisionCode::Granted,
            history: PackageDecisionCode::Granted,
        },
    }
}

fn invalid_facts() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_PURCHASE_FACTS_INVALID",
        "The current Commercial purchase facts are invalid.",
    )
}
