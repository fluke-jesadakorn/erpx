#![forbid(unsafe_code)]

use erpx_kernel::AuthorityScope;
use erpx_ui_foundation::feed::{
    ComposerState, FeedCardView, FeedViewState, HomeFeedFilter, WorkspaceFeedTab, group_by_day,
    level_badge_for_scope, workspace_feed_path,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFeedState {
    pub workspace_slug: String,
    pub feed: FeedViewState,
    pub composer: ComposerState,
    pub active_tab: WorkspaceFeedTab,
}

impl WorkspaceFeedState {
    pub fn new(workspace: crate::WorkspaceKind) -> Self {
        let slug = workspace.root().to_string();
        Self {
            workspace_slug: slug.clone(),
            feed: FeedViewState::empty(HomeFeedFilter::Context(slug)),
            composer: ComposerState::new(),
            active_tab: WorkspaceFeedTab::Feed,
        }
    }
    pub fn feed_path(&self) -> String {
        workspace_feed_path(&self.workspace_slug, WorkspaceFeedTab::Feed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeFeedState {
    pub view: FeedViewState,
    pub composer: ComposerState,
    pub selected_context: Option<String>,
    pub selected_hashtag: Option<String>,
}

impl HomeFeedState {
    pub fn new() -> Self {
        Self {
            view: FeedViewState::empty(HomeFeedFilter::All),
            composer: ComposerState::new(),
            selected_context: None,
            selected_hashtag: None,
        }
    }
    /// Home aggregates all contexts but badges each card by its workspace chip + level badge.
    pub fn aggregate(
        _workspaces: &[crate::WorkspaceKind],
        cards: Vec<FeedCardView>,
    ) -> FeedViewState {
        let groups = group_by_day(cards);
        let total: u64 = groups.iter().map(|g| g.items.len() as u64).sum();
        let has_more = false;
        FeedViewState {
            groups,
            has_more,
            next_cursor: None,
            total_count: total,
            filter: HomeFeedFilter::All,
            stale: false,
        }
    }
}

pub fn level_badge(scope: &AuthorityScope) -> &'static str {
    level_badge_for_scope(scope)
}

pub fn workspace_chip_for_card(card: &FeedCardView) -> &str {
    &card.workspace_chip
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkspaceKind;
    #[test]
    fn workspace_feed_path_works() {
        let state = WorkspaceFeedState::new(WorkspaceKind::Projects);
        assert_eq!(state.feed_path(), "/project/feed");
    }
    #[test]
    fn home_aggregates_all_workspaces() {
        let cards = vec![];
        let view =
            HomeFeedState::aggregate(&[WorkspaceKind::Projects, WorkspaceKind::Talent], cards);
        assert_eq!(view.total_count, 0);
    }
}
