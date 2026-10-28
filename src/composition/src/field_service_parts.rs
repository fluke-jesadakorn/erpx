//! Actual owner ports assembled at Service. No listener, route, background
//! worker or independently committed business transaction is created here.
mod administration;
mod command;
mod finance;
mod inventory;
mod notifications;
mod preparation;
mod purchases;
mod source;
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Field Service owner, request or retained source changed.",
    )
}
pub use administration::{
    FieldServicePartsAdministration, FieldServicePartsAdministrationError,
    FieldServicePartsConfiguration, FieldServicePartsEnvironment,
};

/// Utility-only owner bundle BEFORE the normal Holding authorizer takes its
/// first snapshot. The caller retains that actual Control/Holding unit through
/// commit; none of these locks grants any record, stock, package or Finance right.
#[derive(Clone)]
pub struct FieldServicePartsSourcePreparation;
impl HoldingWorkSourcePreparation for FieldServicePartsSourcePreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::parts_draft::lock_owned_parts_draft_sources(c).await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_server_collaboration::public::field_service_notifications::lock_field_service_notification_sources(c).await?;
            erpx_server_support::public::field_service::lock_service_ticket_sources(c).await?;
            erpx_server_assets::public::field_service_reference::lock_field_service_asset_sources(
                c,
            )
            .await?;
            erpx_server_finance::public::field_service_customer::lock_field_service_customer_sources(c).await?;
            erpx_server_people_payroll::public::identity_facts::lock_people_authority_sources(c)
                .await?;
            erpx_server_identity_access::public::field_service_assignment::lock_field_service_assignment_sources(c).await?;
            erpx_server_field_service::public::parts_source::lock_field_service_order_sources(c)
                .await?;
            erpx_server_commerce::public::field_service_reservation::lock_field_service_stock_scope_sources(c).await?;
            erpx_server_quality::public::field_service_reservation::lock_field_service_reservation_holds(c).await?;
            erpx_server_inventory::public::postgres_consumption::lock_reserved_consumption_sources(
                c,
            )
            .await?;
            erpx_server_inventory::public::postgres_consumption::lock_inventory_consumption_sources(
                c,
                &mut inventory::FieldServiceFinanceFacts { holding: None },
            )
            .await
        })
    }
}

#[cfg(feature = "business-http")]
mod http_transport;

#[cfg(feature = "business-http")]
pub use administration::{HttpIngressError, OtherPost, OtherPostInput};
