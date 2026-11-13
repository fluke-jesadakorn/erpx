use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Longest accepted emergency-access reason, matching the retained contract.
pub const MAX_REASON_CHARS: usize = 1000;
pub const MIN_REASON_CHARS: usize = 10;

/// One reviewed emergency-access command body. The HTTP body never selects the
/// actor: Holding/Company scope is explicit, identity comes from the session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecoveryInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_slug: Option<String>,
    pub company_id: String,
    pub reason: String,
}

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "VALIDATION_FAILED", message)
}

impl RecoveryInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let Some(object) = value.as_object() else {
            return Err(invalid("Choose the Company and Holding for emergency access."));
        };
        let mut seen = Map::new();
        for (name, field) in object {
            let known = matches!(name.as_str(), "holdingSlug" | "companyId" | "reason");
            if !known || seen.insert(name.clone(), field.clone()).is_some() {
                return Err(invalid("Choose the Company and Holding for emergency access."));
            }
        }
        let company_id = match object.get("companyId") {
            Some(Value::String(value)) if !value.is_empty() && value.len() <= 160 => value.clone(),
            _ => return Err(invalid("Choose the Company and Holding for emergency access.")),
        };
        let holding_slug = match object.get("holdingSlug") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.is_empty() && value.len() <= 120 => {
                Some(value.clone())
            }
            _ => return Err(invalid("Choose the Company and Holding for emergency access.")),
        };
        let reason = match object.get("reason") {
            Some(Value::String(value)) => value
                .trim_matches(|c: char| {
                    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
                })
                .to_string(),
            _ => return Err(invalid("Describe the reviewed emergency reason.")),
        };
        let length = reason.encode_utf16().count();
        if !(MIN_REASON_CHARS..=MAX_REASON_CHARS).contains(&length) {
            return Err(invalid("Describe the reviewed emergency reason."));
        }
        Ok(Self {
            holding_slug,
            company_id,
            reason,
        })
    }
}

/// Idempotency keys are opaque, bounded and printable.
pub fn valid_key(value: &str) -> bool {
    (8..=180).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
}

/// Temporary access is capped at one hour and never extends the reviewer grant.
pub fn expires_at(now: i64, grant_valid_until: i64) -> i64 {
    now.saturating_add(3_600_000).min(grant_valid_until)
}

pub fn platform_scope() -> &'static str {
    "Platform administration"
}

pub fn holding_label(display_name: &str, company_id: &str) -> String {
    format!("{display_name} · {company_id}")
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmergencyAccess {
    pub company_id: String,
    pub holding_slug: Option<String>,
    pub scope_label: String,
    pub reason: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmergencyAccessList {
    pub access: Vec<EmergencyAccess>,
    pub unavailable_scopes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RecoveryReceipt {
    pub company_id: String,
    pub actor_id: String,
    pub request_hash: String,
    pub result: Value,
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This request key was already used for a different emergency access action.",
    )
}

pub fn recovery_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "RECOVERY_UNAVAILABLE",
        "This account is not a designated recovery administrator in this workspace.",
    )
}

pub fn not_active() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "EMERGENCY_ACCESS_NOT_ACTIVE",
        "No active temporary access exists for this session and workspace.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_strict_recovery_body() {
        let input = RecoveryInput::parse(&json!({
            "companyId": "c-1",
            "holdingSlug": "acme",
            "reason": "  Break-glass response to a reviewed outage  "
        }))
        .expect("valid");
        assert_eq!(input.company_id, "c-1");
        assert_eq!(input.holding_slug.as_deref(), Some("acme"));
        assert_eq!(input.reason, "Break-glass response to a reviewed outage");
    }

    #[test]
    fn rejects_unknown_duplicate_and_short_fields() {
        for body in [
            json!({"companyId": "c", "reason": "short"}),
            json!({"companyId": "c", "reason": "long enough reason", "extra": true}),
            json!({"companyId": "", "reason": "long enough reason"}),
            json!({"companyId": "c", "holdingSlug": 4, "reason": "long enough reason"}),
            json!(["c"]),
        ] {
            assert!(RecoveryInput::parse(&body).is_err(), "{body}");
        }
    }

    #[test]
    fn caps_temporary_access_at_one_hour_and_the_grant_expiry() {
        assert_eq!(expires_at(1_000, 9_999_999), 3_601_000);
        assert_eq!(expires_at(1_000, 2_000), 2_000);
    }

    #[test]
    fn accepts_only_bounded_printable_keys() {
        assert!(valid_key("reviewed:emergency-2026.11.13_a"));
        assert!(!valid_key("short"));
        assert!(!valid_key("has space padding"));
        assert!(!valid_key(&"a".repeat(181)));
    }
}
