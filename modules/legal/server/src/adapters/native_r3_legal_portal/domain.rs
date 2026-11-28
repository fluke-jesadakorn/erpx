//! Pure types and bounds for the native Legal portal ingress.
//!
//! Only two subpaths are bounded here: the list read (no path) and the
//! quick-access read/write. Detail, download, upload and command stay with the
//! TypeScript owner until their session and multi-owner authority is ported.
//! No SQL and no I/O belong in this file.

use serde::{Deserialize, Serialize};

pub const LIST_OPERATION: &str = "legal.portal.list.read";
pub const QUICK_ACCESS_OPERATION: &str = "legal.portal.quick-access";
pub const QUICK_ACCESS_SURFACE: &str = "legal_portal";
pub const BODY_LIMIT: usize = 4096;
pub const MAX_SLUG: usize = 128;
pub const MAX_PIN_ID_UNITS: usize = 240;
pub const MAX_PINS: usize = 6;
pub const MAX_KEY_UNITS: usize = 200;

/// The unenforced purchase predicate is the only one this slice executes. In
/// enforced mode the Legal owner supplies a dynamic commercial predicate that
/// cannot yet be reproduced without a cross-owner bridge.
pub const PURCHASE_PREDICATE_OPEN: &str = "TRUE";

/// Exact `legalPortalListRequest` row projection. `canonicalDestinationLinks`
/// rewrites only `href`-named fields, so this shape passes through unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortalShareWire {
    pub id: String,
    pub document_id: String,
    pub expires_at: i64,
    pub user_id: Option<String>,
    pub name: String,
    pub version: i64,
    pub checksum: String,
}

impl PortalShareWire {
    /// `legalPortalQuickAccess` derives the available pin set from the same
    /// share list with a `share:` prefix.
    pub fn pin_id(&self) -> String {
        format!("share:{}", self.id)
    }
}

/// Exact retired quick-access wire shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessPreferenceWire {
    pub pinned_ids: Option<Vec<String>>,
    pub version: i64,
}

/// Strict retired `{pinnedIds, expectedVersion}` body. Unknown fields, a
/// negative version, more than six pins, empty or overlong pins and duplicate
/// pins are all rejected before any SQL runs.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickAccessBody {
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
}

pub fn valid_body(body: &QuickAccessBody) -> bool {
    if body.expected_version < 0 {
        return false;
    }
    match &body.pinned_ids {
        None => true,
        Some(ids) => {
            let mut seen = std::collections::BTreeSet::new();
            ids.len() <= MAX_PINS
                && ids.iter().all(|id| {
                    !id.is_empty()
                        && id.encode_utf16().count() <= MAX_PIN_ID_UNITS
                        && seen.insert(id.as_str())
                })
        }
    }
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SLUG
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_key(value: &str) -> bool {
    !value.is_empty() && value.encode_utf16().count() <= MAX_KEY_UNITS
}

pub fn valid_predicate(value: &str) -> bool {
    value == PURCHASE_PREDICATE_OPEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_key_and_predicate_bounds_match_the_retired_guards() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
        assert!(valid_key("k"));
        assert!(!valid_key(""));
        assert!(!valid_key(&"x".repeat(MAX_KEY_UNITS + 1)));
        assert!(valid_predicate("TRUE"));
        assert!(!valid_predicate("TRUE OR 1=1"));
        assert!(!valid_predicate("TRUE; DROP TABLE legal_shares"));
    }

    #[test]
    fn body_validation_is_strict() {
        for json in [
            r#"{"pinnedIds":null,"expectedVersion":0}"#,
            r#"{"pinnedIds":["share:a"],"expectedVersion":2}"#,
        ] {
            let body: QuickAccessBody = serde_json::from_str(json).unwrap();
            assert!(valid_body(&body), "{json}");
        }
        for json in [
            r#"{"pinnedIds":["share:a"],"expectedVersion":-1}"#,
            r#"{"pinnedIds":["share:a","share:a"],"expectedVersion":0}"#,
            r#"{"pinnedIds":[""],"expectedVersion":0}"#,
            r#"{"pinnedIds":["a","b","c","d","e","f","g"],"expectedVersion":0}"#,
        ] {
            let body: QuickAccessBody = serde_json::from_str(json).unwrap();
            assert!(!valid_body(&body), "{json}");
        }
        for json in [
            r#"{"pinnedIds":null,"expectedVersion":0,"extra":1}"#,
            r#"{"pinnedIds":0,"expectedVersion":0}"#,
        ] {
            assert!(serde_json::from_str::<QuickAccessBody>(json).is_err(), "{json}");
        }
        let big = format!(r#"{{"pinnedIds":["{}"],"expectedVersion":0}}"#, "x".repeat(241));
        let body: QuickAccessBody = serde_json::from_str(&big).unwrap();
        assert!(!valid_body(&body));
    }

    #[test]
    fn wire_shapes_are_byte_exact_and_have_no_destination_fields() {
        let share = PortalShareWire {
            id: "s1".into(),
            document_id: "d1".into(),
            expires_at: 99,
            user_id: None,
            name: "Draft".into(),
            version: 2,
            checksum: "abc".into(),
        };
        assert_eq!(share.pin_id(), "share:s1");
        let rows = serde_json::to_value(vec![share]).unwrap();
        assert_eq!(rows[0].as_object().unwrap().len(), 7);
        assert_eq!(rows[0]["document_id"], serde_json::json!("d1"));
        let pref = serde_json::to_value(QuickAccessPreferenceWire {
            pinned_ids: Some(vec!["share:s1".into()]),
            version: 3,
        })
        .unwrap();
        assert_eq!(
            pref,
            serde_json::json!({"pinnedIds": ["share:s1"], "version": 3})
        );
    }
}
