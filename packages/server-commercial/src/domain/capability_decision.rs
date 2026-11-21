//! Retained capability decision semantics.
//!
//! This module ports `domain/capability-decision.ts` without adding an ambient
//! entitlement fallback. Purchases, current personal authority, release
//! readiness and record policy remain independent gates; a Commercial decision
//! never supplies any of the other three.

use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight,
    PackageDecision, PackageDecisionCode,
};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One owner-supplied gate. An allowed gate without a current revision is not
/// usable, matching the retained `!allowed || !revision` check.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CapabilityGate {
    pub allowed: bool,
    pub revision: String,
}

impl CapabilityGate {
    pub fn withheld() -> Self {
        Self {
            allowed: false,
            revision: String::new(),
        }
    }

    fn ready(&self) -> bool {
        self.allowed && !self.revision.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct CapabilityRevisions {
    pub release: String,
    pub authority: String,
    pub record: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum CapabilityDecisionCode {
    Granted,
    PackageRequired,
    PackageDraining,
    PackageInactive,
    DependencyRequired,
    ReleaseNotReady,
    AuthorityRequired,
    RecordDenied,
}

impl CapabilityDecisionCode {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Granted => "GRANTED",
            Self::PackageRequired => "PACKAGE_REQUIRED",
            Self::PackageDraining => "PACKAGE_DRAINING",
            Self::PackageInactive => "PACKAGE_INACTIVE",
            Self::DependencyRequired => "DEPENDENCY_REQUIRED",
            Self::ReleaseNotReady => "RELEASE_NOT_READY",
            Self::AuthorityRequired => "AUTHORITY_REQUIRED",
            Self::RecordDenied => "RECORD_DENIED",
        }
    }

    pub fn from_package(code: PackageDecisionCode) -> Self {
        match code {
            PackageDecisionCode::Granted => Self::Granted,
            PackageDecisionCode::PackageRequired => Self::PackageRequired,
            PackageDecisionCode::PackageDraining => Self::PackageDraining,
            PackageDecisionCode::PackageInactive => Self::PackageInactive,
            PackageDecisionCode::DependencyRequired => Self::DependencyRequired,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct EffectiveCapabilityDecision {
    pub allowed: bool,
    pub code: CapabilityDecisionCode,
    pub revisions: CapabilityRevisions,
}

/// Owner-supplied facts and gates. A purchase caller never supplies authority,
/// release readiness or record policy; those owners intersect the result.
#[derive(Clone, Copy, Debug)]
pub struct EffectiveCapabilityInput<'a> {
    pub company_id: &'a str,
    pub capability: &'a str,
    pub purpose: CapabilityPurpose,
    pub grants: &'a [CompanyPackageGrant],
    pub history: &'a [CompanyPackageHistoryRight],
    pub now: TimestampMillis,
    pub release: &'a CapabilityGate,
    pub authority: &'a CapabilityGate,
    pub record: &'a CapabilityGate,
}

/// Apply the retained precedence: purchase first, then release, authority and
/// the actual record policy. Missing revisions fail closed.
pub fn effective_capability_decision(
    catalogue: &CommercialCatalogue,
    input: EffectiveCapabilityInput<'_>,
) -> EffectiveCapabilityDecision {
    let purchased = catalogue.purchased_capability(
        input.grants,
        input.company_id,
        input.capability,
        input.purpose,
        input.now.get(),
        input.history,
    );
    let revisions = CapabilityRevisions {
        release: input.release.revision.clone(),
        authority: input.authority.revision.clone(),
        record: input.record.revision.clone(),
    };
    let code = if !purchased.allowed {
        CapabilityDecisionCode::from_package(purchased.code)
    } else if !input.release.ready() {
        CapabilityDecisionCode::ReleaseNotReady
    } else if !input.authority.ready() {
        CapabilityDecisionCode::AuthorityRequired
    } else if !input.record.ready() {
        CapabilityDecisionCode::RecordDenied
    } else {
        CapabilityDecisionCode::Granted
    };
    EffectiveCapabilityDecision {
        allowed: code == CapabilityDecisionCode::Granted,
        code,
        revisions,
    }
}

/// Exact retained `capabilityRevisionSource` payload. Row order and clock ticks
/// alone never change it; a start, expiry or retention boundary does.
pub fn capability_revision_source(
    company_id: &str,
    grants: &[CompanyPackageGrant],
    history: &[CompanyPackageHistoryRight],
    now: TimestampMillis,
) -> Result<String, BusinessError> {
    crate::domain::purchase_wire_revision::capability_revision_source(
        company_id, grants, history, now,
    )
}

/// SHA-256 of the retained canonical source, lowercase hex. This is an
/// observation revision, not authority, a release or a command witness.
pub fn capability_revision(
    company_id: &str,
    grants: &[CompanyPackageGrant],
    history: &[CompanyPackageHistoryRight],
    now: TimestampMillis,
) -> Result<String, BusinessError> {
    Ok(hex::encode(Sha256::digest(
        capability_revision_source(company_id, grants, history, now)?.as_bytes(),
    )))
}

/// The retained presentation revision also binds the capability, purpose and
/// effective decision. It is not used by `PurchasedAccess`, whose revision is
/// the purchase source revision alone.
pub(crate) fn decision_revision(
    source_revision: &str,
    capability: &str,
    purpose: CapabilityPurpose,
    decision: &EffectiveCapabilityDecision,
) -> Result<String, BusinessError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Decision<'a> {
        capability: &'a str,
        purpose: CapabilityPurpose,
        allowed: bool,
        code: CapabilityDecisionCode,
        revisions: &'a CapabilityRevisions,
    }
    let wire = serde_json::to_string(&Decision {
        capability,
        purpose,
        allowed: decision.allowed,
        code: decision.code,
        revisions: &decision.revisions,
    })
    .map_err(|_| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "COMMERCIAL_DECISION_ENCODING",
            "The capability decision could not be encoded.",
        )
    })?;
    Ok(hex::encode(Sha256::digest(
        format!("{source_revision}{wire}").as_bytes(),
    )))
}

/// Convenience for tests and owner adapters that already have purchase facts.
/// The catalogue's retained decision remains the only purchase authority.
pub fn purchased_decision(
    catalogue: &CommercialCatalogue,
    grants: &[CompanyPackageGrant],
    company_id: &str,
    capability: &str,
    purpose: CapabilityPurpose,
    history: &[CompanyPackageHistoryRight],
    now: TimestampMillis,
) -> PackageDecision {
    catalogue.purchased_capability(grants, company_id, capability, purpose, now.get(), history)
}
