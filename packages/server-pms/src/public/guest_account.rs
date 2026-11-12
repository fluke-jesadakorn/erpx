//! Native PMS guest-account/charge ports and the PostgreSQL owner adapter.
//! Composition supplies the current Account/assignment authority and the
//! serializable unit; this module mints no authority. Journal posting,
//! cash/card settlement, refunds, invoice/credit-note issuance and stock stay
//! with their own owner/provider ports.
pub use crate::adapters::guest_account_http::{
    PmsGuestAccountHttpBackend, PmsGuestAccountHttpCommandRequest, PmsGuestAccountHttpContext,
    PmsGuestAccountHttpFuture, PmsGuestAccountHttpResponse, PmsGuestAccountHttpTarget,
    pms_guest_account_router,
};
pub use crate::adapters::guest_account_postgres::{
    PgPmsGuestAccountRepository, lock_guest_account_sources,
};
pub use crate::application::guest_account::{
    PmsGuestAccountRepository, guest_account_request_hash, pms_guest_account,
};
pub use crate::application::room_board::PmsBoardFuture;
pub use crate::domain::guest_account::{
    MAX_ID, MAX_MONEY, MAX_REFERENCES, MAX_REFUNDS, MAX_SOURCE_KEY, MAX_TEXT, PmsAdjustmentInput,
    PmsChargeAddInput, PmsChargeImportInput, PmsGuestAccountActor, PmsGuestAccountCommand,
    PmsGuestAccountFacts, PmsGuestAccountInput, PmsGuestAccountOpenInput, PmsGuestAccountOperation,
    PmsGuestAccountPlan, PmsGuestAccountResult, PmsGuestAccountUpdateInput, PmsTransferInput,
    invalid as guest_account_invalid, payment_port_required, plan_guest_account, price_amounts,
    read_limit as guest_account_read_limit, record_invalid as guest_account_record_invalid,
};
