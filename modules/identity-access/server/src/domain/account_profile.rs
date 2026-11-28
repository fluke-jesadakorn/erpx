//! Pure Account profile vocabulary for the native profile read.
//!
//! No I/O, no SQL, no HTTP. The owner application maps a persistence record to
//! this projection so the wire shape stays byte-compatible with the retired
//! TypeScript `readAccountProfile` projection.

use erpx_platform_shared_server::public::RecordId;
use serde::Serialize;

/// Preferences projected into the Account profile response. A missing
/// preference row yields the same defaults as `defaultAccountPreferences` in
/// the TypeScript owner domain (`en` / `system` / version `0`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountPreferences {
    pub language: String,
    pub appearance: String,
    pub version: i64,
}

impl Default for AccountPreferences {
    fn default() -> Self {
        Self {
            language: "en".to_owned(),
            appearance: "system".to_owned(),
            version: 0,
        }
    }
}

/// Complete Account profile payload. Field names and nesting match the
/// TypeScript `{ data: profile }` envelope exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountProfile {
    pub name: String,
    pub email: String,
    pub preferences: AccountPreferences,
}

/// Raw Control row: `language`, `appearance` and `version` are null when the
/// user has no `account_preferences` row yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRecord {
    pub name: String,
    pub email: String,
    pub language: Option<String>,
    pub appearance: Option<String>,
    pub version: Option<i32>,
}

/// Pure projection. The TypeScript authority defaults the whole preference
/// object only when `version` is null; otherwise it passes the stored language
/// and appearance through unchanged. Preserve that behaviour exactly.
pub fn project_profile(record: ProfileRecord) -> AccountProfile {
    let preferences = match record.version {
        None => AccountPreferences::default(),
        Some(version) => AccountPreferences {
            language: record.language.unwrap_or_else(|| "en".to_owned()),
            appearance: record.appearance.unwrap_or_else(|| "system".to_owned()),
            version: i64::from(version),
        },
    };
    AccountProfile {
        name: record.name,
        email: record.email,
        preferences,
    }
}

/// Bounded, well-formed user id for the private context header.
pub fn valid_user_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_preference_row_uses_owner_defaults() {
        let profile = project_profile(ProfileRecord {
            name: "Ada".into(),
            email: "ada@example.test".into(),
            language: None,
            appearance: None,
            version: None,
        });
        assert_eq!(
            profile,
            AccountProfile {
                name: "Ada".into(),
                email: "ada@example.test".into(),
                preferences: AccountPreferences::default(),
            }
        );
        assert_eq!(
            serde_json::to_value(&profile).unwrap(),
            serde_json::json!({
                "name": "Ada",
                "email": "ada@example.test",
                "preferences": {"language": "en", "appearance": "system", "version": 0}
            })
        );
    }

    #[test]
    fn stored_preferences_are_projected_without_rewriting() {
        let profile = project_profile(ProfileRecord {
            name: "Ada".into(),
            email: "ada@example.test".into(),
            language: Some("th".into()),
            appearance: Some("dark".into()),
            version: Some(7),
        });
        assert_eq!(profile.preferences.version, 7);
        assert_eq!(profile.preferences.language, "th");
        assert_eq!(profile.preferences.appearance, "dark");
    }

    #[test]
    fn user_ids_are_bounded() {
        assert!(valid_user_id("user-1"));
        assert!(!valid_user_id(""));
        assert!(!valid_user_id("../admin"));
        assert!(!valid_user_id(&"x".repeat(201)));
    }
}
