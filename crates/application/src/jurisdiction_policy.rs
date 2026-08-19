//! Versioned jurisdiction and regulated-sector policy-pack controls.
//!
//! Application policy references official-source register identifiers and
//! certified versions. Numeric tax, payroll, contribution, or regulatory
//! parameters remain in their owning domains; this module deliberately has no
//! field capable of carrying a rate or claiming automatic compliance.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const THAILAND_OFFICIAL_SOURCE_IDS: &[&str] = &[
    "TH-RD-ETAX-001",
    "TH-RD-ETAX-002",
    "TH-RD-PIT-001",
    "TH-RD-WHT-001",
    "TH-SSO-001",
    "TH-PDPC-001",
    "TH-PDPC-002",
    "TH-PDPC-003",
    "TH-BOT-001",
    "TH-BOT-002",
    "TH-OIC-001",
    "TH-MOPH-001",
];

pub const POLICY_ASSURANCE_NOTICE: &str =
    "ERPX assists governed operations; activation evidence is not a claim of automatic compliance.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectorScope {
    GeneralBackOffice,
    BankingBackOffice,
    InsuranceBackOffice,
    HealthcareBackOffice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPackState {
    Draft,
    Review,
    Certified,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialSourceVersion {
    pub source_id: String,
    pub retrieved_at: DateTime<Utc>,
    pub effective_from: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertifiedPolicyRule {
    pub rule_id: String,
    pub owner_context: String,
    pub source_ids: Vec<String>,
    pub interpretation_version: u32,
    pub interpretation_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JurisdictionPolicyPack {
    pub pack_id: String,
    pub jurisdiction: String,
    pub sector: SectorScope,
    pub version: u32,
    pub state: PolicyPackState,
    pub business_owner: String,
    pub certified_by: Option<String>,
    pub certified_at: Option<DateTime<Utc>>,
    pub certification_sha256: Option<String>,
    pub sources: Vec<OfficialSourceVersion>,
    pub rules: Vec<CertifiedPolicyRule>,
}

impl JurisdictionPolicyPack {
    pub fn validate_for_activation(
        &self,
        expected_version: u32,
        activated_at: DateTime<Utc>,
    ) -> Result<(), JurisdictionPolicyError> {
        if self.pack_id.trim().is_empty()
            || self.jurisdiction != "TH"
            || self.version == 0
            || self.version != expected_version
        {
            return Err(JurisdictionPolicyError::InvalidPack);
        }
        if self.state != PolicyPackState::Certified {
            return Err(JurisdictionPolicyError::NotCertified);
        }
        let certified_by = self
            .certified_by
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(JurisdictionPolicyError::MissingCertification)?;
        if certified_by == self.business_owner
            || self.certified_at.is_none_or(|value| value > activated_at)
            || self
                .certification_sha256
                .as_deref()
                .is_none_or(|value| !is_sha256(value))
        {
            return Err(JurisdictionPolicyError::MissingCertification);
        }

        let mut source_ids = BTreeSet::new();
        for source in &self.sources {
            if !THAILAND_OFFICIAL_SOURCE_IDS.contains(&source.source_id.as_str())
                || !source_ids.insert(source.source_id.as_str())
                || source.retrieved_at > activated_at
                || source.effective_from > activated_at
                || source
                    .superseded_at
                    .is_some_and(|until| until <= activated_at)
                || !is_sha256(&source.content_sha256)
            {
                return Err(JurisdictionPolicyError::InvalidSourceEvidence);
            }
        }
        if source_ids.is_empty() || self.rules.is_empty() {
            return Err(JurisdictionPolicyError::InvalidSourceEvidence);
        }
        let mut rule_ids = BTreeSet::new();
        for rule in &self.rules {
            if rule.rule_id.trim().is_empty()
                || rule.owner_context.trim().is_empty()
                || rule.interpretation_version == 0
                || !is_sha256(&rule.interpretation_sha256)
                || !rule_ids.insert(rule.rule_id.as_str())
                || rule.source_ids.is_empty()
                || rule
                    .source_ids
                    .iter()
                    .any(|source_id| !source_ids.contains(source_id.as_str()))
            {
                return Err(JurisdictionPolicyError::InvalidRuleEvidence);
            }
        }
        Ok(())
    }

    pub fn authorize_operation(
        &self,
        request: &JurisdictionOperationRequest,
        activated_at: DateTime<Utc>,
    ) -> Result<JurisdictionDecision, JurisdictionPolicyError> {
        self.validate_for_activation(request.pack_version, activated_at)?;
        if request.realm != OperationRealm::InternalWorkforce
            || request.scope_key.trim().is_empty()
            || request.purpose.trim().is_empty()
        {
            return Err(JurisdictionPolicyError::ScopeDenied);
        }
        let required_sources = request.operation.required_sources();
        if !required_sources.iter().all(|required| {
            self.sources
                .iter()
                .any(|source| source.source_id == *required)
        }) {
            return Err(JurisdictionPolicyError::MissingRequiredSource);
        }
        if !operation_allowed_for_sector(self.sector, request.operation) {
            return Err(JurisdictionPolicyError::OutOfProductScope);
        }
        Ok(JurisdictionDecision {
            pack_id: self.pack_id.clone(),
            pack_version: self.version,
            certification_sha256: self
                .certification_sha256
                .clone()
                .ok_or(JurisdictionPolicyError::MissingCertification)?,
            source_versions: self
                .sources
                .iter()
                .filter(|source| required_sources.contains(&source.source_id.as_str()))
                .map(|source| (source.source_id.clone(), source.content_sha256.clone()))
                .collect(),
            assurance_notice: POLICY_ASSURANCE_NOTICE,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationRealm {
    InternalWorkforce,
    CustomerPortal,
    CandidatePortal,
    SignatoryPortal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JurisdictionOperation {
    ElectronicTaxDocument,
    PayrollTax,
    WithholdingTax,
    SocialSecurity,
    PrivacyGovernance,
    BankingRiskBackOffice,
    InsuranceRiskBackOffice,
    HealthcareSecurityBackOffice,
    InsuranceClaimsAdjudication,
    ClinicalCare,
}

impl JurisdictionOperation {
    const fn required_sources(self) -> &'static [&'static str] {
        match self {
            Self::ElectronicTaxDocument => &["TH-RD-ETAX-001", "TH-RD-ETAX-002"],
            Self::PayrollTax => &["TH-RD-PIT-001"],
            Self::WithholdingTax => &["TH-RD-WHT-001"],
            Self::SocialSecurity => &["TH-SSO-001"],
            Self::PrivacyGovernance => &["TH-PDPC-001", "TH-PDPC-002"],
            Self::BankingRiskBackOffice => &["TH-BOT-001", "TH-BOT-002"],
            Self::InsuranceRiskBackOffice | Self::InsuranceClaimsAdjudication => &["TH-OIC-001"],
            Self::HealthcareSecurityBackOffice | Self::ClinicalCare => &["TH-MOPH-001"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JurisdictionOperationRequest {
    pub pack_version: u32,
    pub scope_key: String,
    pub realm: OperationRealm,
    pub operation: JurisdictionOperation,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JurisdictionDecision {
    pub pack_id: String,
    pub pack_version: u32,
    pub certification_sha256: String,
    pub source_versions: Vec<(String, String)>,
    pub assurance_notice: &'static str,
}

/// Consumer-owned port. The adapter must resolve an exact certified version;
/// silently substituting a newer interpretation is forbidden.
#[async_trait]
pub trait JurisdictionPolicyPackPort: Send + Sync {
    async fn load_exact_version(
        &self,
        pack_id: &str,
        version: u32,
    ) -> Result<Option<JurisdictionPolicyPack>, JurisdictionPolicyError>;

    async fn append_certification_evidence(
        &self,
        pack: &JurisdictionPolicyPack,
    ) -> Result<(), JurisdictionPolicyError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum JurisdictionPolicyError {
    #[error("jurisdiction policy pack is invalid")]
    InvalidPack,
    #[error("jurisdiction policy pack is not certified")]
    NotCertified,
    #[error("independent certification evidence is missing or stale")]
    MissingCertification,
    #[error("official-source evidence is invalid, unknown, stale, or superseded")]
    InvalidSourceEvidence,
    #[error("certified rule evidence is invalid")]
    InvalidRuleEvidence,
    #[error("the required official source version is absent")]
    MissingRequiredSource,
    #[error("operation is outside the certified organization scope or realm")]
    ScopeDenied,
    #[error("operation is outside the ERPX back-office product boundary")]
    OutOfProductScope,
}

fn operation_allowed_for_sector(sector: SectorScope, operation: JurisdictionOperation) -> bool {
    match operation {
        JurisdictionOperation::InsuranceClaimsAdjudication
        | JurisdictionOperation::ClinicalCare => false,
        JurisdictionOperation::BankingRiskBackOffice => sector == SectorScope::BankingBackOffice,
        JurisdictionOperation::InsuranceRiskBackOffice => {
            sector == SectorScope::InsuranceBackOffice
        }
        JurisdictionOperation::HealthcareSecurityBackOffice => {
            sector == SectorScope::HealthcareBackOffice
        }
        _ => true,
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, hour, 0, 0)
            .single()
            .expect("time")
    }

    fn pack(sector: SectorScope, source_ids: &[&str]) -> JurisdictionPolicyPack {
        JurisdictionPolicyPack {
            pack_id: "th-pack".into(),
            jurisdiction: "TH".into(),
            sector,
            version: 7,
            state: PolicyPackState::Certified,
            business_owner: "owner".into(),
            certified_by: Some("legal-reviewer".into()),
            certified_at: Some(at(1)),
            certification_sha256: Some("a".repeat(64)),
            sources: source_ids
                .iter()
                .map(|source_id| OfficialSourceVersion {
                    source_id: (*source_id).into(),
                    retrieved_at: at(0),
                    effective_from: at(0),
                    superseded_at: None,
                    content_sha256: "b".repeat(64),
                })
                .collect(),
            rules: vec![CertifiedPolicyRule {
                rule_id: "rule-1".into(),
                owner_context: "finance".into(),
                source_ids: source_ids.iter().map(|value| (*value).into()).collect(),
                interpretation_version: 3,
                interpretation_sha256: "c".repeat(64),
            }],
        }
    }

    #[test]
    fn thailand_pack_pins_registered_sources_without_embedding_rates() {
        let pack = pack(
            SectorScope::GeneralBackOffice,
            &["TH-RD-ETAX-001", "TH-RD-ETAX-002"],
        );
        let decision = pack
            .authorize_operation(
                &JurisdictionOperationRequest {
                    pack_version: 7,
                    scope_key: "holding/holding/company".into(),
                    realm: OperationRealm::InternalWorkforce,
                    operation: JurisdictionOperation::ElectronicTaxDocument,
                    purpose: "issue governed tax document".into(),
                },
                at(2),
            )
            .expect("certified decision");
        assert_eq!(decision.source_versions.len(), 2);
        assert!(decision.assurance_notice.contains("not a claim"));
    }

    #[test]
    fn unknown_or_superseded_source_and_self_certification_fail_closed() {
        let mut candidate = pack(SectorScope::GeneralBackOffice, &["TH-RD-PIT-001"]);
        candidate.sources[0].source_id = "UNREGISTERED".into();
        assert_eq!(
            candidate.validate_for_activation(7, at(2)),
            Err(JurisdictionPolicyError::InvalidSourceEvidence)
        );
        candidate.sources[0].source_id = "TH-RD-PIT-001".into();
        candidate.certified_by = Some("owner".into());
        assert_eq!(
            candidate.validate_for_activation(7, at(2)),
            Err(JurisdictionPolicyError::MissingCertification)
        );
    }

    #[test]
    fn sector_overlays_are_internal_and_never_adjudicate_claims_or_clinical_care() {
        let insurance = pack(SectorScope::InsuranceBackOffice, &["TH-OIC-001"]);
        let request = |operation, realm| JurisdictionOperationRequest {
            pack_version: 7,
            scope_key: "holding/holding".into(),
            realm,
            operation,
            purpose: "risk evidence".into(),
        };
        assert!(
            insurance
                .authorize_operation(
                    &request(
                        JurisdictionOperation::InsuranceRiskBackOffice,
                        OperationRealm::InternalWorkforce,
                    ),
                    at(2),
                )
                .is_ok()
        );
        assert_eq!(
            insurance.authorize_operation(
                &request(
                    JurisdictionOperation::InsuranceClaimsAdjudication,
                    OperationRealm::InternalWorkforce,
                ),
                at(2),
            ),
            Err(JurisdictionPolicyError::OutOfProductScope)
        );
        assert_eq!(
            insurance.authorize_operation(
                &request(
                    JurisdictionOperation::InsuranceRiskBackOffice,
                    OperationRealm::CustomerPortal,
                ),
                at(2),
            ),
            Err(JurisdictionPolicyError::ScopeDenied)
        );
    }
}
