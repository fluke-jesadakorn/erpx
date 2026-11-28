//! Native owner slice for `GET|POST /api/v1/holdings/{holding_slug}/media-publications`.
//!
//! The axum router owns the bounded list/create semantics and the complete
//! payload. `domain` holds pure validation, `application` owns the create flow,
//! `postgres` executes the owner SQL and `http` is the transport. No TypeScript
//! business fallback and no caller-supplied SQL exist in this slice. The
//! TypeScript Next handler remains in place until the lead flips the gateway.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{
    create_publication, CreateRequest, PreparedCreate, PublicationFileStore, PublicationFuture,
    PublicationObject, PublicationStore, SourceLink,
};
pub use domain::{
    is_publishable_purpose, is_publishable_target, stable_id, CreatePublicationInput,
    PublicationList, PublicationRow,
};
pub use http::{
    media_publications_router, media_publications_router_for_holdings, MediaPublicationsTarget,
};
pub use postgres::{PgPublicationStore, LIST_APPROVED_SQL, LIST_MANAGE_SQL};
