#![forbid(unsafe_code)]

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedCardView {
    pub feed_item_id: Uuid,
    pub holding_id: Uuid,
    pub workspace_slug: String,
    pub author_principal_id: Uuid,
    pub author_display_name: String,
    pub visibility_scope_label: String,
    pub classification: String,
    pub created_at: DateTime<Utc>,
    pub day_bucket: NaiveDate,
    pub day_label: String,
    pub workspace_chip: String,
    pub level_badge: String,
    pub hashtags: Vec<String>,
    pub tombstone: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayGroupView {
    pub day_bucket: NaiveDate,
    pub label: String,
    pub items: Vec<FeedCardView>,
}

pub fn group_by_day(mut items: Vec<FeedCardView>) -> Vec<DayGroupView> {
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut groups: Vec<DayGroupView> = Vec::new();
    for item in items {
        let bucket = item.day_bucket;
        let label = item.day_label.clone();
        if let Some(last) = groups.last_mut().filter(|g| g.day_bucket == bucket) {
            last.items.push(item);
        } else {
            groups.push(DayGroupView {
                day_bucket: bucket,
                label,
                items: vec![item],
            });
        }
    }
    groups
}

pub fn workspace_chip_for_context(context_slug: &str) -> String {
    context_slug.to_string()
}

pub fn level_badge_for_scope(scope: &erpx_kernel::AuthorityScope) -> &'static str {
    match scope {
        erpx_kernel::AuthorityScope::Platform => "Platform",
        erpx_kernel::AuthorityScope::Holding { .. } => "Holding",
        erpx_kernel::AuthorityScope::Company { .. } => "Company",
        erpx_kernel::AuthorityScope::Branch { .. } => "Branch",
        erpx_kernel::AuthorityScope::Subtree { .. } => "Subtree",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HomeFeedFilter {
    All,
    Following,
    Hashtag(String),
    Context(String),
    MyDay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedViewState {
    pub groups: Vec<DayGroupView>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub total_count: u64,
    pub filter: HomeFeedFilter,
    pub stale: bool,
}

impl FeedViewState {
    pub fn empty(filter: HomeFeedFilter) -> Self {
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
    pub workspace_slug: Option<String>,
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
            workspace_slug: None,
            mention_query: None,
            hashtag_query: None,
            can_submit: false,
            error: None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() && self.hashtags.is_empty() {
            return Err("Post cannot be empty".into());
        }
        if self.text.len() > 65536 {
            return Err("Post too long".into());
        }
        if self.hashtags.len() > erpx_kernel::feed::MAX_HASHTAGS_PER_ITEM {
            return Err("Too many hashtags".into());
        }
        for tag in &self.hashtags {
            erpx_kernel::feed::validate_hashtag(tag).map_err(|e| e.to_string())?;
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
pub enum WorkspaceFeedTab {
    Feed,
    Records,
    Analytics,
}

impl WorkspaceFeedTab {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Feed => "Feed",
            Self::Records => "Records",
            Self::Analytics => "Analytics",
        }
    }
    pub const fn path_suffix(self) -> &'static str {
        match self {
            Self::Feed => "feed",
            Self::Records => "records",
            Self::Analytics => "analytics",
        }
    }
}

pub fn workspace_feed_path(workspace_root: &str, tab: WorkspaceFeedTab) -> String {
    format!(
        "{}/{}",
        workspace_root.trim_end_matches('/'),
        tab.path_suffix()
    )
}

pub fn home_feed_path() -> &'static str {
    "/"
}

pub fn all_workspace_slugs() -> Vec<&'static str> {
    vec![
        "identity",
        "platform",
        "organization",
        "commercial",
        "brand",
        "customer",
        "customer-360",
        "support",
        "finance",
        "sales",
        "pos",
        "procurement",
        "inventory",
        "asset",
        "maintenance",
        "quality",
        "operations",
        "people",
        "payroll",
        "legal",
        "marketing",
        "crm",
        "manufacturing",
        "planning",
        "project",
        "commerce",
        "talent",
        "learning",
        "benefits",
        "field-service",
        "collaboration",
        "agent",
    ]
}

pub fn home_aggregates_all_contexts(filter: &HomeFeedFilter) -> bool {
    matches!(filter, HomeFeedFilter::All)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeAggregation {
    pub workspaces: Vec<String>,
    pub total_items: usize,
    pub day_groups: Vec<DayGroupView>,
}

impl HomeAggregation {
    pub fn from_groups(groups: Vec<DayGroupView>) -> Self {
        let total = groups.iter().map(|g| g.items.len()).sum();
        let workspaces = groups
            .iter()
            .flat_map(|g| g.items.iter().map(|i| i.workspace_slug.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            workspaces,
            total_items: total,
            day_groups: groups,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[test]
    fn group_by_day_orders() {
        let now = Utc::now();
        let today = now.date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let cards = vec![
            FeedCardView {
                feed_item_id: Uuid::new_v4(),
                holding_id: Uuid::new_v4(),
                workspace_slug: "finance".into(),
                author_principal_id: Uuid::new_v4(),
                author_display_name: "CEO".into(),
                visibility_scope_label: "Holding".into(),
                classification: "internal".into(),
                created_at: now,
                day_bucket: today,
                day_label: "Today".into(),
                workspace_chip: "finance".into(),
                level_badge: "Holding".into(),
                hashtags: vec![],
                tombstone: false,
                stale: false,
            },
            FeedCardView {
                feed_item_id: Uuid::new_v4(),
                holding_id: Uuid::new_v4(),
                workspace_slug: "sales".into(),
                author_principal_id: Uuid::new_v4(),
                author_display_name: "Alice".into(),
                visibility_scope_label: "Branch".into(),
                classification: "internal".into(),
                created_at: now - chrono::Duration::days(1),
                day_bucket: yesterday,
                day_label: "Yesterday".into(),
                workspace_chip: "sales".into(),
                level_badge: "Branch".into(),
                hashtags: vec!["launch".into()],
                tombstone: false,
                stale: false,
            },
        ];
        let groups = group_by_day(cards);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].day_bucket, today);
    }
    #[test]
    fn workspace_paths() {
        assert_eq!(
            workspace_feed_path("/finance", WorkspaceFeedTab::Feed),
            "/finance/feed"
        );
        assert_eq!(home_feed_path(), "/");
    }
    #[test]
    fn composer_validation() {
        let mut c = ComposerState::new();
        assert!(c.validate().is_err());
        c.text = "Hello @finance/journal-1 #launch".into();
        c.hashtags = vec!["launch".into()];
        assert!(c.validate().is_ok());
    }
}
