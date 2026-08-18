//! Typed inbound adapters for the paired Procurement and Inventory contexts.
//! Pool construction and canonical authentication remain composition concerns.

use std::sync::Arc;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey,
    ProjectionCheckpoint, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryMetadata,
    QueryResult, ReadYourWrites,
};
use erpx_integration_contracts::procurement as procurement_contract;
use erpx_inventory::{
    GetStockItem, GetStockItemResult, InventoryMutationResult, IssueStock, ListStockItems,
    ReceiveStock, RegisterStockItem, RegisterStockItemResult, ReleaseInventoryReservation,
    ReservationId, ReservationStatus, ReserveInventory, StockItemId, StockItemList,
    StockItemListFilter, StockItemView, StockMovementId, WarehouseId,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_procurement::{
    CancelPurchaseOrder, CreatePurchaseOrder, CreatePurchaseOrderResult, GetPurchaseOrder,
    GetPurchaseOrderResult, ListPurchaseOrders, PurchaseOrderId, PurchaseOrderLine,
    PurchaseOrderLineId, PurchaseOrderList, PurchaseOrderListFilter, PurchaseOrderMutationResult,
    PurchaseOrderStatus, PurchaseOrderView, ReleasePurchaseOrder, SupplierId,
};
use erpx_proto::common;
mod platform {
    pub use erpx_proto::erpx::inventory::v2::*;
    pub use erpx_proto::erpx::procurement::v2::*;
}
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

use platform::inventory_command_service_server::InventoryCommandService;
use platform::inventory_query_service_server::InventoryQueryService;
use platform::procurement_command_service_server::ProcurementCommandService;
use platform::procurement_query_service_server::ProcurementQueryService;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplyChainTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyChainAccessMode {
    Read,
    Write,
}

pub struct SupplyChainAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: SupplyChainTenantRoute,
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
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &SupplyChainTenantRoute {
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
    tenant_scope: TenantScope,
    tenant_route: SupplyChainTenantRoute,
}

impl AuthenticatedSupplyChainRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        tenant_route: SupplyChainTenantRoute,
    ) -> Result<Self, SupplyChainAuthenticationError> {
        actor
            .validate()
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        validate_route(&tenant_route).map_err(|_| SupplyChainAuthenticationError::Internal)?;
        validate_route_scope(&tenant_route, &tenant_scope)
            .map_err(|_| SupplyChainAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            tenant_route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
    pub const fn tenant_route(&self) -> &SupplyChainTenantRoute {
        &self.tenant_route
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
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<CreatePurchaseOrder>,
    ) -> Result<CommandReceipt<CreatePurchaseOrderResult>, CqrsError>;
    async fn release(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<ReleasePurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, CqrsError>;
    async fn cancel(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<CancelPurchaseOrder>,
    ) -> Result<CommandReceipt<PurchaseOrderMutationResult>, CqrsError>;
}

#[async_trait]
pub trait ProcurementQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: SupplyChainTenantRoute,
        envelope: QueryEnvelope<GetPurchaseOrder>,
    ) -> Result<QueryResult<GetPurchaseOrderResult>, CqrsError>;
    async fn list(
        &self,
        route: SupplyChainTenantRoute,
        envelope: QueryEnvelope<ListPurchaseOrders>,
    ) -> Result<QueryResult<PurchaseOrderList>, CqrsError>;
}

#[async_trait]
pub trait InventoryCommandApplication: Send + Sync + 'static {
    async fn register(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<RegisterStockItem>,
    ) -> Result<CommandReceipt<RegisterStockItemResult>, CqrsError>;
    async fn receive(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<ReceiveStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn issue(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<IssueStock>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn reserve(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<ReserveInventory>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
    async fn release_reservation(
        &self,
        route: SupplyChainTenantRoute,
        envelope: CommandEnvelope<ReleaseInventoryReservation>,
    ) -> Result<CommandReceipt<InventoryMutationResult>, CqrsError>;
}

#[async_trait]
pub trait InventoryQueryApplication: Send + Sync + 'static {
    async fn get(
        &self,
        route: SupplyChainTenantRoute,
        envelope: QueryEnvelope<GetStockItem>,
    ) -> Result<QueryResult<GetStockItemResult>, CqrsError>;
    async fn list(
        &self,
        route: SupplyChainTenantRoute,
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
        let decoded = decode_query(
            input.metadata,
            GetPurchaseOrder {
                purchase_order_id: procurement_id(&input.purchase_order_id)?,
            },
        )?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), metadata, decoded).await?;
        let result = self
            .application
            .get(route, envelope)
            .await
            .map_err(status_from_cqrs)?;
        Ok(Response::new(platform::GetPurchaseOrderResponse {
            order: result.value.order.as_ref().map(encode_order),
            metadata: Some(encode_query_metadata(&result.metadata)),
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
    route: SupplyChainTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: SupplyChainTenantRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: SupplyChainRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(SupplyChainTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = authenticator
        .authenticate(SupplyChainAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: SupplyChainAccessMode::Write,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated supply-chain authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
}
async fn authenticate_query<Q, G: SupplyChainRequestAuthenticator>(
    authenticator: &G,
    transport_metadata: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(SupplyChainTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = authenticator
        .authenticate(SupplyChainAuthenticationRequest {
            transport_metadata,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: SupplyChainAccessMode::Read,
        })
        .await
        .map_err(status_from_authentication)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated supply-chain authority is invalid"))?;
    Ok((authenticated.tenant_route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
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
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
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
    let route = decode_route(metadata.tenant_route)?;
    let correlation_id = parse_correlation_id(&metadata.correlation_id)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id"))?,
        correlation_id,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit, correlation_id)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    validate_route_scope(&route, &envelope.tenant_scope)?;
    Ok(DecodedQuery { route, envelope })
}

fn decode_route(value: Option<common::TenantRoute>) -> Result<SupplyChainTenantRoute, Status> {
    let value = value.ok_or_else(|| invalid("tenant route is required"))?;
    let route = SupplyChainTenantRoute {
        tenant_id: value.tenant_id.trim().into(),
        holding_ref: value.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(route: &SupplyChainTenantRoute) -> Result<(), Status> {
    if [&route.tenant_id, &route.holding_ref]
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(route: &SupplyChainTenantRoute, scope: &TenantScope) -> Result<(), Status> {
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.holding_ref, "holding_ref")? != *holding {
        Err(invalid("tenant route scope mismatch"))
    } else {
        Ok(())
    }
}
fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
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
fn decode_scope(value: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match value
        .scope
        .ok_or_else(|| invalid("tenant scope is empty"))?
    {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(v) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
        }),
        Scope::Company(v) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
        }),
        Scope::Branch(v) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_id: parse_uuid(&v.company_id, "company_id")?,
            branch_id: parse_uuid(&v.branch_id, "branch_id")?,
        }),
        Scope::Subtree(v) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&v.holding_id, "holding_id")?,
            company_ids: v
                .company_ids
                .iter()
                .map(|x| parse_uuid(x, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: v
                .branch_ids
                .iter()
                .map(|x| parse_uuid(x, "branch_id"))
                .collect::<Result<_, _>>()?,
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
fn encode_scope(value: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    let scope = match value {
        TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
        TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
            holding_id: holding_id.to_string(),
        }),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company(common::CompanyScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch(common::BranchScope {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.to_string(),
        }),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree(common::SubtreeScope {
            holding_id: holding_id.to_string(),
            company_ids: company_ids.iter().map(ToString::to_string).collect(),
            branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
        }),
    };
    common::TenantScope { scope: Some(scope) }
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
}
