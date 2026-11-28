//! Pure types and validation for the bounded native media-publication slice.
//!
//! The wire names mirror the retired TypeScript Zod schema and the raw row
//! names of `attachment_publications`. Publication rows are deliberately
//! snake_case: the Next route returned `SELECT *` rows unchanged, so the
//! native payload must match those bytes rather than remap them at the edge.

use erpx_platform_shared_server::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Target types whose attachment policy registers `publication: true`. This is
/// the same closed set as `attachmentPolicyRegistry` in the Files owner.
pub const PUBLISHABLE_TARGETS: [&str; 9] = [
    "holding_brand",
    "branch_brand",
    "company_storefront_brand",
    "product",
    "marketing_campaign",
    "marketing_asset",
    "ecommerce_listing",
    "pms_property",
    "pms_room_type",
];

/// Purposes that may back a public media publication.
pub const PUBLISHABLE_PURPOSES: [&str; 7] = [
    "primary",
    "inverse",
    "logo",
    "product_image",
    "creative",
    "listing_media",
    "booking_media",
];

pub fn is_publishable_target(value: &str) -> bool {
    PUBLISHABLE_TARGETS.contains(&value)
}

pub fn is_publishable_purpose(value: &str) -> bool {
    PUBLISHABLE_PURPOSES.contains(&value)
}

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", message)
}

fn text<'a>(object: &'a serde_json::Map<String, Value>, key: &str, max: usize) -> Result<&'a str, BusinessError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Invalid publication."))?;
    if value.is_empty() || value.chars().count() > max {
        return Err(invalid("Invalid publication."));
    }
    Ok(value)
}

/// Bounded create input, parsed exactly like the retired Zod schema:
/// `{sourceLinkId,targetType,targetId,altText,ordinal?}`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatePublicationInput {
    pub source_link_id: String,
    pub target_type: String,
    pub target_id: String,
    pub alt_text: String,
    pub ordinal: i64,
}

impl CreatePublicationInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| invalid("Invalid publication."))?;
        let source_link_id = text(object, "sourceLinkId", 180)?.to_owned();
        let target_type = text(object, "targetType", 80)?.to_owned();
        let target_id = text(object, "targetId", 180)?.to_owned();
        let alt_text = text(object, "altText", 500)?.trim().to_owned();
        if alt_text.is_empty() {
            return Err(invalid("Provide concise alt text."));
        }
        let ordinal = match object.get("ordinal") {
            None | Some(Value::Null) => 0,
            Some(Value::Number(number)) => number
                .as_i64()
                .filter(|value| (0..=10_000).contains(value))
                .ok_or_else(|| invalid("Provide a valid display order."))?,
            Some(_) => return Err(invalid("Provide a valid display order.")),
        };
        Ok(Self {
            source_link_id,
            target_type,
            target_id,
            alt_text,
            ordinal,
        })
    }
}

/// One row of the `attachment_publications` projection. Field names are the
/// exact SQL column names; the retired TS list/create paths returned the same
/// rows without presentation remapping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicationRow {
    pub id: String,
    /// Only the create path (`SELECT *`) returns this column; the list SELECT
    /// omits it, so it stays absent from that payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_id: Option<String>,
    pub source_link_id: String,
    pub target_type: String,
    pub target_id: String,
    pub derivative_object_key: String,
    pub derivative_media_type: String,
    pub derivative_checksum_sha256: String,
    pub derivative_bytes: i64,
    pub alt_text: String,
    pub ordinal: i64,
    pub revision: i64,
    pub state: String,
    pub created_by: String,
    pub created_at: i64,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<i64>,
    pub revocation_reason: Option<String>,
}

/// Complete native read payload; serializes to
/// `{targetType,targetId,orderVersion,items}`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationList {
    pub target_type: String,
    pub target_id: String,
    pub order_version: i64,
    pub items: Vec<PublicationRow>,
}

/// Stable publication identity: the same prefix, hash and truncation as the
/// retired `stableId("media-publication", actor, idempotencyKey)` helper.
pub fn stable_id(company: &str, user: &str, key: &str) -> String {
    let mut input = String::with_capacity(company.len() + user.len() + key.len() + 2);
    input.push_str(company);
    input.push('\0');
    input.push_str(user);
    input.push('\0');
    input.push_str(key);
    let digest = hex::encode(Sha256::digest(input.as_bytes()));
    format!("media-publication-{}", &digest[..32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_defaults_and_trims_alt_text() {
        let raw = serde_json::json!({
            "sourceLinkId": "link-1",
            "targetType": "holding_brand",
            "targetId": "acme",
            "altText": "  Acme mark  ",
        });
        let parsed = CreatePublicationInput::parse(&raw).expect("valid input");
        assert_eq!(parsed.source_link_id, "link-1");
        assert_eq!(parsed.alt_text, "Acme mark");
        assert_eq!(parsed.ordinal, 0);
    }

    #[test]
    fn parse_rejects_out_of_bounds_fields() {
        let base = serde_json::json!({
            "sourceLinkId": "link-1",
            "targetType": "holding_brand",
            "targetId": "acme",
            "altText": "Acme mark",
        });
        for value in [
            serde_json::json!({"ordinal": -1}),
            serde_json::json!({"ordinal": 10_001}),
            serde_json::json!({"ordinal": "1"}),
            serde_json::json!({"altText": "   "}),
        ] {
            let mut raw = base.clone();
            let object = raw.as_object_mut().expect("object");
            for (key, item) in value.as_object().expect("object") {
                object.insert(key.clone(), item.clone());
            }
            assert_eq!(
                CreatePublicationInput::parse(&raw).unwrap_err().code,
                "VALIDATION_FAILED"
            );
        }
    }

    #[test]
    fn publishable_sets_are_closed() {
        assert!(is_publishable_target("holding_brand"));
        assert!(is_publishable_target("pms_room_type"));
        assert!(!is_publishable_target("leave_request"));
        assert!(is_publishable_purpose("product_image"));
        assert!(!is_publishable_purpose("medical_certificate"));
    }

    #[test]
    fn stable_id_is_deterministic_and_key_scoped() {
        let a = stable_id("company-1", "user-1", "key-00000001");
        let b = stable_id("company-1", "user-1", "key-00000001");
        let c = stable_id("company-1", "user-1", "key-00000002");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("media-publication-"));
        assert_eq!(a.len(), "media-publication-".len() + 32);
    }
}
