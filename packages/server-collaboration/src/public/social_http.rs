//! Social owner HTTP ingress. No session, Company or member authority here.
pub const CONTRACT_VERSION: &str = "collaboration.social.http.v1";

pub use crate::adapters::social_http::{
    FeedQuery, SocialFeedData, SocialHttpBackend, SocialHttpCommandRequest, SocialHttpFuture,
    SocialHttpReadReply, SocialHttpReadRequest, SocialHttpState, SocialHttpTarget, SocialReadKind,
    SocialThreadData, ThreadQuery, comment_action, create_comment, create_post, read_feed,
    read_thread, set_post_follow, set_reaction, set_topic_follow, social_router,
};
pub use crate::adapters::social_http_wire::{DecodedSocialCommand, decode_social_command};
