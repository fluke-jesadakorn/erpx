//! Quick-access preference port. Pins are selectors only; current authorized
//! catalogue IDs must be supplied by the caller.
pub use crate::application::quick_access_preferences::{
    QuickAccessReceipt, QuickAccessRepository, request_hash, result_json,
};
use crate::application::quick_access_preferences::{
    read_quick_access_preferences, save_quick_access_preferences,
};
pub use crate::domain::quick_access_preferences::{
    MAX_ID_UNITS, MAX_PINS, MAX_REQUEST_KEY_UNITS, QuickAccessPreference, SaveQuickAccessInput,
    idempotency_conflict, shortcut_unavailable, validate, version_conflict,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

pub use crate::adapters::quick_access_preferences::PgQuickAccessRepository;

pub async fn read_quick_access(
    connection: &mut PgConnection,
    user_id: &str,
    surface: &str,
    context_key: &str,
) -> Result<QuickAccessPreference, PgTransactionError<BusinessError>> {
    read_quick_access_preferences(
        &mut PgQuickAccessRepository::new(connection),
        user_id,
        surface,
        context_key,
    )
    .await
}

pub async fn save_quick_access(
    connection: &mut PgConnection,
    input: &SaveQuickAccessInput,
    now: i64,
) -> Result<QuickAccessPreference, PgTransactionError<BusinessError>> {
    save_quick_access_preferences(&mut PgQuickAccessRepository::new(connection), input, now).await
}
