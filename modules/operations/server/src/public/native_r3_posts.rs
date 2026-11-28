//! Public surface for the native L06 post sub-resource routes.
//!
//! Composition mounts [`router_for_holdings`] and supplies the `TaskCreator`
//! and `AiProvider` ports. TypeScript consumers keep strict transports; the
//! retired post sub-resource SQL stays owned by this crate.

pub use crate::adapters::native_r3_posts::http::{
    PostsTarget, configured, router_for_holdings,
};
pub use crate::adapters::native_r3_posts::postgres::{PgPosts, renumber};
pub use crate::application::native_r3_posts::{
    AiDraftHandler, AiDraftPort, AiOutput, AiProvider, AiProvider as Provider,
    CommentFacts, PriorDraft, TaskCommandHandler, TaskCommandPort, TaskCreator, TaskFacts,
    TaskSourceHandler, TaskSourcePort,
};
pub use crate::domain::native_r3_posts::{
    AiDraftView, PostTaskResult, SourceItem, TaskSourceView,
};
