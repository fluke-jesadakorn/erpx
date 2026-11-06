//! Native Marketing lead-candidate, engagement, campaign and asset-profile
//! facts.
//!
//! Marketing owns `marketing_assets`, `marketing_lead_candidates`,
//! `marketing_engagement_events`, `marketing_asset_events` and
//! `marketing_campaigns`. This module validates the exact command/read shapes;
//! it never reads or writes Files, CRM, Commerce or Finance rows. CRM handoff
//! is supplied by a public port implemented at composition with the existing
//! CRM owner.
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, TimestampMillis};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CANDIDATE_LIMIT: usize = 200;
pub const ASSET_LIMIT: usize = 500;
pub const HISTORY_LIMIT: usize = 500;
pub const MAX_JSON: usize = 65_536;
const CODE_LIMIT: usize = 60;
const NAME_LIMIT: usize = 160;
const TEXT_LIMIT: usize = 3_000;
const DETAIL_LIMIT: usize = 3_000;
const EVENT_LIMIT: usize = 80;
const ATTRIBUTION_KEYS: usize = 50;
const ATTRIBUTION_VALUE: usize = 300;

#[derive(Clone, Debug)]
pub struct MarketingLeadActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub role: Option<String>,
    pub authority_revision: Option<i64>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl MarketingLeadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "marketing")
    }

    fn require(
        &self,
        permission: &'static str,
        code: &'static str,
        message: &'static str,
        now: TimestampMillis,
    ) -> Result<(), BusinessError> {
        if !self.has(permission) {
            return Err(BusinessError::new(FailureKind::Forbidden, code, message));
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

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require(
            "records.read",
            "MARKETING_CAMPAIGN_PERMISSION_REQUIRED",
            "Your selected role does not grant Marketing read access.",
            now,
        )
    }

    pub fn require_write(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require(
            "records.write",
            "MARKETING_RECORD_WRITE_REQUIRED",
            "Your selected role does not grant Marketing write access.",
            now,
        )
    }

    pub fn branch_scope(&self) -> Result<&str, BusinessError> {
        self.branch_id
            .as_ref()
            .map(|value| value.as_str())
            .ok_or_else(branch_scope_required)
    }

    pub fn department_scope(&self) -> Option<&str> {
        self.department_id.as_ref().map(|value| value.as_str())
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "userId": self.user_id.as_str(),
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": self.department_id.as_ref().map(|value| value.as_str()),
            "assignmentId": self.assignment_id.as_str(),
            "role": self.role,
            "revision": self.authority_revision,
            "assignmentVersion": self.assignment_version,
            "modules": self.modules,
            "permissions": self.permissions,
        })
        .to_string()
    }
}

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

pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(is_edge_whitespace)
}

fn utf16(value: &str) -> usize {
    value.encode_utf16().count()
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

fn valid_text(value: &str, min: usize, max: usize) -> bool {
    let count = utf16(js_trim(value));
    (min..=max).contains(&count) && !value.contains('\0')
}

fn valid_optional_text(value: &Option<String>, min: usize, max: usize) -> bool {
    value
        .as_deref()
        .is_none_or(|value| valid_text(value, min, max))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    if !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn truncate_utf16(value: &str, limit: usize) -> String {
    let mut used = 0;
    value
        .chars()
        .take_while(|character| {
            used += character.len_utf16();
            used <= limit
        })
        .collect()
}

fn valid_attribution(attribution: &BTreeMap<String, String>) -> bool {
    attribution.len() <= ATTRIBUTION_KEYS
        && attribution.iter().all(|(key, value)| {
            !key.is_empty()
                && key.len() <= 200
                && utf16(value) <= ATTRIBUTION_VALUE
                && !key.chars().any(char::is_control)
                && !value.chars().any(char::is_control)
        })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketingCampaignCreateInput {
    pub code: String,
    pub name: String,
}

impl MarketingCampaignCreateInput {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let code = js_trim(&self.code);
        if code.len() < 2
            || code.len() > CODE_LIMIT
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(invalid(
                "MARKETING_CAMPAIGN_INPUT_INVALID",
                "Use a 2–60 character campaign code with letters, digits, dashes or underscores.",
            ));
        }
        if !valid_text(&self.name, 2, NAME_LIMIT) {
            return Err(invalid(
                "MARKETING_CAMPAIGN_INPUT_INVALID",
                "Use a bounded campaign name.",
            ));
        }
        Ok(())
    }

    pub fn code(&self) -> String {
        js_trim(&self.code).to_owned()
    }

    pub fn name(&self) -> String {
        js_trim(&self.name).to_owned()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketingAssetCreateInput {
    pub kind: String,
    pub name: String,
    pub configuration_json: String,
    pub preview_json: String,
}

impl MarketingAssetCreateInput {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !matches!(
            self.kind.as_str(),
            "form" | "content" | "domain" | "provider"
        ) || !valid_text(&self.name, 2, NAME_LIMIT)
            || self.configuration_json.len() > MAX_JSON
            || self.preview_json.len() > MAX_JSON
        {
            return Err(invalid(
                "MARKETING_ASSET_INPUT_INVALID",
                "Review the Marketing asset profile.",
            ));
        }
        let configuration: serde_json::Value = serde_json::from_str(&self.configuration_json)
            .map_err(|_| {
                invalid(
                    "MARKETING_PRESENTATION_INVALID",
                    "Review the saved asset presentation.",
                )
            })?;
        let preview: serde_json::Value =
            serde_json::from_str(&self.preview_json).map_err(|_| {
                invalid(
                    "MARKETING_PRESENTATION_INVALID",
                    "Review the saved asset preview.",
                )
            })?;
        if !configuration.is_object() || !preview.is_object() {
            return Err(invalid(
                "MARKETING_PRESENTATION_INVALID",
                "Review the saved asset presentation.",
            ));
        }
        if self.kind == "form"
            && configuration
                .get("consentRequired")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
        {
            return Err(invalid(
                "MARKETING_CONSENT_REQUIRED",
                "Lead capture forms must require consent before they can be saved.",
            ));
        }
        Ok(())
    }

    pub fn name(&self) -> String {
        js_trim(&self.name).to_owned()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketingLeadCandidateInput {
    pub form_id: String,
    pub form_version: i64,
    pub branch_id: String,
    pub contact_name: String,
    pub contact_detail: String,
    pub consent_evidence: String,
    pub attribution: BTreeMap<String, String>,
    pub brief: String,
}

impl MarketingLeadCandidateInput {
    pub fn validate(&self, actor: &MarketingLeadActor) -> Result<(), BusinessError> {
        if !valid_id(&self.form_id)
            || !valid_id(&self.branch_id)
            || !(1..=9_007_199_254_740_991).contains(&self.form_version)
            || !valid_text(&self.contact_name, 2, NAME_LIMIT)
            || !valid_text(&self.contact_detail, 3, DETAIL_LIMIT)
            || !valid_text(&self.consent_evidence, 3, TEXT_LIMIT)
            || !valid_text(&self.brief, 3, TEXT_LIMIT)
            || !valid_attribution(&self.attribution)
        {
            return Err(invalid(
                "MARKETING_CANDIDATE_INPUT_INVALID",
                "Review the captured candidate details.",
            ));
        }
        if self.branch_id != actor.branch_scope()? {
            return Err(invalid(
                "MARKETING_BRANCH_SCOPE_INVALID",
                "Create the candidate in the selected branch workspace.",
            ));
        }
        Ok(())
    }

    pub fn attribution_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(&self.attribution).map_err(|_| {
            invalid(
                "MARKETING_ATTRIBUTION_INVALID",
                "Review the attribution data.",
            )
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketingLeadActionInput {
    pub action: String,
    pub expected_version: i64,
    pub reason: String,
    pub event_type: Option<String>,
    pub channel: Option<String>,
    pub due_date: Option<String>,
}

impl MarketingLeadActionInput {
    pub fn validate(&self, now: i64) -> Result<MarketingLeadAction, BusinessError> {
        if !(1..=9_007_199_254_740_991).contains(&self.expected_version)
            || !valid_text(&self.reason, 3, TEXT_LIMIT)
        {
            return Err(invalid(
                "MARKETING_CANDIDATE_ACTION_INVALID",
                "Review the candidate action.",
            ));
        }
        match self.action.as_str() {
            "record-engagement" => {
                if !valid_optional_text(&self.event_type, 2, EVENT_LIMIT)
                    || self.event_type.is_none()
                    || !valid_optional_text(&self.channel, 2, EVENT_LIMIT)
                    || self.channel.is_none()
                {
                    return Err(invalid(
                        "ENGAGEMENT_DETAIL_REQUIRED",
                        "Event type and channel are required.",
                    ));
                }
                Ok(MarketingLeadAction::RecordEngagement)
            }
            "handoff" => {
                if let Some(due_date) = self.due_date.as_deref()
                    && !valid_date(due_date)
                {
                    return Err(invalid(
                        "MARKETING_HANDOFF_DUE_DATE_INVALID",
                        "Use a valid CRM follow-up date.",
                    ));
                }
                let _ = now;
                Ok(MarketingLeadAction::Handoff)
            }
            _ => Err(invalid(
                "INVALID_MARKETING_TRANSITION",
                "Choose a supported candidate action.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketingLeadAction {
    RecordEngagement,
    Handoff,
}

impl MarketingLeadAction {
    pub fn operation(self) -> &'static str {
        match self {
            Self::RecordEngagement => "marketing.candidate.engagement",
            Self::Handoff => "marketing.candidate.handoff",
        }
    }
}

/// Owner row read for `marketing_assets`. Configuration and preview are
/// Marketing-owned presentation payloads; Files/Commerce publication readiness
/// is never inferred here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarketingAssetProfile {
    pub id: String,
    pub company_id: String,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub version: i64,
    pub configuration_json: String,
    pub preview_json: String,
}

impl MarketingAssetProfile {
    pub fn validate(&self, actor: &MarketingLeadActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id
            || !valid_id(&self.id)
            || !matches!(
                self.kind.as_str(),
                "form" | "content" | "domain" | "provider"
            )
            || !valid_text(&self.name, 2, NAME_LIMIT)
            || !matches!(
                self.status.as_str(),
                "draft" | "active" | "rejected" | "retired"
            )
            || self.version < 1
            || self.configuration_json.len() > MAX_JSON
            || self.preview_json.len() > MAX_JSON
            || serde_json::from_str::<serde_json::Value>(&self.configuration_json)
                .ok()
                .is_none_or(|value| !value.is_object())
            || serde_json::from_str::<serde_json::Value>(&self.preview_json)
                .ok()
                .is_none_or(|value| !value.is_object())
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MarketingLeadCandidateRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub form_id: String,
    pub form_version: i64,
    pub form_name: String,
    pub contact_name: String,
    pub contact_detail: String,
    pub consent_evidence: String,
    pub attribution_json: String,
    pub brief: String,
    pub status: String,
    pub crm_lead_id: Option<String>,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl MarketingLeadCandidateRow {
    pub fn validate(&self, actor: &MarketingLeadActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id
            || actor
                .branch_id
                .as_ref()
                .is_some_and(|value| value.as_str() != self.branch_id)
            || actor
                .department_scope()
                .is_some_and(|value| self.department_id.as_deref() != Some(value))
            || !valid_id(&self.id)
            || !valid_id(&self.form_id)
            || !valid_id(&self.branch_id)
            || !valid_text(&self.form_name, 2, NAME_LIMIT)
            || !valid_text(&self.contact_name, 2, NAME_LIMIT)
            || !valid_text(&self.contact_detail, 3, DETAIL_LIMIT)
            || !valid_text(&self.consent_evidence, 3, TEXT_LIMIT)
            || !valid_text(&self.brief, 3, TEXT_LIMIT)
            || self.form_version < 1
            || self.version < 1
            || self.created_at < 0
            || self.updated_at < self.created_at
            || !matches!(self.status.as_str(), "pending" | "handed_off" | "rejected")
            || self
                .crm_lead_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_id(value))
            || serde_json::from_str::<BTreeMap<String, String>>(&self.attribution_json)
                .ok()
                .is_none_or(|value| !valid_attribution(&value))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MarketingEngagementRow {
    pub id: String,
    pub candidate_id: String,
    pub version: i64,
    pub event_type: String,
    pub channel: String,
    pub evidence: String,
    pub actor_id: String,
    pub created_at: i64,
}

impl MarketingEngagementRow {
    pub fn validate(&self, candidate_id: &str) -> Result<(), BusinessError> {
        if self.candidate_id != candidate_id
            || !valid_id(&self.id)
            || self.version < 1
            || !valid_text(&self.event_type, 2, EVENT_LIMIT)
            || !valid_text(&self.channel, 2, EVENT_LIMIT)
            || !valid_text(&self.evidence, 3, TEXT_LIMIT)
            || !valid_id(&self.actor_id)
            || self.created_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MarketingFormAssetFacts {
    pub configuration_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketingCandidateReadback {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub crm_lead_id: Option<String>,
    pub created_by: String,
    pub form_id: String,
    pub form_version: i64,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub contact_name: String,
    pub contact_detail: String,
    pub consent_evidence: String,
    pub attribution_json: String,
    pub brief: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketingCampaignReadback {
    pub id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MarketingLeadDetail {
    pub candidate: MarketingLeadCandidateRow,
    pub history: Vec<MarketingEngagementRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MarketingAssetProfiles {
    pub assets: Vec<MarketingAssetProfile>,
}

/// Exact request passed to the CRM public handoff port. It mirrors the CRM
/// `LeadCreateInput` shape; Marketing never writes CRM rows itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketingCrmLeadRequest {
    pub branch_id: String,
    pub title: String,
    pub contact_name: String,
    pub contact_detail: String,
    pub source: String,
    pub source_evidence: String,
    pub source_kind: String,
    pub source_id: String,
    pub campaign_id: Option<String>,
    pub brief: String,
    pub due_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketingCrmLeadResult {
    pub lead_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketingCommandReceipt {
    pub id: String,
    #[serde(default)]
    pub status: String,
    pub version: i64,
    #[serde(default)]
    pub crm_lead_id: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

impl MarketingCommandReceipt {
    pub fn to_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(self).map_err(|_| source_invalid())
    }

    pub fn from_json(value: &str) -> Result<Self, BusinessError> {
        let receipt: Self = serde_json::from_str(value).map_err(|_| source_invalid())?;
        if !valid_id(&receipt.id) || receipt.version < 1 {
            return Err(source_invalid());
        }
        Ok(receipt)
    }
}

pub fn crm_request(
    candidate: &MarketingLeadCandidateRow,
    due_date: Option<&str>,
    now: i64,
) -> Result<MarketingCrmLeadRequest, BusinessError> {
    let attribution: serde_json::Value = serde_json::from_str(&candidate.attribution_json)
        .map_err(|_| {
            invalid(
                "MARKETING_ATTRIBUTION_INVALID",
                "Review the attribution data.",
            )
        })?;
    let campaign_id = attribution
        .get("campaignId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| valid_id(value))
        .map(str::to_owned);
    let source_label = attribution
        .get("source")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            attribution
                .get("utmSource")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("Unspecified");
    let due_date = due_date.map(str::to_owned).unwrap_or_else(|| {
        let days = 7_i64;
        let millis = now.saturating_add(days * 86_400_000);
        date_from_millis(millis)
    });
    if utf16(&candidate.contact_detail) > 2_000 || utf16(&candidate.brief) > 2_000 {
        return Err(invalid(
            "MARKETING_HANDOFF_SOURCE_INVALID",
            "Shorten the candidate contact detail or brief before CRM handoff.",
        ));
    }
    let source = format!(
        "Marketing form {}; campaign {}; source {}",
        candidate.form_id,
        campaign_id.as_deref().unwrap_or("Unspecified"),
        source_label
    );
    let title = truncate_utf16(&format!("Marketing lead · {}", candidate.contact_name), 160);
    Ok(MarketingCrmLeadRequest {
        branch_id: candidate.branch_id.clone(),
        title,
        contact_name: candidate.contact_name.clone(),
        contact_detail: candidate.contact_detail.clone(),
        source: truncate_utf16(&source, 2_000),
        source_evidence: format!(
            "Reviewed candidate {} from form {} version {}",
            candidate.id, candidate.form_id, candidate.form_version
        ),
        source_kind: "form".into(),
        source_id: candidate.id.clone(),
        campaign_id,
        brief: candidate.brief.clone(),
        due_date,
    })
}

/// Gregorian date for a UTC millisecond timestamp, matching the retained
/// `toISOString().slice(0,10)` follow-up default without adding a date crate.
fn date_from_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn form_configuration_allows_capture(configuration_json: &str) -> Result<bool, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(configuration_json).map_err(|_| {
        invalid(
            "MARKETING_PRESENTATION_INVALID",
            "Review the saved form presentation.",
        )
    })?;
    Ok(value
        .get("consentRequired")
        .and_then(serde_json::Value::as_bool)
        == Some(true))
}

/// Forms that reference approved content or a public creative require the
/// Files-owned readiness port. This owner read fails closed instead of
/// inferring publication/creative readiness from a client label.
pub fn form_requires_files_readiness(configuration_json: &str) -> Result<bool, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(configuration_json).map_err(|_| {
        invalid(
            "MARKETING_PRESENTATION_INVALID",
            "Review the saved form presentation.",
        )
    })?;
    Ok(value
        .get("contentAssetId")
        .is_some_and(|value| !value.is_null())
        || value
            .get("mediaPublicationId")
            .is_some_and(|value| !value.is_null()))
}

pub fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

pub fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::NotFound, code, message)
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MARKETING_PERMISSION_REQUIRED",
        "Your selected role does not grant the current Marketing write authority.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_SOURCE_INVALID",
        "The current Marketing owner source is invalid.",
    )
}

pub fn branch_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MARKETING_BRANCH_SCOPE_REQUIRED",
        "Select a branch workspace before creating or reviewing lead candidates.",
    )
}

pub fn candidate_not_found() -> BusinessError {
    not_found(
        "MARKETING_CANDIDATE_NOT_FOUND",
        "Lead candidate not found in this workspace.",
    )
}

pub fn candidate_changed() -> BusinessError {
    conflict(
        "MARKETING_CANDIDATE_CHANGED",
        "The lead candidate changed. Reload and retry.",
    )
}

pub fn campaign_code_taken() -> BusinessError {
    conflict(
        "MARKETING_CAMPAIGN_CODE_TAKEN",
        "This campaign code is already used in the Company.",
    )
}

pub fn form_not_ready() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MARKETING_FORM_NOT_READY",
        "The approved form presentation is currently unavailable.",
    )
}

pub fn form_readiness_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_FORM_READINESS_PORT_REQUIRED",
        "The Files-owned form/creative readiness port is not mounted for this form.",
    )
}

pub fn independent_review_required() -> BusinessError {
    conflict(
        "MARKETING_INDEPENDENT_REVIEW_REQUIRED",
        "A different reviewer must hand this candidate to CRM.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn request_invalid() -> BusinessError {
    invalid(
        "MARKETING_REQUEST_INVALID",
        "Use one bounded current Marketing request.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_READBACK_INVALID",
        "The recorded Marketing change could not be read back.",
    )
}
