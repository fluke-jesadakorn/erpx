//! Reviewed source data is not a session, selected grant or approval capability.
use super::directory_provenance::SelectedSource;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const DIRECTORY_SOURCE_REVIEW_MS: i64 = 300_000;
pub const DIRECTORY_SOURCE_REVIEW_KIND: &str = "directory.source-select";
const MAX_SAFE: i64 = 9_007_199_254_740_991;

/// Identity-private witness attached only after actual same-Control MFA and
/// normal IAM. External data cannot construct or deserialize this proof.
#[derive(Clone, Debug)]
pub(crate) struct DirectoryAdministrationProof {
    pub holding_id: String,
    pub company_id: String,
    pub connection_id: String,
    pub user_id: String,
    pub session_id: String,
    pub deadline: i64,
    pub unit: (String, i32, String, String),
}

pub fn review_error(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "Directory source approval changed. Review the current proposal.",
    )
}

pub fn normalize_directory_source_reason(reason: &str) -> Result<String, BusinessError> {
    let reason = reason.trim_matches(super::directory_provider_values::js_space);
    if !(3..=1000).contains(&reason.chars().count()) {
        return Err(BusinessError::invalid(
            "DIRECTORY_SOURCE_REASON_REQUIRED",
            "Use a review reason from 3 through 1000 Unicode code points.",
        ));
    }
    Ok(reason.into())
}

/// Exact nonplaintext proposal material. Decoding stored data never authorizes it.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectorySourceBinding {
    pub(crate) holding_id: String,
    pub(crate) company_id: String,
    pub(crate) connection_id: String,
    pub(crate) expected_revision: Option<i64>,
    pub(crate) revision: i64,
    pub(crate) key_id: String,
    pub(crate) profile_tag: String,
    pub(crate) connection_version: i64,
    pub(crate) config_fingerprint: String,
}
impl DirectorySourceBinding {
    pub(crate) fn new(
        holding: &str,
        company: &str,
        connection: &str,
        previous: Option<i64>,
        source: &SelectedSource,
    ) -> Result<Self, BusinessError> {
        if source.revision.get() > MAX_SAFE || source.connection_version > MAX_SAFE {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"));
        }
        Ok(Self {
            holding_id: holding.into(),
            company_id: company.into(),
            connection_id: connection.into(),
            expected_revision: previous,
            revision: source.revision.get(),
            key_id: source.key_id.clone(),
            profile_tag: hex::encode(source.profile_tag.database_bytes()),
            connection_version: source.connection_version,
            config_fingerprint: hex::encode(source.config_fingerprint),
        })
    }
    pub fn revision(&self) -> i64 {
        self.revision
    }
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProposerPins {
    pub user_id: String,
    pub person_id: Option<String>,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub role: String,
    pub role_revision: i64,
    pub identity_version: i64,
    pub session_id: String,
    pub selection_revision: i64,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub expires_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredSourceProposal {
    pub format_version: u8,
    pub binding: DirectorySourceBinding,
    pub proposer: ProposerPins,
    pub reason: String,
    pub created_at: i64,
    pub expires_at: i64,
}
impl StoredSourceProposal {
    pub(crate) fn require_live(&self, now: i64) -> Result<(), BusinessError> {
        if self.format_version != 1
            || self.created_at > now
            || self.expires_at <= now
            || self.expires_at > self.created_at.saturating_add(DIRECTORY_SOURCE_REVIEW_MS)
            || self.reason.trim().is_empty()
            || self.reason.len() > 4096
        {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_EXPIRED"));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectorySourceReviewResult {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub source_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<String>,
}
