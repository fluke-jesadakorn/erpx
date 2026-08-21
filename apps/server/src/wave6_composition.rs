//! Runtime composition for the Wave 6 paired holding bounded contexts.
//!
//! The inbound gRPC adapters remain storage agnostic. This module is the sole
//! bridge from authenticated server state and holding routing to the context
//! command/query handlers.

use std::{env, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use erpx_assets as assets;
use erpx_assets_sqlx as assets_sqlx;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_cqrs_sqlx::{ReadPool, WritePool};
use erpx_crm as crm;
use erpx_crm_sqlx as crm_sqlx;
use erpx_infrastructure::{
    HoldingDatabaseRouter, HoldingHoldingBinding, HoldingProjectionSchedulerRepository,
    ProjectionJobKind, ProjectionPolicy, ProjectionScheduleRequest,
    ProjectionSchedulerAuthorization, RoutedDatabaseSide, SqlxControlPlane,
    SqlxHoldingStoreRegistry,
};
use erpx_inventory as inventory;
use erpx_inventory_sqlx as inventory_sqlx;
use erpx_kernel::{
    ActorAttribution, AuthorityScope, CorrelationId, ErrorCategory, ErrorContext, KernelError,
    SystemClock,
};
use erpx_maintenance as maintenance;
use erpx_maintenance_sqlx as maintenance_sqlx;
use erpx_marketing as marketing;
use erpx_marketing_sqlx as marketing_sqlx;
use erpx_procurement as procurement;
use erpx_procurement_sqlx as procurement_sqlx;
use tonic::Status;
use uuid::Uuid;

use crate::AppState;
use crate::contexts::{
    assets_cqrs::{
        AssetsAccessMode, AssetsAuthenticationError, AssetsAuthenticationRequest,
        AssetsCommandApplication, AssetsHoldingRoute, AssetsQueryApplication,
        AssetsRequestAuthenticator, AuthenticatedAssetsRequest,
    },
    crm_cqrs::{
        AuthenticatedCrmRequest, CrmAccessMode, CrmAuthenticationError, CrmAuthenticationRequest,
        CrmCommandApplication, CrmHoldingRoute, CrmQueryApplication, CrmRequestAuthenticator,
    },
    maintenance_cqrs::{
        AuthenticatedMaintenanceRequest, MaintenanceAccessMode, MaintenanceAuthenticationError,
        MaintenanceAuthenticationRequest, MaintenanceCommandApplication, MaintenanceHoldingRoute,
        MaintenanceQueryApplication, MaintenanceRequestAuthenticator,
    },
    marketing_cqrs::{
        AuthenticatedMarketingRequest, MarketingAccessMode, MarketingAuthenticationError,
        MarketingAuthenticationRequest, MarketingCommandApplication, MarketingHoldingRoute,
        MarketingQueryApplication, MarketingRequestAuthenticator,
    },
    procurement_inventory_cqrs::{
        AuthenticatedSupplyChainRequest, GetGoodsReceipt, GetPurchaseRequisition,
        InventoryCommandApplication, InventoryQueryApplication, ProcurementCommandApplication,
        ProcurementQueryApplication, SupplyChainAccessMode, SupplyChainAuthenticationError,
        SupplyChainAuthenticationRequest, SupplyChainHoldingRoute, SupplyChainRequestAuthenticator,
    },
};
use crate::holding_workspace::{PresentedHoldingRoute, resolve_holding_workspace};
use crate::wave4_composition::{authenticate_context, authenticate_context_for_holding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

enum ProcurementWorkflowCommand {
    CreateRequisition(procurement::CreatePurchaseRequisition),
    SubmitRequisition(procurement::SubmitPurchaseRequisition),
    ApproveRequisition(procurement::ApprovePurchaseRequisition),
    RejectRequisition(procurement::RejectPurchaseRequisition),
    CancelRequisition(procurement::CancelPurchaseRequisition),
    CreateGoodsReceipt(procurement::CreateGoodsReceipt),
    AcceptGoodsReceipt(procurement::AcceptGoodsReceipt),
    RejectGoodsReceipt(procurement::RejectGoodsReceipt),
    CancelGoodsReceipt(procurement::CancelGoodsReceipt),
}

#[derive(Debug, Clone)]
struct PreauthorizedAccess {
    principal_ref: String,
    authority_scope: AuthorityScope,
    mode: AccessMode,
}

impl PreauthorizedAccess {
    fn permits(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        write: bool,
    ) -> bool {
        actor.principal_ref == self.principal_ref
            && authority_scope == &self.authority_scope
            && self.authority_scope.contains(target_scope)
            && write == (self.mode == AccessMode::Write)
    }

    fn entitled(&self, scope: &AuthorityScope, write: bool) -> bool {
        self.authority_scope.contains(scope) && write == (self.mode == AccessMode::Write)
    }
}

macro_rules! access_ports {
    (
        $module:ident,
        $action_write:expr,
        $entitlement_method:ident
    ) => {
        #[async_trait]
        impl $module::AuthorizationPort for PreauthorizedAccess {
            async fn authorize(
                &self,
                request: $module::ports::AuthorizationRequest<'_>,
            ) -> Result<(), $module::PortError> {
                if !self.permits(
                    request.actor,
                    request.authority_scope,
                    request.target_scope,
                    ($action_write)(request.action),
                ) {
                    return Err($module::PortError::Denied);
                }
                Ok(())
            }
        }

        #[async_trait]
        impl $module::EntitlementPort for PreauthorizedAccess {
            async fn $entitlement_method(
                &self,
                scope: &AuthorityScope,
                write: bool,
            ) -> Result<(), $module::PortError> {
                if !self.entitled(scope, write) {
                    return Err($module::PortError::EntitlementDenied);
                }
                Ok(())
            }
        }
    };
}

access_ports!(
    marketing,
    |action: marketing::MarketingAction| action.write(),
    ensure_marketing_enabled
);
access_ports!(
    crm,
    |action: crm::CrmAction| action.write(),
    ensure_crm_enabled
);
access_ports!(
    procurement,
    |action: procurement::ProcurementAction| action.requires_write_entitlement(),
    ensure_procurement_enabled
);

#[async_trait]
impl procurement::ProcurementWorkflowAuthorizationPort for PreauthorizedAccess {
    async fn authorize_procurement_workflow(
        &self,
        request: procurement::ProcurementWorkflowAuthorizationRequest<'_>,
    ) -> Result<(), procurement::PortError> {
        let write = !matches!(
            request.action,
            procurement::ProcurementWorkflowAction::ViewRequisition
                | procurement::ProcurementWorkflowAction::ViewGoodsReceipt
        );
        if !self.permits(
            request.actor,
            request.authority_scope,
            request.target_scope,
            write,
        ) {
            return Err(procurement::PortError::Denied);
        }
        Ok(())
    }
}
access_ports!(
    inventory,
    |action: inventory::InventoryAction| action.requires_write_entitlement(),
    ensure_inventory_enabled
);
access_ports!(
    assets,
    |action: assets::AssetAction| action.requires_write_entitlement(),
    ensure_assets_enabled
);
access_ports!(
    maintenance,
    |action: maintenance::MaintenanceAction| action.requires_write_entitlement(),
    ensure_maintenance_enabled
);

#[derive(Clone)]
pub(crate) struct Wave6HoldingApplication {
    router: Option<Arc<HoldingDatabaseRouter>>,
    control: Arc<SqlxControlPlane>,
    projection_scheduler_actor: Option<String>,
}

impl Wave6HoldingApplication {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            router: state.holding_router.clone(),
            control: state.store.clone(),
            projection_scheduler_actor: env::var("ERPX_HOLDING_PROJECTION_SCHEDULER_ACTOR_REF")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    async fn schedule_incremental_projection(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        projection_name: &str,
        command_id: uuid::Uuid,
        correlation_id: uuid::Uuid,
    ) {
        let Some(actor_ref) = self.projection_scheduler_actor.as_deref() else {
            tracing::warn!(
                projection_name,
                "holding projection scheduler actor is not configured"
            );
            return;
        };
        let scheduled = async {
            let binding = HoldingHoldingBinding::new(holding_id, data_profile_key)?;
            let authorization = ProjectionSchedulerAuthorization::new(
                actor_ref,
                "project typed CQRS command receipt",
                correlation_id,
            )?;
            let scheduler = HoldingProjectionSchedulerRepository::new(
                self.control.pools().write.clone(),
                Arc::new(SqlxHoldingStoreRegistry::new(self.control.clone())),
                authorization,
            );
            let now = Utc::now();
            scheduler
                .schedule(
                    ProjectionScheduleRequest {
                        binding,
                        projection_name: projection_name.into(),
                        job_key: format!("cqrs-command:{command_id}"),
                        kind: ProjectionJobKind::Incremental,
                        available_at: now,
                        policy: ProjectionPolicy::new(500, 1, 10, 1, 60)?,
                        payload: serde_json::json!({}),
                    },
                    now,
                )
                .await?;
            Ok::<(), erpx_infrastructure::HoldingProjectionSchedulerError>(())
        }
        .await;
        if let Err(error) = scheduled {
            tracing::warn!(
                projection_name,
                ?error,
                "typed command projection could not be scheduled"
            );
        }
    }

    async fn write_pool(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<WritePool, CqrsError> {
        let pool = self
            .route_pool(
                holding_id,
                data_profile_key,
                RoutedDatabaseSide::Write,
                correlation_id,
                context,
            )
            .await?;
        WritePool::for_holding(
            pool,
            cqrs_holding_uuid(holding_id, correlation_id, context)?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, context))
    }

    async fn read_pool(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<ReadPool, CqrsError> {
        let pool = self
            .route_pool(
                holding_id,
                data_profile_key,
                RoutedDatabaseSide::Read,
                correlation_id,
                context,
            )
            .await?;
        ReadPool::for_holding(
            pool,
            cqrs_holding_uuid(holding_id, correlation_id, context)?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, context))
    }

    async fn route_pool(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        side: RoutedDatabaseSide,
        correlation_id: CorrelationId,
        context: &'static str,
    ) -> Result<sqlx::PgPool, CqrsError> {
        let router = self.router.as_ref().ok_or_else(|| {
            context_failure(
                correlation_id,
                format!("{context}.routing_unavailable"),
                ErrorCategory::Unavailable,
                "Holding database routing is unavailable.",
                true,
            )
        })?;
        let routed = router
            .route(holding_id, data_profile_key, side)
            .await
            .map_err(|_| {
                context_failure(
                    correlation_id,
                    format!("{context}.storage_unavailable"),
                    ErrorCategory::Unavailable,
                    "Context storage is temporarily unavailable.",
                    true,
                )
            })?;
        Ok(routed.database_pool().clone())
    }

    fn access(
        envelope_actor: &ActorAttribution,
        scope: &AuthorityScope,
        mode: AccessMode,
    ) -> PreauthorizedAccess {
        PreauthorizedAccess {
            principal_ref: envelope_actor.principal_ref.clone(),
            authority_scope: scope.clone(),
            mode,
        }
    }

    async fn execute_procurement_workflow(
        &self,
        route: SupplyChainHoldingRoute,
        context: procurement::ProcurementWorkflowCommandContext,
        command: ProcurementWorkflowCommand,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        let command_id = context.command_id.as_uuid();
        let correlation_id = context.correlation_id;
        let pool = self
            .write_pool(
                &route.holding_id,
                &route.data_profile_key,
                correlation_id,
                "procurement",
            )
            .await?;
        let access = Self::access(&context.actor, &context.authority_scope, AccessMode::Write);
        let adapter = procurement_sqlx::ProcurementWorkflowSqlxAdapter::new(pool);
        let service = procurement::ProcurementWorkflowCommandService::new(
            adapter.clone(),
            adapter,
            access,
            SystemClock,
        );
        let stored = match command {
            ProcurementWorkflowCommand::CreateRequisition(command) => {
                service.create_requisition(context, command).await
            }
            ProcurementWorkflowCommand::SubmitRequisition(command) => {
                service.submit_requisition(context, command).await
            }
            ProcurementWorkflowCommand::ApproveRequisition(command) => {
                service.approve_requisition(context, command).await
            }
            ProcurementWorkflowCommand::RejectRequisition(command) => {
                service.reject_requisition(context, command).await
            }
            ProcurementWorkflowCommand::CancelRequisition(command) => {
                service.cancel_requisition(context, command).await
            }
            ProcurementWorkflowCommand::CreateGoodsReceipt(command) => {
                service.create_goods_receipt(context, command).await
            }
            ProcurementWorkflowCommand::AcceptGoodsReceipt(command) => {
                service.accept_goods_receipt(context, command).await
            }
            ProcurementWorkflowCommand::RejectGoodsReceipt(command) => {
                service.reject_goods_receipt(context, command).await
            }
            ProcurementWorkflowCommand::CancelGoodsReceipt(command) => {
                service.cancel_goods_receipt(context, command).await
            }
        }
        .map_err(|error| procurement_workflow_failure(error, correlation_id))?;
        self.schedule_incremental_projection(
            &route.holding_id,
            &route.data_profile_key,
            procurement_sqlx::WORKFLOW_PROJECTION_NAME,
            command_id,
            correlation_id.as_uuid(),
        )
        .await;
        Ok(stored)
    }
}

#[async_trait]
impl MarketingCommandApplication for Wave6HoldingApplication {
    async fn execute(
        &self,
        route: MarketingHoldingRoute,
        envelope: CommandEnvelope<marketing::MarketingCommand>,
    ) -> Result<CommandReceipt<marketing::MarketingMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.holding_id,
                &route.data_profile_key,
                envelope.correlation_id,
                "marketing",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Write);
        marketing::MarketingCommandHandler::new(
            marketing_sqlx::MarketingWriteRepository::new(pool.clone()),
            marketing_sqlx::MarketingCommandAdapter::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl MarketingQueryApplication for Wave6HoldingApplication {
    async fn execute(
        &self,
        route: MarketingHoldingRoute,
        envelope: QueryEnvelope<marketing::MarketingQuery>,
    ) -> Result<QueryResult<marketing::MarketingQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.holding_id,
                &route.data_profile_key,
                envelope.correlation_id,
                "marketing",
            )
            .await?;
        let read = marketing_sqlx::MarketingReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "marketing"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        marketing::MarketingQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

#[async_trait]
impl CrmCommandApplication for Wave6HoldingApplication {
    async fn execute(
        &self,
        route: CrmHoldingRoute,
        envelope: CommandEnvelope<crm::CrmCommand>,
    ) -> Result<CommandReceipt<crm::CrmMutationResult>, CqrsError> {
        let pool = self
            .write_pool(
                &route.holding_id,
                &route.data_profile_key,
                envelope.correlation_id,
                "crm",
            )
            .await?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Write);
        crm::CrmCommandHandler::new(
            crm_sqlx::CrmWriteRepository::new(pool.clone()),
            crm_sqlx::CrmCommandAdapter::new(pool.clone()),
            access.clone(),
            access,
            SystemClock,
        )
        .with_qualified_lead_resolver(crm_sqlx::SqlxQualifiedLeadResolver::new(pool))
        .handle(envelope)
        .await
    }
}

#[async_trait]
impl CrmQueryApplication for Wave6HoldingApplication {
    async fn execute(
        &self,
        route: CrmHoldingRoute,
        envelope: QueryEnvelope<crm::CrmQuery>,
    ) -> Result<QueryResult<crm::CrmQueryResponse>, CqrsError> {
        let pool = self
            .read_pool(
                &route.holding_id,
                &route.data_profile_key,
                envelope.correlation_id,
                "crm",
            )
            .await?;
        let read = crm_sqlx::CrmReadRepository::new(pool)
            .map_err(|_| storage_configuration_error(envelope.correlation_id, "crm"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        crm::CrmQueryHandler::new(read, access.clone(), access)
            .handle(envelope)
            .await
    }
}

macro_rules! holding_handler {
    ($self:ident, $route:ident, $envelope:ident, $context:literal, $core:ident, $adapter:ident, $handler:ident, $write:ident, $uow:ident) => {{
        let pool = $self
            .write_pool(
                &$route.holding_id,
                &$route.data_profile_key,
                $envelope.correlation_id,
                $context,
            )
            .await?;
        let access = Self::access(&$envelope.actor, &$envelope.active_scope, AccessMode::Write);
        $core::$handler::new(
            $adapter::$write::new(pool.clone()),
            $adapter::$uow::new(pool),
            access.clone(),
            access,
            SystemClock,
        )
        .handle($envelope)
        .await
    }};
}

macro_rules! holding_query_handler {
    ($self:ident, $route:ident, $envelope:ident, $context:literal, $core:ident, $adapter:ident, $handler:ident, $read:ident) => {{
        let pool = $self
            .read_pool(
                &$route.holding_id,
                &$route.data_profile_key,
                $envelope.correlation_id,
                $context,
            )
            .await?;
        let read = $adapter::$read::new(pool)
            .map_err(|_| storage_configuration_error($envelope.correlation_id, $context))?;
        let access = Self::access(&$envelope.actor, &$envelope.active_scope, AccessMode::Read);
        $core::$handler::new(read, access.clone(), access)
            .handle($envelope)
            .await
    }};
}

macro_rules! holding_projected_handler {
    ($self:ident, $route:ident, $envelope:ident, $context:literal, $core:ident, $adapter:ident, $handler:ident, $write:ident, $uow:ident, [$($projection:expr),+ $(,)?]) => {{
        let command_id = $envelope.command_id.as_uuid();
        let correlation_id = $envelope.correlation_id.as_uuid();
        let receipt = holding_handler!(
            $self,
            $route,
            $envelope,
            $context,
            $core,
            $adapter,
            $handler,
            $write,
            $uow
        )?;
        $(
            $self
                .schedule_incremental_projection(
                    &$route.holding_id,
                    &$route.data_profile_key,
                    $projection,
                    command_id,
                    correlation_id,
                )
                .await;
        )+
        Ok(receipt)
    }};
}

#[async_trait]
impl ProcurementCommandApplication for Wave6HoldingApplication {
    async fn create(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<procurement::CreatePurchaseOrderResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "procurement",
            procurement,
            procurement_sqlx,
            ProcurementCommandHandler,
            ProcurementWriteRepository,
            ProcurementCommandAdapter
        )
    }
    async fn release(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::ReleasePurchaseOrder>,
    ) -> Result<CommandReceipt<procurement::PurchaseOrderMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "procurement",
            procurement,
            procurement_sqlx,
            ProcurementCommandHandler,
            ProcurementWriteRepository,
            ProcurementCommandAdapter
        )
    }
    async fn cancel(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CancelPurchaseOrder>,
    ) -> Result<CommandReceipt<procurement::PurchaseOrderMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "procurement",
            procurement,
            procurement_sqlx,
            ProcurementCommandHandler,
            ProcurementWriteRepository,
            ProcurementCommandAdapter
        )
    }

    async fn create_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CreatePurchaseRequisition>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::CreateRequisition(envelope.command),
        )
        .await
    }

    async fn submit_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::SubmitPurchaseRequisition>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::SubmitRequisition(envelope.command),
        )
        .await
    }

    async fn approve_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::ApprovePurchaseRequisition>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::ApproveRequisition(envelope.command),
        )
        .await
    }

    async fn reject_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::RejectPurchaseRequisition>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::RejectRequisition(envelope.command),
        )
        .await
    }

    async fn cancel_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CancelPurchaseRequisition>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::CancelRequisition(envelope.command),
        )
        .await
    }

    async fn create_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CreateGoodsReceipt>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::CreateGoodsReceipt(envelope.command),
        )
        .await
    }

    async fn accept_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::AcceptGoodsReceipt>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::AcceptGoodsReceipt(envelope.command),
        )
        .await
    }

    async fn reject_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::RejectGoodsReceipt>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::RejectGoodsReceipt(envelope.command),
        )
        .await
    }

    async fn cancel_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<procurement::CancelGoodsReceipt>,
    ) -> Result<procurement::StoredProcurementWorkflowCommit, CqrsError> {
        envelope.validate()?;
        let context = procurement_workflow_context(&envelope);
        self.execute_procurement_workflow(
            route,
            context,
            ProcurementWorkflowCommand::CancelGoodsReceipt(envelope.command),
        )
        .await
    }
}

#[async_trait]
impl ProcurementQueryApplication for Wave6HoldingApplication {
    async fn get(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<procurement::GetPurchaseOrder>,
    ) -> Result<QueryResult<procurement::GetPurchaseOrderResult>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "procurement",
            procurement,
            procurement_sqlx,
            ProcurementQueryHandler,
            ProcurementReadRepository
        )
    }
    async fn list(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<procurement::ListPurchaseOrders>,
    ) -> Result<QueryResult<procurement::PurchaseOrderList>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "procurement",
            procurement,
            procurement_sqlx,
            ProcurementQueryHandler,
            ProcurementReadRepository
        )
    }

    async fn get_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetPurchaseRequisition>,
    ) -> Result<QueryResult<Option<procurement::PurchaseRequisitionView>>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let pool = self
            .read_pool(
                &route.holding_id,
                &route.data_profile_key,
                correlation_id,
                "procurement",
            )
            .await?;
        let read = procurement_sqlx::ProcurementWorkflowReadAdapter::new(pool)
            .map_err(|_| storage_configuration_error(correlation_id, "procurement"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        procurement::ProcurementWorkflowAuthorizationPort::authorize_procurement_workflow(
            &access,
            procurement::ProcurementWorkflowAuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action: procurement::ProcurementWorkflowAction::ViewRequisition,
            },
        )
        .await
        .map_err(|error| procurement_workflow_port_failure(error, correlation_id))?;
        let projection = procurement::ProcurementWorkflowReadRepository::get_requisition(
            &read,
            &envelope.active_scope,
            envelope.query.requisition_id,
            &envelope.consistency,
        )
        .await
        .map_err(|error| procurement_workflow_port_failure(error, correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|value| !envelope.active_scope.contains(&value.scope))
        {
            return Err(procurement_workflow_port_failure(
                procurement::PortError::Denied,
                correlation_id,
            ));
        }
        Ok(QueryResult {
            value: projection.value,
            metadata: QueryMetadata {
                correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }

    async fn get_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetGoodsReceipt>,
    ) -> Result<QueryResult<Option<procurement::GoodsReceiptView>>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let pool = self
            .read_pool(
                &route.holding_id,
                &route.data_profile_key,
                correlation_id,
                "procurement",
            )
            .await?;
        let read = procurement_sqlx::ProcurementWorkflowReadAdapter::new(pool)
            .map_err(|_| storage_configuration_error(correlation_id, "procurement"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Read);
        procurement::ProcurementWorkflowAuthorizationPort::authorize_procurement_workflow(
            &access,
            procurement::ProcurementWorkflowAuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action: procurement::ProcurementWorkflowAction::ViewGoodsReceipt,
            },
        )
        .await
        .map_err(|error| procurement_workflow_port_failure(error, correlation_id))?;
        let projection = procurement::ProcurementWorkflowReadRepository::get_goods_receipt(
            &read,
            &envelope.active_scope,
            envelope.query.goods_receipt_id,
            &envelope.consistency,
        )
        .await
        .map_err(|error| procurement_workflow_port_failure(error, correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|value| !envelope.active_scope.contains(&value.scope))
        {
            return Err(procurement_workflow_port_failure(
                procurement::PortError::Denied,
                correlation_id,
            ));
        }
        Ok(QueryResult {
            value: projection.value,
            metadata: QueryMetadata {
                correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl InventoryCommandApplication for Wave6HoldingApplication {
    async fn register(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<inventory::RegisterStockItem>,
    ) -> Result<CommandReceipt<inventory::RegisterStockItemResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryCommandHandler,
            InventoryWriteRepository,
            InventoryCommandAdapter
        )
    }
    async fn receive(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<inventory::ReceiveStock>,
    ) -> Result<CommandReceipt<inventory::InventoryMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryCommandHandler,
            InventoryWriteRepository,
            InventoryCommandAdapter
        )
    }
    async fn issue(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<inventory::IssueStock>,
    ) -> Result<CommandReceipt<inventory::InventoryMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryCommandHandler,
            InventoryWriteRepository,
            InventoryCommandAdapter
        )
    }
    async fn reserve(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<inventory::ReserveInventory>,
    ) -> Result<CommandReceipt<inventory::InventoryMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryCommandHandler,
            InventoryWriteRepository,
            InventoryCommandAdapter
        )
    }
    async fn release_reservation(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<inventory::ReleaseInventoryReservation>,
    ) -> Result<CommandReceipt<inventory::InventoryMutationResult>, CqrsError> {
        holding_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryCommandHandler,
            InventoryWriteRepository,
            InventoryCommandAdapter
        )
    }
}

#[async_trait]
impl InventoryQueryApplication for Wave6HoldingApplication {
    async fn get(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<inventory::GetStockItem>,
    ) -> Result<QueryResult<inventory::GetStockItemResult>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryQueryHandler,
            InventoryReadRepository
        )
    }
    async fn list(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<inventory::ListStockItems>,
    ) -> Result<QueryResult<inventory::StockItemList>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "inventory",
            inventory,
            inventory_sqlx,
            InventoryQueryHandler,
            InventoryReadRepository
        )
    }
}

#[async_trait]
impl AssetsCommandApplication for Wave6HoldingApplication {
    async fn create(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::CreateAsset>,
    ) -> Result<CommandReceipt<assets::CreateAssetResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [assets_sqlx::PROJECTION_NAME]
        )
    }
    async fn place_in_service(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::PlaceAssetInService>,
    ) -> Result<CommandReceipt<assets::AssetMutationResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [assets_sqlx::PROJECTION_NAME]
        )
    }
    async fn mark_degraded(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::MarkAssetDegraded>,
    ) -> Result<CommandReceipt<assets::AssetMutationResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [assets_sqlx::PROJECTION_NAME]
        )
    }
    async fn begin_maintenance(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::BeginMaintenance>,
    ) -> Result<CommandReceipt<assets::AssetMutationResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [
                assets_sqlx::PROJECTION_NAME,
                maintenance_sqlx::ASSET_FACT_PROJECTION_NAME,
            ]
        )
    }
    async fn return_to_service(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::ReturnAssetToService>,
    ) -> Result<CommandReceipt<assets::AssetMutationResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [assets_sqlx::PROJECTION_NAME]
        )
    }
    async fn retire(
        &self,
        route: AssetsHoldingRoute,
        envelope: CommandEnvelope<assets::RetireAsset>,
    ) -> Result<CommandReceipt<assets::AssetMutationResult>, CqrsError> {
        holding_projected_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetCommandHandler,
            AssetsWriteRepository,
            AssetsCommandAdapter,
            [assets_sqlx::PROJECTION_NAME]
        )
    }
}

#[async_trait]
impl AssetsQueryApplication for Wave6HoldingApplication {
    async fn get(
        &self,
        route: AssetsHoldingRoute,
        envelope: QueryEnvelope<assets::GetAsset>,
    ) -> Result<QueryResult<assets::GetAssetResult>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetQueryHandler,
            AssetsReadRepository
        )
    }
    async fn list(
        &self,
        route: AssetsHoldingRoute,
        envelope: QueryEnvelope<assets::ListAssets>,
    ) -> Result<QueryResult<assets::AssetList>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "assets",
            assets,
            assets_sqlx,
            AssetQueryHandler,
            AssetsReadRepository
        )
    }
}

#[async_trait]
impl MaintenanceCommandApplication for Wave6HoldingApplication {
    async fn open(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::OpenWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::OpenWorkOrderResult>, CqrsError> {
        let command_id = envelope.command_id.as_uuid();
        let correlation_id = envelope.correlation_id;
        let raw_pool = self
            .route_pool(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
                correlation_id,
                "maintenance",
            )
            .await?;
        let pool = WritePool::for_holding(
            raw_pool.clone(),
            cqrs_holding_uuid(&route.holding_id, correlation_id, "maintenance")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "maintenance"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Write);
        let receipt = maintenance::MaintenanceCommandHandler::new(
            maintenance_sqlx::MaintenanceWriteRepository::new(pool.clone()),
            maintenance_sqlx::MaintenanceCommandAdapter::new(pool),
            access.clone(),
            access,
            maintenance_sqlx::MaintenanceAssetFactRepository::new(raw_pool),
            SystemClock,
        )
        .handle(envelope)
        .await?;
        self.schedule_incremental_projection(
            &route.holding_id,
            &route.data_profile_key,
            maintenance_sqlx::PROJECTION_NAME,
            command_id,
            correlation_id.as_uuid(),
        )
        .await;
        Ok(receipt)
    }
    async fn schedule(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::ScheduleWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn start(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::StartWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn block(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::BlockWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn resume(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::ResumeWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn complete(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::CompleteWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn cancel(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::CancelWorkOrder>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
    async fn record_downtime(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<maintenance::RecordDowntime>,
    ) -> Result<CommandReceipt<maintenance::MaintenanceMutationResult>, CqrsError> {
        self.maintenance_command(route, envelope).await
    }
}

impl Wave6HoldingApplication {
    async fn maintenance_command<C>(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<<MaintenanceHandler as CommandHandler<C>>::Response>, CqrsError>
    where
        C: Send + Sync,
        MaintenanceHandler: CommandHandler<C>,
    {
        let command_id = envelope.command_id.as_uuid();
        let correlation_id = envelope.correlation_id;
        let raw_pool = self
            .route_pool(
                &route.holding_id,
                &route.data_profile_key,
                RoutedDatabaseSide::Write,
                correlation_id,
                "maintenance",
            )
            .await?;
        let pool = WritePool::for_holding(
            raw_pool.clone(),
            cqrs_holding_uuid(&route.holding_id, correlation_id, "maintenance")?,
        )
        .map_err(|_| invalid_holding_binding(correlation_id, "maintenance"))?;
        let access = Self::access(&envelope.actor, &envelope.active_scope, AccessMode::Write);
        let receipt = maintenance::MaintenanceCommandHandler::new(
            maintenance_sqlx::MaintenanceWriteRepository::new(pool.clone()),
            maintenance_sqlx::MaintenanceCommandAdapter::new(pool),
            access.clone(),
            access,
            maintenance_sqlx::MaintenanceAssetFactRepository::new(raw_pool),
            SystemClock,
        )
        .handle(envelope)
        .await?;
        self.schedule_incremental_projection(
            &route.holding_id,
            &route.data_profile_key,
            maintenance_sqlx::PROJECTION_NAME,
            command_id,
            correlation_id.as_uuid(),
        )
        .await;
        Ok(receipt)
    }
}

type MaintenanceHandler = maintenance::MaintenanceCommandHandler<
    maintenance_sqlx::MaintenanceWriteRepository,
    maintenance_sqlx::MaintenanceCommandAdapter,
    PreauthorizedAccess,
    PreauthorizedAccess,
    maintenance_sqlx::MaintenanceAssetFactRepository,
    SystemClock,
>;

#[async_trait]
impl MaintenanceQueryApplication for Wave6HoldingApplication {
    async fn get(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: QueryEnvelope<maintenance::GetWorkOrder>,
    ) -> Result<QueryResult<maintenance::GetWorkOrderResult>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "maintenance",
            maintenance,
            maintenance_sqlx,
            MaintenanceQueryHandler,
            MaintenanceReadRepository
        )
    }
    async fn list(
        &self,
        route: MaintenanceHoldingRoute,
        envelope: QueryEnvelope<maintenance::ListWorkOrders>,
    ) -> Result<QueryResult<maintenance::WorkOrderList>, CqrsError> {
        holding_query_handler!(
            self,
            route,
            envelope,
            "maintenance",
            maintenance,
            maintenance_sqlx,
            MaintenanceQueryHandler,
            MaintenanceReadRepository
        )
    }
}

#[derive(Clone)]
pub(crate) struct Wave6ServerAuthenticator {
    state: AppState,
}

impl Wave6ServerAuthenticator {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }

    // Keep all transport authority inputs explicit at the shared wave boundary.
    #[allow(clippy::too_many_arguments)]
    async fn authenticate(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        actor: &ActorAttribution,
        scope: &AuthorityScope,
        holding_id: &str,
        authorization_holding: Option<&str>,
        subscription_id: Option<&str>,
        mfa_code: Option<&str>,
        mode: AccessMode,
        context: &'static str,
    ) -> Result<ActorAttribution, Status> {
        let write = mode == AccessMode::Write;
        let permission = if write {
            write_permission(context)
        } else {
            read_permission(context)
        };
        if let Some(authorization_holding) = authorization_holding {
            authenticate_context_for_holding(
                &self.state,
                metadata,
                actor,
                scope,
                holding_id,
                authorization_holding,
                permission,
                product_name(context),
                write,
                subscription_id,
                mfa_code,
            )
            .await
        } else {
            authenticate_context(
                &self.state,
                metadata,
                actor,
                scope,
                holding_id,
                permission,
                product_name(context),
                write,
                subscription_id,
                mfa_code,
            )
            .await
        }
    }
}

macro_rules! authenticator {
    (
        $trait:path,
        $request:path,
        $authenticated:path,
        $error:path,
        $read_mode:path,
        $context:literal
    ) => {
        #[async_trait]
        impl $trait for Wave6ServerAuthenticator {
            async fn authenticate(&self, request: $request) -> Result<$authenticated, $error> {
                let mode = if request.access_mode() == $read_mode {
                    AccessMode::Read
                } else {
                    AccessMode::Write
                };
                let actor = self
                    .authenticate(
                        request.transport_metadata(),
                        request.presented_actor(),
                        request.presented_scope(),
                        &request.presented_route().holding_id,
                        None,
                        request.subscription_id(),
                        request.mfa_code(),
                        mode,
                        $context,
                    )
                    .await
                    .map_err(map_auth_error)?;
                <$authenticated>::try_new(
                    actor,
                    request.presented_scope().clone(),
                    request.presented_route().clone(),
                )
            }
        }
    };
}

#[async_trait]
impl MarketingRequestAuthenticator for Wave6ServerAuthenticator {
    async fn authenticate(
        &self,
        request: MarketingAuthenticationRequest,
    ) -> Result<AuthenticatedMarketingRequest, MarketingAuthenticationError> {
        let mode = if request.access_mode() == MarketingAccessMode::Read {
            AccessMode::Read
        } else {
            AccessMode::Write
        };
        let presented_route = request
            .presented_route()
            .map(|route| PresentedHoldingRoute {
                holding_id: &route.holding_id,
                data_profile_key: &route.data_profile_key,
            });
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            presented_route,
            request.subscription_id(),
            mode == AccessMode::Write,
        )
        .await
        .map_err(map_auth_error)?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        let actor = self
            .authenticate(
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                Some(&workspace.holding_id),
                subscription_id.as_deref(),
                request.mfa_code(),
                mode,
                "marketing",
            )
            .await
            .map_err(map_auth_error)?;
        AuthenticatedMarketingRequest::try_new(
            actor,
            request.presented_scope().clone(),
            MarketingHoldingRoute {
                holding_id: workspace.holding_id,
                data_profile_key: workspace.data_profile_key,
            },
        )
    }
}

#[async_trait]
impl CrmRequestAuthenticator for Wave6ServerAuthenticator {
    async fn authenticate(
        &self,
        request: CrmAuthenticationRequest,
    ) -> Result<AuthenticatedCrmRequest, CrmAuthenticationError> {
        let mode = if request.access_mode() == CrmAccessMode::Read {
            AccessMode::Read
        } else {
            AccessMode::Write
        };
        let presented_route = request
            .presented_route()
            .map(|route| PresentedHoldingRoute {
                holding_id: &route.holding_id,
                data_profile_key: &route.data_profile_key,
            });
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            presented_route,
            request.subscription_id(),
            mode == AccessMode::Write,
        )
        .await
        .map_err(map_auth_error)?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        let actor = self
            .authenticate(
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                Some(&workspace.holding_id),
                subscription_id.as_deref(),
                request.mfa_code(),
                mode,
                "crm",
            )
            .await
            .map_err(map_auth_error)?;
        AuthenticatedCrmRequest::try_new(
            actor,
            request.presented_scope().clone(),
            CrmHoldingRoute {
                holding_id: workspace.holding_id,
                data_profile_key: workspace.data_profile_key,
            },
        )
    }
}
authenticator!(
    SupplyChainRequestAuthenticator,
    SupplyChainAuthenticationRequest,
    AuthenticatedSupplyChainRequest,
    SupplyChainAuthenticationError,
    SupplyChainAccessMode::Read,
    "supply_chain"
);
#[async_trait]
impl AssetsRequestAuthenticator for Wave6ServerAuthenticator {
    async fn authenticate(
        &self,
        request: AssetsAuthenticationRequest,
    ) -> Result<AuthenticatedAssetsRequest, AssetsAuthenticationError> {
        let mode = if request.access_mode() == AssetsAccessMode::Read {
            AccessMode::Read
        } else {
            AccessMode::Write
        };
        let presented_route = request
            .presented_route()
            .map(|route| PresentedHoldingRoute {
                holding_id: &route.holding_id,
                data_profile_key: &route.data_profile_key,
            });
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            presented_route,
            request.subscription_id(),
            mode == AccessMode::Write,
        )
        .await
        .map_err(map_auth_error)?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        let actor = self
            .authenticate(
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                Some(&workspace.holding_id),
                subscription_id.as_deref(),
                request.mfa_code(),
                mode,
                "assets",
            )
            .await
            .map_err(map_auth_error)?;
        AuthenticatedAssetsRequest::try_new(
            actor,
            request.presented_scope().clone(),
            AssetsHoldingRoute {
                holding_id: workspace.holding_id,
                data_profile_key: workspace.data_profile_key,
            },
        )
    }
}

#[async_trait]
impl MaintenanceRequestAuthenticator for Wave6ServerAuthenticator {
    async fn authenticate(
        &self,
        request: MaintenanceAuthenticationRequest,
    ) -> Result<AuthenticatedMaintenanceRequest, MaintenanceAuthenticationError> {
        let mode = if request.access_mode() == MaintenanceAccessMode::Read {
            AccessMode::Read
        } else {
            AccessMode::Write
        };
        let presented_route = request
            .presented_route()
            .map(|route| PresentedHoldingRoute {
                holding_id: &route.holding_id,
                data_profile_key: &route.data_profile_key,
            });
        let workspace = resolve_holding_workspace(
            &self.state,
            request.presented_scope(),
            presented_route,
            request.subscription_id(),
            mode == AccessMode::Write,
        )
        .await
        .map_err(map_auth_error)?;
        let customer_id = workspace.customer_id.to_string();
        let subscription_id = workspace.subscription_id.map(|value| value.to_string());
        let actor = self
            .authenticate(
                request.transport_metadata(),
                request.presented_actor(),
                request.presented_scope(),
                &customer_id,
                Some(&workspace.holding_id),
                subscription_id.as_deref(),
                request.mfa_code(),
                mode,
                "maintenance",
            )
            .await
            .map_err(map_auth_error)?;
        AuthenticatedMaintenanceRequest::try_new(
            actor,
            request.presented_scope().clone(),
            MaintenanceHoldingRoute {
                holding_id: workspace.holding_id,
                data_profile_key: workspace.data_profile_key,
            },
        )
    }
}

trait AuthenticationError: Sized {
    fn unauthenticated() -> Self;
    fn denied() -> Self;
    fn unavailable() -> Self;
    fn internal() -> Self;
}

macro_rules! authentication_error {
    ($error:path) => {
        impl AuthenticationError for $error {
            fn unauthenticated() -> Self {
                <$error>::Unauthenticated
            }
            fn denied() -> Self {
                <$error>::Denied
            }
            fn unavailable() -> Self {
                <$error>::Unavailable
            }
            fn internal() -> Self {
                <$error>::Internal
            }
        }
    };
}

authentication_error!(MarketingAuthenticationError);
authentication_error!(CrmAuthenticationError);
authentication_error!(SupplyChainAuthenticationError);
authentication_error!(AssetsAuthenticationError);
authentication_error!(MaintenanceAuthenticationError);

fn map_auth_error<E: AuthenticationError>(status: Status) -> E {
    tracing::warn!(
        grpc_code = ?status.code(),
        reason = status.message(),
        "typed wave6 authorization rejected"
    );
    match status.code() {
        tonic::Code::Unauthenticated => E::unauthenticated(),
        tonic::Code::PermissionDenied | tonic::Code::InvalidArgument => E::denied(),
        tonic::Code::Unavailable
        | tonic::Code::DeadlineExceeded
        | tonic::Code::FailedPrecondition => E::unavailable(),
        _ => E::internal(),
    }
}

fn read_permission(context: &str) -> &'static str {
    match context {
        "marketing" | "crm" => "marketing-crm.read",
        "supply_chain" => "scm.read",
        "assets" => "assets.read",
        "maintenance" => "maintenance.read",
        _ => "context.read",
    }
}

fn write_permission(context: &str) -> &'static str {
    match context {
        "marketing" | "crm" => "marketing-crm.write",
        "supply_chain" => "scm.write",
        "assets" => "assets.write",
        "maintenance" => "maintenance.write",
        _ => "context.write",
    }
}

fn product_name(context: &str) -> &'static str {
    match context {
        "supply_chain" => "scm",
        "assets" => "assets",
        "maintenance" => "maintenance",
        "marketing" => "marketing",
        "crm" => "crm",
        _ => "context",
    }
}

fn storage_configuration_error(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.read_configuration"),
        ErrorCategory::Internal,
        "Context read storage could not be initialized.",
        false,
    )
}

fn cqrs_holding_uuid(
    holding_id: &str,
    correlation_id: CorrelationId,
    context: &'static str,
) -> Result<Uuid, CqrsError> {
    Uuid::parse_str(holding_id).map_err(|_| invalid_holding_binding(correlation_id, context))
}

fn procurement_workflow_context<C>(
    envelope: &CommandEnvelope<C>,
) -> procurement::ProcurementWorkflowCommandContext {
    procurement::ProcurementWorkflowCommandContext {
        command_id: envelope.command_id,
        idempotency_key: envelope.idempotency_key.clone(),
        correlation_id: envelope.correlation_id,
        actor: envelope.actor.clone(),
        authority_scope: envelope.active_scope.clone(),
        audit: envelope.audit.clone(),
        expected_version: envelope.expected_aggregate_version,
    }
}

fn procurement_workflow_failure(
    error: procurement::ProcurementWorkflowApplicationError,
    correlation_id: CorrelationId,
) -> CqrsError {
    use procurement::ProcurementWorkflowApplicationError as Error;

    match error {
        Error::Port(port) => procurement_workflow_port_failure(port, correlation_id),
        Error::Domain(_)
        | Error::UnexpectedCreateVersion
        | Error::MissingExpectedVersion
        | Error::InvalidSource => context_failure(
            correlation_id,
            "procurement.workflow.invalid",
            ErrorCategory::Invalid,
            "The Procurement workflow request is invalid.",
            false,
        ),
        Error::ScopeOutsideAuthority | Error::HumanDecisionRequired => context_failure(
            correlation_id,
            "procurement.workflow.denied",
            ErrorCategory::Denied,
            "The Procurement workflow operation is not permitted.",
            false,
        ),
    }
}

fn procurement_workflow_port_failure(
    error: procurement::PortError,
    correlation_id: CorrelationId,
) -> CqrsError {
    use procurement::PortError;

    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        PortError::Denied | PortError::EntitlementDenied => context_failure(
            correlation_id,
            "procurement.workflow.denied",
            ErrorCategory::Denied,
            "The Procurement workflow operation is not permitted.",
            false,
        ),
        PortError::NotFound => context_failure(
            correlation_id,
            "procurement.workflow.not_found",
            ErrorCategory::NotFound,
            "The Procurement workflow record was not found.",
            false,
        ),
        PortError::Conflict { .. } | PortError::IdempotencyConflict => context_failure(
            correlation_id,
            "procurement.workflow.conflict",
            ErrorCategory::Conflict,
            "The Procurement workflow record changed or the command identity conflicts.",
            false,
        ),
        PortError::Unavailable => context_failure(
            correlation_id,
            "procurement.workflow.unavailable",
            ErrorCategory::Unavailable,
            "Procurement workflow storage is temporarily unavailable.",
            true,
        ),
        PortError::Internal => context_failure(
            correlation_id,
            "procurement.workflow.internal",
            ErrorCategory::Internal,
            "Procurement could not complete the workflow operation.",
            false,
        ),
    }
}

fn invalid_holding_binding(correlation_id: CorrelationId, context: &'static str) -> CqrsError {
    context_failure(
        correlation_id,
        format!("{context}.invalid_holding_identity"),
        ErrorCategory::Internal,
        "The routed holding identity is invalid.",
        false,
    )
}

fn context_failure(
    correlation_id: CorrelationId,
    code: impl Into<String>,
    category: ErrorCategory,
    safe_message: impl Into<String>,
    retryable: bool,
) -> CqrsError {
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::{product_name, read_permission, write_permission};

    #[test]
    fn asset_and_maintenance_routes_use_their_module_role_permissions() {
        assert_eq!(read_permission("assets"), "assets.read");
        assert_eq!(write_permission("assets"), "assets.write");
        assert_eq!(read_permission("maintenance"), "maintenance.read");
        assert_eq!(write_permission("maintenance"), "maintenance.write");
        assert_eq!(product_name("assets"), "assets");
        assert_eq!(product_name("maintenance"), "maintenance");
    }

    #[test]
    fn marketing_and_crm_share_the_combined_business_module_permissions() {
        for context in ["marketing", "crm"] {
            assert_eq!(read_permission(context), "marketing-crm.read");
            assert_eq!(write_permission(context), "marketing-crm.write");
        }
    }
}
