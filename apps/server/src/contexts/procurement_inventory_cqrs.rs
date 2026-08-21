//! Typed inbound adapters for the paired Procurement and Inventory contexts.
//! Pool construction and canonical authentication remain composition concerns.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::{
    RecordTypeId, RecordVersionRef, StableRecordRef, procurement as procurement_contract,
};
use erpx_inventory::{
    GetStockItem, GetStockItemResult, InventoryMutationResult, IssueStock, ListStockItems,
    ReceiveStock, RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation,
    ReservationId, ReservationStatus, ReserveInventory, StockItemId, StockItemList,
    StockItemListFilter, StockItemView, StockMovementId, WarehouseId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, QueryId, RecordId, SessionId,
};
use erpx_procurement::{
    AcceptGoodsReceipt, ApprovePurchaseRequisition, CancelGoodsReceipt, CancelPurchaseOrder,
    CancelPurchaseRequisition, CreateGoodsReceipt, CreatePurchaseOrder, CreatePurchaseOrderResult,
    CreatePurchaseRequisition, GetPurchaseOrder, GetPurchaseOrderResult, GoodsReceiptId,
    GoodsReceiptLine, GoodsReceiptStatus, GoodsReceiptView, InventoryStockItemId,
    ListPurchaseOrders, PurchaseOrderId, PurchaseOrderLine, PurchaseOrderLineId, PurchaseOrderList,
    PurchaseOrderListFilter, PurchaseOrderMutationResult, PurchaseOrderStatus, PurchaseOrderView,
    PurchaseRequisitionId, PurchaseRequisitionLine, PurchaseRequisitionStatus,
    PurchaseRequisitionView, RejectGoodsReceipt, RejectPurchaseRequisition, ReleasePurchaseOrder,
    StoredProcurementWorkflowCommit, SubmitPurchaseRequisition, SupplierId,
};
use erpx_proto::common;
mod platform {
    pub use erpx_proto::erpx::inventory::*;
    pub use erpx_proto::erpx::procurement::*;
}
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::inventory_command_service_server::InventoryCommandService;
use platform::inventory_query_service_server::InventoryQueryService;
use platform::procurement_command_service_server::ProcurementCommandService;
use platform::procurement_query_service_server::ProcurementQueryService;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplyChainHoldingRoute {
    pub holding_id: String,
    pub data_profile_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyChainAccessMode {
    Read,
    Write,
}

pub struct SupplyChainAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: AuthorityScope,
    presented_route: SupplyChainHoldingRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: SupplyChainAccessMode,
}

impl SupplyChainAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &AuthorityScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &SupplyChainHoldingRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> SupplyChainAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSupplyChainRequest {
    actor: ActorAttribution,
    active_scope: AuthorityScope,
    holding_route: SupplyChainHoldingRoute,
}

impl AuthenticatedSupplyChainRequest {
    pub fn try_new(
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        holding_route: SupplyChainHoldingRoute,
    ) -> Result<Self, SupplyChainAuthenticationError> {
        actor
            .validate()
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        active_scope
            .validate()
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        validate_route(&holding_route).map_err(|_| SupplyChainAuthenticationError::Internal)?;
        validate_route_scope(&holding_route, &active_scope)
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            active_scope,
            holding_route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn active_scope(&self) -> &AuthorityScope {
        &self.active_scope
    }
    pub const fn holding_route(&self) -> &SupplyChainHoldingRoute {
        &self.holding_route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyChainAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait SupplyChainRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: SupplyChainAuthenticationRequest,
    ) -> Result<AuthenticatedSupplyChainRequest, SupplyChainAuthenticationError>;
}

#[async_trait]
pub trait ProcurementCommandApplication: Send + Sync + 'static {
    async fn create(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<CreatePurchaseOrderResult>, CqrsError>;
    async fn release(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<ReleasePurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, CqrsError>;
    async fn cancel(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CancelPurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, CqrsError>;
    async fn create_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CreatePurchaseRequisition>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn submit_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<SubmitPurchaseRequisition>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn approve_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<ApprovePurchaseRequisition>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn reject_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<RejectPurchaseRequisition>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn cancel_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CancelPurchaseRequisition>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn create_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CreateGoodsReceipt>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn accept_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<AcceptGoodsReceipt>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn reject_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<RejectGoodsReceipt>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
    async fn cancel_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<CancelGoodsReceipt>,
    ) -> Result<StoredProcurementWorkflowCommit, CqrsError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetPurchaseRequisition {
    pub requisition_id: PurchaseRequisitionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetGoodsReceipt {
    pub goods_receipt_id: GoodsReceiptId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcurementOwnerCommandTarget {
    ApproveRequisition(PurchaseRequisitionId),
    ReleasePurchaseOrder(PurchaseOrderId),
    AcceptGoodsReceipt(GoodsReceiptId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerCommandEvaluation {
    pub source_version: u64,
    pub allowed: bool,
    pub evaluated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait ProcurementQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<GetPurchaseOrderResult>, CqrsError>;
    async fn list(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<ListPurchaseOrders>,
    ) -> Result<QueryResult<PurchaseOrderList>, CqrsError>;
    async fn get_requisition(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetPurchaseRequisition>,
    ) -> Result<QueryResult<Option<PurchaseRequisitionView>>, CqrsError>;
    async fn get_goods_receipt(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetGoodsReceipt>,
    ) -> Result<QueryResult<Option<GoodsReceiptView>>, CqrsError>;
    async fn evaluate_owner_command(
        &self,
        route: SupplyChainHoldingRoute,
        actor: ActorAttribution,
        active_scope: AuthorityScope,
        correlation_id: CorrelationId,
        target: ProcurementOwnerCommandTarget,
    ) -> Result<Option<OwnerCommandEvaluation>, CqrsError>;
}

#[async_trait]
pub trait InventoryCommandApplication: Send + Sync + 'static {
    async fn register(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<RegisterStockItem>,
    ) -> Result<CommandReceipt<RegisterStockItemResult>, CqrsError>;
    async fn receive(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<ReceiveStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn issue(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<IssueStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn reserve(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<ReserveInventory>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn release_reservation(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: CommandEnvelope<ReleaseInventoryReservation>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
}

#[async_trait]
pub trait InventoryQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<GetStockItemResult>, CqrsError>;
    async fn list(
        &self,
        route: SupplyChainHoldingRoute,
        envelope: QueryEnvelope<ListStockItems>,
    ) -> Result<QueryResult<StockItemList>, CqrsError>;
}

macro_rules! rpc {
    ($name:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name<A, G> {
            application: Arc<A>,
            authenticator: Arc<G>,
        }
        impl<A, G> $name<A, G> {
            pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
                Self {
                    application,
                    authenticator,
                }
            }
        }
    };
}
rpc!(ProcurementCommandRpc);
rpc!(ProcurementQueryRpc);
rpc!(InventoryCommandRpc);
rpc!(InventoryQueryRpc);

#[async_trait]
impl<A: ProcurementCommandApplication, G: SupplyChainRequestAuthenticator> ProcurementCommandService
    for ProcurementCommandRpc<A, G>
{
    async fn create(
        &self,
        request: Request<platform::CreatePurchaseOrderRequest>,
    ) -> Result<Response<platform::PurchaseOrderMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = CreatePurchaseOrder {
            purchase_order_id: procurement_id(&input.purchase_order_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            order_code: input.order_code,
            supplier_id: SupplierId::try_from_uuid(parse_uuid(&input.supplier_id, "supplier_id")?)
                .map_err(|_| invalid("supplier_id"))?,
            currency: input.currency,
            lines: input
                .lines
                .into_iter()
                .map(decode_order_line)
                .collect::<Result<_, _>>()?,
            source_requisition: decode_record_version(
                input
                    .source_requisition
                    .ok_or_else(|| invalid("source_requisition is required"))?,
            )?,
            contract: input.contract.map(decode_record_version).transpose()?,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .create(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(order_mutation_response(
            &receipt,
            &receipt.response.order,
        )))
    }

    async fn release(
        &self,
        request: Request<platform::ReleasePurchaseOrderRequest>,
    ) -> Result<Response<platform::PurchaseOrderMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            ReleasePurchaseOrder {
                purchase_order_id: procurement_id(&input.purchase_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .release(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(order_mutation_response(
            &receipt,
            &receipt.response.order,
        )))
    }

    async fn cancel(
        &self,
        request: Request<platform::CancelPurchaseOrderRequest>,
    ) -> Result<Response<platform::PurchaseOrderMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let decoded = decode_command(
            input.metadata,
            CancelPurchaseOrder {
                purchase_order_id: procurement_id(&input.purchase_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), metadata, decoded).await?;
        let receipt = self
            .application
            .cancel(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(order_mutation_response(
            &receipt,
            &receipt.response.order,
        )))
    }

    async fn create_requisition(
        &self,
        request: Request<platform::CreatePurchaseRequisitionRequest>,
    ) -> Result<Response<platform::PurchaseRequisitionMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = CreatePurchaseRequisition {
            requisition_id: requisition_id(&input.requisition_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            requisition_code: input.requisition_code,
            requesting_department_code: input.requesting_department_code,
            budget_code: input.budget_code,
            currency: input.currency,
            lines: input
                .lines
                .into_iter()
                .map(decode_requisition_line)
                .collect::<Result<_, _>>()?,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let stored = self
            .application
            .create_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(requisition_mutation_response(&stored)?))
    }

    async fn submit_requisition(
        &self,
        request: Request<platform::PurchaseRequisitionIdentityRequest>,
    ) -> Result<Response<platform::PurchaseRequisitionMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                SubmitPurchaseRequisition {
                    requisition_id: requisition_id(&input.requisition_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .submit_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(requisition_mutation_response(&stored)?))
    }

    async fn approve_requisition(
        &self,
        request: Request<platform::PurchaseRequisitionIdentityRequest>,
    ) -> Result<Response<platform::PurchaseRequisitionMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                ApprovePurchaseRequisition {
                    requisition_id: requisition_id(&input.requisition_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .approve_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(requisition_mutation_response(&stored)?))
    }

    async fn reject_requisition(
        &self,
        request: Request<platform::PurchaseRequisitionIdentityRequest>,
    ) -> Result<Response<platform::PurchaseRequisitionMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                RejectPurchaseRequisition {
                    requisition_id: requisition_id(&input.requisition_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .reject_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(requisition_mutation_response(&stored)?))
    }

    async fn cancel_requisition(
        &self,
        request: Request<platform::PurchaseRequisitionIdentityRequest>,
    ) -> Result<Response<platform::PurchaseRequisitionMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                CancelPurchaseRequisition {
                    requisition_id: requisition_id(&input.requisition_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .cancel_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(requisition_mutation_response(&stored)?))
    }

    async fn create_goods_receipt(
        &self,
        request: Request<platform::CreateGoodsReceiptRequest>,
    ) -> Result<Response<platform::GoodsReceiptMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = CreateGoodsReceipt {
            goods_receipt_id: goods_receipt_id(&input.goods_receipt_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            receipt_code: input.receipt_code,
            source_purchase_order: decode_record_version(
                input
                    .source_purchase_order
                    .ok_or_else(|| invalid("source_purchase_order is required"))?,
            )?,
            lines: input
                .lines
                .into_iter()
                .map(decode_goods_receipt_line)
                .collect::<Result<_, _>>()?,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let stored = self
            .application
            .create_goods_receipt(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(goods_receipt_mutation_response(&stored)?))
    }

    async fn accept_goods_receipt(
        &self,
        request: Request<platform::GoodsReceiptIdentityRequest>,
    ) -> Result<Response<platform::GoodsReceiptMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                AcceptGoodsReceipt {
                    goods_receipt_id: goods_receipt_id(&input.goods_receipt_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .accept_goods_receipt(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(goods_receipt_mutation_response(&stored)?))
    }

    async fn reject_goods_receipt(
        &self,
        request: Request<platform::GoodsReceiptIdentityRequest>,
    ) -> Result<Response<platform::GoodsReceiptMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                RejectGoodsReceipt {
                    goods_receipt_id: goods_receipt_id(&input.goods_receipt_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .reject_goods_receipt(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(goods_receipt_mutation_response(&stored)?))
    }

    async fn cancel_goods_receipt(
        &self,
        request: Request<platform::GoodsReceiptIdentityRequest>,
    ) -> Result<Response<platform::GoodsReceiptMutationResponse>, Status> {
        let (metadata, input) = (request.metadata().clone(), request.into_inner());
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(
                input.metadata,
                CancelGoodsReceipt {
                    goods_receipt_id: goods_receipt_id(&input.goods_receipt_id)?,
                },
            )?,
        )
        .await?;
        let stored = self
            .application
            .cancel_goods_receipt(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(goods_receipt_mutation_response(&stored)?))
    }
}

#[async_trait]
impl<A: ProcurementQueryApplication, G: SupplyChainRequestAuthenticator> ProcurementQueryService
    for ProcurementQueryRpc<A, G>
{
    async fn get(
        &self,
        request: Request<platform::GetPurchaseOrderRequest>,
    ) -> Result<Response<platform::GetPurchaseOrderResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let purchase_order_id = procurement_id(&input.purchase_order_id)?;
        let decoded = decode_query(input.metadata, GetPurchaseOrder { purchase_order_id })?;
        let (route, envelope, write_authorized) =
            authenticate_owner_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let capability_route = route.clone();
        let capability_actor = envelope.actor.clone();
        let capability_scope = envelope.active_scope.clone();
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let evaluation = if write_authorized {
            self.application
                .evaluate_owner_command(
                    capability_route,
                    capability_actor,
                    capability_scope,
                    result.metadata.correlation_id,
                    ProcurementOwnerCommandTarget::ReleasePurchaseOrder(purchase_order_id),
                )
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        Ok(Response::new(platform::GetPurchaseOrderResponse {
            order: result.value.order.as_ref().map(encode_order),
            metadata: Some(encode_query_metadata(&result.metadata)),
            owner_capabilities: result.value.order.as_ref().map(|order| {
                procurement_owner_capabilities(
                    order.version,
                    result.metadata.projection.freshness,
                    ProcurementOwnerCommandTarget::ReleasePurchaseOrder(purchase_order_id),
                    evaluation.as_ref(),
                )
            }),
        }))
    }
    async fn list(
        &self,
        request: Request<platform::ListPurchaseOrdersRequest>,
    ) -> Result<Response<platform::ListPurchaseOrdersResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let filter = PurchaseOrderListFilter {
            status: input.status.map(decode_order_status).transpose()?,
            supplier_id: input
                .supplier_id
                .as_deref()
                .map(|value| {
                    SupplierId::try_from_uuid(parse_uuid(value, "supplier_id")?)
                        .map_err(|_| invalid("supplier_id"))
                })
                .transpose()?,
            page_size: u16::try_from(input.page_size).map_err(|_| invalid("page_size"))?,
            cursor: input.cursor,
        };
        let decoded = decode_query(input.metadata, ListPurchaseOrders { filter })?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListPurchaseOrdersResponse {
            orders: result.value.orders.iter().map(encode_order).collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn get_requisition(
        &self,
        request: Request<platform::GetPurchaseRequisitionRequest>,
    ) -> Result<Response<platform::GetPurchaseRequisitionResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let requisition_id = requisition_id(&input.requisition_id)?;
        let (route, envelope, write_authorized) = authenticate_owner_query(
            self.authenticator.as_ref(),
            metadata,
            decode_query(input.metadata, GetPurchaseRequisition { requisition_id })?,
        )
        .await?;
        let capability_route = route.clone();
        let capability_actor = envelope.actor.clone();
        let capability_scope = envelope.active_scope.clone();
        let result = self
            .application
            .get_requisition(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let evaluation = if write_authorized {
            self.application
                .evaluate_owner_command(
                    capability_route,
                    capability_actor,
                    capability_scope,
                    result.metadata.correlation_id,
                    ProcurementOwnerCommandTarget::ApproveRequisition(requisition_id),
                )
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        Ok(Response::new(platform::GetPurchaseRequisitionResponse {
            requisition: result.value.as_ref().map(encode_requisition),
            metadata: Some(encode_query_metadata(&result.metadata)),
            owner_capabilities: result.value.as_ref().map(|requisition| {
                procurement_owner_capabilities(
                    requisition.version,
                    result.metadata.projection.freshness,
                    ProcurementOwnerCommandTarget::ApproveRequisition(requisition_id),
                    evaluation.as_ref(),
                )
            }),
        }))
    }

    async fn get_goods_receipt(
        &self,
        request: Request<platform::GetGoodsReceiptRequest>,
    ) -> Result<Response<platform::GetGoodsReceiptResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let goods_receipt_id = goods_receipt_id(&input.goods_receipt_id)?;
        let (route, envelope, write_authorized) = authenticate_owner_query(
            self.authenticator.as_ref(),
            metadata,
            decode_query(input.metadata, GetGoodsReceipt { goods_receipt_id })?,
        )
        .await?;
        let capability_route = route.clone();
        let capability_actor = envelope.actor.clone();
        let capability_scope = envelope.active_scope.clone();
        let result = self
            .application
            .get_goods_receipt(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        let evaluation = if write_authorized {
            self.application
                .evaluate_owner_command(
                    capability_route,
                    capability_actor,
                    capability_scope,
                    result.metadata.correlation_id,
                    ProcurementOwnerCommandTarget::AcceptGoodsReceipt(goods_receipt_id),
                )
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        Ok(Response::new(platform::GetGoodsReceiptResponse {
            goods_receipt: result.value.as_ref().map(encode_goods_receipt),
            metadata: Some(encode_query_metadata(&result.metadata)),
            owner_capabilities: result.value.as_ref().map(|goods_receipt| {
                procurement_owner_capabilities(
                    goods_receipt.version,
                    result.metadata.projection.freshness,
                    ProcurementOwnerCommandTarget::AcceptGoodsReceipt(goods_receipt_id),
                    evaluation.as_ref(),
                )
            }),
        }))
    }
}

#[async_trait]
impl<A: InventoryCommandApplication, G: SupplyChainRequestAuthenticator> InventoryCommandService
    for InventoryCommandRpc<A, G>
{
    async fn register(
        &self,
        request: Request<platform::RegisterStockItemRequest>,
    ) -> Result<Response<platform::StockItemMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = RegisterStockItem {
            stock_item_id: stock_id(&input.stock_item_id)?,
            scope: decode_scope(input.scope.ok_or_else(|| invalid("scope is required"))?)?,
            warehouse_id: WarehouseId::try_from_uuid(parse_uuid(
                &input.warehouse_id,
                "warehouse_id",
            )?)
            .map_err(|_| invalid("warehouse_id"))?,
            item_code: input.item_code,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .register(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(item_mutation_response(
            &receipt,
            &receipt.response.item,
        )))
    }
    async fn receive(
        &self,
        request: Request<platform::ReceiveStockRequest>,
    ) -> Result<Response<platform::StockItemMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let procurement_line = input
            .procurement_line
            .map(|value| {
                procurement_contract::PurchaseOrderLineRef::try_from_uuid(parse_uuid(
                    &value.purchase_order_line_id,
                    "purchase_order_line_id",
                )?)
                .map_err(|_| invalid("purchase_order_line_id"))
            })
            .transpose()?;
        let command = ReceiveStock {
            stock_item_id: stock_id(&input.stock_item_id)?,
            movement_id: movement_id(&input.movement_id)?,
            quantity: input.quantity,
            procurement_line,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .receive(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(item_mutation_response(
            &receipt,
            &receipt.response.item,
        )))
    }
    async fn issue(
        &self,
        request: Request<platform::IssueStockRequest>,
    ) -> Result<Response<platform::StockItemMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = IssueStock {
            stock_item_id: stock_id(&input.stock_item_id)?,
            movement_id: movement_id(&input.movement_id)?,
            quantity: input.quantity,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .issue(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(item_mutation_response(
            &receipt,
            &receipt.response.item,
        )))
    }
    async fn reserve(
        &self,
        request: Request<platform::ReserveInventoryRequest>,
    ) -> Result<Response<platform::StockItemMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = ReserveInventory {
            stock_item_id: stock_id(&input.stock_item_id)?,
            reservation_id: reservation_id(&input.reservation_id)?,
            quantity: input.quantity,
            demand_reference: input.demand_reference,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .reserve(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(item_mutation_response(
            &receipt,
            &receipt.response.item,
        )))
    }
    async fn release_reservation(
        &self,
        request: Request<platform::ReleaseInventoryReservationRequest>,
    ) -> Result<Response<platform::StockItemMutationResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let command = ReleaseInventoryReservation {
            stock_item_id: stock_id(&input.stock_item_id)?,
            reservation_id: reservation_id(&input.reservation_id)?,
        };
        let (route, envelope) = authenticate_command(
            self.authenticator.as_ref(),
            metadata,
            decode_command(input.metadata, command)?,
        )
        .await?;
        let receipt = self
            .application
            .release_reservation(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(item_mutation_response(
            &receipt,
            &receipt.response.item,
        )))
    }
}

#[async_trait]
impl<A: InventoryQueryApplication, G: SupplyChainRequestAuthenticator> InventoryQueryService
    for InventoryQueryRpc<A, G>
{
    async fn get(
        &self,
        request: Request<platform::GetStockItemRequest>,
    ) -> Result<Response<platform::GetStockItemResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let (route, envelope) = authenticate_query(
            self.authenticator.as_ref(),
            metadata,
            decode_query(
                input.metadata,
                GetStockItem {
                    stock_item_id: stock_id(&input.stock_item_id)?,
                },
            )?,
        )
        .await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetStockItemResponse {
            item: result.value.item.as_ref().map(encode_item),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
    async fn list(
        &self,
        request: Request<platform::ListStockItemsRequest>,
    ) -> Result<Response<platform::ListStockItemsResponse>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let filter = StockItemListFilter {
            warehouse_id: input
                .warehouse_id
                .as_deref()
                .map(|value| {
                    WarehouseId::try_from_uuid(parse_uuid(value, "warehouse_id")?)
                        .map_err(|_| invalid("warehouse_id"))
                })
                .transpose()?,
            item_code: input.item_code,
            page_size: u16::try_from(input.page_size).map_err(|_| invalid("page_size"))?,
            cursor: input.cursor,
        };
        let (route, envelope) = authenticate_query(
            self.authenticator.as_ref(),
            metadata,
            decode_query(input.metadata, ListStockItems { filter })?,
        )
        .await?;
        let result = self
            .application
            .list(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::ListStockItemsResponse {
            items: result.value.items.iter().map(encode_item).collect(),
            next_cursor: result.value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

struct DecodedCommand<C> {
    route: SupplyChainHoldingRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: SupplyChainHoldingRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: SupplyChainRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(SupplyChainHoldingRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(SupplyChainAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: SupplyChainAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated supply-chain authority is invalid"))?;
    Ok((authenticated.holding_route().clone(), envelope))
}
async fn authenticate_query<Q, G: SupplyChainRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(SupplyChainHoldingRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(SupplyChainAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.active_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: SupplyChainAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.active_scope = authenticated.active_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated supply-chain authority is invalid"))?;
    Ok((authenticated.holding_route().clone(), envelope))
}

async fn authenticate_owner_query<Q, G: SupplyChainRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(SupplyChainHoldingRoute, QueryEnvelope<Q>, bool), Status> {
    let (route, envelope) =
        authenticate_query(authenticator, transport_metadata.clone(), decoded).await?;
    if !direct_human_actor(&envelope.actor) {
        return Ok((route, envelope, false));
    }
    let write = authenticator
        .authenticate(SupplyChainAuthenticationRequest {
            transport_metadata,
            presented_actor: envelope.actor.clone(),
            presented_scope: envelope.active_scope.clone(),
            presented_route: route.clone(),
            subscription_id: None,
            mfa_code: None,
            access_mode: SupplyChainAccessMode::Write,
        })
        .await
        .ok();
    let write_authorized = write.as_ref().is_some_and(|authenticated| {
        authenticated.actor() == &envelope.actor
            && authenticated.active_scope() == &envelope.active_scope
            && authenticated.holding_route() == &route
    });
    Ok((route, envelope, write_authorized))
}

fn direct_human_actor(actor: &ActorAttribution) -> bool {
    actor.is_human()
        && actor.session_id.is_some()
        && actor.represented_principal_ref.is_none()
        && actor.support_capability_id.is_none()
}

fn procurement_owner_capabilities(
    projected_version: u64,
    freshness: FreshnessState,
    target: ProcurementOwnerCommandTarget,
    evaluation: Option<&OwnerCommandEvaluation>,
) -> platform::ProcurementOwnerCapabilities {
    let exact = evaluation.is_some_and(|evaluation| {
        evaluation.allowed
            && evaluation.source_version == projected_version
            && freshness == FreshnessState::Current
    });
    platform::ProcurementOwnerCapabilities {
        source_version: evaluation.map_or(0, |evaluation| evaluation.source_version),
        approve_requisition: exact
            && matches!(target, ProcurementOwnerCommandTarget::ApproveRequisition(_)),
        release_purchase_order: exact
            && matches!(
                target,
                ProcurementOwnerCommandTarget::ReleasePurchaseOrder(_)
            ),
        accept_goods_receipt: exact
            && matches!(target, ProcurementOwnerCommandTarget::AcceptGoodsReceipt(_)),
        evaluated_at: evaluation
            .map(|evaluation| evaluation.evaluated_at.to_rfc3339())
            .unwrap_or_default(),
    }
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key"))?,
        correlation_id,
        causation_id: metadata
            .causation_id
            .map(|value| parse_event_id(&value))
            .transpose()?,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: metadata
            .subscription_id
            .filter(|value| !value.trim().is_empty()),
        mfa_code: metadata.mfa_code.filter(|value| !value.trim().is_empty()),
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.holding_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        active_scope: decode_scope(
            metadata
                .active_scope
                .ok_or_else(|| invalid("active_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.active_scope)?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_route(value: Option<common::HoldingRoute>) -> Result<SupplyChainHoldingRoute, Status> {
    let value = value.ok_or_else(|| invalid("holding route is required"))?;
    let route = SupplyChainHoldingRoute {
        holding_id: value.holding_id.trim().into(),
        data_profile_key: value.data_profile_key.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(route: &SupplyChainHoldingRoute) -> Result<(), Status> {
    if [&route.holding_id, &route.data_profile_key]
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("holding route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(
    route: &SupplyChainHoldingRoute,
    scope: &AuthorityScope,
) -> Result<(), Status> {
    let holding = match scope {
        AuthorityScope::Platform => return Ok(()),
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.data_profile_key, "data_profile_key")? != *holding {
        Err(invalid("holding route scope mismatch"))
    } else {
        Ok(())
    }
}
fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::HoldingUser) => ActorKind::HoldingUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: optional_uuid(&value.session_id, "session_id")?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id"))?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
        support_capability_id: optional_uuid(
            &value.support_capability_id,
            "support_capability_id",
        )?,
    })
}
fn decode_audit(
    value: Option<common::AuditContext>,
    correlation_id: CorrelationId,
) -> Result<AuditContext, Status> {
    let value = value.ok_or_else(|| invalid("audit is required"))?;
    if !value.correlation_id.trim().is_empty()
        && parse_correlation_id(&value.correlation_id)? != correlation_id
    {
        return Err(invalid("audit correlation mismatch"));
    }
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: nonempty(value.module),
    })
}
fn decode_scope(value: common::AuthorityScope) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match value
        .scope
        .ok_or_else(|| invalid("holding scope is empty"))?
    {
        Scope::Platform(_) => Ok(AuthorityScope::Platform),
        Scope::Holding(v) => Ok(AuthorityScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(AuthorityScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(AuthorityScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(AuthorityScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            organization_node_id: parse_uuid(&v.organization_node_id, "organization_node_id")?,
        }),
    }
}
fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|v| v.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(v)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: v.projection,
                partition: v.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: parse_event_id(&v.event_id)?,
                    global_sequence: v.global_sequence,
                },
                timeout_millis: v.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate consistency is unsupported"))
        }
    }
}

fn decode_order_line(
    value: platform::PurchaseOrderLineRecord,
) -> Result<PurchaseOrderLine, Status> {
    Ok(PurchaseOrderLine {
        line_id: PurchaseOrderLineId::try_from_uuid(parse_uuid(&value.line_id, "line_id")?)
            .map_err(|_| invalid("line_id"))?,
        item_code: value.item_code,
        ordered_quantity: value.ordered_quantity,
        unit_price_minor: value.unit_price_minor,
        currency: value.currency,
        destination_code: value.destination_code,
    })
}
fn decode_requisition_line(
    value: platform::PurchaseRequisitionLineRecord,
) -> Result<PurchaseRequisitionLine, Status> {
    Ok(PurchaseRequisitionLine {
        line_id: erpx_procurement::PurchaseRequisitionLineId::try_from_uuid(parse_uuid(
            &value.line_id,
            "requisition_line_id",
        )?)
        .map_err(|_| invalid("requisition_line_id"))?,
        item_code: value.item_code,
        requested_quantity: value.requested_quantity,
        estimated_unit_price_minor: value.estimated_unit_price_minor,
        currency: value.currency,
        destination_code: value.destination_code,
    })
}

fn decode_goods_receipt_line(
    value: platform::GoodsReceiptLineRecord,
) -> Result<GoodsReceiptLine, Status> {
    Ok(GoodsReceiptLine {
        line_id: erpx_procurement::GoodsReceiptLineId::try_from_uuid(parse_uuid(
            &value.line_id,
            "goods_receipt_line_id",
        )?)
        .map_err(|_| invalid("goods_receipt_line_id"))?,
        purchase_order_line_id: PurchaseOrderLineId::try_from_uuid(parse_uuid(
            &value.purchase_order_line_id,
            "purchase_order_line_id",
        )?)
        .map_err(|_| invalid("purchase_order_line_id"))?,
        inventory_stock_item_id: InventoryStockItemId::try_from_uuid(parse_uuid(
            &value.inventory_stock_item_id,
            "inventory_stock_item_id",
        )?)
        .map_err(|_| invalid("inventory_stock_item_id"))?,
        received_quantity: value.received_quantity,
    })
}
fn decode_order_status(value: i32) -> Result<PurchaseOrderStatus, Status> {
    match platform::PurchaseOrderStatus::try_from(value).ok() {
        Some(platform::PurchaseOrderStatus::Draft) => Ok(PurchaseOrderStatus::Draft),
        Some(platform::PurchaseOrderStatus::Released) => Ok(PurchaseOrderStatus::Released),
        Some(platform::PurchaseOrderStatus::Cancelled) => Ok(PurchaseOrderStatus::Cancelled),
        _ => Err(invalid("purchase order status")),
    }
}
fn encode_order(view: &PurchaseOrderView) -> platform::PurchaseOrderRecord {
    platform::PurchaseOrderRecord {
        purchase_order_id: view.purchase_order_id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        order_code: view.order_code.clone(),
        supplier_id: view.supplier_id.as_uuid().to_string(),
        currency: view.currency.clone(),
        lines: view
            .lines
            .iter()
            .map(|line| platform::PurchaseOrderLineRecord {
                line_id: line.line_id.as_uuid().to_string(),
                item_code: line.item_code.clone(),
                ordered_quantity: line.ordered_quantity,
                unit_price_minor: line.unit_price_minor,
                currency: line.currency.clone(),
                destination_code: line.destination_code.clone(),
            })
            .collect(),
        status: match view.status {
            PurchaseOrderStatus::Draft => platform::PurchaseOrderStatus::Draft as i32,
            PurchaseOrderStatus::Released => platform::PurchaseOrderStatus::Released as i32,
            PurchaseOrderStatus::Cancelled => platform::PurchaseOrderStatus::Cancelled as i32,
        },
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
        source_requisition: view.source_requisition.as_ref().map(encode_record_version),
        contract: view.contract.as_ref().map(encode_record_version),
    }
}

fn encode_requisition(view: &PurchaseRequisitionView) -> platform::PurchaseRequisitionRecord {
    platform::PurchaseRequisitionRecord {
        requisition_id: view.requisition_id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        requisition_code: view.requisition_code.clone(),
        requesting_department_code: view.requesting_department_code.clone(),
        budget_code: view.budget_code.clone(),
        currency: view.currency.clone(),
        lines: view
            .lines
            .iter()
            .map(|line| platform::PurchaseRequisitionLineRecord {
                line_id: line.line_id.as_uuid().to_string(),
                item_code: line.item_code.clone(),
                requested_quantity: line.requested_quantity,
                estimated_unit_price_minor: line.estimated_unit_price_minor,
                currency: line.currency.clone(),
                destination_code: line.destination_code.clone(),
            })
            .collect(),
        status: match view.status {
            PurchaseRequisitionStatus::Draft => platform::PurchaseRequisitionStatus::Draft as i32,
            PurchaseRequisitionStatus::Submitted => {
                platform::PurchaseRequisitionStatus::Submitted as i32
            }
            PurchaseRequisitionStatus::Approved => {
                platform::PurchaseRequisitionStatus::Approved as i32
            }
            PurchaseRequisitionStatus::Rejected => {
                platform::PurchaseRequisitionStatus::Rejected as i32
            }
            PurchaseRequisitionStatus::Cancelled => {
                platform::PurchaseRequisitionStatus::Cancelled as i32
            }
        },
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn encode_goods_receipt(view: &GoodsReceiptView) -> platform::GoodsReceiptRecord {
    platform::GoodsReceiptRecord {
        goods_receipt_id: view.goods_receipt_id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        receipt_code: view.receipt_code.clone(),
        source_purchase_order: Some(encode_record_version(&view.source_purchase_order)),
        lines: view
            .lines
            .iter()
            .map(|line| platform::GoodsReceiptLineRecord {
                line_id: line.line_id.as_uuid().to_string(),
                purchase_order_line_id: line.purchase_order_line_id.as_uuid().to_string(),
                inventory_stock_item_id: line.inventory_stock_item_id.as_uuid().to_string(),
                received_quantity: line.received_quantity,
            })
            .collect(),
        status: match view.status {
            GoodsReceiptStatus::PendingAcceptance => {
                platform::GoodsReceiptStatus::PendingAcceptance as i32
            }
            GoodsReceiptStatus::Accepted => platform::GoodsReceiptStatus::Accepted as i32,
            GoodsReceiptStatus::Rejected => platform::GoodsReceiptStatus::Rejected as i32,
            GoodsReceiptStatus::Cancelled => platform::GoodsReceiptStatus::Cancelled as i32,
        },
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}

fn decode_record_version(value: platform::RecordVersionRef) -> Result<RecordVersionRef, Status> {
    let record_type = RecordTypeId::try_new(value.record_type)
        .map_err(|_| invalid("record_version.record_type"))?;
    let owner = record_type.descriptor().owner;
    if value.owner_context != owner.slug() {
        return Err(invalid("record_version.owner_context"));
    }
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(parse_uuid(&value.record_id, "record_version.record_id")?)
            .map_err(|_| invalid("record_version.record_id"))?,
        owner,
        record_type,
    )
    .map_err(|_| invalid("record_version"))?;
    RecordVersionRef::try_new(record, value.version).map_err(|_| invalid("record_version.version"))
}

fn encode_record_version(value: &RecordVersionRef) -> platform::RecordVersionRef {
    platform::RecordVersionRef {
        record_id: value.record().id().to_string(),
        version: value.version(),
        owner_context: value.record().owner_context().slug().into(),
        record_type: value.record().record_type().as_str().into(),
    }
}
fn encode_item(view: &StockItemView) -> platform::StockItemRecord {
    platform::StockItemRecord {
        stock_item_id: view.stock_item_id.as_uuid().to_string(),
        source_version: view.version,
        scope: Some(encode_scope(&view.scope)),
        warehouse_id: view.warehouse_id.as_uuid().to_string(),
        item_code: view.item_code.clone(),
        on_hand: view.on_hand,
        reserved: view.reserved,
        available: view.available,
        reservations: view
            .reservations
            .iter()
            .map(|v| platform::InventoryReservationRecord {
                reservation_id: v.reservation_id.as_uuid().to_string(),
                quantity: v.quantity,
                demand_reference: v.demand_reference.clone(),
                status: match v.status {
                    ReservationStatus::Active => {
                        platform::InventoryReservationStatus::Active as i32
                    }
                    ReservationStatus::Released => {
                        platform::InventoryReservationStatus::Released as i32
                    }
                },
            })
            .collect(),
        created_at: view.created_at.to_rfc3339(),
        updated_at: view.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn order_mutation_response<R>(
    receipt: &CommandReceipt<R>,
    view: &PurchaseOrderView,
) -> platform::PurchaseOrderMutationResponse {
    platform::PurchaseOrderMutationResponse {
        receipt: Some(encode_receipt(receipt)),
        order: Some(encode_order(view)),
    }
}
fn item_mutation_response<R>(
    receipt: &CommandReceipt<R>,
    view: &StockItemView,
) -> platform::StockItemMutationResponse {
    platform::StockItemMutationResponse {
        receipt: Some(encode_receipt(receipt)),
        item: Some(encode_item(view)),
    }
}

fn requisition_mutation_response(
    stored: &StoredProcurementWorkflowCommit,
) -> Result<platform::PurchaseRequisitionMutationResponse, Status> {
    let erpx_procurement::ProcurementWorkflowAggregate::Requisition(value) = &stored.aggregate
    else {
        return Err(Status::internal(
            "Procurement workflow response type mismatch",
        ));
    };
    Ok(platform::PurchaseRequisitionMutationResponse {
        receipt: Some(encode_workflow_receipt(
            stored,
            "purchase_requisition",
            value.id().as_uuid(),
            value.version(),
        )),
        requisition: Some(encode_requisition(&PurchaseRequisitionView::from(value))),
    })
}

fn goods_receipt_mutation_response(
    stored: &StoredProcurementWorkflowCommit,
) -> Result<platform::GoodsReceiptMutationResponse, Status> {
    let erpx_procurement::ProcurementWorkflowAggregate::GoodsReceipt(value) = &stored.aggregate
    else {
        return Err(Status::internal(
            "Procurement workflow response type mismatch",
        ));
    };
    Ok(platform::GoodsReceiptMutationResponse {
        receipt: Some(encode_workflow_receipt(
            stored,
            "goods_receipt",
            value.id().as_uuid(),
            value.version(),
        )),
        goods_receipt: Some(encode_goods_receipt(&GoodsReceiptView::from(value))),
    })
}

fn encode_workflow_receipt(
    stored: &StoredProcurementWorkflowCommit,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: stored.command_id.to_string(),
        correlation_id: stored.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: "procurement".into(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.to_string(),
            version,
        }),
        committed_events: stored
            .committed_events
            .iter()
            .map(|position| common::EventPosition {
                event_id: position.event_id.to_string(),
                global_sequence: position.global_sequence,
            })
            .collect(),
        committed_at: stored.committed_at.to_rfc3339(),
        replayed: stored.replayed,
    }
}

fn encode_receipt<R>(receipt: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: receipt.command_id.to_string(),
        correlation_id: receipt.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: receipt.aggregate.context.clone(),
            aggregate_type: receipt.aggregate.aggregate_type.clone(),
            aggregate_id: receipt.aggregate.aggregate_id.to_string(),
            version: receipt.aggregate.version,
        }),
        committed_events: receipt
            .committed_events
            .iter()
            .map(|v| common::EventPosition {
                event_id: v.event_id.to_string(),
                global_sequence: v.global_sequence,
            })
            .collect(),
        committed_at: receipt.committed_at.to_rfc3339(),
        replayed: receipt.replayed,
    }
}
fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(encode_projection_metadata(&value.projection)),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}
fn encode_projection_metadata(value: &ProjectionMetadata) -> common::ProjectionMetadata {
    common::ProjectionMetadata {
        checkpoint: value.checkpoint.as_ref().map(encode_checkpoint),
        freshness: match value.freshness {
            FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
            FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
            FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
            FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
        },
        lag_millis: value.lag_millis,
    }
}
fn encode_checkpoint(value: &ProjectionCheckpoint) -> common::ProjectionCheckpoint {
    common::ProjectionCheckpoint {
        projection: value.projection.clone(),
        partition: value.partition.clone(),
        event_id: value.event_id.to_string(),
        global_sequence: value.global_sequence,
        updated_at: value.updated_at.to_rfc3339(),
    }
}
#[allow(deprecated)]
fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    let requirement = match value {
        QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
        QueryConsistency::ReadYourWrites(v) => {
            Requirement::ReadYourWrites(common::ReadYourWrites {
                command_id: String::new(),
                projection: v.projection.clone(),
                partition: v.partition.clone(),
                event_id: v.target.event_id.to_string(),
                global_sequence: v.target.global_sequence,
                timeout_millis: v.timeout_millis,
            })
        }
    };
    common::QueryConsistency {
        requirement: Some(requirement),
    }
}
fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    let scope = match value {
        AuthorityScope::Platform => Scope::Platform(common::PlatformScope {}),
        AuthorityScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(common::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(common::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => Scope::Subtree(common::SubtreeScope {
            holding_id: holding_id.to_string(),
            organization_node_id: organization_node_id.to_string(),
        }),
    };
    common::AuthorityScope { scope: Some(scope) }
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    let value = Uuid::parse_str(value.trim()).map_err(|_| invalid(field))?;
    if value.is_nil() {
        Err(invalid(field))
    } else {
        Ok(value)
    }
}
fn optional_uuid(value: &str, field: &'static str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, field).map(Some)
    }
}
fn parse_event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?).map_err(|_| invalid("event_id"))
}
fn parse_correlation_id(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id"))
}
fn procurement_id(value: &str) -> Result<PurchaseOrderId, Status> {
    PurchaseOrderId::try_from_uuid(parse_uuid(value, "purchase_order_id")?)
        .map_err(|_| invalid("purchase_order_id"))
}
fn requisition_id(value: &str) -> Result<PurchaseRequisitionId, Status> {
    PurchaseRequisitionId::try_from_uuid(parse_uuid(value, "requisition_id")?)
        .map_err(|_| invalid("requisition_id"))
}
fn goods_receipt_id(value: &str) -> Result<GoodsReceiptId, Status> {
    GoodsReceiptId::try_from_uuid(parse_uuid(value, "goods_receipt_id")?)
        .map_err(|_| invalid("goods_receipt_id"))
}
fn stock_id(value: &str) -> Result<StockItemId, Status> {
    StockItemId::try_from_uuid(parse_uuid(value, "stock_item_id")?)
        .map_err(|_| invalid("stock_item_id"))
}
fn movement_id(value: &str) -> Result<StockMovementId, Status> {
    StockMovementId::try_from_uuid(parse_uuid(value, "movement_id")?)
        .map_err(|_| invalid("movement_id"))
}
fn reservation_id(value: &str) -> Result<ReservationId, Status> {
    ReservationId::try_from_uuid(parse_uuid(value, "reservation_id")?)
        .map_err(|_| invalid("reservation_id"))
}
fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
fn invalid(message: &'static str) -> Status {
    Status::invalid_argument(message)
}
fn status_from_cqrs(error: CqrsError) -> Status {
    match error.category() {
        ErrorCategory::Invalid => Status::invalid_argument("supply-chain request is invalid"),
        ErrorCategory::Denied => Status::permission_denied("supply-chain request is denied"),
        ErrorCategory::NotFound => Status::not_found("supply-chain record was not found"),
        ErrorCategory::Conflict => Status::aborted("supply-chain record changed"),
        ErrorCategory::NotReady => Status::unavailable("supply-chain projection is not ready"),
        ErrorCategory::Unavailable => Status::unavailable("supply-chain is unavailable"),
        ErrorCategory::Internal => Status::internal("supply-chain operation failed"),
    }
}
fn status_from_authentication(error: SupplyChainAuthenticationError) -> Status {
    match error {
        SupplyChainAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication is required")
        }
        SupplyChainAuthenticationError::Denied => Status::permission_denied("authority is denied"),
        SupplyChainAuthenticationError::Unavailable => {
            Status::unavailable("authentication is unavailable")
        }
        SupplyChainAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_statuses_and_nil_ids_fail_closed() {
        assert_eq!(
            decode_order_status(platform::PurchaseOrderStatus::Released as i32).unwrap(),
            PurchaseOrderStatus::Released
        );
        assert!(stock_id(&Uuid::nil().to_string()).is_err());
    }
    #[test]
    fn module_has_no_pool_construction() {
        let source = include_str!("procurement_inventory_cqrs.rs");
        assert!(!source.contains(concat!("Pg", "Pool")));
        assert!(!source.contains(concat!("Write", "Pool")));
    }

    #[test]
    fn owner_capabilities_require_current_exact_projection_and_only_name_real_commands() {
        let target = ProcurementOwnerCommandTarget::ApproveRequisition(
            PurchaseRequisitionId::try_from_uuid(Uuid::from_u128(10)).unwrap(),
        );
        let evaluation = OwnerCommandEvaluation {
            source_version: 4,
            allowed: true,
            evaluated_at: chrono::Utc::now(),
        };
        let current =
            procurement_owner_capabilities(4, FreshnessState::Current, target, Some(&evaluation));
        assert!(current.approve_requisition);
        assert!(!current.release_purchase_order);
        assert!(!current.accept_goods_receipt);
        assert!(
            !procurement_owner_capabilities(4, FreshnessState::Stale, target, Some(&evaluation),)
                .approve_requisition
        );
        assert!(
            !procurement_owner_capabilities(3, FreshnessState::Current, target, Some(&evaluation),)
                .approve_requisition
        );
    }
}
