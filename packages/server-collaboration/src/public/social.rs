//! Native Social owner port. No commit, purchase or cross-owner authority here.
pub const CONTRACT_VERSION: &str = "collaboration.social.v1";

pub use crate::adapters::social::{
    CurrentSocialEffects, FeedComment, FeedFilter, FeedPost, FeedRecord, apply_plan,
    lock_social_sources, read_comment, read_comments, read_feed, read_post, read_thread,
    retain_effects,
};
pub use crate::application::social::{Access, SocialUnit, authorize_read, execute};
pub use crate::domain::social::{
    Audience, Change, CommentSnapshot, CommentState, FeedCursor, FollowTarget, LinkedRecord,
    LinkedRecordInput, Mention, MentionInput, MentionKind, OwnerPlan, PostSnapshot, PostState,
    Receipt, SocialActor, hashtags, normalize_tag, normalize_tags, plan_receipt, post_not_found,
    post_tags, text,
};
