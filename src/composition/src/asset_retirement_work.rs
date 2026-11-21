//! Bind the Maintenance retirement predicate to the selected Asset source.
//! The caller must separately retain actor authorization and purchase guards.
use erpx_server_assets::public::asset_purchase_source::CurrentAssetPurchaseSource;
use erpx_server_maintenance::public::{
    asset_purchase_locks::MaintenanceAssetPurchaseLocks,
    asset_purchase_record::{CurrentAssetRetirementWork, observe_asset_retirement_work},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

pub struct CurrentAssetRetirementGuard<'a> {
    work: CurrentAssetRetirementWork<'a>,
}
impl CurrentAssetRetirementGuard<'_> {
    /// The Asset is intentionally mutated; separately retain its post-write image.
    pub async fn require_current_after_effects(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.work.require_current(c).await
    }
}
/// Use the retained scoped target selector, matching the legacy work predicate.
/// This selector can differ from the persisted id under a nondeterministic collation.
/// Capture before the retire request and retain until deferred checks complete.
pub async fn inspect_asset_retirement_guard<'a>(
    c: &mut PgConnection,
    asset: &'a CurrentAssetPurchaseSource,
    locks: &'a MaintenanceAssetPurchaseLocks,
) -> Result<CurrentAssetRetirementGuard<'a>, Error> {
    asset.require_current(&mut *c).await?;
    if !asset.exists() {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "ASSETS_SOURCE_NOT_FOUND",
            "The Asset source was not found in this workspace.",
        )
        .into());
    }
    let work = observe_asset_retirement_work(&mut *c, locks, asset.source_id()).await?;
    work.require_current(&mut *c).await?;
    if !work.is_clear() {
        return Err(BusinessError::new(FailureKind::Conflict, "CONFLICT", "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    asset.require_current(c).await?;
    Ok(CurrentAssetRetirementGuard { work })
}
