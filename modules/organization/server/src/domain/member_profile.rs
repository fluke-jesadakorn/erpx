//! Native member-profile save facts and planner.
//!
//! This module validates the exact retained TypeScript `profileInput` wire
//! shape and the retained optimistic-version rules. It never reads or writes
//! another owner's rows; identity membership, File media and Work projections
//! remain composition concerns supplied as ports at the application boundary.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

pub const MEMBER_PROFILE_SAVE_OPERATION: &str = "member.profile.update";
pub const MEMBER_PROFILE_RECORD_TYPE: &str = "member_profiles";

const MAX_BIO: usize = 500;
const MAX_LINKS: usize = 3;
const MAX_LABEL: usize = 40;
const MAX_URL: usize = 500;

/// One retained profile link. The retained schema requires an HTTPS URL.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemberProfileLink {
    pub label: String,
    pub url: String,
}

/// Exact retained `profileInput` facts: trimmed bio, at most three HTTPS links
/// and a non-negative expected version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberProfileSaveInput {
    pub bio: String,
    pub links: Vec<MemberProfileLink>,
    pub expected_version: i64,
}

impl MemberProfileSaveInput {
    pub fn parse(raw: &serde_json::Value) -> Result<Self, BusinessError> {
        let bio = match raw.get("bio") {
            Some(serde_json::Value::String(value)) => value.trim().to_owned(),
            _ => return Err(request_invalid()),
        };
        if bio.encode_utf16().count() > MAX_BIO {
            return Err(request_invalid());
        }
        let links_value = raw.get("links").and_then(|value| value.as_array());
        let Some(links_value) = links_value else {
            return Err(request_invalid());
        };
        if links_value.len() > MAX_LINKS {
            return Err(request_invalid());
        }
        let mut links = Vec::with_capacity(links_value.len());
        for item in links_value {
            let label = match item.get("label") {
                Some(serde_json::Value::String(value)) => value.trim().to_owned(),
                _ => return Err(request_invalid()),
            };
            if label.is_empty() || label.encode_utf16().count() > MAX_LABEL {
                return Err(request_invalid());
            }
            let url = match item.get("url") {
                Some(serde_json::Value::String(value)) => value.trim().to_owned(),
                _ => return Err(request_invalid()),
            };
            if url.is_empty() || url.encode_utf16().count() > MAX_URL || !https_url(&url) {
                return Err(request_invalid());
            }
            if item.as_object().is_some_and(|object| object.len() != 2) {
                return Err(request_invalid());
            }
            links.push(MemberProfileLink { label, url });
        }
        let expected_version = match raw.get("version") {
            Some(serde_json::Value::Number(value)) => value.as_i64(),
            _ => None,
        }
        .filter(|value| *value >= 0)
        .ok_or_else(request_invalid)?;
        Ok(Self {
            bio,
            links,
            expected_version,
        })
    }

    pub fn links_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(&self.links).map_err(|_| request_invalid())
    }
}

/// Unambiguous HTTPS URL check without a network call: exact scheme, a host and
/// no user info or fragment. This mirrors the retained `z.url()` + `https:`
/// refinement used by the TypeScript schema.
fn https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    !authority.is_empty()
        && !authority
            .bytes()
            .any(|byte| matches!(byte, b'@' | b'?' | b'#' | b' '))
        && authority.rsplit_once(':').is_none_or(|(_, port)| {
            !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Result of the retained save planner: a fresh version and a signal whether
/// the profile row must be inserted (`version == 0`) or updated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemberProfileSavePlan {
    pub version: i64,
    pub insert: bool,
}

/// Retained optimistic-version rule. Version 0 creates the profile; any other
/// version must match the current row exactly.
pub fn plan_save(
    expected_version: i64,
    current_version: Option<i64>,
) -> Result<MemberProfileSavePlan, BusinessError> {
    match (expected_version, current_version) {
        (0, None) => Ok(MemberProfileSavePlan {
            version: 1,
            insert: true,
        }),
        (0, Some(_)) => Err(conflict(
            "PROFILE_EXISTS",
            "Your profile already exists. Reload before saving again.",
        )),
        (expected, Some(current)) if expected == current => Ok(MemberProfileSavePlan {
            version: expected + 1,
            insert: false,
        }),
        (_, None) => Err(not_found()),
        (_, Some(_)) => Err(conflict(
            "PROFILE_VERSION_CHANGED",
            "Your profile changed. Reload and review before saving.",
        )),
    }
}

/// Fresh current authority is composed by the caller exactly as for other
/// native owner commands. This only checks the supplied facts.
pub fn require_current(
    actor: &MemberProfileActor,
    now: erpx_platform_shared_server::public::TimestampMillis,
) -> Result<(), BusinessError> {
    if actor.user_id.is_empty()
        || actor.company_id.as_str().is_empty()
        || actor.assignment_id.is_empty()
        || actor.work_context.len() != 64
        || !actor
            .work_context
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ORGANIZATION_MEMBER_AUTHORITY_INVALID",
            "The current member authority is unavailable.",
        ));
    }
    if now.get() >= actor.expires_at {
        return Err(conflict(
            "WORK_CONTEXT_CHANGED",
            "Your work role changed. Reload and review before saving.",
        ));
    }
    Ok(())
}

/// Minimal fresh actor snapshot. Composition resolves the Account session and
/// current IAM assignment and supplies it; this crate never mints one.
#[derive(Clone, Debug)]
pub struct MemberProfileActor {
    pub user_id: String,
    pub company_id: erpx_platform_shared_server::public::CompanyId,
    pub assignment_id: String,
    pub role: String,
    pub work_context: String,
    pub authority_json: String,
    pub expires_at: i64,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_MEMBER_REQUEST_INVALID",
        "Review the profile details and try again.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROFILE_NOT_FOUND",
        "This profile is unavailable.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was used for a different profile change. Start a new action.",
    )
}

pub fn command_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_INCOMPLETE",
        "This command did not finish. Retry the same action.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_accepts_retained_profile_input() {
        let input = MemberProfileSaveInput::parse(&json!({
            "bio": "  hello  ",
            "links": [{"label":" Site ","url":"https://example.com"}],
            "version": 3
        }))
        .unwrap();
        assert_eq!(input.bio, "hello");
        assert_eq!(input.expected_version, 3);
        assert_eq!(input.links[0].label, "Site");
    }

    #[test]
    fn parse_rejects_non_https_and_oversized_links() {
        assert!(
            MemberProfileSaveInput::parse(
                &json!({"bio":"","links":[{"label":"x","url":"http://example.com"}],"version":0})
            )
            .is_err()
        );
        let many = (0..4)
            .map(|_| json!({"label":"x","url":"https://example.com"}))
            .collect::<Vec<_>>();
        assert!(
            MemberProfileSaveInput::parse(&json!({"bio":"","links":many,"version":0})).is_err()
        );
    }

    #[test]
    fn planner_enforces_optimistic_versions() {
        assert_eq!(
            plan_save(0, None).unwrap(),
            MemberProfileSavePlan {
                version: 1,
                insert: true
            }
        );
        assert_eq!(
            plan_save(4, Some(4)).unwrap(),
            MemberProfileSavePlan {
                version: 5,
                insert: false
            }
        );
        assert!(plan_save(4, Some(5)).is_err());
        assert!(plan_save(0, Some(1)).is_err());
    }

    #[test]
    fn https_url_checks_scheme_and_authority() {
        assert!(https_url("https://example.com/path"));
        assert!(!https_url("http://example.com"));
        assert!(!https_url("https://user@example.com"));
    }
}
