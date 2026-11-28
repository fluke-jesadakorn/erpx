//! Native migration lane 34 for manage_erpx.
//!
//! The beta feedback list read moved from a TypeScript SELECT to this owner-local
//! axum router. The router owns the byte-identical sqlx query, the Company scope
//! and the response envelope; the TypeScript adapter only transports the result.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{
    BetaFeedbackQuery, EmailReceiptFuture, EmailReceiptQuery, FeedbackFuture, load,
    load_email_receipt,
};
pub use domain::{
    EMAIL_RECEIPT_SQL, FEEDBACK_SQL, EmailReceipt, EmailReceiptResponse, FeedbackItem,
    FeedbackPage, FeedbackResponse, MAX_FEEDBACK, configuration_invalid, invalid_request, missing,
    unavailable, valid_company_id, valid_delivery_key, valid_slug,
};
pub use http::{LaneTarget, router, router_for_holdings, router_with_reader};
pub use postgres::{PgBetaFeedbackQuery, PgEmailReceiptQuery};
