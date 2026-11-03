//! Private, current-authority Inventory availability readback. The caller must
//! still hold the current Account session, rendered-context CAS and the
//! Inventory purchase; the projection itself only reads owner facts.
use crate::field_service_reservation_read::{FieldServiceRenderedContext, context};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_inventory::public::availability_read::{
    AvailabilityRow, lock_availability_sources, read_availability,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryAvailabilityResponse {
    pub data: Vec<AvailabilityRow>,
    pub context: FieldServiceRenderedContext,
}

#[derive(Clone)]
pub struct InventoryAvailabilityReadService<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> InventoryAvailabilityReadService<C> {
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
    ) -> Result<InventoryAvailabilityResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut selected = workspace()?;
                    selected.mutation = false;
                    if selected.company_id != expected_company {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "INVENTORY_AVAILABILITY_SCOPE_REQUIRED",
                            "Use the current Company.",
                        ));
                    }
                    Ok(selected)
                },
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::Exact("inventory.read".into()),
                    module: Some("inventory".into()),
                    destination: "inventory.availability.read".into(),
                    api: true,
                },
                InventoryAvailabilitySources,
                move |c, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_AVAILABILITY_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        let mut reader = PgCompanyPurchaseReader::new(
                            current.holding_id().clone(),
                            company.clone(),
                            c,
                            &catalogue,
                        );
                        reader
                            .inspect(
                                &PurchaseRequest {
                                    capabilities: vec!["inventory.stock-core".into()],
                                    purpose: CapabilityPurpose::History,
                                    mode: PackageEnforcementMode::Enforced,
                                },
                                now,
                            )
                            .await?
                            .require_allowed()?;
                        lock_availability_sources(c).await?;
                        let branch = current
                            .selection()
                            .workspace()
                            .branch_id
                            .as_ref()
                            .map(|value| value.as_str());
                        let data = read_availability(c, company.as_str(), branch).await?;
                        Ok(InventoryAvailabilityResponse {
                            data,
                            context: context(current)?,
                        })
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
struct InventoryAvailabilitySources;
impl HoldingWorkSourcePreparation for InventoryAvailabilitySources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_availability_sources(c).await?;
            Ok(())
        })
    }
}
