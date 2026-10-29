//! Utility fences for the original Service first physical Count. These run
//! before the selected Holding authorizer establishes the first MVCC snapshot.
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
type FenceUnit = (String, i32, String, String);
type Error = erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>;
fn fence_error() -> Error {
    erpx_server_runtime::public::postgres::PersistenceError::new(
        "SERVICE_INITIAL_COUNT_FENCE_REQUIRED",
    )
    .into()
}
async fn fence_unit(c: &mut PgConnection) -> Result<FenceUnit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(fence_error());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
#[derive(Clone)]
pub struct ServiceInitialPhysicalStockSources {
    environment: ServiceInitialPhysicalStockEnvironment,
}
impl HoldingWorkSourcePreparation for ServiceInitialPhysicalStockSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_pms::public::initial_count_modules::lock_initial_count_module_sources(
                c,
                self.environment.mode,
            )
            .await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_server_workflow::public::service_initial_count_receipt::lock_service_initial_count_receipts(c).await?;
            erpx_server_workflow::public::service_count_decision_evidence::lock_service_count_decision_evidence(c).await?;
            erpx_server_inventory::public::service_initial_physical_count::lock_initial_count_inventory_sources(c).await?;
            erpx_server_quality::public::initial_count_history::lock_initial_count_quality_sources(
                c,
            )
            .await?;
            erpx_server_commerce::public::service_stock_masters::lock_service_stock_masters(c)
                .await?;
            erpx_server_commerce::public::service_stock_masters::lock_service_stock_mapping(c)
                .await?;
            erpx_server_finance::public::initial_count_finance::lock_initial_count_finance_sources(
                c,
            )
            .await?;
            erpx_server_workflow::public::stock_count_flow::lock_stock_count_flow_sources(c)
                .await?;
            erpx_server_identity_access::public::journal_flow_participants::lock_journal_flow_participant_sources(c).await?;
            erpx_server_people_payroll::public::journal_flow_people::lock_journal_flow_people_sources(c).await?;
            erpx_server_organization::public::journal_flow_department::lock_journal_flow_department_sources(c).await?;
            erpx_server_organization::public::field_service_workspace::lock_service_workspace_locations(c).await?;
            erpx_server_collaboration::public::service_count_flow_notification::lock_count_flow_notifications(c).await?;
            erpx_server_files::public::count_submission::lock_count_binding_sources(c).await?;
            // Only this environment instance can vouch for this exact unit.
            // Obtain an xid only after every owner utility lock has succeeded.
            let unit = fence_unit(c).await?;
            let mut fences = self.environment.fences.lock().map_err(|_| fence_error())?;
            if !fences.contains(&unit) {
                if fences.len() >= 4096 {
                    fences.pop_front();
                }
                fences.push_back(unit);
            }
            Ok(())
        })
    }
}

/// Trusted runtime release context. Never construct from HTTP request data.
#[derive(Clone)]
pub struct ServiceInitialPhysicalStockEnvironment {
    mode: erpx_server_pms::public::initial_count_modules::InitialCountModuleMode,
    artifact: String,
    engineering: Option<
        erpx_server_platform_administration::public::legal_release::EngineeringReadConfiguration,
    >,
    fences: Arc<Mutex<VecDeque<FenceUnit>>>,
}
impl ServiceInitialPhysicalStockEnvironment {
    /// Construct only from trusted runtime configuration; clones share fences.
    pub fn new(
        mode: erpx_server_pms::public::initial_count_modules::InitialCountModuleMode,
        artifact: String,
        engineering:Option<erpx_server_platform_administration::public::legal_release::EngineeringReadConfiguration>,
    ) -> Self {
        Self {
            mode,
            artifact,
            engineering,
            fences: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    pub fn sources(&self) -> ServiceInitialPhysicalStockSources {
        ServiceInitialPhysicalStockSources {
            environment: self.clone(),
        }
    }
    async fn require_fence(&self, c: &mut PgConnection) -> Result<(), Error> {
        let unit = fence_unit(c).await?;
        if !self
            .fences
            .lock()
            .map_err(|_| fence_error())?
            .contains(&unit)
        {
            return Err(fence_error());
        }
        Ok(())
    }
}
pub async fn require_count_modules(
    c: &mut PgConnection,
    current: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
    environment: &ServiceInitialPhysicalStockEnvironment,
) -> Result<
    erpx_server_pms::public::initial_count_modules::CurrentInitialCountModules,
    erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>,
> {
    environment.require_fence(c).await?;
    let witness = erpx_server_pms::public::initial_count_modules::read_initial_count_modules(
        c,
        &current.selection().workspace().company_id,
        environment.mode,
        &environment.artifact,
        environment.engineering.as_ref(),
    )
    .await?;
    if !witness.allowed() {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "FEATURE_COMING_SOON",
            "This workflow needs a separate pilot release.",
        )
        .into());
    }
    witness.require_current(c).await?;
    Ok(witness)
}
