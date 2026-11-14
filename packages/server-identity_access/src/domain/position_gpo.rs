//! Portable Position GPO mapping policy and delivery facts.
//!
//! A published mapping is one immutable revision plus the mutable current
//! `position_gpo_policies` row; delivery to the Directory is a separate queued
//! job. Nothing here reads a database, HTTP request, provider or clock; callers
//! supply current authority, Directory evidence and the serializable unit.
use erpx_server_shared::public::BusinessError;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;

pub const MAX_ID: usize = 180;
pub const MAX_REASON: usize = 1_000;
pub const DIRECTORY_APPLY_PERMISSION: &str = "iam.directory.apply";
pub const IT_ROLE: &str = "it_admin";
pub const HOLDING_SCOPE: &str = "holding";

pub const PUBLISH_OPERATION: &str = "technology.position-gpo.publish";
pub const UNMAP_OPERATION: &str = "technology.position-gpo.unmap";
pub const JOB_RETRY_OPERATION: &str = "technology.position-gpo.retry";
pub const CONFIG_RETRY_OPERATION: &str = "technology.position-gpo.config-retry";
pub const INTERRUPTED_AFTER_MS: i64 = 900_000;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        code,
        message,
    )
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        code,
        message,
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "GPO_REQUEST_INVALID",
        "Review the Position Directory mapping and try again.",
    )
}
pub fn it_authority_required() -> BusinessError {
    forbidden(
        "IT_AUTHORITY_REQUIRED",
        "Choose an active Company IT assignment to publish GPO mappings.",
    )
}
pub fn fresh_mfa_required() -> BusinessError {
    forbidden(
        "FRESH_MFA_REQUIRED",
        "Verify MFA before changing Directory access.",
    )
}
pub fn position_not_found() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::NotFound,
        "POSITION_NOT_FOUND",
        "Active position not found.",
    )
}
pub fn version_conflict() -> BusinessError {
    conflict(
        "VERSION_CONFLICT",
        "The Position GPO changed. Reload before publishing.",
    )
}
pub fn preview_changed() -> BusinessError {
    conflict(
        "GPO_PREVIEW_CHANGED",
        "Review the current Position mapping impact again.",
    )
}
pub fn mapping_changed() -> BusinessError {
    conflict(
        "GPO_MAPPING_CHANGED",
        "Reload the current Position mapping before removing it.",
    )
}
pub fn job_changed() -> BusinessError {
    conflict(
        "GPO_JOB_CHANGED",
        "Only a blocked current job can be retried.",
    )
}
pub fn config_changed() -> BusinessError {
    conflict(
        "GPO_CONFIG_CHANGED",
        "Only a blocked current configuration can be retried.",
    )
}
pub fn connection_required() -> BusinessError {
    conflict(
        "DIRECTORY_CONNECTION_REQUIRED",
        "Choose an active Directory connection.",
    )
}
pub fn write_not_ready() -> BusinessError {
    conflict(
        "DIRECTORY_WRITE_NOT_READY",
        "Test and enable Directory writes before GPO delivery.",
    )
}
pub fn target_invalid() -> BusinessError {
    BusinessError::invalid(
        "DIRECTORY_TARGET_INVALID",
        "Choose a current Directory object from this connection.",
    )
}
pub fn readback_required() -> BusinessError {
    conflict(
        "DIRECTORY_READBACK_REQUIRED",
        "The Directory did not confirm the membership.",
    )
}
pub fn link_disabled() -> BusinessError {
    conflict(
        "GPO_LINK_DISABLED",
        "Enable the existing GPO link in the Directory before using this mapping.",
    )
}
pub fn unlink_readback_required() -> BusinessError {
    conflict(
        "GPO_UNLINK_READBACK_REQUIRED",
        "The previous GPO OU link remains in the Directory.",
    )
}
pub fn config_blocked() -> BusinessError {
    conflict(
        "GPO_CONFIG_BLOCKED",
        "The GPO Directory configuration needs IT attention.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different Position GPO command.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "GPO_READBACK_INVALID",
        "The Position GPO command was not persisted as submitted.",
    )
}
pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "GPO_SOURCE_INVALID",
        "The Position GPO source is unavailable.",
    )
}
fn parse<T: DeserializeOwned>(raw: &Value) -> Result<T, BusinessError> {
    serde_json::from_value(raw.clone()).map_err(|_| request_invalid())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID
}

fn valid_reason(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() < 3 || trimmed.len() > MAX_REASON {
        return None;
    }
    Some(trimmed.to_owned())
}

fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionGpoInput {
    pub position_id: String,
    pub expected_version: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub user_filter_group_id: String,
    pub computer_target_ou_id: String,
    pub reason: String,
}

impl PositionGpoInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let input: Self = parse(raw)?;
        input.validate()
    }

    fn validate(mut self) -> Result<Self, BusinessError> {
        if !valid_id(&self.position_id)
            || !valid_id(&self.connection_id)
            || !valid_id(&self.gpo_id)
            || !valid_id(&self.user_filter_group_id)
            || !valid_id(&self.computer_target_ou_id)
            || self.expected_version < 0
        {
            return Err(request_invalid());
        }
        self.reason = valid_reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(self)
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "positionId": self.position_id,
            "expectedVersion": self.expected_version,
            "connectionId": self.connection_id,
            "gpoId": self.gpo_id,
            "userFilterGroupId": self.user_filter_group_id,
            "computerTargetOuId": self.computer_target_ou_id,
            "reason": self.reason,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishInput {
    #[serde(flatten)]
    pub input: PositionGpoInput,
    pub preview_token: String,
}

impl PublishInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let parsed: Self = parse(raw)?;
        if !valid_token(&parsed.preview_token) {
            return Err(request_invalid());
        }
        let input = parsed.input.validate()?;
        Ok(Self {
            input,
            preview_token: parsed.preview_token,
        })
    }

    pub fn request_json(&self) -> Value {
        let mut value = self.input.to_json();
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "previewToken".into(),
                Value::String(self.preview_token.clone()),
            );
        }
        value
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmapInput {
    pub position_id: String,
    pub expected_version: i64,
    pub reason: String,
}

impl UnmapInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let input: Self = parse(raw)?;
        input.validate()
    }

    fn validate(mut self) -> Result<Self, BusinessError> {
        if !valid_id(&self.position_id) || self.expected_version < 1 {
            return Err(request_invalid());
        }
        self.reason = valid_reason(&self.reason).ok_or_else(request_invalid)?;
        Ok(self)
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "positionId": self.position_id,
            "expectedVersion": self.expected_version,
            "reason": self.reason,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmapPublishInput {
    #[serde(flatten)]
    pub input: UnmapInput,
    pub preview_token: String,
}

impl UnmapPublishInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let parsed: Self = parse(raw)?;
        if !valid_token(&parsed.preview_token) {
            return Err(request_invalid());
        }
        let input = parsed.input.validate()?;
        Ok(Self {
            input,
            preview_token: parsed.preview_token,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRetryInput {
    pub id: String,
    pub expected_version: i64,
    pub reason: String,
}

impl JobRetryInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let mut input: Self = parse(raw)?;
        if !valid_id(&input.id) || input.expected_version < 1 {
            return Err(request_invalid());
        }
        input.reason = valid_reason(&input.reason).ok_or_else(request_invalid)?;
        Ok(input)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRetryInput {
    pub position_id: String,
    pub expected_version: i64,
    pub reason: String,
}

impl ConfigRetryInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let mut input: Self = parse(raw)?;
        if !valid_id(&input.position_id) || input.expected_version < 1 {
            return Err(request_invalid());
        }
        input.reason = valid_reason(&input.reason).ok_or_else(request_invalid)?;
        Ok(input)
    }
}

/// Current authority facts for one IT command. `it_holding` is read in the
/// Holding transaction; `fresh_mfa` is a Control-store fact verified by the
/// ingress for the same session and never read from the Holding database.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpoAuthority {
    pub it_holding: bool,
}

/// A fresh-MFA verification fact for the acting session.
#[derive(Clone, Copy, Debug)]
pub struct FreshMfa(pub bool);

impl GpoAuthority {
    pub fn require_write(self, fresh_mfa: FreshMfa) -> Result<(), BusinessError> {
        if !self.it_holding {
            return Err(it_authority_required());
        }
        if !fresh_mfa.0 {
            return Err(fresh_mfa_required());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyRow {
    pub version: i64,
    pub revision: i64,
    pub user_filter_group_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirectoryState {
    pub version: i64,
    pub tested_version: Option<i64>,
    pub write_enabled: bool,
}

impl DirectoryState {
    pub fn write_ready(&self) -> bool {
        self.write_enabled && self.tested_version == Some(self.version)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionRow {
    pub id: String,
    pub provider: String,
    pub config: Value,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionFacts {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HolderRow {
    pub engagement_id: String,
    pub person_id: String,
    pub version: i64,
    pub name: String,
    pub account_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingRow {
    pub version: i64,
    pub revision: i64,
    pub title: String,
    pub connection_id: Option<String>,
    pub gpo_id: Option<String>,
    pub user_filter_group_id: Option<String>,
    pub computer_target_ou_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobRow {
    pub id: String,
    pub company_id: String,
    pub position_id: String,
    pub person_id: String,
    pub engagement_id: Option<String>,
    pub account_id: Option<String>,
    pub revision: i64,
    pub engagement_version: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub filter_group_id: String,
    pub computer_target_ou_id: String,
    pub desired_action: String,
    pub status: String,
    pub version: i64,
    pub rule_id: Option<String>,
    pub source_id: Option<String>,
}

impl JobRow {
    /// A delivery row without rule/account/engagement facts cannot be applied.
    pub fn deliverable(&self) -> bool {
        self.account_id.is_some()
            && self.engagement_id.is_some()
            && self.rule_id.as_deref().is_some_and(|v| !v.is_empty())
            && self.source_id.as_deref().is_some_and(|v| !v.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobStateRow {
    pub version: i64,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigRow {
    pub version: i64,
    pub revision: i64,
    pub directory_config_status: String,
}

/// Facts needed to queue one desired membership delivery.
#[derive(Clone, Debug, PartialEq)]
pub struct DesiredRow {
    pub engagement_id: String,
    pub company_id: String,
    pub position_id: String,
    pub person_id: String,
    pub engagement_version: i64,
    pub rule_id: String,
    pub legacy_position_id: Option<String>,
    pub revision: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub filter_group_id: String,
    pub computer_target_ou_id: String,
    pub account_id: Option<String>,
}

impl DesiredRow {
    pub fn source_id(&self) -> String {
        rule_source_id(
            &self.rule_id,
            &self.engagement_id,
            self.legacy_position_id.is_some(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClaimRow {
    pub connection_id: String,
    pub group_id: String,
    pub member_id: String,
    pub source_id: String,
    pub rule_id: String,
    pub company_id: String,
    pub position_id: String,
    pub person_id: String,
    pub engagement_id: String,
    pub engagement_version: i64,
    pub revision: i64,
    pub gpo_id: String,
    pub computer_target_ou_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnershipRow {
    pub connection_id: String,
    pub gpo_id: String,
    pub kind: String,
    pub target_id: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleStatusRow {
    pub directory_config_status: String,
}

/// The reviewed Directory evidence a publish token commits to.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewResult {
    pub value: Value,
    pub token: String,
    pub version: i64,
    pub revision: i64,
    pub person_ids: Vec<String>,
    pub queued: i64,
    pub awaiting_linkage: i64,
}

pub fn sha256_hex(value: &Value) -> String {
    let encoded = serde_json::to_vec(value).unwrap_or_default();
    let digest = Sha256::digest(encoded);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A reviewed preview token binds the normalized input and Directory evidence.
pub fn preview_token(input: &Value, result: &Value, directory: &Value) -> String {
    sha256_hex(&serde_json::json!({
        "input": input,
        "result": result,
        "directory": directory,
    }))
}

pub fn membership_key(
    connection_id: &str,
    group_id: &str,
    account_id: &str,
    source_id: &str,
) -> String {
    serde_json::json!([connection_id, group_id, account_id, source_id]).to_string()
}

pub fn waiting_key(
    company_id: &str,
    engagement_id: &str,
    position_id: &str,
    rule_id: &str,
    revision: i64,
) -> String {
    serde_json::json!([company_id, engagement_id, position_id, rule_id, revision]).to_string()
}

/// The legacy direct mapping and the newer rule path keep distinct sources.
pub fn rule_source_id(rule_id: &str, engagement_id: &str, legacy: bool) -> String {
    if legacy {
        format!("position-gpo:{engagement_id}")
    } else {
        format!(
            "gpo-rule:{}",
            serde_json::json!([rule_id, engagement_id]).to_string()
        )
    }
}

/// Stable delivery identity: the same effect replays into one job row.
pub fn delivery_id(parts: &[String]) -> String {
    format!("gpo:v3:{}", sha256_hex(&serde_json::json!(parts)))
}

/// Immutable revision identity for one publish/unmap command.
pub fn revision_id(prefix: &str, key: &str) -> String {
    format!("{prefix}-{key}")
}

pub fn same_ids(left: &[String], right: &[String]) -> bool {
    left.len() == right.len() && left.iter().all(|value| right.contains(value))
}

/// Directory evidence after a targeted write must contain the effect.
pub fn inventory_object<'a>(
    inventory: &'a Inventory,
    id: &str,
    kind: &str,
) -> Option<&'a InventoryObject> {
    inventory
        .objects
        .iter()
        .find(|object| object.id == id && object.kind == kind)
}

pub fn inventory_filters(inventory: &Inventory, gpo_id: &str) -> Vec<String> {
    inventory_object(inventory, gpo_id, "gpo")
        .and_then(|object| object.properties.get("securityFilterGroupIds"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn inventory_links(inventory: &Inventory, ou_id: &str) -> Vec<(String, bool)> {
    inventory_object(inventory, ou_id, "ou")
        .and_then(|object| object.properties.get("gpoLinks"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|link| {
                    Some((
                        link.get("gpoId")?.as_str()?.to_owned(),
                        link.get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub observed_at: i64,
    pub objects: Vec<InventoryObject>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InventoryObject {
    pub id: String,
    pub kind: String,
    pub version: String,
    pub properties: Value,
    pub member_ids: Vec<String>,
}

/// A bounded Directory effect. It carries no authority; the provider profile
/// still decides whether the installed connector approved the operation.
#[derive(Clone, Debug, PartialEq)]
pub enum GpoAction {
    Filter {
        object_id: String,
        group_ids: Vec<String>,
    },
    Link {
        object_id: String,
        parent_id: String,
        enabled: bool,
        enforced: bool,
        remove: bool,
    },
    Members {
        object_id: String,
        member_ids: Vec<String>,
        add: bool,
    },
}
