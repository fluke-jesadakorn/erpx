use erpx_server_shared::public::{
    BusinessError,
    json_numbers::{deserialize_optional_safe_i64, deserialize_safe_i64},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageDefinition {
    pub id: String,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub version: i64,
    pub product_id: String,
    pub kind: String,
    pub visible_capabilities: Vec<String>,
    pub included_capabilities: Vec<String>,
    pub dependencies: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogueDocument {
    schema: String,
    products: Vec<String>,
    packages: Vec<PackageDefinition>,
}

/// Only the checked-in owner catalogue constructs this model. A request DTO
/// cannot replace commercial definitions or select a private resource path.
pub struct CommercialCatalogue {
    products: Vec<String>,
    packages: Vec<PackageDefinition>,
}
impl CommercialCatalogue {
    pub(crate) fn parse_json(document: &str) -> Result<Self, BusinessError> {
        let raw: CatalogueDocument =
            serde_json::from_str(document).map_err(|_| invalid_catalogue())?;
        if raw.schema != "erpx-commercial-packages/v1"
            || raw.products.is_empty()
            || raw.packages.is_empty()
        {
            return Err(invalid_catalogue());
        }
        let products = raw.products.iter().collect::<BTreeSet<_>>();
        let mut identities = BTreeSet::new();
        if products.len() != raw.products.len() {
            return Err(invalid_catalogue());
        }
        for p in &raw.packages {
            if p.id.is_empty()
                || p.version < 1
                || !products.contains(&p.product_id)
                || !["base", "addon"].contains(&p.kind.as_str())
                || !identities.insert((&p.id, p.version))
            {
                return Err(invalid_catalogue());
            }
            let capabilities = p
                .visible_capabilities
                .iter()
                .chain(&p.included_capabilities)
                .collect::<BTreeSet<_>>();
            if capabilities.len() != p.visible_capabilities.len() + p.included_capabilities.len()
                || capabilities.iter().any(|s| s.is_empty())
            {
                return Err(invalid_catalogue());
            }
            if p.dependencies
                .iter()
                .any(|id| id == &p.id || !raw.packages.iter().any(|parent| &parent.id == id))
            {
                return Err(invalid_catalogue());
            }
        }
        Ok(Self {
            products: raw.products,
            packages: raw.packages,
        })
    }
    pub fn definitions(&self) -> &[PackageDefinition] {
        &self.packages
    }
    pub fn products(&self) -> &[String] {
        &self.products
    }
    pub fn definition(&self, id: &str, version: i64) -> Option<&PackageDefinition> {
        self.packages
            .iter()
            .find(|p| p.id == id && p.version == version)
    }
    pub fn capabilities(&self) -> Vec<&str> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for p in &self.packages {
            for capability in p
                .visible_capabilities
                .iter()
                .chain(&p.included_capabilities)
            {
                if seen.insert(capability.as_str()) {
                    result.push(capability.as_str());
                }
            }
        }
        result
    }

    pub fn purchased_capability(
        &self,
        grants: &[CompanyPackageGrant],
        company: &str,
        capability: &str,
        purpose: CapabilityPurpose,
        now: i64,
        history: &[CompanyPackageHistoryRight],
    ) -> PackageDecision {
        let scoped = grants
            .iter()
            .filter(|g| g.company_id == company && g.starts_at <= now)
            .collect::<Vec<_>>();
        let mut denial = PackageDecisionCode::PackageRequired;
        if purpose == CapabilityPurpose::History {
            let sources = scoped
                .iter()
                .filter(|g| g.state != "inactive")
                .map(|g| (g.package_id.as_str(), g.package_version))
                .chain(
                    history
                        .iter()
                        .filter(|h| {
                            h.company_id == company
                                && h.retained_at >= 0
                                && h.retained_at <= MAX_SAFE
                                && h.retained_at <= now
                                && h.source_grant_version > 0
                                && h.source_grant_version <= MAX_SAFE
                                && !h.source_event_id.is_empty()
                        })
                        .map(|h| (h.package_id.as_str(), h.package_version)),
                )
                .collect::<Vec<_>>();
            for (id, version) in &sources {
                let Some(definition) = self.definition(id, *version) else {
                    continue;
                };
                if !offers(definition, capability) {
                    continue;
                }
                if definition.dependencies.iter().any(|dependency| {
                    !sources.iter().any(|(id, version)| {
                        *id == dependency && self.definition(id, *version).is_some()
                    })
                }) {
                    denial = PackageDecisionCode::DependencyRequired;
                    continue;
                }
                return PackageDecision::granted();
            }
        } else {
            for grant in &scoped {
                let Some(definition) = self.definition(&grant.package_id, grant.package_version)
                else {
                    continue;
                };
                if !offers(definition, capability) {
                    continue;
                }
                if !usable(grant, now) && purpose == CapabilityPurpose::NewWork {
                    denial = if grant.state == "inactive" {
                        PackageDecisionCode::PackageInactive
                    } else {
                        PackageDecisionCode::PackageDraining
                    };
                    continue;
                }
                if grant.state == "inactive" {
                    denial = PackageDecisionCode::PackageInactive;
                    continue;
                }
                if definition.dependencies.iter().any(|dependency| {
                    !scoped.iter().any(|parent| {
                        parent.package_id == *dependency
                            && self
                                .definition(&parent.package_id, parent.package_version)
                                .is_some()
                            && if purpose == CapabilityPurpose::NewWork {
                                usable(parent, now)
                            } else {
                                parent.state != "inactive"
                            }
                    })
                }) {
                    denial = PackageDecisionCode::DependencyRequired;
                    continue;
                }
                return PackageDecision::granted();
            }
        }
        PackageDecision {
            allowed: false,
            code: denial,
        }
    }

    pub fn requires_settlement_review(
        &self,
        current: Option<&CompanyPackageGrant>,
        next: &CompanyPackageGrant,
    ) -> bool {
        if next.state == "inactive" {
            return true;
        }
        let Some(current) = current else { return false };
        if current.state == "inactive" {
            return false;
        }
        if next.starts_at > current.starts_at {
            return true;
        }
        let (Some(before), Some(after)) = (
            self.definition(&current.package_id, current.package_version),
            self.definition(&next.package_id, next.package_version),
        ) else {
            return true;
        };
        before
            .visible_capabilities
            .iter()
            .chain(&before.included_capabilities)
            .any(|capability| !offers(after, capability))
    }

    pub fn validate_dependencies(
        &self,
        grant: &CompanyPackageGrant,
        all: &[CompanyPackageGrant],
        now: i64,
    ) -> Result<(), BusinessError> {
        let definition = self
            .definition(&grant.package_id, grant.package_version)
            .ok_or_else(|| grant_error("UNKNOWN_PACKAGE_VERSION"))?;
        let dependents = all
            .iter()
            .filter(|child| {
                child.company_id == grant.company_id
                    && child.state != "inactive"
                    && self
                        .definition(&child.package_id, child.package_version)
                        .is_some_and(|p| p.dependencies.contains(&grant.package_id))
            })
            .collect::<Vec<_>>();
        if grant.state == "inactive" {
            return if dependents.is_empty() {
                Ok(())
            } else {
                Err(grant_error("PACKAGE_DEPENDENCY_IN_USE"))
            };
        }
        for dependency in &definition.dependencies {
            let current = all.iter().find(|candidate| {
                candidate.company_id == grant.company_id && candidate.package_id == *dependency
            });
            if current.is_none_or(|parent| {
                self.definition(&parent.package_id, parent.package_version)
                    .is_none()
                    || parent.state == "inactive"
                    || parent.starts_at > grant.starts_at
                    || grant.state == "active"
                        && (parent.state != "active"
                            || parent.ends_at.is_some_and(|end| {
                                grant.ends_at.is_none_or(|child_end| end < child_end) || end <= now
                            }))
            }) {
                return Err(grant_error("PACKAGE_DEPENDENCY_REQUIRED"));
            }
        }
        if definition.kind == "base"
            && dependents.iter().any(|child| {
                child.starts_at < grant.starts_at
                    || grant.state == "active"
                        && child.state == "active"
                        && grant.ends_at.is_some_and(|end| {
                            child.ends_at.is_none_or(|child_end| child_end > end)
                        })
            })
        {
            return Err(grant_error("PACKAGE_DEPENDENCY_PERIOD_CONFLICT"));
        }
        Ok(())
    }

    pub fn validate_grant_change(
        &self,
        current: Option<&CompanyPackageGrant>,
        next: &CompanyPackageGrant,
        expected_version: i64,
        outstanding_obligations: i64,
    ) -> Result<(), BusinessError> {
        if self
            .definition(&next.package_id, next.package_version)
            .is_none()
        {
            return Err(grant_error("UNKNOWN_PACKAGE_VERSION"));
        }
        if !["active", "draining", "inactive"].contains(&next.state.as_str()) {
            return Err(grant_error("INVALID_PACKAGE_STATE"));
        }
        if next.company_id.is_empty()
            || js_trim(&next.reason).is_empty()
            || js_trim(&next.evidence).is_empty()
        {
            return Err(grant_error("GRANT_EVIDENCE_REQUIRED"));
        }
        if next.starts_at < 0
            || next.starts_at > MAX_SAFE
            || next
                .ends_at
                .is_some_and(|end| end <= next.starts_at || end > MAX_SAFE)
        {
            return Err(grant_error("INVALID_GRANT_PERIOD"));
        }
        if !(0..=MAX_SAFE).contains(&expected_version)
            || next.version < -MAX_SAFE
            || next.version > MAX_SAFE
            || current.map_or(0, |g| g.version) != expected_version
            || expected_version.checked_add(1) != Some(next.version)
        {
            return Err(grant_error("STALE_PACKAGE_GRANT"));
        }
        if current
            .is_some_and(|g| g.company_id != next.company_id || g.package_id != next.package_id)
        {
            return Err(grant_error("PACKAGE_GRANT_IDENTITY_MISMATCH"));
        }
        if current.is_none() && next.state != "active"
            || current.is_some_and(|g| g.state == "active") && next.state == "inactive"
        {
            return Err(grant_error("DRAIN_PACKAGE_FIRST"));
        }
        if self.requires_settlement_review(current, next) && outstanding_obligations != 0 {
            return Err(grant_error("PACKAGE_OBLIGATIONS_REMAIN"));
        }
        Ok(())
    }
}

const MAX_SAFE: i64 = 9_007_199_254_740_991;
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanyPackageGrant {
    pub company_id: String,
    pub package_id: String,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub package_version: i64,
    pub state: String,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub starts_at: i64,
    #[serde(deserialize_with = "deserialize_optional_safe_i64")]
    pub ends_at: Option<i64>,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub version: i64,
    pub reason: String,
    pub evidence: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanyPackageHistoryRight {
    pub company_id: String,
    pub package_id: String,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub package_version: i64,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub source_grant_version: i64,
    pub source_event_id: String,
    #[serde(deserialize_with = "deserialize_safe_i64")]
    pub retained_at: i64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityPurpose {
    NewWork,
    SettleExisting,
    History,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PackageEnforcementMode {
    Disabled,
    Shadow,
    Enforced,
}
impl PackageEnforcementMode {
    pub fn permits(self, decision: PackageDecision) -> bool {
        self != Self::Enforced || decision.allowed
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageDecisionCode {
    Granted,
    PackageRequired,
    PackageDraining,
    PackageInactive,
    DependencyRequired,
}
impl PackageDecisionCode {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Granted => "GRANTED",
            Self::PackageRequired => "PACKAGE_REQUIRED",
            Self::PackageDraining => "PACKAGE_DRAINING",
            Self::PackageInactive => "PACKAGE_INACTIVE",
            Self::DependencyRequired => "DEPENDENCY_REQUIRED",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PackageDecision {
    pub allowed: bool,
    pub code: PackageDecisionCode,
}
impl PackageDecision {
    fn granted() -> Self {
        Self {
            allowed: true,
            code: PackageDecisionCode::Granted,
        }
    }
}
fn offers(definition: &PackageDefinition, capability: &str) -> bool {
    definition
        .visible_capabilities
        .iter()
        .chain(&definition.included_capabilities)
        .any(|c| c == capability)
}
fn usable(grant: &CompanyPackageGrant, now: i64) -> bool {
    grant.state == "active" && grant.ends_at.is_none_or(|end| end > now)
}
fn invalid_catalogue() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_CATALOGUE_INVALID",
        "The versioned Commercial catalogue is invalid.",
    )
}
fn grant_error(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The package grant change is invalid.")
}
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c:char|matches!(c,'\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
