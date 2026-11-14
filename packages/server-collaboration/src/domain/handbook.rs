//! Company handbook publication decisions.
//!
//! This module owns article/revision state transitions only. Current Company
//! membership, service-owner/editor authority, purchase access and source
//! visibility remain separate owner ports supplied by composition; no value in
//! this module is authority.
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};

pub const MAX_SAFE_VERSION: i64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Policy,
    Procedure,
    Faq,
    ServiceGuide,
}
impl Category {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "policy" => Ok(Self::Policy),
            "procedure" => Ok(Self::Procedure),
            "faq" => Ok(Self::Faq),
            "service_guide" => Ok(Self::ServiceGuide),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Procedure => "procedure",
            Self::Faq => "faq",
            Self::ServiceGuide => "service_guide",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArticleAction {
    Publish,
    Archive,
    Restore,
}
impl ArticleAction {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "publish" => Ok(Self::Publish),
            "archive" => Ok(Self::Archive),
            "restore" => Ok(Self::Restore),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Archive => "archive",
            Self::Restore => "restore",
        }
    }
    pub fn status(self) -> &'static str {
        match self {
            Self::Publish => "published",
            Self::Archive => "archived",
            Self::Restore => "restored",
        }
    }
}

/// The current authorization result. `can_edit` includes a service owner or an
/// appointed editor; `can_manage_editors` is the service-owner decision only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedHandbookActor {
    pub user_id: String,
    pub company_id: String,
    pub can_read: bool,
    pub can_edit: bool,
    pub can_manage_editors: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftContent {
    pub title: String,
    pub category: Category,
    pub body: String,
}

/// Stored article facts. `draft` is the current draft revision content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArticleSnapshot {
    pub id: String,
    pub version: i64,
    pub draft_revision: i64,
    pub published_revision: Option<i64>,
    pub archived: bool,
    pub draft: DraftContent,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub id: String,
    pub version: i64,
    pub revision: i64,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    CreateArticle {
        title: String,
        category: Category,
        body: String,
    },
    UpdateArticle {
        id: String,
        expected_version: i64,
        title: String,
        category: Category,
        body: String,
    },
    ArticleAction {
        id: String,
        expected_version: i64,
        action: ArticleAction,
    },
    SetEditor {
        user_id: String,
        active: bool,
        expected_version: i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerPlan {
    CreateArticle {
        id: String,
        content: DraftContent,
    },
    UpdateArticle {
        id: String,
        expected_version: i64,
        revision: i64,
        content: DraftContent,
    },
    PublishArticle {
        id: String,
        expected_version: i64,
        revision: i64,
    },
    SetArchived {
        id: String,
        expected_version: i64,
        archived: bool,
    },
    SetEditor {
        user_id: String,
        active: bool,
        expected_version: i64,
    },
}

fn invalid() -> BusinessError {
    BusinessError::invalid("HANDBOOK_INPUT_INVALID", "Provide a valid handbook value.")
}

pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Stores the title exactly as Zod's `.trim()` does and enforces the original
/// 200 code-unit limit.
pub fn article_title(value: &str) -> Result<String, BusinessError> {
    let trimmed = js_trim(value);
    if utf16_len(trimmed) > 200 {
        return Err(invalid());
    }
    Ok(trimmed.into())
}

/// The body is not trimmed by the original schema, only bounded.
pub fn article_body(value: &str) -> Result<String, BusinessError> {
    if utf16_len(value) > 40_000 {
        return Err(invalid());
    }
    Ok(value.into())
}

pub fn validate_editor_user(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || utf16_len(value) > 160 {
        return Err(invalid());
    }
    Ok(())
}

pub fn validate_expected_version(value: i64, allow_zero: bool) -> Result<(), BusinessError> {
    let minimum = if allow_zero { 0 } else { 1 };
    if value < minimum || value > MAX_SAFE_VERSION {
        return Err(invalid());
    }
    Ok(())
}

pub fn publication_ready(title: &str, body: &str) -> bool {
    utf16_len(js_trim(title)) >= 2 && utf16_len(js_trim(body)) >= 2
}

pub fn require_read(actor: &AuthorizedHandbookActor) -> Result<(), BusinessError> {
    if actor.can_read {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        ))
    }
}

pub fn require_edit(actor: &AuthorizedHandbookActor) -> Result<(), BusinessError> {
    require_read(actor)?;
    if actor.can_edit {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "HANDBOOK_EDITOR_REQUIRED",
            "An owner-appointed handbook editor is required.",
        ))
    }
}

pub fn require_manage_editors(actor: &AuthorizedHandbookActor) -> Result<(), BusinessError> {
    require_read(actor)?;
    if actor.can_manage_editors {
        Ok(())
    } else {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "HANDBOOK_OWNER_REQUIRED",
            "The current service owner must appoint handbook editors.",
        ))
    }
}

pub fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "HANDBOOK_SOURCE_CHANGED",
        "The original handbook source or current editor authority changed.",
    )
}

fn next_version(version: i64) -> Result<i64, BusinessError> {
    version
        .checked_add(1)
        .filter(|next| version > 0 && *next <= MAX_SAFE_VERSION)
        .ok_or_else(changed)
}

/// Exact owner result prepared before the Workflow ledger writes a receipt.
pub fn plan_receipt(_key: &CommandKey, plan: &OwnerPlan) -> Result<Receipt, BusinessError> {
    Ok(match plan {
        OwnerPlan::CreateArticle { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            revision: 1,
            status: "draft",
        },
        OwnerPlan::UpdateArticle {
            id,
            expected_version,
            revision,
            ..
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            revision: *revision,
            status: "draft",
        },
        OwnerPlan::PublishArticle {
            id,
            expected_version,
            revision,
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            revision: *revision,
            status: "published",
        },
        OwnerPlan::SetArchived {
            id,
            expected_version,
            archived,
        } => Receipt {
            id: id.clone(),
            version: next_version(*expected_version)?,
            revision: 0,
            status: if *archived { "archived" } else { "restored" },
        },
        OwnerPlan::SetEditor {
            user_id,
            active,
            expected_version,
        } => Receipt {
            id: user_id.clone(),
            version: if *expected_version == 0 {
                1
            } else {
                next_version(*expected_version)?
            },
            revision: 0,
            status: if *active { "active" } else { "inactive" },
        },
    })
}

/// A native list cursor is an opaque `timestamp:id` pair. IDs are command-derived
/// and contain only ASCII alphanumerics/`:_.-`, so the separator is unambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListCursor {
    pub at: i64,
    pub id: String,
}
impl ListCursor {
    pub fn encode(&self) -> String {
        format!("{}:{}", self.at, self.id)
    }
    pub fn decode(value: &str) -> Result<Self, BusinessError> {
        let (at, id) = value.split_once(':').ok_or_else(cursor_invalid)?;
        let at = at.parse::<i64>().map_err(|_| cursor_invalid())?;
        if !(0..=MAX_SAFE_VERSION).contains(&at) || id.is_empty() || id.len() > 512 {
            return Err(cursor_invalid());
        }
        Ok(Self { at, id: id.into() })
    }
}
fn cursor_invalid() -> BusinessError {
    BusinessError::invalid("INVALID_CURSOR", "Restart the handbook list.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_uses_ecmascript_trim_while_body_keeps_original_text() {
        assert_eq!(
            article_title("\u{feff}  Policy  \u{feff}").unwrap(),
            "Policy"
        );
        assert_eq!(article_body("  body  ").unwrap(), "  body  ");
        assert_eq!(article_title(&"a".repeat(200)).unwrap().len(), 200);
        assert!(article_title(&"a".repeat(201)).is_err());
        assert!(article_body(&"a".repeat(40_001)).is_err());
    }

    #[test]
    fn publication_requires_two_code_units_of_title_and_body_without_trimming_storage() {
        assert!(!publication_ready(" a ", "body"));
        assert!(!publication_ready("ab", " b "));
        assert!(publication_ready(" ab ", "cd"));
    }

    #[test]
    fn receipt_versions_are_bounded_and_reflect_the_exact_owner_state() {
        let key = CommandKey::new("handbook-command").unwrap();
        let plan = OwnerPlan::UpdateArticle {
            id: "article".into(),
            expected_version: 4,
            revision: 3,
            content: DraftContent {
                title: "Policy".into(),
                category: Category::Policy,
                body: "Body".into(),
            },
        };
        assert_eq!(
            plan_receipt(&key, &plan).unwrap(),
            Receipt {
                id: "article".into(),
                version: 5,
                revision: 3,
                status: "draft"
            }
        );
        let max = OwnerPlan::SetArchived {
            id: "article".into(),
            expected_version: MAX_SAFE_VERSION,
            archived: true,
        };
        assert_eq!(
            plan_receipt(&key, &max).unwrap_err().code,
            "HANDBOOK_SOURCE_CHANGED"
        );
    }

    #[test]
    fn cursor_round_trips_and_rejects_unbounded_values() {
        let cursor = ListCursor {
            at: 1_700_000_000_000,
            id: "article-handbook-command".into(),
        };
        assert_eq!(ListCursor::decode(&cursor.encode()).unwrap(), cursor);
        for value in [
            "".to_owned(),
            "abc".to_owned(),
            "-1:article".to_owned(),
            "1:".to_owned(),
            "1:".to_owned() + &"x".repeat(513),
        ] {
            assert_eq!(
                ListCursor::decode(&value).unwrap_err().code,
                "INVALID_CURSOR"
            );
        }
    }
}
