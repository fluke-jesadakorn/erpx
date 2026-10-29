//! Unmounted Service initial physical Count coordinator. Each entrypoint must
//! run inside the genuine selected Holding authorizer unit, using these source
//! fences before its first snapshot. This facade grants no actor authority.
pub use crate::service_initial_physical_stock_action::{
    ServiceCountActionCommand, ServiceCountAssignmentCommand, assign_service_count_flow,
    execute_service_count_action,
};
pub use crate::service_initial_physical_stock_command::{
    InitialCountActionInput, InitialCountCreate, normalize_initial_count_action,
    normalize_initial_count_create,
};
pub use crate::service_initial_physical_stock_create::create_service_initial_count;
pub use crate::service_initial_physical_stock_read::{
    InitialCountSelection, InitialCountWorkspace, read_initial_count_selection,
};
pub use crate::service_initial_physical_stock_sources::{
    ServiceInitialPhysicalStockEnvironment, ServiceInitialPhysicalStockSources,
};
pub use erpx_server_files::public::count_submission::CountBindingFeatures;
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    service_initial_count_authority::ServiceCountAuthorityPurpose,
};
pub use erpx_server_inventory::public::service_initial_physical_count::CountAction;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock};
pub use erpx_server_workflow::public::stock_count_flow::normalize_stock_count_assignment;
use sqlx::PgConnection;
/// Read the witnessed original record and its current user-facing action flags.
pub async fn read_service_initial_count<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    id: &str,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    let source = crate::service_initial_physical_stock_record::read_service_count_record(
        c,
        current,
        clock,
        id,
        ServiceCountAuthorityPurpose::Read,
    )
    .await?;
    let result = source.wire(current)?;
    source.require_current(c, clock).await?;
    source.flow.require_current(c, current, clock).await?;
    module.require_current(c).await?;
    Ok(result)
}
/// Read the current witnessed Flow, including its saved assignments and history.
pub async fn read_service_initial_count_flow<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    id: &str,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let module = crate::service_initial_physical_stock_sources::require_count_modules(
        c,
        current,
        environment,
    )
    .await?;
    let source = crate::service_initial_physical_stock_record::read_service_count_record(
        c,
        current,
        clock,
        id,
        ServiceCountAuthorityPurpose::Read,
    )
    .await?;
    let result = source.flow.wire()?;
    source.require_current(c, clock).await?;
    source.flow.require_current(c, current, clock).await?;
    module.require_current(c).await?;
    Ok(result)
}
