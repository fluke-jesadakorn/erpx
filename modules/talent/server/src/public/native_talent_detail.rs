//! Stable owner surface for the native Talent application-detail read.
//!
//! Domain facts, the read-only query/handler port, the real sqlx reader and
//! the axum router constructor. Composition mounts the router with an owner
//! pool; nothing here prepares SQL in TypeScript or falls back to a TS reader.

pub use crate::adapters::native_talent_detail::{
    PgTalentDetailRead, TalentDetailTarget, router, router_for_holdings, router_with_reader,
};
pub use crate::application::native_talent_detail::{
    TalentDetailHandler, TalentDetailQuery, TalentDetailRead, TalentDetailReadFuture,
};
pub use crate::domain::native_talent_detail::{
    DETAIL_APPLICATION_SQL, DETAIL_HANDOFF_SQL, DETAIL_HISTORY_SQL, DETAIL_OFFER_SQL,
    DETAIL_PROPOSAL_SQL, MAX_IDENTIFIER, TalentDetail, application_not_found, invalid,
    validate_id, validate_optional_scope,
};
