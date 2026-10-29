//! Explicit native Account mail worker. Provider construction never selects an
//! ambient credential or a successful default delivery adapter.
pub use crate::adapters::account_mail_postgres::PostgresAccountMail;
#[cfg(feature = "mail-resend")]
pub use crate::adapters::account_mail_transport::ResendAccountMail;
#[cfg(feature = "mail-smtp")]
pub use crate::adapters::account_mail_transport::{AccountSmtpTls, SmtpAccountMail};
pub use crate::application::account_mail::{
    AccountMailAttempt, AccountMailClaim, AccountMailDispatch, AccountMailOpening,
    AccountMailProvider, AccountMailRepository, AccountMailSendFuture, AccountMailWorker,
};
pub use crate::domain::account_mail::{
    ACCOUNT_MAIL_LEASE_MS, ACCOUNT_MAIL_MAX_ATTEMPTS, ACCOUNT_MAIL_PROVIDER_TIMEOUT_MS,
    AccountEmailMessage, AccountMailFailure, AccountMailPolicy, AccountMailProviderKind,
    AccountMailProviderProfile, AccountMailSendOutcome, CurrentAccountMailProof,
    RESEND_IDEMPOTENCY_WINDOW_MS, account_mail_can_retry_unknown, account_mail_next_attempt,
    account_mail_proof_available,
};
