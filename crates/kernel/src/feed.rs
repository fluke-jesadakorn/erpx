#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_HASHTAGS_PER_ITEM: usize = 20;
pub const MAX_HASHTAG_LEN: usize = 64;
pub const MIN_HASHTAG_LEN: usize = 2;
pub const MAX_FEED_COMMENT_DEPTH: usize = 3;
pub const MAX_FEED_PAGE_SIZE: usize = 100;
pub const DEFAULT_FEED_PAGE_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeedItemId(pub Uuid);

impl FeedItemId {
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn try_from_uuid(value: Uuid) -> Result<Self, crate::IdentifierError> {
        if value.is_nil() || value.get_version_num() != 4 {
            return Err(crate::IdentifierError::Nil);
        }
        Ok(Self(value))
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashtagTopicId(pub Uuid);

impl HashtagTopicId {
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn try_from_uuid(value: Uuid) -> Result<Self, crate::IdentifierError> {
        if value.is_nil() || value.get_version_num() != 4 {
            return Err(crate::IdentifierError::Nil);
        }
        Ok(Self(value))
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FeedValidationError {
    #[error("hashtag is invalid")]
    InvalidHashtag,
    #[error("too many hashtags")]
    TooManyHashtags,
    #[error("feed item is invalid")]
    InvalidFeedItem,
    #[error("share would broaden audience or classification")]
    ShareAudienceExpansion,
    #[error("comment depth exceeded")]
    DepthExceeded,
    #[error("feed filter is invalid")]
    InvalidFilter,
}

pub fn validate_hashtag(tag: &str) -> Result<(), FeedValidationError> {
    if tag.len() < MIN_HASHTAG_LEN || tag.len() > MAX_HASHTAG_LEN {
        return Err(FeedValidationError::InvalidHashtag);
    }
    if !tag
        .bytes()
        .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_'))
    {
        return Err(FeedValidationError::InvalidHashtag);
    }
    if tag.chars().any(char::is_control) {
        return Err(FeedValidationError::InvalidHashtag);
    }
    Ok(())
}

pub fn normalize_hashtag(raw: &str) -> Result<String, FeedValidationError> {
    let trimmed = raw.trim().trim_start_matches('#').to_ascii_lowercase();
    validate_hashtag(&trimmed)?;
    Ok(trimmed)
}

pub fn day_bucket(at: DateTime<Utc>) -> NaiveDate {
    at.date_naive()
}

pub fn day_bucket_label(today: NaiveDate, bucket: NaiveDate) -> &'static str {
    if bucket == today {
        "Today"
    } else if bucket == today - chrono::Duration::days(1) {
        "Yesterday"
    } else {
        "Earlier"
    }
}

pub fn next_feed_cursor(last_created_at: DateTime<Utc>, last_id: FeedItemId) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let raw = format!(
        "{}:{}",
        last_created_at.timestamp_millis(),
        last_id.as_uuid()
    );
    B64.encode(raw.as_bytes())
}

pub fn parse_feed_cursor(cursor: &str) -> Option<(DateTime<Utc>, FeedItemId)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let decoded = B64.decode(cursor).ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let (millis_str, uuid_str) = s.split_once(':')?;
    let millis: i64 = millis_str.parse().ok()?;
    let uuid = Uuid::parse_str(uuid_str).ok()?;
    let dt = DateTime::<Utc>::from_timestamp_millis(millis)?;
    let id = FeedItemId::try_from_uuid(uuid).ok()?;
    Some((dt, id))
}

pub fn is_visible_scope(
    viewer_scope: &crate::AuthorityScope,
    item_scope: &crate::AuthorityScope,
) -> bool {
    viewer_scope.contains(item_scope)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SharedFeedFilter {
    pub contexts: Vec<String>,
    pub day: Option<NaiveDate>,
    pub hashtag: Option<String>,
    pub author_me: bool,
    pub following_only: bool,
}

impl SharedFeedFilter {
    pub fn validate(&self) -> Result<(), FeedValidationError> {
        if let Some(tag) = &self.hashtag {
            validate_hashtag(tag)?;
        }
        if self.contexts.len() > 32 {
            return Err(FeedValidationError::InvalidFilter);
        }
        for ctx in &self.contexts {
            if ctx.trim().is_empty() || ctx.len() > 64 || ctx.chars().any(char::is_control) {
                return Err(FeedValidationError::InvalidFilter);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashtag_validation() {
        assert!(validate_hashtag("launch").is_ok());
        assert!(validate_hashtag("q4_2026").is_ok());
        assert!(validate_hashtag("A").is_err());
        assert!(validate_hashtag("Hashtag").is_err());
        assert_eq!(normalize_hashtag("#Launch"), Ok("launch".to_string()));
        assert_eq!(normalize_hashtag("  #Q4_2026  "), Ok("q4_2026".to_string()));
    }
    #[test]
    fn day_bucket_label_works() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        assert_eq!(day_bucket_label(today, today), "Today");
        assert_eq!(
            day_bucket_label(today, today - chrono::Duration::days(1)),
            "Yesterday"
        );
        assert_eq!(
            day_bucket_label(today, today - chrono::Duration::days(2)),
            "Earlier"
        );
    }
    #[test]
    fn scope_visibility() {
        use crate::{AuthorityScope, HoldingId};
        let hid = HoldingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let cid = Uuid::new_v4();
        let bid = Uuid::new_v4();
        let holding = AuthorityScope::Holding {
            holding_id: hid.as_uuid(),
        };
        let company = AuthorityScope::Company {
            holding_id: hid.as_uuid(),
            company_id: cid,
        };
        let branch = AuthorityScope::Branch {
            holding_id: hid.as_uuid(),
            company_id: cid,
            branch_id: bid,
        };
        assert!(is_visible_scope(&holding, &company));
        assert!(is_visible_scope(&holding, &branch));
        assert!(!is_visible_scope(&company, &holding));
        assert!(is_visible_scope(&company, &branch));
        assert!(!is_visible_scope(&branch, &company));
    }
    #[test]
    fn pagination_cursor_roundtrip() {
        let now = Utc::now();
        let id = FeedItemId::new_v4();
        let cursor = next_feed_cursor(now, id);
        let parsed = parse_feed_cursor(&cursor).unwrap();
        assert_eq!(parsed.0.timestamp_millis(), now.timestamp_millis());
        assert_eq!(parsed.1, id);
    }
}
