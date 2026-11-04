//! Native Handbook owner port. No commit or cross-owner authority here.
pub const CONTRACT_VERSION: &str = "collaboration.handbook.v1";

pub use crate::adapters::handbook::{
    CurrentHandbookEffects, HandbookArticleRead, HandbookArticleSummary, HandbookEditorOptions,
    HandbookEditorRow, HandbookHistoryRow, HandbookListPage, HandbookListRow, HandbookRevisionRow,
    apply_plan, lock_handbook_sources, read_article, read_article_detail, read_editor_options,
    read_list, read_settings_version, retain_effects,
};
pub use crate::adapters::handbook_wire::{DecodedHandbookCommand, decode_handbook_command};
pub use crate::application::handbook::{Access, HandbookUnit, authorize_read, execute};
pub use crate::domain::handbook::{
    ArticleAction, ArticleSnapshot, AuthorizedHandbookActor, Category, Change, DraftContent,
    ListCursor, OwnerPlan, Receipt, article_body, article_title, changed, js_trim, plan_receipt,
    publication_ready, require_edit, require_manage_editors, require_read, validate_editor_user,
    validate_expected_version,
};
