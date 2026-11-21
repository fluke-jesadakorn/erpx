//! Asset-owned port for Maintenance's retained retire-request predicate.
//! This port does not authorize an actor or select an Asset.
use crate::application::asset::AssetFuture;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// The composition adapter must bind this proof to the authorized Asset's
/// actual database ID, not a client-provided label. Its source owner owns the
/// predicate and same-unit source locks. Keep it through deferred effects.
pub trait AssetRetirementWorkEffect: Send {
    fn is_clear(&self) -> bool;
    fn require_current<'a>(&'a self, c: &'a mut PgConnection) -> AssetFuture<'a, (), Error>;
}
impl<T: AssetRetirementWorkEffect + ?Sized> AssetRetirementWorkEffect for Box<T> {
    fn is_clear(&self) -> bool {
        (**self).is_clear()
    }
    fn require_current<'a>(&'a self, c: &'a mut PgConnection) -> AssetFuture<'a, (), Error> {
        (**self).require_current(c)
    }
}
pub trait AssetRetirementWorkSource: Send {
    type Effect: AssetRetirementWorkEffect;
    fn observe<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        asset_id: &'a str,
    ) -> AssetFuture<'a, Self::Effect, Error>;
}
/// Called only after the real scoped Asset has been selected and actor/work
/// authorization checked. The returned proof must be rechecked immediately
/// after all late effects and before commit in the original transaction.
pub async fn prepare_asset_retirement_work<S: AssetRetirementWorkSource + ?Sized>(
    source: &mut S,
    c: &mut PgConnection,
    authorized_asset_id: &str,
) -> Result<S::Effect, Error> {
    let effect = source.observe(&mut *c, authorized_asset_id).await?;
    effect.require_current(c).await?;
    if !effect.is_clear() {
        return Err(BusinessError::new(FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
    }
    Ok(effect)
}
