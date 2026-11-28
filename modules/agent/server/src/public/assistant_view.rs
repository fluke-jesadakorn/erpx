//! Stable Agent assistant-view surface: query, PostgreSQL adapter and HTTP
//! ingress. Composition binds the backend to authority and the read-only unit.
pub use crate::adapters::assistant_view_http::{
    AssistantViewHttpBackend, AssistantViewHttpFuture, AssistantViewHttpRequest,
    AssistantViewHttpTarget, assistant_view_router_for_holdings,
};
pub use crate::adapters::assistant_view_postgres::PgAssistantView;
pub use crate::application::assistant_view::{
    AssistantGovernanceFacts, AssistantTurnRow, AssistantViewContext, AssistantViewFuture,
    AssistantViewPorts, assistant_view,
};
pub use crate::domain::assistant_view::{
    AssistantViewMode, AssistantViewRequest, canonical_page, conversation_key, page_guide,
    supported_root,
};
