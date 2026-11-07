//! Platform Administration control-plane facts and lifecycle rules.
//!
//! These types are portable owner facts. They never mint a session, Platform
//! role, Holding binding or permission; the composition backend resolves the
//! current Account, Platform assignment and Holding scope before calling the
//! application use cases.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const HOLDING_ID_MAX: usize = 24;
pub const SLUG_MAX: usize = 40;
pub const DISPLAY_NAME_MAX: usize = 120;
pub const REASON_MAX: usize = 1000;
pub const EVENT_ID_MAX: usize = 181;
pub const LAST_ERROR_MAX: usize = 64;
pub const DEAD_LETTER_LIMIT: usize = 20;
pub const MIGRATION_TRACK: &str = "holding-write";
pub const MIGRATION_ERROR_MAX: usize = 1000;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformResourceSlot {
    pub id: String,
    pub write: String,
    pub read: String,
    pub files: String,
    #[serde(default)]
    pub beta: bool,
    #[serde(default)]
    pub internal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformProvisioningRow {
    pub id: String,
    pub holding_id: String,
    pub slug: String,
    pub display_name: String,
    pub write_binding: String,
    pub read_binding: String,
    pub files_binding: String,
    pub status: String,
    pub requested_by: String,
    pub reason: String,
    pub error: Option<String>,
    pub version: i64,
    pub requested_at: i64,
    pub activated_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformProvisioningSnapshot {
    pub rows: Vec<PlatformProvisioningRow>,
    pub available_slots: Vec<PlatformResourceSlot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvisionHoldingInput {
    pub holding_id: String,
    pub slug: String,
    pub display_name: String,
    pub write_binding: String,
    pub read_binding: String,
    pub files_binding: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionHoldingResult {
    pub id: String,
    pub holding_id: String,
    pub slug: String,
    pub display_name: String,
    pub status: String,
    pub version: i64,
    pub activated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoverDeadLetterInput {
    pub event_id: String,
    pub expected_attempts: i64,
    pub expected_last_error: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverDeadLetterResult {
    pub event_id: String,
    pub holding_id: String,
    pub status: String,
    pub previous_attempts: i64,
    pub previous_error: Option<String>,
    pub recovered_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformDeadLetter {
    pub id: String,
    pub event_type: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub dead_letter_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldingJobStatistics {
    pub pending_events: i64,
    pub dead_letter_events: i64,
    pub claimed_events: i64,
    pub oldest_pending_at: Option<i64>,
    pub dead_letters: Vec<PlatformDeadLetter>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMigrationState {
    pub track: String,
    pub applied: i64,
    pub pending: Vec<String>,
    pub latest_applied: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlServiceHealth {
    pub service: String,
    pub heartbeat_at: i64,
    pub details: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformHoldingIdentity {
    pub id: String,
    pub slug: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformHoldingOperation {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub status: String,
    pub pending_events: i64,
    pub dead_letter_events: i64,
    pub claimed_events: i64,
    pub oldest_pending_at: Option<i64>,
    pub job_heartbeat_at: Option<i64>,
    pub error: Option<String>,
    pub dead_letters: Vec<PlatformDeadLetter>,
    pub migrations: PlatformMigrationState,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformOperationTotals {
    pub holdings: i64,
    pub pending_events: i64,
    pub dead_letter_events: i64,
    pub claimed_events: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformOperationsSummary {
    pub generated_at: i64,
    pub actor_role: String,
    pub control_health: Vec<ControlServiceHealth>,
    pub holdings: Vec<PlatformHoldingOperation>,
    pub totals: PlatformOperationTotals,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformOperationsSnapshot {
    pub provisioning: PlatformProvisioningSnapshot,
    pub operations: PlatformOperationsSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvisioningPlan {
    pub actor: PlatformActor,
    pub slot: PlatformResourceSlot,
    pub input: ProvisionHoldingInput,
    pub key: String,
    pub digest: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryPlan {
    pub actor: PlatformActor,
    pub holding_id: String,
    pub input: RecoverDeadLetterInput,
    pub key: String,
    pub digest: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvisioningReceipt {
    pub request_hash: String,
    pub row: PlatformProvisioningRow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReceipt {
    pub request_hash: String,
    pub result_json: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationRunStatus {
    Running,
    Succeeded,
    Failed,
}
impl MigrationRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunHoldingMigrationInput {
    pub holding_id: String,
    pub track: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHoldingMigrationResult {
    pub run_id: String,
    pub holding_id: String,
    pub track: String,
    pub status: MigrationRunStatus,
    pub applied_before: i64,
    pub applied_after: i64,
    pub pending: Vec<String>,
    pub finished_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRunPlan {
    pub actor: PlatformActor,
    pub holding: PlatformHoldingIdentity,
    pub input: RunHoldingMigrationInput,
    pub key: String,
    pub digest: String,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRunReceipt {
    pub request_hash: String,
    pub status: MigrationRunStatus,
    pub result_json: Option<String>,
    pub error: Option<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn invalid(code: &'static str, message: &'static str) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::invalid(code, message)
}

fn conflict(
    code: &'static str,
    message: &'static str,
) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        code,
        message,
    )
}

fn forbidden(
    code: &'static str,
    message: &'static str,
) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        code,
        message,
    )
}

fn unavailable(
    code: &'static str,
    message: &'static str,
) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        code,
        message,
    )
}

pub fn require_platform_read(
    actor: &PlatformActor,
) -> Result<(), erpx_server_shared::public::BusinessError> {
    if matches!(actor.role.as_str(), "platform_admin" | "platform_auditor") {
        Ok(())
    } else {
        Err(forbidden(
            "PLATFORM_OPERATIONS_FORBIDDEN",
            "A current Platform assignment is required.",
        ))
    }
}

pub fn require_platform_admin(
    actor: &PlatformActor,
) -> Result<(), erpx_server_shared::public::BusinessError> {
    require_platform_read(actor)?;
    if actor.role == "platform_admin" {
        Ok(())
    } else {
        Err(forbidden(
            "FORBIDDEN",
            "Only a Platform administrator can perform this action.",
        ))
    }
}

pub fn validate_command_key(key: &str) -> Result<(), erpx_server_shared::public::BusinessError> {
    erpx_server_shared::public::CommandKey::new(key).map(|_| ())
}

pub fn validate_holding_id(value: &str) -> Result<(), erpx_server_shared::public::BusinessError> {
    if !(3..=HOLDING_ID_MAX).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Choose a configured Holding resource slot.",
        ));
    }
    Ok(())
}

pub fn validate_slug(value: &str) -> Result<(), erpx_server_shared::public::BusinessError> {
    let bytes = value.as_bytes();
    if !(3..=SLUG_MAX).contains(&bytes.len())
        || !bytes[0].is_ascii_lowercase()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Use a lowercase company URL slug with 3-40 characters.",
        ));
    }
    Ok(())
}

pub fn validate_display_name(
    value: &str,
) -> Result<String, erpx_server_shared::public::BusinessError> {
    let trimmed = value.trim();
    if !(2..=DISPLAY_NAME_MAX).contains(&trimmed.len()) {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Enter a Holding display name between 2 and 120 characters.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_reason(
    value: &str,
    minimum: usize,
) -> Result<String, erpx_server_shared::public::BusinessError> {
    let trimmed = value.trim();
    if trimmed.len() < minimum || value.len() > REASON_MAX {
        return Err(invalid(
            "VALIDATION_FAILED",
            if minimum >= 10 {
                "Explain why this Holding should be activated."
            } else {
                "Explain why this action is safe."
            },
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_provisioning(
    actor: &PlatformActor,
    slot: &PlatformResourceSlot,
    input: &ProvisionHoldingInput,
) -> Result<ProvisionHoldingInput, erpx_server_shared::public::BusinessError> {
    require_platform_admin(actor)?;
    validate_holding_id(&input.holding_id)?;
    if input.holding_id != slot.id {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Choose a configured, unbound Holding resource slot.",
        ));
    }
    if slot.internal {
        return Err(forbidden(
            "RESERVED_RESOURCE_SLOT",
            "This resource slot is reserved for ERPX.",
        ));
    }
    validate_slug(&input.slug)?;
    let display_name = validate_display_name(&input.display_name)?;
    let reason = validate_reason(&input.reason, 10)?;
    if input.write_binding != slot.write
        || input.read_binding != slot.read
        || input.files_binding != slot.files
    {
        return Err(invalid(
            "VALIDATION_FAILED",
            "The selected resource slot no longer matches the operator registry.",
        ));
    }
    Ok(ProvisionHoldingInput {
        holding_id: input.holding_id.clone(),
        slug: input.slug.clone(),
        display_name,
        write_binding: slot.write.clone(),
        read_binding: slot.read.clone(),
        files_binding: slot.files.clone(),
        reason,
    })
}

pub fn validate_recovery(
    actor: &PlatformActor,
    holding_id: &str,
    input: &RecoverDeadLetterInput,
) -> Result<RecoverDeadLetterInput, erpx_server_shared::public::BusinessError> {
    require_platform_admin(actor)?;
    validate_holding_id(holding_id)?;
    if !(2..=EVENT_ID_MAX).contains(&input.event_id.len())
        || !input
            .event_id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !input.event_id.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b':' | b'-')
        })
    {
        return Err(invalid("VALIDATION_FAILED", "Choose a valid failed job."));
    }
    if input.expected_attempts < 1 || !input.expected_attempts.is_positive() {
        return Err(invalid(
            "VALIDATION_FAILED",
            "The failed job attempt count is invalid.",
        ));
    }
    if input
        .expected_last_error
        .as_ref()
        .is_some_and(|error| error.len() > LAST_ERROR_MAX)
    {
        return Err(invalid(
            "VALIDATION_FAILED",
            "The failed job error is invalid.",
        ));
    }
    Ok(RecoverDeadLetterInput {
        event_id: input.event_id.clone(),
        expected_attempts: input.expected_attempts,
        expected_last_error: input.expected_last_error.clone(),
        reason: validate_reason(&input.reason, 3)?,
    })
}

pub fn provisioning_digest(
    actor: &PlatformActor,
    key: &str,
    input: &ProvisionHoldingInput,
) -> String {
    let value = serde_json::json!({
        "actor": actor.user_id,
        "assignment": actor.assignment_id,
        "role": actor.role,
        "idempotencyKey": key,
        "input": input,
    });
    sha256_hex(value.to_string().as_bytes())
}

pub fn recovery_digest(
    actor: &PlatformActor,
    key: &str,
    holding_id: &str,
    input: &RecoverDeadLetterInput,
) -> String {
    let value = serde_json::json!({
        "actor": actor.user_id,
        "assignment": actor.assignment_id,
        "role": actor.role,
        "operation": "platform.job.retry",
        "key": key,
        "holdingId": holding_id,
        "input": input,
    });
    sha256_hex(value.to_string().as_bytes())
}

pub fn validate_migration_run(
    actor: &PlatformActor,
    holding_id: &str,
    input: &RunHoldingMigrationInput,
) -> Result<RunHoldingMigrationInput, erpx_server_shared::public::BusinessError> {
    require_platform_admin(actor)?;
    validate_holding_id(holding_id)?;
    if input.holding_id != holding_id {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Choose the Holding selected for this migration run.",
        ));
    }
    if input.track != MIGRATION_TRACK {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Only Holding write migrations can be run from this console.",
        ));
    }
    Ok(RunHoldingMigrationInput {
        holding_id: holding_id.to_owned(),
        track: MIGRATION_TRACK.to_owned(),
        reason: validate_reason(&input.reason, 3)?,
    })
}

pub fn migration_digest(
    actor: &PlatformActor,
    key: &str,
    holding_id: &str,
    input: &RunHoldingMigrationInput,
) -> String {
    let value = serde_json::json!({
        "actor": actor.user_id,
        "assignment": actor.assignment_id,
        "role": actor.role,
        "operation": "platform.holding.migrate",
        "key": key,
        "holdingId": holding_id,
        "input": input,
    });
    sha256_hex(value.to_string().as_bytes())
}

pub fn migration_run_result(
    receipt: &MigrationRunReceipt,
) -> Result<RunHoldingMigrationResult, erpx_server_shared::public::BusinessError> {
    match receipt.status {
        MigrationRunStatus::Succeeded => {
            let retained = receipt.result_json.as_deref().ok_or_else(|| {
                unavailable(
                    "COMMAND_RECEIPT_INVALID",
                    "The previous migration receipt is unavailable.",
                )
            })?;
            serde_json::from_str(retained).map_err(|_| {
                unavailable(
                    "COMMAND_RECEIPT_INVALID",
                    "The previous migration receipt is unavailable.",
                )
            })
        }
        MigrationRunStatus::Running => Err(conflict(
            "MIGRATION_IN_PROGRESS",
            "The previous migration run is still being finalized. Retry the same command later.",
        )),
        MigrationRunStatus::Failed => Err(conflict(
            "MIGRATION_FAILED",
            "The previous migration run failed. Review the platform migration receipt before retrying.",
        )),
    }
}

pub fn provisioning_result(
    row: &PlatformProvisioningRow,
) -> Result<ProvisionHoldingResult, erpx_server_shared::public::BusinessError> {
    let activated_at = row.activated_at.ok_or_else(|| {
        conflict(
            "PROVISIONING_INCOMPLETE",
            "Holding provisioning is still being prepared. Refresh the Platform operations page before continuing.",
        )
    })?;
    if row.status != "active" {
        return Err(conflict(
            "PROVISIONING_INCOMPLETE",
            "Holding provisioning is still being prepared. Refresh the Platform operations page before continuing.",
        ));
    }
    Ok(ProvisionHoldingResult {
        id: row.id.clone(),
        holding_id: row.holding_id.clone(),
        slug: row.slug.clone(),
        display_name: row.display_name.clone(),
        status: "active".into(),
        version: row.version,
        activated_at,
    })
}

pub fn available_resource_slots(
    slots: &[PlatformResourceSlot],
    rows: &[PlatformProvisioningRow],
    used_bindings: &[String],
) -> Vec<PlatformResourceSlot> {
    let used: BTreeSet<&str> = used_bindings.iter().map(String::as_str).collect();
    let pending: BTreeSet<&str> = rows
        .iter()
        .filter(|row| row.status == "provisioning")
        .flat_map(|row| {
            [
                row.holding_id.as_str(),
                row.write_binding.as_str(),
                row.read_binding.as_str(),
                row.files_binding.as_str(),
            ]
        })
        .collect();
    slots
        .iter()
        .filter(|slot| {
            !slot.internal
                && !used.contains(slot.id.as_str())
                && !used.contains(slot.write.as_str())
                && !used.contains(slot.read.as_str())
                && !used.contains(slot.files.as_str())
                && !pending.contains(slot.id.as_str())
                && !pending.contains(slot.write.as_str())
                && !pending.contains(slot.read.as_str())
                && !pending.contains(slot.files.as_str())
        })
        .cloned()
        .collect()
}

pub fn holding_operation_status(statistics: &HoldingJobStatistics) -> &'static str {
    if statistics.dead_letter_events > 0 {
        "attention"
    } else if statistics.pending_events > 0 || statistics.claimed_events > 0 {
        "pending"
    } else {
        "healthy"
    }
}

pub fn unavailable_holding_operation(
    holding: &PlatformHoldingIdentity,
    job_heartbeat_at: Option<i64>,
) -> PlatformHoldingOperation {
    PlatformHoldingOperation {
        id: holding.id.clone(),
        slug: holding.slug.clone(),
        display_name: holding.display_name.clone(),
        status: "unavailable".into(),
        pending_events: 0,
        dead_letter_events: 0,
        claimed_events: 0,
        oldest_pending_at: None,
        job_heartbeat_at,
        error: Some("Holding database or operational tables are unavailable.".into()),
        dead_letters: Vec::new(),
        migrations: PlatformMigrationState {
            track: "holding-write".into(),
            applied: 0,
            pending: Vec::new(),
            latest_applied: None,
            error: Some("Holding database or operational tables are unavailable.".into()),
        },
    }
}

pub fn operation_totals(holdings: &[PlatformHoldingOperation]) -> PlatformOperationTotals {
    PlatformOperationTotals {
        holdings: holdings.len() as i64,
        pending_events: holdings.iter().map(|row| row.pending_events).sum(),
        dead_letter_events: holdings.iter().map(|row| row.dead_letter_events).sum(),
        claimed_events: holdings.iter().map(|row| row.claimed_events).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot() -> PlatformResourceSlot {
        PlatformResourceSlot {
            id: "h004".into(),
            write: "H004_DB".into(),
            read: "H004_READ_DB".into(),
            files: "H004_FILES".into(),
            beta: false,
            internal: false,
        }
    }

    fn input() -> ProvisionHoldingInput {
        ProvisionHoldingInput {
            holding_id: "h004".into(),
            slug: "northstar".into(),
            display_name: " Northstar ".into(),
            write_binding: "H004_DB".into(),
            read_binding: "H004_READ_DB".into(),
            files_binding: "H004_FILES".into(),
            reason: "Reviewed onboarding and verified the isolated resource health.".into(),
        }
    }

    fn actor(role: &str) -> PlatformActor {
        PlatformActor {
            user_id: "operator".into(),
            assignment_id: "platform-assignment".into(),
            role: role.into(),
        }
    }

    fn row(id: &str, status: &str, holding_id: &str, slug: &str) -> PlatformProvisioningRow {
        PlatformProvisioningRow {
            id: id.into(),
            holding_id: holding_id.into(),
            slug: slug.into(),
            display_name: "Holding".into(),
            write_binding: format!("{}_DB", holding_id.to_uppercase()),
            read_binding: format!("{}_READ_DB", holding_id.to_uppercase()),
            files_binding: format!("{}_FILES", holding_id.to_uppercase()),
            status: status.into(),
            requested_by: "operator".into(),
            reason: "reason".into(),
            error: None,
            version: 1,
            requested_at: 0,
            activated_at: None,
            updated_at: 0,
        }
    }

    #[test]
    fn provisioning_rejects_auditors_reserved_slots_and_binding_drift() {
        let mut call = input();
        assert_eq!(
            validate_provisioning(&actor("platform_auditor"), &slot(), &call)
                .unwrap_err()
                .code,
            "FORBIDDEN"
        );
        let mut reserved = slot();
        reserved.internal = true;
        assert_eq!(
            validate_provisioning(&actor("platform_admin"), &reserved, &call)
                .unwrap_err()
                .code,
            "RESERVED_RESOURCE_SLOT"
        );
        call.write_binding = "OTHER_DB".into();
        assert_eq!(
            validate_provisioning(&actor("platform_admin"), &slot(), &call)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        call = input();
        call.slug = "Bad Slug".into();
        assert!(validate_provisioning(&actor("platform_admin"), &slot(), &call).is_err());
        call = input();
        call.reason = "short".into();
        assert!(validate_provisioning(&actor("platform_admin"), &slot(), &call).is_err());
    }

    #[test]
    fn provisioning_normalizes_bounded_text_and_uses_a_stable_digest() {
        let normalized =
            validate_provisioning(&actor("platform_admin"), &slot(), &input()).unwrap();
        assert_eq!(normalized.display_name, "Northstar");
        assert_eq!(
            normalized.reason,
            "Reviewed onboarding and verified the isolated resource health."
        );
        let first = provisioning_digest(&actor("platform_admin"), "provision-key-01", &normalized);
        let second = provisioning_digest(&actor("platform_admin"), "provision-key-01", &normalized);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(
            first,
            provisioning_digest(&actor("platform_admin"), "provision-key-02", &normalized)
        );
    }

    #[test]
    fn available_slots_exclude_internal_used_and_pending_bindings() {
        let slots = vec![
            slot(),
            PlatformResourceSlot {
                id: "h005".into(),
                write: "H005_DB".into(),
                read: "H005_READ_DB".into(),
                files: "H005_FILES".into(),
                beta: false,
                internal: false,
            },
            PlatformResourceSlot {
                id: "h003".into(),
                write: "H003_DB".into(),
                read: "H003_READ_DB".into(),
                files: "H003_FILES".into(),
                beta: false,
                internal: true,
            },
        ];
        assert_eq!(
            available_resource_slots(&slots, &[], &[])
                .iter()
                .map(|slot| slot.id.as_str())
                .collect::<Vec<_>>(),
            ["h004", "h005"]
        );
        assert_eq!(
            available_resource_slots(
                &slots,
                &[row("p", "provisioning", "h004", "northstar")],
                &[]
            )
            .iter()
            .map(|slot| slot.id.as_str())
            .collect::<Vec<_>>(),
            ["h005"]
        );
        assert_eq!(
            available_resource_slots(&slots, &[], &["H005_FILES".into()])
                .iter()
                .map(|slot| slot.id.as_str())
                .collect::<Vec<_>>(),
            ["h004"]
        );
    }

    #[test]
    fn operation_status_and_totals_preserve_attention_pending_and_unavailable() {
        let healthy = HoldingJobStatistics {
            pending_events: 0,
            dead_letter_events: 0,
            claimed_events: 0,
            oldest_pending_at: None,
            dead_letters: Vec::new(),
        };
        let mut pending = healthy.clone();
        pending.pending_events = 2;
        let mut attention = pending.clone();
        attention.dead_letter_events = 1;
        assert_eq!(holding_operation_status(&healthy), "healthy");
        assert_eq!(holding_operation_status(&pending), "pending");
        assert_eq!(holding_operation_status(&attention), "attention");
        let unavailable = unavailable_holding_operation(
            &PlatformHoldingIdentity {
                id: "h001".into(),
                slug: "acme".into(),
                display_name: "Acme".into(),
            },
            Some(5),
        );
        assert_eq!(unavailable.status, "unavailable");
        let totals = operation_totals(&[
            PlatformHoldingOperation {
                id: "h001".into(),
                slug: "acme".into(),
                display_name: "Acme".into(),
                status: "attention".into(),
                pending_events: 2,
                dead_letter_events: 1,
                claimed_events: 3,
                oldest_pending_at: Some(4),
                job_heartbeat_at: Some(5),
                error: None,
                dead_letters: Vec::new(),
                migrations: PlatformMigrationState {
                    track: "holding-write".into(),
                    applied: 7,
                    pending: Vec::new(),
                    latest_applied: Some("0007".into()),
                    error: None,
                },
            },
            unavailable,
        ]);
        assert_eq!(totals.holdings, 2);
        assert_eq!(totals.pending_events, 2);
        assert_eq!(totals.dead_letter_events, 1);
        assert_eq!(totals.claimed_events, 3);
    }

    #[test]
    fn recovery_rejects_auditors_stale_counts_and_unsafe_event_ids() {
        let input = RecoverDeadLetterInput {
            event_id: "platform-operation-test-recover".into(),
            expected_attempts: 6,
            expected_last_error: Some("PROJECTOR_TIMEOUT".into()),
            reason: "Provider is healthy and the projection is safe to replay.".into(),
        };
        assert_eq!(
            validate_recovery(&actor("platform_auditor"), "h001", &input)
                .unwrap_err()
                .code,
            "FORBIDDEN"
        );
        assert!(validate_recovery(&actor("platform_admin"), "h001", &input).is_ok());
        let mut bad = input.clone();
        bad.expected_attempts = 0;
        assert!(validate_recovery(&actor("platform_admin"), "h001", &bad).is_err());
        bad = input.clone();
        bad.event_id = "Event With Space".into();
        assert!(validate_recovery(&actor("platform_admin"), "h001", &bad).is_err());
        bad = input.clone();
        bad.expected_last_error = Some("x".repeat(65));
        assert!(validate_recovery(&actor("platform_admin"), "h001", &bad).is_err());
        bad = input;
        bad.reason = "x".into();
        assert!(validate_recovery(&actor("platform_admin"), "h001", &bad).is_err());
    }

    fn migration_input() -> RunHoldingMigrationInput {
        RunHoldingMigrationInput {
            holding_id: "h001".into(),
            track: MIGRATION_TRACK.into(),
            reason: "Apply the reviewed release schema before enabling the next workflow.".into(),
        }
    }

    #[test]
    fn migration_runs_require_admin_the_holding_identity_and_only_the_write_track() {
        assert_eq!(
            validate_migration_run(&actor("platform_auditor"), "h001", &migration_input())
                .unwrap_err()
                .code,
            "FORBIDDEN"
        );
        let mut changed = migration_input();
        changed.track = "holding-read".into();
        assert_eq!(
            validate_migration_run(&actor("platform_admin"), "h001", &changed)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        let mut changed = migration_input();
        changed.holding_id = "h002".into();
        assert_eq!(
            validate_migration_run(&actor("platform_admin"), "h001", &changed)
                .unwrap_err()
                .code,
            "VALIDATION_FAILED"
        );
        let mut changed = migration_input();
        changed.reason = " x ".into();
        assert!(validate_migration_run(&actor("platform_admin"), "h001", &changed).is_err());
        assert!(
            validate_migration_run(&actor("platform_admin"), "erpx", &migration_input()).is_err()
        );
    }

    #[test]
    fn migration_digest_binds_the_actor_holding_key_and_normalized_input() {
        let unchanged =
            validate_migration_run(&actor("platform_admin"), "h001", &migration_input()).unwrap();
        let first = migration_digest(
            &actor("platform_admin"),
            "platform-migration-key-1",
            "h001",
            &unchanged,
        );
        assert_eq!(first.len(), 64);
        assert_eq!(
            first,
            migration_digest(
                &actor("platform_admin"),
                "platform-migration-key-1",
                "h001",
                &unchanged,
            )
        );
        assert_ne!(
            first,
            migration_digest(
                &actor("platform_admin"),
                "platform-migration-key-2",
                "h001",
                &unchanged,
            )
        );
        assert_ne!(
            first,
            migration_digest(
                &actor("platform_admin"),
                "platform-migration-key-1",
                "h002",
                &unchanged,
            )
        );
    }

    #[test]
    fn migration_receipts_replay_only_succeeded_results_and_fail_closed_otherwise() {
        let result = RunHoldingMigrationResult {
            run_id: "platform-migration-1".into(),
            holding_id: "h001".into(),
            track: MIGRATION_TRACK.into(),
            status: MigrationRunStatus::Succeeded,
            applied_before: 4,
            applied_after: 5,
            pending: Vec::new(),
            finished_at: 10,
        };
        let succeeded = MigrationRunReceipt {
            request_hash: "hash".into(),
            status: MigrationRunStatus::Succeeded,
            result_json: Some(serde_json::to_string(&result).unwrap()),
            error: None,
        };
        assert_eq!(migration_run_result(&succeeded).unwrap(), result);
        assert_eq!(
            migration_run_result(&MigrationRunReceipt {
                status: MigrationRunStatus::Succeeded,
                result_json: None,
                ..succeeded.clone()
            })
            .unwrap_err()
            .code,
            "COMMAND_RECEIPT_INVALID"
        );
        assert_eq!(
            migration_run_result(&MigrationRunReceipt {
                status: MigrationRunStatus::Running,
                ..succeeded.clone()
            })
            .unwrap_err()
            .code,
            "MIGRATION_IN_PROGRESS"
        );
        assert_eq!(
            migration_run_result(&MigrationRunReceipt {
                status: MigrationRunStatus::Failed,
                ..succeeded
            })
            .unwrap_err()
            .code,
            "MIGRATION_FAILED"
        );
        assert_eq!(
            MigrationRunStatus::parse("succeeded"),
            Some(MigrationRunStatus::Succeeded)
        );
        assert_eq!(MigrationRunStatus::parse("unknown"), None);
    }
}
