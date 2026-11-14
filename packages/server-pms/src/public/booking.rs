//! Native PMS public-booking link and private-mail ports.
//!
//! Composition supplies the current Account session/purchase authority, the
//! runtime secret, the public origin, the provider transport, the at-rest mail
//! cipher and the caller-owned PostgreSQL unit. This module mints no authority,
//! no provider credential, no payment result and no finance posting.
//!
//! Still owned by missing native producers (not part of this slice): public
//! quote pricing/offers, reservation creation for the website, Stripe payment
//! settlement/allocation, refunds and change settlement.
pub use crate::adapters::booking_postgres::{
    PgPmsBookingMailRepository, drain_booking_mail_turn, queue_booking_mail_on_postgres,
};
pub use crate::application::booking::{
    PmsBookingDeliveryReport, PmsBookingMailCipher, PmsBookingMailConfig, PmsBookingMailProvider,
    PmsBookingMailRepository, PmsBookingPurchaseGuard, authorize_booking_link,
    booking_mail_body, booking_mail_idempotency_key, booking_management_summary,
    booking_rate_limit, deliver_booking_emails, queue_booking_mail,
};
pub use crate::application::room_board::PmsBoardFuture;
pub use crate::domain::booking::{
    BLOCKED_MAIL_CODES, DEFAULT_RATE_LIMIT, DELIVERY_BATCH, MAIL_BLOCK_RETRY_MS, MAIL_LEASE_MS,
    MAIL_RETRY_CAP_MS, MAX_MAIL_ATTEMPTS, MAX_SUMMARY_ACCOUNTS, MAX_SUMMARY_ENTRIES,
    MAX_SUMMARY_ROOMS, MAX_TOKEN_LEN, PmsBookingCancellation, PmsBookingDeliveryOutcome,
    PmsBookingEmailClaim, PmsBookingEmailRow, PmsBookingGuest, PmsBookingHotel, PmsBookingLinkKind,
    PmsBookingMailFacts, PmsBookingMailKind, PmsBookingMailPayload, PmsBookingQueuedMail,
    PmsBookingSummary, PmsBookingSummaryRoom, PmsBookingTokenRef, PmsBookingTokenRow,
    RATE_LIMIT_CLEANUP_BUCKETS, RATE_WINDOW_MS, TOKEN_TTL_MS, blocked_mail_code,
    booking_link_token, booking_mail_payload, booking_mail_subject, booking_public_url,
    booking_summary, booking_token_hash, cancellation_eligibility, delivery_failure_outcome,
    delivery_retry_outcome, encode_uri_component, mail_lease_deadline, normalize_recovery_email,
    rate_limit_bucket, rate_limit_exceeded, sign_booking_value, split_booking_link,
    stale_rate_limit_bucket, validate_presented_token, verify_booking_link,
};
