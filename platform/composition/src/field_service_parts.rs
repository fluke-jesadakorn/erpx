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
use erpx_identity_access_server::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
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
            // Canonical global order: Commercial -> Finance -> Files. The
            // purchase authority fence precedes every Finance write-intent
            // source so period close and Field parts cannot deadlock.
            erpx_commercial_server::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_finance_server::public::parts_draft::lock_owned_parts_draft_sources(c).await?;
            erpx_finance_server::public::field_service_billing::lock_billing_sources(c).await?;
            erpx_collaboration_server::public::field_service_notifications::lock_field_service_notification_sources(c).await?;
            erpx_support_server::public::field_service::lock_service_ticket_sources(c).await?;
            erpx_assets_server::public::field_service_reference::lock_field_service_asset_sources(
                c,
            )
            .await?;
            erpx_finance_server::public::field_service_customer::lock_field_service_customer_sources(c).await?;
            erpx_people_payroll_server::public::identity_facts::lock_people_authority_sources(c)
                .await?;
            erpx_identity_access_server::public::field_service_assignment::lock_field_service_assignment_sources(c).await?;
            erpx_field_service_server::public::parts_source::lock_field_service_order_sources(c)
                .await?;
            erpx_commerce_server::public::field_service_reservation::lock_field_service_stock_scope_sources(c).await?;
            erpx_quality_server::public::field_service_reservation::lock_field_service_reservation_holds(c).await?;
            erpx_inventory_server::public::postgres_consumption::lock_reserved_consumption_sources(
                c,
            )
            .await?;
            erpx_inventory_server::public::postgres_consumption::lock_inventory_consumption_sources(
                c,
                &mut inventory::FieldServiceFinanceFacts { holding: None },
            )
            .await
        })
    }
}

#[cfg(feature = "field-service-http")]
mod http_transport;
#[cfg(feature = "field-service-http")]
pub(crate) use http_transport::HttpDispatch;

#[cfg(feature = "field-service-http")]
pub use administration::{HttpIngressError, OtherPost, OtherPostInput};

#[cfg(feature = "field-service-http")]
mod request;

#[cfg(feature = "field-service-http")]
pub use request::RequestDispatcher;

#[cfg(feature = "field-service-http")]
mod create_order;

#[cfg(feature = "field-service-http")]
pub use create_order::OrderCreateDispatcher;

#[cfg(feature = "field-service-http")]
mod visit;

#[cfg(feature = "field-service-http")]
pub use visit::VisitDispatcher;

#[cfg(feature = "field-service-http")]
mod agreement_review;
#[cfg(feature = "field-service-http")]
pub use agreement_review::AgreementReviewDispatcher;

#[cfg(feature = "field-service-http")]
mod agreement_create;
#[cfg(feature = "field-service-http")]
pub use agreement_create::AgreementCreateDispatcher;
