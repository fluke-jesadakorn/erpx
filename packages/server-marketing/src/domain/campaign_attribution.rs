//! Native Marketing campaign ledger and lead-capture attribution facts.
//!
//! These are owner read projections only. They never create a campaign, link a
//! cost, activate an asset or hand a candidate to CRM. Composition retains the
//! normal Account/Holding session, current authority, package admission and
//! serializable owner transaction; this domain enforces the current Company
//! and selected branch/department scope, `records.read`, the `marketing`
//! module, work-context expiry, exact campaign/attribution row shapes,
//! deterministic ordering and bounded page sizes before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;

pub const CAMPAIGN_LIMIT: usize = 500;
pub const ATTRIBUTION_LIMIT: usize = 1_000;
const CODE_LIMIT: usize = 60;
const NAME_LIMIT: usize = 160;
const ATTRIBUTION_KEY_LIMIT: usize = 300;
pub const UNSPECIFIED: &str = "Unspecified";

/// Trusted runtime configuration for candidate provenance, independent of the
/// entitlement presentation. This is not a request or authorization field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketingAttributionMode {
    Disabled,
    Shadow,
    Enforced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateProvenance {
    All,
    CapturedForm,
}

#[derive(Clone, Debug)]
pub struct MarketingCampaignActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl MarketingCampaignActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "marketing")
    }

    pub fn require_report(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MARKETING_CAMPAIGN_PERMISSION_REQUIRED",
                "Your selected role does not grant Marketing read access.",
            ));
        }
        if !self.has_module() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MARKETING_MODULE_FORBIDDEN",
                "Your current role cannot access Marketing.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_CAMPAIGN_SOURCE_INVALID",
        "The current Marketing campaign source is invalid.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::invalid(
        "MARKETING_CAMPAIGN_TOO_LARGE",
        "Narrow the Marketing campaign scope before reading it.",
    )
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

/// Limits count UTF-16 code units, matching the native transport
/// revalidation. Control rejection intentionally keeps the full
/// `char::is_control` superset (including C1 controls), which the
/// transport permits; the native read fails closed there instead.
fn valid_text(value: &str, limit: usize) -> bool {
    let length = value.encode_utf16().count();
    length > 0 && length <= limit && !value.chars().any(char::is_control)
}

/// Exact ECMAScript `String.prototype.trim` edge set, including the zero-width
/// no-break space. U+0085 (NEL) and U+00AD (soft hyphen) are not part of it,
/// matching the native transport revalidation on the read path.
fn is_edge_whitespace(character: char) -> bool {
    matches!(character,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}')
}

fn trim_edge_whitespace(value: &str) -> &str {
    value.trim_matches(is_edge_whitespace)
}

fn valid_code(value: &str) -> bool {
    (2..=CODE_LIMIT).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_name(value: &str) -> bool {
    let trimmed = trim_edge_whitespace(value);
    !value.contains('\0')
        && value.encode_utf16().count() <= NAME_LIMIT
        && trimmed.encode_utf16().count() >= 2
}

fn valid_attribution_key(value: &str) -> bool {
    valid_text(value, ATTRIBUTION_KEY_LIMIT) && trim_edge_whitespace(value) == value
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarketingCampaignRow {
    pub id: String,
    pub company_id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
    pub created_at: i64,
}

impl MarketingCampaignRow {
    pub fn validate(&self, actor: &MarketingCampaignActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_code(&self.code)
            || !valid_name(&self.name)
            || self.version < 1
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarketingAttributionRow {
    pub campaign: String,
    pub source: String,
    pub captured: i64,
    pub pending: i64,
    pub handed_off: i64,
    pub rejected: i64,
}

impl MarketingAttributionRow {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_attribution_key(&self.campaign)
            || !valid_attribution_key(&self.source)
            || self.captured < 1
            || self.pending < 0
            || self.handed_off < 0
            || self.rejected < 0
            || self
                .pending
                .checked_add(self.handed_off)
                .and_then(|value| value.checked_add(self.rejected))
                != Some(self.captured)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarketingCampaignPurchaseControls {
    #[serde(rename = "newWork")]
    pub new_work: bool,
    #[serde(rename = "settleExisting")]
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct MarketingCampaignEntitlement {
    pub enforced: bool,
    pub history_allowed: bool,
    pub new_work: bool,
    pub settle_existing: bool,
    pub revision: String,
}

impl MarketingCampaignEntitlement {
    pub fn require_history(&self) -> Result<(), BusinessError> {
        if self.enforced && !self.history_allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "The Marketing addon or retained history is required.",
            ));
        }
        Ok(())
    }

    pub fn purchase(&self) -> Option<MarketingCampaignPurchaseControls> {
        self.enforced.then(|| MarketingCampaignPurchaseControls {
            new_work: self.new_work,
            settle_existing: self.settle_existing,
            history: self.history_allowed,
            revision: self.revision.clone(),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MarketingCampaignReport {
    pub campaigns: Vec<MarketingCampaignRow>,
    pub attribution: Vec<MarketingAttributionRow>,
    pub purchase: Option<MarketingCampaignPurchaseControls>,
}

pub(crate) fn validate_report(
    actor: &MarketingCampaignActor,
    campaigns: Vec<MarketingCampaignRow>,
    attribution: Vec<MarketingAttributionRow>,
) -> Result<MarketingCampaignReport, BusinessError> {
    if campaigns.len() > CAMPAIGN_LIMIT || attribution.len() > ATTRIBUTION_LIMIT {
        return Err(too_large());
    }
    let mut previous_campaign: Option<(String, String)> = None;
    let mut campaign_ids = std::collections::BTreeSet::new();
    for row in &campaigns {
        row.validate(actor)?;
        if !campaign_ids.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        let current = (row.code.clone(), row.id.clone());
        if previous_campaign
            .as_ref()
            .is_some_and(|previous| current <= *previous)
        {
            return Err(source_invalid());
        }
        previous_campaign = Some(current);
    }
    let mut previous_attribution: Option<(i64, String, String)> = None;
    let mut attribution_keys = std::collections::BTreeSet::new();
    for row in &attribution {
        row.validate()?;
        if !attribution_keys.insert((row.campaign.clone(), row.source.clone())) {
            return Err(source_invalid());
        }
        let current = (row.captured, row.campaign.clone(), row.source.clone());
        if previous_attribution.as_ref().is_some_and(|previous| {
            previous.0 < current.0
                || (previous.0 == current.0
                    && (previous.1 > current.1
                        || (previous.1 == current.1 && previous.2 >= current.2)))
        }) {
            return Err(source_invalid());
        }
        previous_attribution = Some(current);
    }
    Ok(MarketingCampaignReport {
        campaigns,
        attribution,
        purchase: None,
    })
}
