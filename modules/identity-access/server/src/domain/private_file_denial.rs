//! Portable facts for the private-file denial authority witness.
//!
//! The witness is an opaque equality token: a SHA-256 of the acting context and
//! a closed, read-only image of the IAM, Organization and People facts behind a
//! rejected private-file request. Nothing here opens a transaction, reads an
//! HTTP request or grants access; callers supply validated actor facts and the
//! serialized image produced by the owner adapter.
use erpx_platform_shared_server::public::BusinessError;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write;

pub const MAX_ID: usize = 200;

pub fn witness_request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRIVATE_FILE_DENIAL_REQUEST_INVALID",
        "Review the private-file request and try again.",
    )
}

pub fn witness_image_required() -> BusinessError {
    BusinessError::invalid(
        "PRIVATE_FILE_DENIAL_IMAGE_REQUIRED",
        "The private-file denial evidence is unavailable.",
    )
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID
}

/// The five authority facts the witness binds. The remaining `WorkContext`
/// fields are checked by the caller's own identity comparison; they are not
/// source facts and never authorize a read here.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrivateFileDenialActor {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub session_id: Option<String>,
}

impl PrivateFileDenialActor {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let actor: Self = serde_json::from_value(raw.clone()).map_err(|_| witness_request_invalid())?;
        actor.validate()
    }

    fn validate(self) -> Result<Self, BusinessError> {
        if !valid_id(&self.company_id)
            || !valid_id(&self.user_id)
            || !valid_id(&self.assignment_id)
            || self.role.len() > MAX_ID
            || self
                .session_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
        {
            return Err(witness_request_invalid());
        }
        Ok(self)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "companyId": self.company_id,
            "userId": self.user_id,
            "assignmentId": self.assignment_id,
            "role": self.role,
            "sessionId": self.session_id,
        })
    }
}

/// The same opaque binding the owner used before the native port: a SHA-256 of
/// the JSON pair `[actor, image]`. The image is the exact text of a closed
/// `jsonb_build_object` projection; it is never parsed or widened here.
pub fn witness_hash(actor: &PrivateFileDenialActor, image: &str) -> String {
    if image.is_empty() {
        return String::new();
    }
    let encoded = serde_json::to_vec(&json!([actor.to_json(), image])).unwrap_or_default();
    let digest = Sha256::digest(encoded);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> Value {
        json!({
            "companyId": "company-1", "userId": "user-1",
            "assignmentId": "assignment-1", "role": "it_admin",
            "sessionId": "session-1"
        })
    }

    #[test]
    fn actor_validation_is_bounded_and_preserves_the_optional_session() {
        let parsed = PrivateFileDenialActor::parse(&actor()).unwrap();
        assert_eq!(parsed.session_id.as_deref(), Some("session-1"));
        assert_eq!(parsed.to_json()["companyId"], json!("company-1"));

        let mut missing = actor();
        missing.as_object_mut().unwrap().remove("assignmentId");
        assert_eq!(
            PrivateFileDenialActor::parse(&missing).unwrap_err().code,
            "PRIVATE_FILE_DENIAL_REQUEST_INVALID"
        );
        let mut long = actor();
        long["userId"] = json!("u".repeat(MAX_ID + 1));
        assert_eq!(
            PrivateFileDenialActor::parse(&long).unwrap_err().code,
            "PRIVATE_FILE_DENIAL_REQUEST_INVALID"
        );
        let mut anonymous = actor();
        anonymous["sessionId"] = Value::Null;
        assert_eq!(
            PrivateFileDenialActor::parse(&anonymous).unwrap().session_id,
            None
        );
    }

    #[test]
    fn witness_hash_is_stable_and_binds_actor_and_image() {
        let first = PrivateFileDenialActor::parse(&actor()).unwrap();
        let image = r#"{"chain":[],"members":[]}"#;
        let hash = witness_hash(&first, image);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, witness_hash(&first, image));
        assert_ne!(hash, witness_hash(&first, "{}"));
        let mut other = actor();
        other["userId"] = json!("user-2");
        assert_ne!(hash, witness_hash(&PrivateFileDenialActor::parse(&other).unwrap(), image));
        assert_eq!(witness_hash(&first, ""), "");
    }
}
