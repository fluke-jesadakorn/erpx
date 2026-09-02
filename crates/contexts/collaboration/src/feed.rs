#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{AuthorityScope, DataClassification, HoldingId, PrincipalId, PrincipalKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{Audience, CollaborationError, CommentId, PrincipalRef, ThreadId};

pub use erpx_kernel::feed::DEFAULT_FEED_PAGE_SIZE;
pub use erpx_kernel::feed::MAX_FEED_COMMENT_DEPTH;
pub use erpx_kernel::feed::MAX_FEED_PAGE_SIZE;
pub use erpx_kernel::feed::MAX_HASHTAG_LEN;
pub use erpx_kernel::feed::MAX_HASHTAGS_PER_ITEM;
pub use erpx_kernel::feed::MIN_HASHTAG_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeedItemId(pub Uuid);

impl FeedItemId {
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn try_from_uuid(value: Uuid) -> Result<Self, CollaborationError> {
        if value.is_nil() || value.get_version_num() != 4 {
            return Err(CollaborationError::InvalidIdentifier("feed_item_id"));
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
    pub fn try_from_uuid(value: Uuid) -> Result<Self, CollaborationError> {
        if value.is_nil() || value.get_version_num() != 4 {
            return Err(CollaborationError::InvalidIdentifier("hashtag_topic_id"));
        }
        Ok(Self(value))
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FeedError {
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

pub fn validate_hashtag(tag: &str) -> Result<(), FeedError> {
    erpx_kernel::feed::validate_hashtag(tag).map_err(|_| FeedError::InvalidHashtag)
}

pub fn normalize_hashtag(raw: &str) -> Result<String, FeedError> {
    erpx_kernel::feed::normalize_hashtag(raw)
        .map(|s| s)
        .map_err(|_| FeedError::InvalidHashtag)
}

pub fn day_bucket(at: DateTime<Utc>) -> NaiveDate {
    erpx_kernel::feed::day_bucket(at)
}

pub fn day_bucket_label(today: NaiveDate, bucket: NaiveDate) -> &'static str {
    erpx_kernel::feed::day_bucket_label(today, bucket)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashtagTopic {
    pub id: HashtagTopicId,
    pub tag: String,
    pub created_by: PrincipalRef,
    pub created_at: DateTime<Utc>,
}

impl HashtagTopic {
    pub fn try_new(
        id: HashtagTopicId,
        tag: String,
        created_by: PrincipalRef,
        created_at: DateTime<Utc>,
    ) -> Result<Self, FeedError> {
        validate_hashtag(&tag)?;
        if created_by.kind() == PrincipalKind::Service {
            return Err(FeedError::InvalidHashtag);
        }
        Ok(Self {
            id,
            tag,
            created_by,
            created_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedItem {
    pub id: FeedItemId,
    pub holding_id: HoldingId,
    pub thread_id: ThreadId,
    pub comment_id: Option<CommentId>,
    pub anchor: StableRecordRef,
    pub author: PrincipalRef,
    pub audience: Audience,
    pub visibility_scope: AuthorityScope,
    pub classification: DataClassification,
    pub created_at: DateTime<Utc>,
    pub day_bucket: NaiveDate,
    pub hashtags: Vec<String>,
    pub record_refs: Vec<StableRecordRef>,
    pub share_of: Option<FeedItemId>,
    pub reaction_counts: std::collections::BTreeMap<String, u32>,
    pub reply_depth: usize,
    pub parent_comment_id: Option<CommentId>,
}

impl FeedItem {
    pub fn validate(&self) -> Result<(), FeedError> {
        self.anchor
            .validate()
            .map_err(|_| FeedError::InvalidFeedItem)?;
        self.visibility_scope
            .validate()
            .map_err(|_| FeedError::InvalidFeedItem)?;
        if self.hashtags.len() > MAX_HASHTAGS_PER_ITEM {
            return Err(FeedError::TooManyHashtags);
        }
        let mut seen = std::collections::BTreeSet::new();
        for tag in &self.hashtags {
            validate_hashtag(tag)?;
            if !seen.insert(tag) {
                return Err(FeedError::InvalidHashtag);
            }
        }
        if self.reply_depth > MAX_FEED_COMMENT_DEPTH {
            return Err(FeedError::DepthExceeded);
        }
        if self.day_bucket != day_bucket(self.created_at) {
            return Err(FeedError::InvalidFeedItem);
        }
        Ok(())
    }

    pub fn is_visible_to(
        &self,
        viewer_scope: &AuthorityScope,
        viewer_principal: PrincipalRef,
    ) -> bool {
        viewer_scope.contains(&self.visibility_scope) && self.audience.includes(viewer_principal)
    }

    pub fn can_share_to(
        &self,
        share_audience: &Audience,
        share_classification: DataClassification,
    ) -> Result<(), FeedError> {
        if !self.audience.can_narrow_to(share_audience) {
            return Err(FeedError::ShareAudienceExpansion);
        }
        if !share_classification.is_permitted_by(self.classification)
            && share_classification != self.classification
        {
            let stricter = self.classification.stricter(share_classification);
            if stricter != share_classification {
                return Err(FeedError::ShareAudienceExpansion);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FeedFilter {
    pub contexts: Vec<String>,
    pub day: Option<NaiveDate>,
    pub hashtag: Option<String>,
    pub topic_record: Option<StableRecordRef>,
    pub author_me: bool,
    pub following_only: bool,
    pub author_principal_id: Option<PrincipalId>,
}

impl FeedFilter {
    pub fn validate(&self) -> Result<(), FeedError> {
        if let Some(tag) = &self.hashtag {
            validate_hashtag(tag)?;
        }
        if let Some(record) = &self.topic_record {
            record.validate().map_err(|_| FeedError::InvalidFilter)?;
        }
        if self.contexts.len() > 32 {
            return Err(FeedError::InvalidFilter);
        }
        for ctx in &self.contexts {
            if ctx.trim().is_empty() || ctx.len() > 64 || ctx.chars().any(char::is_control) {
                return Err(FeedError::InvalidFilter);
            }
        }
        Ok(())
    }

    pub fn matches(&self, item: &FeedItem, viewer_principal: PrincipalRef) -> bool {
        if let Some(day) = self.day {
            if item.day_bucket != day {
                return false;
            }
        }
        if let Some(tag) = &self.hashtag {
            if !item.hashtags.contains(tag) {
                return false;
            }
        }
        if let Some(record) = &self.topic_record {
            if &item.anchor != record && !item.record_refs.contains(record) {
                return false;
            }
        }
        if self.author_me && item.author != viewer_principal {
            return false;
        }
        if !self.contexts.is_empty() {
            let ctx = item.anchor.owner_context().slug();
            if !self.contexts.contains(&ctx.to_string()) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    pub next_cursor: Option<String>,
    pub authorized_count: u64,
}

pub fn next_feed_cursor(last_created_at: DateTime<Utc>, last_id: FeedItemId) -> String {
    erpx_kernel::feed::next_feed_cursor(
        last_created_at,
        erpx_kernel::feed::FeedItemId(last_id.as_uuid()),
    )
}

pub fn parse_feed_cursor(cursor: &str) -> Option<(DateTime<Utc>, FeedItemId)> {
    let (dt, kid) = erpx_kernel::feed::parse_feed_cursor(cursor)?;
    let id = FeedItemId::try_from_uuid(kid.as_uuid()).ok()?;
    Some((dt, id))
}

pub fn paginate_feed(mut items: Vec<FeedItem>, page_size: usize, cursor: Option<&str>) -> FeedPage {
    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.as_uuid().cmp(&a.id.as_uuid()))
    });
    let page_size = page_size.clamp(1, MAX_FEED_PAGE_SIZE);
    let authorized_count = items.len() as u64;
    let start = if let Some(c) = cursor.and_then(parse_feed_cursor) {
        let (cursor_at, cursor_id) = c;
        items
            .iter()
            .position(|item| {
                item.created_at < cursor_at
                    || (item.created_at == cursor_at && item.id.as_uuid() < cursor_id.as_uuid())
            })
            .unwrap_or(items.len())
    } else {
        0
    };
    let slice = &items[start..];
    let page_items: Vec<FeedItem> = slice.iter().take(page_size).cloned().collect();
    let next_cursor = if slice.len() > page_size {
        let last = &page_items[page_size - 1];
        Some(next_feed_cursor(last.created_at, last.id))
    } else {
        None
    };
    FeedPage {
        items: page_items,
        next_cursor,
        authorized_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::{HoldingId, PrincipalId, PrincipalKind, RecordId};
    use uuid::Uuid;

    fn holding() -> HoldingId {
        HoldingId::try_from_uuid(Uuid::new_v4()).unwrap()
    }
    fn principal(kind: PrincipalKind) -> PrincipalRef {
        PrincipalRef::new(PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(), kind)
    }
    fn record(ctx: ContextId, rtype: &str) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ctx,
            RecordTypeId::try_new(rtype).unwrap(),
        )
        .unwrap()
    }
    fn audience_holding(hid: HoldingId) -> Audience {
        Audience::Internal {
            scope: AuthorityScope::Holding {
                holding_id: hid.as_uuid(),
            },
            principals: Vec::new(),
        }
    }
    fn audience_company(hid: HoldingId, cid: Uuid) -> Audience {
        Audience::Internal {
            scope: AuthorityScope::Company {
                holding_id: hid.as_uuid(),
                company_id: cid,
            },
            principals: Vec::new(),
        }
    }
    fn audience_branch(hid: HoldingId, cid: Uuid, bid: Uuid) -> Audience {
        Audience::Internal {
            scope: AuthorityScope::Branch {
                holding_id: hid.as_uuid(),
                company_id: cid,
                branch_id: bid,
            },
            principals: Vec::new(),
        }
    }

    #[test]
    fn hashtag_validation_rejects_bad() {
        assert!(validate_hashtag("launch").is_ok());
        assert!(validate_hashtag("q4_2026").is_ok());
        assert!(validate_hashtag("A").is_err());
        assert!(validate_hashtag("Hashtag").is_err());
        assert!(validate_hashtag("hashtag!").is_err());
        assert_eq!(normalize_hashtag("#Launch"), Ok("launch".to_string()));
        assert_eq!(normalize_hashtag("  #Q4_2026  "), Ok("q4_2026".to_string()));
    }

    #[test]
    fn holding_ceo_sees_all_scopes() {
        let hid = holding();
        let cid = Uuid::new_v4();
        let bid = Uuid::new_v4();
        let ceo_scope = AuthorityScope::Holding {
            holding_id: hid.as_uuid(),
        };
        let mgr_scope = AuthorityScope::Company {
            holding_id: hid.as_uuid(),
            company_id: cid,
        };
        let staff_scope = AuthorityScope::Branch {
            holding_id: hid.as_uuid(),
            company_id: cid,
            branch_id: bid,
        };

        let ceo = principal(PrincipalKind::Human);
        let company_item = FeedItem {
            id: FeedItemId::new_v4(),
            holding_id: hid,
            thread_id: crate::domain::ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
            comment_id: None,
            anchor: record(ContextId::Projects, "project.project"),
            author: principal(PrincipalKind::Human),
            audience: audience_company(hid, cid),
            visibility_scope: AuthorityScope::Company {
                holding_id: hid.as_uuid(),
                company_id: cid,
            },
            classification: DataClassification::Internal,
            created_at: Utc::now(),
            day_bucket: Utc::now().date_naive(),
            hashtags: vec![],
            record_refs: vec![],
            share_of: None,
            reaction_counts: Default::default(),
            reply_depth: 0,
            parent_comment_id: None,
        };
        let branch_item = FeedItem {
            visibility_scope: AuthorityScope::Branch {
                holding_id: hid.as_uuid(),
                company_id: cid,
                branch_id: bid,
            },
            audience: audience_branch(hid, cid, bid),
            ..company_item.clone()
        };
        let holding_item = FeedItem {
            visibility_scope: AuthorityScope::Holding {
                holding_id: hid.as_uuid(),
            },
            audience: audience_holding(hid),
            ..company_item.clone()
        };
        assert!(company_item.is_visible_to(&ceo_scope, ceo));
        assert!(branch_item.is_visible_to(&ceo_scope, ceo));
        assert!(holding_item.is_visible_to(&ceo_scope, ceo));

        assert!(company_item.is_visible_to(&mgr_scope, ceo));
        // Company manager sees branch under same company (contains)
        assert!(branch_item.is_visible_to(&mgr_scope, ceo));
        // manager at Company cannot see Holding-wide
        assert!(!holding_item.is_visible_to(&mgr_scope, ceo));
        // staff at Branch cannot see Company-wide (Branch does not contain Company)
        assert!(!company_item.is_visible_to(&staff_scope, ceo));
        assert!(branch_item.is_visible_to(&staff_scope, ceo));
        // branch staff cannot see holding-wide
        assert!(!holding_item.is_visible_to(&staff_scope, ceo));
    }

    #[test]
    fn share_inherits_stricter() {
        let hid = holding();
        let item = FeedItem {
            id: FeedItemId::new_v4(),
            holding_id: hid,
            thread_id: crate::domain::ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
            comment_id: None,
            anchor: record(ContextId::Finance, "finance.invoice"),
            author: principal(PrincipalKind::Human),
            audience: audience_holding(hid),
            visibility_scope: AuthorityScope::Holding {
                holding_id: hid.as_uuid(),
            },
            classification: DataClassification::Confidential,
            created_at: Utc::now(),
            day_bucket: Utc::now().date_naive(),
            hashtags: vec![],
            record_refs: vec![],
            share_of: None,
            reaction_counts: Default::default(),
            reply_depth: 0,
            parent_comment_id: None,
        };
        let cid = Uuid::new_v4();
        let narrower = audience_company(hid, cid);
        assert!(
            item.can_share_to(&narrower, DataClassification::Confidential)
                .is_ok()
        );
        let broader = Audience::Internal {
            scope: AuthorityScope::Platform,
            principals: vec![],
        };
        assert!(
            item.can_share_to(&broader, DataClassification::Confidential)
                .is_err()
        );
    }

    #[test]
    fn day_bucket_grouping() {
        let now = Utc::now();
        let today = now.date_naive();
        assert_eq!(day_bucket(now), today);
        assert_eq!(day_bucket_label(today, today), "Today");
        assert_eq!(
            day_bucket_label(today, today - chrono::Duration::days(1)),
            "Yesterday"
        );
    }

    #[test]
    fn paginate_feed_cursor() {
        let hid = holding();
        let mut items = Vec::new();
        for i in 0..5 {
            let mut item = FeedItem {
                id: FeedItemId::new_v4(),
                holding_id: hid,
                thread_id: crate::domain::ThreadId::try_from_uuid(Uuid::new_v4()).unwrap(),
                comment_id: None,
                anchor: record(ContextId::Projects, "project.project"),
                author: principal(PrincipalKind::Human),
                audience: audience_holding(hid),
                visibility_scope: AuthorityScope::Holding {
                    holding_id: hid.as_uuid(),
                },
                classification: DataClassification::Internal,
                created_at: Utc::now() - chrono::Duration::seconds(i * 60),
                day_bucket: Utc::now().date_naive(),
                hashtags: vec![],
                record_refs: vec![],
                share_of: None,
                reaction_counts: Default::default(),
                reply_depth: 0,
                parent_comment_id: None,
            };
            item.day_bucket = day_bucket(item.created_at);
            items.push(item);
        }
        let page = paginate_feed(items.clone(), 2, None);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.authorized_count, 5);
        assert!(page.next_cursor.is_some());
        let page2 = paginate_feed(items, 2, page.next_cursor.as_deref());
        assert_eq!(page2.items.len(), 2);
    }
}
