use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_maintenance::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, GetWorkOrder, GetWorkOrderResult,
    ListWorkOrders, MaintenanceMutationResult, OpenWorkOrder, OpenWorkOrderResult, RecordDowntime,
    ResumeWorkOrder, ScheduleWorkOrder, StartWorkOrder, WorkOrderList, WorkOrderStatus,
    WorkOrderView,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{AssetFactItem, DowntimeItem, MaintenanceRequestItem, WorkOrderItem, WorkOrderItems},
    presenter::MaintenanceContent,
};

pub fn present_work_order(
    value: QueryResult<GetWorkOrderResult>,
) -> QueryResult<Option<MaintenanceContent>> {
    QueryResult {
        value: value
            .value
            .work_order
            .map(WorkOrderItem::from)
            .map(|value| MaintenanceContent::WorkOrder(Box::new(value))),
        metadata: value.metadata,
    }
}

pub fn present_work_orders(
    value: QueryResult<WorkOrderList>,
) -> QueryResult<Option<MaintenanceContent>> {
    let metadata = value.metadata;
    let items = WorkOrderItems::from(value.value);
    QueryResult {
        value: (!items.work_orders.is_empty())
            .then_some(items)
            .map(MaintenanceContent::WorkOrders),
        metadata,
    }
}

impl From<WorkOrderView> for WorkOrderItem {
    fn from(value: WorkOrderView) -> Self {
        Self {
            work_order_id: value.work_order_id.as_uuid().to_string(),
            version: value.version,
            code: value.code,
            scope: format!("{:?}", value.scope),
            request: MaintenanceRequestItem {
                source_event_id: value.request.source_event_id.as_uuid().to_string(),
                source_aggregate_version: value.request.source_aggregate_version,
                asset_id: value.request.asset.as_uuid().to_string(),
                request_code: value.request.request_code,
            },
            asset_fact: AssetFactItem {
                asset_id: value.asset_fact.asset.as_uuid().to_string(),
                asset_code: value.asset_fact.asset_code.as_str().into(),
                name: value.asset_fact.name.as_str().into(),
                location_id: value.asset_fact.location.as_uuid().to_string(),
                condition: format!("{:?}", value.asset_fact.condition).to_ascii_lowercase(),
            },
            title: value.title,
            status: match value.status {
                WorkOrderStatus::Planned => "planned",
                WorkOrderStatus::Scheduled => "scheduled",
                WorkOrderStatus::InProgress => "in_progress",
                WorkOrderStatus::Blocked => "blocked",
                WorkOrderStatus::Completed => "completed",
                WorkOrderStatus::Cancelled => "cancelled",
            }
            .into(),
            scheduled_for: value.scheduled_for.map(|time| time.to_rfc3339()),
            started_at: value.started_at.map(|time| time.to_rfc3339()),
            completed_at: value.completed_at.map(|time| time.to_rfc3339()),
            downtime: value
                .downtime
                .into_iter()
                .map(|entry| DowntimeItem {
                    downtime_id: entry.downtime_id.to_string(),
                    started_at: entry.started_at.to_rfc3339(),
                    ended_at: entry.ended_at.to_rfc3339(),
                    reason: entry.reason,
                })
                .collect(),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<WorkOrderList> for WorkOrderItems {
    fn from(value: WorkOrderList) -> Self {
        Self {
            work_orders: value
                .work_orders
                .into_iter()
                .map(WorkOrderItem::from)
                .collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait MaintenanceProtoTransport: Send + Sync {
    async fn open(
        &self,
        value: CommandEnvelope<OpenWorkOrder>,
    ) -> Result<CommandReceipt<OpenWorkOrderResult>, UiError>;
    async fn schedule(
        &self,
        value: CommandEnvelope<ScheduleWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn start(
        &self,
        value: CommandEnvelope<StartWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn block(
        &self,
        value: CommandEnvelope<BlockWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn resume(
        &self,
        value: CommandEnvelope<ResumeWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn complete(
        &self,
        value: CommandEnvelope<CompleteWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn cancel(
        &self,
        value: CommandEnvelope<CancelWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn record_downtime(
        &self,
        value: CommandEnvelope<RecordDowntime>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError>;
    async fn get(
        &self,
        value: QueryEnvelope<GetWorkOrder>,
    ) -> Result<QueryResult<GetWorkOrderResult>, UiError>;
    async fn list(
        &self,
        value: QueryEnvelope<ListWorkOrders>,
    ) -> Result<QueryResult<WorkOrderList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct MaintenanceGateway<T> {
    transport: T,
}
impl<T> MaintenanceGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}

macro_rules! command_gateway {
    ($command:ty, $response:ty, $method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T: MaintenanceProtoTransport> CommandGateway<$command> for MaintenanceGateway<T> {
            type Response = $response;
            async fn execute(
                &self,
                value: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(value).await
            }
        }
    };
}

command_gateway!(OpenWorkOrder, OpenWorkOrderResult, open);
command_gateway!(ScheduleWorkOrder, MaintenanceMutationResult, schedule);
command_gateway!(StartWorkOrder, MaintenanceMutationResult, start);
command_gateway!(BlockWorkOrder, MaintenanceMutationResult, block);
command_gateway!(ResumeWorkOrder, MaintenanceMutationResult, resume);
command_gateway!(CompleteWorkOrder, MaintenanceMutationResult, complete);
command_gateway!(CancelWorkOrder, MaintenanceMutationResult, cancel);
command_gateway!(RecordDowntime, MaintenanceMutationResult, record_downtime);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: MaintenanceProtoTransport> QueryGateway<GetWorkOrder> for MaintenanceGateway<T> {
    type Response = GetWorkOrderResult;
    async fn execute(
        &self,
        value: QueryEnvelope<GetWorkOrder>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(value).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: MaintenanceProtoTransport> QueryGateway<ListWorkOrders> for MaintenanceGateway<T> {
    type Response = WorkOrderList;
    async fn execute(
        &self,
        value: QueryEnvelope<ListWorkOrders>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.list(value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command<C, R, G: CommandGateway<C, Response = R>>() {}
    fn query<Q, R, G: QueryGateway<Q, Response = R>>() {}
    struct ContractTransport;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl MaintenanceProtoTransport for ContractTransport {
        async fn open(
            &self,
            _: CommandEnvelope<OpenWorkOrder>,
        ) -> Result<CommandReceipt<OpenWorkOrderResult>, UiError> {
            unreachable!()
        }
        async fn schedule(
            &self,
            _: CommandEnvelope<ScheduleWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn start(
            &self,
            _: CommandEnvelope<StartWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn block(
            &self,
            _: CommandEnvelope<BlockWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn resume(
            &self,
            _: CommandEnvelope<ResumeWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn complete(
            &self,
            _: CommandEnvelope<CompleteWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn cancel(
            &self,
            _: CommandEnvelope<CancelWorkOrder>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn record_downtime(
            &self,
            _: CommandEnvelope<RecordDowntime>,
        ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: QueryEnvelope<GetWorkOrder>,
        ) -> Result<QueryResult<GetWorkOrderResult>, UiError> {
            unreachable!()
        }
        async fn list(
            &self,
            _: QueryEnvelope<ListWorkOrders>,
        ) -> Result<QueryResult<WorkOrderList>, UiError> {
            unreachable!()
        }
    }
    #[test]
    fn gateway_covers_every_typed_operation() {
        type Gateway = MaintenanceGateway<ContractTransport>;
        command::<OpenWorkOrder, OpenWorkOrderResult, Gateway>();
        command::<ScheduleWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<StartWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<BlockWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<ResumeWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<CompleteWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<CancelWorkOrder, MaintenanceMutationResult, Gateway>();
        command::<RecordDowntime, MaintenanceMutationResult, Gateway>();
        query::<GetWorkOrder, GetWorkOrderResult, Gateway>();
        query::<ListWorkOrders, WorkOrderList, Gateway>();
    }
}
