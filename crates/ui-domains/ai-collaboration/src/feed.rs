#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_ui_core::Locale;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedItemCard {
    pub feed_item_id: Uuid,
    pub holding_id: Uuid,
    pub thread_id: Uuid,
    pub comment_id: Option<Uuid>,
    pub anchor: StableRecordRef,
    pub author_principal_id: Uuid,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub visibility_scope_label: String,
    pub classification: String,
    pub created_at: DateTime<Utc>,
    pub day_bucket: NaiveDate,
    pub day_label: String,
    pub hashtags: Vec<String>,
    pub record_chips: Vec<RecordChip>,
    pub share_of: Option<Uuid>,
    pub reaction_counts: std::collections::BTreeMap<String, u32>,
    pub viewer_reaction: Option<String>,
    pub reply_depth: usize,
    pub parent_comment_id: Option<Uuid>,
    pub tombstone: bool,
    pub stale: bool,
    pub following: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordChip {
    pub record: StableRecordRef,
    pub display_label: String,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashtagChip {
    pub tag: String,
    pub count: u64,
    pub following: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayGroup {
    pub day_bucket: NaiveDate,
    pub label: String,
    pub items: Vec<FeedItemCard>,
}

pub fn group_by_day(mut items: Vec<FeedItemCard>) -> Vec<DayGroup> {
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut groups: Vec<DayGroup> = Vec::new();
    for item in items {
        let bucket = item.day_bucket;
        let label = item.day_label.clone();
        if let Some(last) = groups.last_mut().filter(|g| g.day_bucket == bucket) {
            last.items.push(item);
        } else {
            groups.push(DayGroup {
                day_bucket: bucket,
                label,
                items: vec![item],
            });
        }
    }
    groups
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedFilterKind {
    All,
    Following,
    Hashtag(String),
    TopicRecord(Uuid),
    MyDay,
    Context(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedViewState {
    pub groups: Vec<DayGroup>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub total_count: u64,
    pub filter: FeedFilterKind,
    pub stale: bool,
}

impl FeedViewState {
    pub fn empty(filter: FeedFilterKind) -> Self {
        Self {
            groups: Vec::new(),
            has_more: false,
            next_cursor: None,
            total_count: 0,
            filter,
            stale: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerState {
    pub text: String,
    pub hashtags: Vec<String>,
    pub record_chips: Vec<RecordChip>,
    pub mention_query: Option<String>,
    pub hashtag_query: Option<String>,
    pub can_submit: bool,
    pub error: Option<String>,
}

impl ComposerState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            hashtags: Vec::new(),
            record_chips: Vec::new(),
            mention_query: None,
            hashtag_query: None,
            can_submit: false,
            error: None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() && self.record_chips.is_empty() && self.hashtags.is_empty() {
            return Err("Feed post cannot be empty".into());
        }
        if self.text.len() > 65536 {
            return Err("Post too long".into());
        }
        if self.hashtags.len() > 20 {
            return Err("Too many hashtags".into());
        }
        Ok(())
    }
}

impl Default for ComposerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedAction {
    Like,
    Comment,
    Share,
    Follow,
    Mention,
}

impl FeedAction {
    pub fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Like, Locale::En) => "Like",
            (Self::Like, Locale::Th) => "ถูกใจ",
            (Self::Comment, Locale::En) => "Comment",
            (Self::Comment, Locale::Th) => "แสดงความคิดเห็น",
            (Self::Share, Locale::En) => "Share",
            (Self::Share, Locale::Th) => "แชร์",
            (Self::Follow, Locale::En) => "Follow",
            (Self::Follow, Locale::Th) => "ติดตาม",
            (Self::Mention, Locale::En) => "Mention",
            (Self::Mention, Locale::Th) => "กล่าวถึง",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyDaySummary {
    pub day: NaiveDate,
    pub count: usize,
    pub items: Vec<FeedItemCard>,
}

pub fn my_day_label(day: NaiveDate, today: NaiveDate, locale: Locale) -> String {
    if day == today {
        match locale {
            Locale::En => "Today".into(),
            Locale::Th => "วันนี้".into(),
        }
    } else if day == today - chrono::Duration::days(1) {
        match locale {
            Locale::En => "Yesterday".into(),
            Locale::Th => "เมื่อวาน".into(),
        }
    } else {
        day.format("%d %b %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use erpx_integration_contracts::{RecordTypeId, catalog::ContextId};
    use erpx_kernel::RecordId;

    fn record(ctx: ContextId, rtype: &str) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ctx,
            RecordTypeId::try_new(rtype).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn group_by_day_orders_desc() {
        let now = Utc::now();
        let today = now.date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let cards = vec![
            FeedItemCard {
                feed_item_id: Uuid::new_v4(),
                holding_id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                comment_id: None,
                anchor: record(ContextId::Finance, "finance.invoice"),
                author_principal_id: Uuid::new_v4(),
                author_display_name: "CEO".into(),
                author_avatar_url: None,
                visibility_scope_label: "Holding".into(),
                classification: "internal".into(),
                created_at: now - chrono::Duration::hours(2),
                day_bucket: today,
                day_label: "Today".into(),
                hashtags: vec![],
                record_chips: vec![],
                share_of: None,
                reaction_counts: Default::default(),
                viewer_reaction: None,
                reply_depth: 0,
                parent_comment_id: None,
                tombstone: false,
                stale: false,
                following: false,
            },
            FeedItemCard {
                feed_item_id: Uuid::new_v4(),
                holding_id: Uuid::new_v4(),
                thread_id: Uuid::new_v4(),
                comment_id: None,
                anchor: record(ContextId::Finance, "finance.invoice"),
                author_principal_id: Uuid::new_v4(),
                author_display_name: "Alice".into(),
                author_avatar_url: None,
                visibility_scope_label: "Company".into(),
                classification: "internal".into(),
                created_at: now - chrono::Duration::days(1),
                day_bucket: yesterday,
                day_label: "Yesterday".into(),
                hashtags: vec!["launch".into()],
                record_chips: vec![],
                share_of: None,
                reaction_counts: Default::default(),
                viewer_reaction: None,
                reply_depth: 0,
                parent_comment_id: None,
                tombstone: false,
                stale: false,
                following: false,
            },
        ];
        let groups = group_by_day(cards);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].day_bucket, today);
        assert_eq!(groups[1].day_bucket, yesterday);
    }

    #[test]
    fn composer_validation() {
        let mut c = ComposerState::new();
        assert!(c.validate().is_err());
        c.text = "Hello @finance/inv-1 #launch".into();
        assert!(c.validate().is_ok());
        c.hashtags = (0..21).map(|i| format!("tag{i}")).collect();
        assert!(c.validate().is_err());
    }

    #[test]
    fn my_day_label_locales() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        assert_eq!(my_day_label(today, today, Locale::En), "Today");
        assert_eq!(
            my_day_label(today - chrono::Duration::days(1), today, Locale::En),
            "Yesterday"
        );
        assert_eq!(
            my_day_label(today - chrono::Duration::days(2), today, Locale::En),
            "28 Aug 2026"
        );
    }
}
