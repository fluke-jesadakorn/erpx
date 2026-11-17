//! Legacy-compatible POS day command identity and replay data.
//!
//! Identity is made from the JSON-parsed, Guest Account-canonicalized *raw*
//! request, before Zod discards unknown fields. Callers must recheck current
//! authority and hold the Company POS advisory lock before consulting receipt
//! storage; an identity or prior receipt never grants authority.
use crate::domain::commands::PosActor;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use erpx_server_workflow::public::commands::{
    CommandJson, canonical_guest_account_json, legacy_guest_account_json,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct PosDayReceiptIdentity {
    pub key: CommandKey,
    pub operation: &'static str,
    pub canonical_input: CommandJson,
    /// UTF-8 input to the legacy SHA-256 calculation.
    pub canonical_identity: String,
    identity_prefix: String,
    company_id: String,
}
fn encode(value: &str) -> Result<String, BusinessError> {
    serde_json::to_string(value).map_err(|_| {
        BusinessError::invalid("POS_COMMAND_INVALID", "Review the POS command fields.")
    })
}
fn identity_prefix(actor: &PosActor, operation: &str) -> Result<String, BusinessError> {
    let branch = actor
        .branch_id
        .as_ref()
        .map(|id| encode(id.as_str()))
        .transpose()?
        .unwrap_or_else(|| "null".into());
    Ok(format!(
        "{{\"user\":{},\"assignment\":{},\"company\":{},\"branch\":{},\"operation\":{},\"input\":",
        encode(actor.user_id.as_str())?,
        encode(actor.assignment_id.as_str())?,
        encode(actor.company_id.as_str())?,
        branch,
        encode(operation)?
    ))
}
impl PosDayReceiptIdentity {
    pub fn parse(
        actor: &PosActor,
        operation: &str,
        key: &str,
        raw_input: &str,
    ) -> Result<Self, BusinessError> {
        let operation = match operation {
            "day.prepare" => "day.prepare",
            "day.confirm" => "day.confirm",
            _ => {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "POS_ACTION_UNKNOWN",
                    "Unknown day close command.",
                ));
            }
        };
        let raw = CommandJson::parse(raw_input)?;
        let canonical_input = canonical_guest_account_json(&raw)?;
        let key = CommandKey::new(key)?;
        let prefix = identity_prefix(actor, operation)?;
        Ok(Self {
            key,
            operation,
            canonical_identity: format!("{}{}{}", prefix, canonical_input.as_str(), "}"),
            identity_prefix: prefix,
            company_id: actor.company_id.as_str().to_owned(),
            canonical_input,
        })
    }
    pub fn company_id(&self) -> &str { &self.company_id }
    pub fn request_hash(&self) -> String { sha256(&self.canonical_identity) }
    pub fn replay_sha256(&self, stored_hash: &str, stored_result_json: &str) -> Result<CommandJson, BusinessError> {
        self.replay(stored_hash, stored_result_json, sha256)
    }
    /// Compute the legacy identity only when a canonical hash did not match,
    /// matching JS short-circuit behavior on aliases that cannot be reversed.
    pub fn legacy_identity(&self) -> Result<String, BusinessError> {
        let legacy = legacy_guest_account_json(&self.canonical_input)?;
        Ok(format!(
            "{}{}{}",
            self.identity_prefix,
            legacy.as_str(),
            "}"
        ))
    }
    /// Check both actual SHA-256 digests before replay. On a match, apply the
    /// same Guest Account conversion the retained TS command applies to the
    /// stored acknowledgement; return it without stripping extra fields.
    pub fn replay(
        &self,
        stored_hash: &str,
        stored_result_json: &str,
        hash: impl Fn(&str) -> String,
    ) -> Result<CommandJson, BusinessError> {
        if stored_hash != hash(&self.canonical_identity)
            && stored_hash != hash(&self.legacy_identity()?)
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "Command number is already used for other data",
            ));
        }
        let result = CommandJson::parse(stored_result_json).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "POS_SOURCE_INVALID",
                "The POS receipt could not be read.",
            )
        })?;
        canonical_guest_account_json(&result)
    }
}
fn sha256(value: &str) -> String { format!("{:x}", Sha256::digest(value.as_bytes())) }
