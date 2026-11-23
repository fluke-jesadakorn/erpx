//! Native Records `request.ai.prepare` drain port.
//!
//! The composition root calls [`drain_request_ai`] on the selected Holding
//! writer inside the single serializable drain unit. Authority is rechecked
//! in Records scope (live assignment grant plus the requester's/assignee's
//! own request row and version); the caller owns the commit/rollback and
//! never retries inside this port. Use the two
//! `erpx_server_workflow::public::request_ai_effects` clears through this
//! drain (they run in the same unit on success).
pub use crate::adapters::request_ai_drain_postgres::{ClaimedProposal, RequestSnapshot};
pub use crate::adapters::request_ai_drain_worker::run_drain_job;
pub use crate::application::request_ai_drain::{
    DrainJobReport, DrainOutcome, ProviderFuture, ProviderOutput, RequestAiProvider,
    drain_request_ai, should_retry,
};
pub use crate::domain::request_ai_drain::{
    DRAIN_LIMIT, LEASE_WINDOW_MS, MAX_REQUEST_ID, is_stale_code, lease_token, request_filter_invalid,
    result_command, run_auth_key, transaction_required, validate_request_filter, validate_result_json,
};
