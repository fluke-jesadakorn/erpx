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
use erpx_server_inventory::public::movement_read::{
    MovementRow, lock_movement_sources, read_movements,
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
    pub currency: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMovementResponse {
    pub data: Vec<MovementRow>,
    pub context: FieldServiceRenderedContext,
    pub next_cursor: Option<String>,
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
                        let chart: Option<String> = sqlx::query_scalar(
                            "SELECT base_currency FROM finance_charts WHERE company_id=$1 AND status='active' ORDER BY id LIMIT 1",
                        )
                        .bind(company.as_str())
                        .fetch_optional(&mut *c)
                        .await?;
                        let currency = match chart {
                            Some(value) => value,
                            None => sqlx::query_scalar(
                                "SELECT default_currency FROM company_money_settings WHERE company_id=$1",
                            )
                            .bind(company.as_str())
                            .fetch_optional(&mut *c)
                            .await?
                            .ok_or_else(|| BusinessError::new(
                                FailureKind::Unavailable,
                                "INVENTORY_AVAILABILITY_CURRENCY_REQUIRED",
                                "The Inventory valuation currency is unavailable.",
                            ))?,
                        };
                        Ok(InventoryAvailabilityResponse {
                            data,
                            context: context(current)?,
                            currency,
                        })
                    })
                },
            )
            .await
    }

    pub async fn read_movements<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        cursor: Option<String>,
    ) -> Result<InventoryMovementResponse, Error>
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
                            "INVENTORY_MOVEMENT_SCOPE_REQUIRED",
                            "Use the current Company.",
                        ));
                    }
                    Ok(selected)
                },
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::Exact("inventory.read".into()),
                    module: Some("inventory".into()),
                    destination: "inventory.movements.read".into(),
                    api: true,
                },
                InventoryMovementSources,
                move |c, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    let cursor = cursor.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "INVENTORY_MOVEMENT_SCOPE_REQUIRED",
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
                        lock_movement_sources(c).await?;
                        let branch = current
                            .selection()
                            .workspace()
                            .branch_id
                            .as_ref()
                            .map(|value| value.as_str());
                        let (data, next_cursor) =
                            read_movements(c, company.as_str(), branch, cursor.as_deref()).await?;
                        Ok(InventoryMovementResponse {
                            data,
                            context: context(current)?,
                            next_cursor,
                        })
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
struct InventoryMovementSources;
impl HoldingWorkSourcePreparation for InventoryMovementSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_movement_sources(c).await?;
            Ok(())
        })
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
