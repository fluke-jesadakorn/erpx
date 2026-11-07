//! Additive source-admin wire intents. These fields cannot select an actor,
//! role, Company or database; current authority comes from the retained hook.
use crate::directory_source_administration::{DirectorySourceDecision, DirectorySourcePreview};
use erpx_server_identity_access::public::directory::{
    DirectorySourceChangeRequest, DirectorySourceReviewResult, normalize_directory_source_reason,
    provenance::{Denial, SourceRevision},
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, RecordId, json_numbers::deserialize_safe_i64,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[cfg_attr(feature = "contracts", ts(type = "number"))]
#[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9007199254740991_i64))]
pub struct DirectorySourceInteger(i64);
impl DirectorySourceInteger {
    pub fn new(value: i64) -> Result<Self, BusinessError> {
        if !(1..=9_007_199_254_740_991).contains(&value) {
            return Err(invalid());
        }
        Ok(Self(value))
    }
    pub fn get(self) -> i64 {
        self.0
    }
}
impl<'de> Deserialize<'de> for DirectorySourceInteger {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(deserialize_safe_i64(deserializer)?).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourcePreviewInput {
    #[cfg_attr(
        feature = "contracts",
        schema(min_length = 1, max_length = 80, pattern = "^[A-Za-z0-9_-]+$")
    )]
    pub key_id: String,
    pub expected_connection_version: DirectorySourceInteger,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature="contracts",ts(optional=nullable))]
    pub expected_source_revision: Option<DirectorySourceInteger>,
    /// Trimmed ECMAScript text, 3..1000 Unicode code points at runtime.
    #[cfg_attr(feature = "contracts", schema(min_length = 3, max_length = 1000))]
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourceApproveInput {
    pub expected_version: DirectorySourceInteger,
    /// Independent review reason, normalized before receipt/audit identity.
    #[cfg_attr(feature = "contracts", schema(min_length = 3, max_length = 1000))]
    pub reason: String,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum DirectorySourceStatus {
    Pending,
    Approved,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourceReviewData {
    pub id: String,
    pub status: DirectorySourceStatus,
    pub version: DirectorySourceInteger,
    pub source_revision: DirectorySourceInteger,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub decision_reason: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourceResponse {
    pub data: DirectorySourceReviewData,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourceError {
    pub code: String,
    pub message: String,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DirectorySourceErrorResponse {
    pub error: DirectorySourceError,
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_SOURCE_BODY_INVALID",
        "Use the bounded Directory source request.",
    )
}
pub fn parse_preview(
    bytes: &[u8],
    connection: &str,
    key: &str,
) -> Result<DirectorySourcePreview, BusinessError> {
    // Request.json() keeps the last duplicate property. Unknown properties are
    // rejected by this new additive contract; actor/grant fields are never read.
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let input: DirectorySourcePreviewInput =
        serde_json::from_value(value).map_err(|_| invalid())?;
    Ok(DirectorySourcePreview {
        request: DirectorySourceChangeRequest::new(
            RecordId::new(connection)?,
            input.key_id,
            input.expected_connection_version.get(),
            input
                .expected_source_revision
                .map(|r| SourceRevision::checked(r.get()))
                .transpose()
                .map_err(Denial::business)?,
        )?,
        key: CommandKey::new(key)?,
        reason: normalize_directory_source_reason(&input.reason)?,
    })
}
pub fn parse_approve(
    bytes: &[u8],
    connection: &str,
    review: &str,
    key: &str,
) -> Result<DirectorySourceDecision, BusinessError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let input: DirectorySourceApproveInput =
        serde_json::from_value(value).map_err(|_| invalid())?;
    Ok(DirectorySourceDecision {
        connection_id: RecordId::new(connection)?,
        review_id: RecordId::new(review)?,
        expected_version: input.expected_version.get(),
        key: CommandKey::new(key)?,
        reason: normalize_directory_source_reason(&input.reason)?,
    })
}
impl TryFrom<DirectorySourceReviewResult> for DirectorySourceResponse {
    type Error = BusinessError;
    fn try_from(result: DirectorySourceReviewResult) -> Result<Self, Self::Error> {
        RecordId::new(&result.id)?;
        let status = match result.status.as_str() {
            "pending" if result.decision_reason.is_none() => DirectorySourceStatus::Pending,
            "approved"
                if result.decision_reason.as_ref().is_some_and(|s| {
                    normalize_directory_source_reason(s).is_ok_and(|v| v == *s)
                }) =>
            {
                DirectorySourceStatus::Approved
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            data: DirectorySourceReviewData {
                id: result.id,
                status,
                version: DirectorySourceInteger::new(result.version)?,
                source_revision: DirectorySourceInteger::new(result.source_revision)?,
                decision_reason: result.decision_reason,
            },
        })
    }
}
