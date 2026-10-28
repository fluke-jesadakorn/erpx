//! Private, current-authority Service reservation readback.
use crate::field_service_parts::FieldServicePartsEnvironment;
use erpx_server_commerce::public::field_service_reservation::{
    FieldServiceStockScopeRequest, lock_field_service_stock_scope_sources,
    read_current_field_service_stock_scope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_field_service::public::reservation_read::{
    FieldServiceReservationRead, FieldServiceReserveEvent, FieldServiceReserveResult,
    FieldServiceStockSnapshot, lock_field_service_reservation_read_sources,
    read_field_service_reservation,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_inventory::public::field_service_stock_read::{
    lock_field_service_stock_read_sources, read_field_service_stock,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId,
};
use erpx_server_workflow::public::record_origin::lock_record_origin_sources;
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FIELD_SERVICE_CONFIGURATION_SCOPE_CHANGED",
        "The configured Field Service scope changed.",
    )
}
fn safe(value: i64) -> Result<i64, BusinessError> {
    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FIELD_SERVICE_READ_LIMIT",
            "The Service source exceeds this bounded read.",
        ));
    }
    Ok(value)
}
struct ReplyBudget(usize);
impl std::io::Write for ReplyBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or_else(|| std::io::Error::other("bounded Service reply"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRenderedContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub assignment_version: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub revision: i64,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=0,maximum=9_007_199_254_740_991_i64))]
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReservationReadResponse {
    pub context: FieldServiceRenderedContext,
    pub order: FieldServiceReserveResult,
    pub receipt: FieldServiceReserveResult,
    pub event: FieldServiceReserveEvent,
    pub reservation: FieldServiceReservationRead,
    pub stock: FieldServiceStockSnapshot,
}
fn context(current: &AuthorizedWorkContext) -> Result<FieldServiceRenderedContext, BusinessError> {
    let selected = current.selection();
    let scope = selected.workspace();
    let grant = selected.assignment();
    Ok(FieldServiceRenderedContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: scope.company_id.as_str().into(),
        branch_id: scope.branch_id.as_ref().map(|v| v.as_str().into()),
        department_id: scope.department_id.as_ref().map(|v| v.as_str().into()),
        branch_slug: scope.branch_slug.clone(),
        department_slug: scope.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: safe(grant.version)?,
        revision: safe(selected.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: safe(current.expires_at())?,
    })
}
#[derive(Clone)]
struct ReadSources;
impl HoldingWorkSourcePreparation for ReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_record_origin_sources(c).await?;
            lock_field_service_reservation_read_sources(c).await?;
            lock_field_service_stock_scope_sources(c).await?;
            lock_field_service_stock_read_sources(c).await
        })
    }
}
#[derive(Clone)]
pub struct FieldServiceReservationReadConfiguration {
    holding: HoldingId,
    company: CompanyId,
    catalogue: Arc<CommercialCatalogue>,
}
impl FieldServiceReservationReadConfiguration {
    pub fn new(
        environment: FieldServicePartsEnvironment,
        holding: HoldingId,
        company: CompanyId,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FieldServicePartsEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FIELD_SERVICE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Field Service production release is not available.",
            ));
        }
        Ok(Self {
            holding,
            company,
            catalogue,
        })
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}
pub struct FieldServiceReservationReadService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    config: FieldServiceReservationReadConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FieldServiceReservationReadService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        config: FieldServiceReservationReadConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            config,
            clock,
        }
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.config.company
    }
    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order: String,
        reservation: String,
    ) -> Result<FieldServiceReservationReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.config.company.clone();
        let input = move || {
            let mut selection = input()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = false;
            Ok(selection)
        };
        let config = self.config.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "service/orders".into(),
                    api: true,
                },
                ReadSources,
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let order = order.clone();
                    let reservation = reservation.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(denied().into());
                        }
                        current.require_permission("records.read")?;
                        let purchase = PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec![
                                    "service.orders".into(),
                                    "service.parts-assets".into(),
                                ],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        purchase.require_allowed()?;
                        let source = read_field_service_reservation(
                            c,
                            current,
                            order.as_str(),
                            reservation.as_str(),
                        )
                        .await?;
                        let part = source.reservation();
                        let stock_scope = read_current_field_service_stock_scope(
                            c,
                            FieldServiceStockScopeRequest {
                                company_id: config.company.clone(),
                                branch_id: RecordId::new(source.branch_id())?,
                                warehouse_id: part.warehouse_id.clone(),
                                product_id: part.product_id.clone(),
                            },
                        )
                        .await?
                        .ok_or_else(denied)?;
                        let stock =
                            read_field_service_stock(c, &part.warehouse_id, &part.product_id)
                                .await?
                                .ok_or_else(denied)?;
                        let s = stock.data();
                        let reply = FieldServiceReservationReadResponse {
                            context: context(current)?,
                            order: source.order().clone(),
                            receipt: source.receipt().clone(),
                            event: source.event(),
                            reservation: part.clone(),
                            stock: FieldServiceStockSnapshot {
                                warehouse_id: s.warehouse_id.clone(),
                                product_id: s.product_id.clone(),
                                quantity_milli: s.quantity_milli,
                                version: s.version,
                            },
                        };
                        let mut reply_budget = ReplyBudget(0);
                        serde_json::to_writer(&mut reply_budget, &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FIELD_SERVICE_READ_LIMIT",
                                "The Service source exceeds this bounded read.",
                            )
                        })?;
                        source
                            .require_current(
                                c,
                                current,
                                (2 * 1024 * 1024_usize).saturating_sub(reply_budget.0),
                            )
                            .await?;
                        stock_scope.require_current(c).await?;
                        stock.require_current(c).await?;
                        PgCompanyPurchaseReader::new(
                            config.holding.clone(),
                            config.company.clone(),
                            c,
                            &config.catalogue,
                        )
                        .revalidate(&purchase.witness, clock.now())
                        .await?
                        .require_allowed()?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(denied().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
    }
}
