use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::{DisplayName, ReferenceCode, assets};
use erpx_kernel::EventId;
use erpx_maintenance::{
    BlockWorkOrder, CancelWorkOrder, CompleteWorkOrder, Downtime, GetWorkOrder, GetWorkOrderResult,
    ListWorkOrders, MaintenanceMutationResult, MaintenanceRequest, OpenWorkOrder,
    OpenWorkOrderResult, RecordDowntime, ResumeWorkOrder, ScheduleWorkOrder, StartWorkOrder,
    WorkOrderId, WorkOrderList, WorkOrderStatus, WorkOrderView,
};
use erpx_proto::erpx::assets::v1::AssetCondition as WireAssetCondition;
use erpx_proto::erpx::maintenance::v1 as wire;
use erpx_ui_foundation::UiError;

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, unsupported_ui, uuid,
};
use crate::{features::maintenance::MaintenanceProtoTransport, transport::typed_cqrs::GrpcChannel};

type Commands =
    wire::maintenance_command_service_client::MaintenanceCommandServiceClient<GrpcChannel>;
type Queries = wire::maintenance_query_service_client::MaintenanceQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct MaintenanceGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl MaintenanceGrpcTransport {
    pub(crate) fn try_active_scope(
        channel: GrpcChannel,
        token: impl Into<String>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_active_scope(token)?,
            commands: Commands::new(channel.clone()),
            queries: Queries::new(channel),
        })
    }

    pub(crate) fn try_new(
        channel: GrpcChannel,
        token: impl Into<String>,
        route: Wave67Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(token, route)?,
            commands: Commands::new(channel.clone()),
            queries: Queries::new(channel),
        })
    }
}

fn result<R>(
    response: wire::MaintenanceWorkOrderMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation: erpx_kernel::CorrelationId,
    build: impl FnOnce(WorkOrderView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    let work_order = decode_work_order(
        response
            .work_order
            .ok_or_else(|| invalid_response("decode maintenance work order"))?,
    )?;
    command_result(response.receipt, build(work_order), command_id, correlation)
}
fn identity(
    metadata: erpx_proto::common::v1::CommandRequestMetadata,
    value: WorkOrderId,
) -> wire::MaintenanceWorkOrderIdentityRequest {
    wire::MaintenanceWorkOrderIdentityRequest {
        metadata: Some(metadata),
        work_order_id: value.as_uuid().to_string(),
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl MaintenanceProtoTransport for MaintenanceGrpcTransport {
    async fn open(
        &self,
        envelope: CommandEnvelope<OpenWorkOrder>,
    ) -> Result<CommandReceipt<OpenWorkOrderResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let source = value.request;
        let assets::Event::MaintenanceRequested {
            asset,
            request_code,
            title,
        } = source.payload()
        else {
            return Err(unsupported_ui("maintenance open source event"));
        };
        let request = wire::AssetMaintenanceRequestEvent {
            event_id: source.event_id().to_string(),
            event_version: u32::from(source.event_version()),
            tenant_scope: Some(encode_scope(source.tenant_scope())),
            aggregate_id: source.aggregate_id().to_string(),
            aggregate_version: source.aggregate_version(),
            correlation_id: source.correlation_id().to_string(),
            causation_id: source.causation_id().map(|id| id.to_string()),
            occurred_at: source.occurred_at().to_rfc3339(),
            asset_id: asset.as_uuid().to_string(),
            request_code: request_code.as_str().into(),
            title: title.as_ref().map(|value| value.as_str().to_owned()),
        };
        let response = self
            .commands
            .clone()
            .open(self.auth.request(
                wire::OpenMaintenanceWorkOrderRequest {
                    metadata: Some(metadata),
                    work_order_id: value.work_order_id.as_uuid().to_string(),
                    request: Some(request),
                    title: value.title,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("maintenance open work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            OpenWorkOrderResult { work_order }
        })
    }
    async fn schedule(
        &self,
        envelope: CommandEnvelope<ScheduleWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .schedule(self.auth.request(
                wire::ScheduleMaintenanceWorkOrderRequest {
                    metadata: Some(metadata),
                    work_order_id: value.work_order_id.as_uuid().to_string(),
                    scheduled_for: value.scheduled_for.to_rfc3339(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("maintenance schedule work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn start(
        &self,
        envelope: CommandEnvelope<StartWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .start(
                self.auth
                    .request(identity(metadata, envelope.command.work_order_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("maintenance start work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn block(
        &self,
        envelope: CommandEnvelope<BlockWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .block(
                self.auth
                    .request(identity(metadata, envelope.command.work_order_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("maintenance block work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn resume(
        &self,
        envelope: CommandEnvelope<ResumeWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .resume(
                self.auth
                    .request(identity(metadata, envelope.command.work_order_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("maintenance resume work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn complete(
        &self,
        envelope: CommandEnvelope<CompleteWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .complete(
                self.auth
                    .request(identity(metadata, envelope.command.work_order_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("maintenance complete work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn cancel(
        &self,
        envelope: CommandEnvelope<CancelWorkOrder>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .cancel(
                self.auth
                    .request(identity(metadata, envelope.command.work_order_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("maintenance cancel work order", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn record_downtime(
        &self,
        envelope: CommandEnvelope<RecordDowntime>,
    ) -> Result<CommandReceipt<MaintenanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let downtime = value.downtime;
        let response = self
            .commands
            .clone()
            .record_downtime(self.auth.request(
                wire::RecordMaintenanceDowntimeRequest {
                    metadata: Some(metadata),
                    work_order_id: value.work_order_id.as_uuid().to_string(),
                    downtime: Some(wire::DowntimeRecord {
                        downtime_id: downtime.downtime_id.to_string(),
                        started_at: downtime.started_at.to_rfc3339(),
                        ended_at: downtime.ended_at.to_rfc3339(),
                        reason: downtime.reason,
                    }),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("maintenance record downtime", status))?
            .into_inner();
        result(response, command_id, correlation, |work_order| {
            MaintenanceMutationResult { work_order }
        })
    }
    async fn get(
        &self,
        envelope: QueryEnvelope<GetWorkOrder>,
    ) -> Result<QueryResult<GetWorkOrderResult>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let response = self
            .queries
            .clone()
            .get(self.auth.request(
                wire::GetMaintenanceWorkOrderRequest {
                    metadata: Some(metadata),
                    work_order_id: envelope.query.work_order_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("maintenance get work order", status))?
            .into_inner();
        query_result(
            response.metadata,
            GetWorkOrderResult {
                work_order: response.work_order.map(decode_work_order).transpose()?,
            },
            correlation,
            consistency,
        )
    }
    async fn list(
        &self,
        envelope: QueryEnvelope<ListWorkOrders>,
    ) -> Result<QueryResult<WorkOrderList>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let filter = envelope.query.filter;
        let response = self
            .queries
            .clone()
            .list(self.auth.request(
                wire::ListMaintenanceWorkOrdersRequest {
                    metadata: Some(metadata),
                    status: filter.status.map(encode_status),
                    asset_id: filter.asset_id.map(|id| id.to_string()),
                    page_size: u32::from(filter.page_size),
                    cursor: filter.cursor,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("maintenance list work orders", status))?
            .into_inner();
        query_result(
            response.metadata,
            WorkOrderList {
                work_orders: response
                    .work_orders
                    .into_iter()
                    .map(decode_work_order)
                    .collect::<Result<_, _>>()?,
                next_cursor: response.next_cursor,
            },
            correlation,
            consistency,
        )
    }
}

fn decode_work_order(value: wire::MaintenanceWorkOrderRecord) -> Result<WorkOrderView, UiError> {
    let request = value
        .request
        .ok_or_else(|| invalid_response("decode maintenance request"))?;
    let fact = value
        .asset_fact
        .ok_or_else(|| invalid_response("decode maintenance asset fact"))?;
    Ok(WorkOrderView {
        work_order_id: WorkOrderId::try_from_uuid(uuid(
            &value.work_order_id,
            "decode maintenance work order",
        )?)
        .map_err(|_| invalid_response("decode maintenance work order"))?,
        version: value.source_version,
        code: value.code,
        scope: decode_scope(value.scope)?,
        request: MaintenanceRequest::try_new(
            EventId::try_from_uuid(uuid(
                &request.source_event_id,
                "decode maintenance request",
            )?)
            .map_err(|_| invalid_response("decode maintenance request"))?,
            request.source_aggregate_version,
            assets::AssetRef::try_from_uuid(uuid(&request.asset_id, "decode maintenance request")?)
                .map_err(|_| invalid_response("decode maintenance request"))?,
            request.request_code,
        )
        .map_err(|_| invalid_response("decode maintenance request"))?,
        asset_fact: assets::AssetConditionFact {
            asset: assets::AssetRef::try_from_uuid(uuid(
                &fact.asset_id,
                "decode maintenance asset fact",
            )?)
            .map_err(|_| invalid_response("decode maintenance asset fact"))?,
            asset_code: ReferenceCode::try_new(fact.asset_code)
                .map_err(|_| invalid_response("decode maintenance asset fact"))?,
            name: DisplayName::try_new(fact.name)
                .map_err(|_| invalid_response("decode maintenance asset fact"))?,
            location: assets::AssetLocationRef::try_from_uuid(uuid(
                &fact.location_id,
                "decode maintenance asset fact",
            )?)
            .map_err(|_| invalid_response("decode maintenance asset fact"))?,
            condition: decode_condition(fact.condition)?,
        },
        title: value.title,
        status: decode_status(value.status)?,
        scheduled_for: value
            .scheduled_for
            .map(|value| timestamp(&value))
            .transpose()?,
        started_at: value
            .started_at
            .map(|value| timestamp(&value))
            .transpose()?,
        completed_at: value
            .completed_at
            .map(|value| timestamp(&value))
            .transpose()?,
        downtime: value
            .downtime
            .into_iter()
            .map(decode_downtime)
            .collect::<Result<_, _>>()?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn decode_downtime(value: wire::DowntimeRecord) -> Result<Downtime, UiError> {
    let value = Downtime {
        downtime_id: uuid(&value.downtime_id, "decode maintenance downtime")?,
        started_at: timestamp(&value.started_at)?,
        ended_at: timestamp(&value.ended_at)?,
        reason: value.reason,
    };
    value
        .validate()
        .map_err(|_| invalid_response("decode maintenance downtime"))?;
    Ok(value)
}
fn encode_status(value: WorkOrderStatus) -> i32 {
    match value {
        WorkOrderStatus::Planned => wire::MaintenanceWorkOrderStatus::Planned as i32,
        WorkOrderStatus::Scheduled => wire::MaintenanceWorkOrderStatus::Scheduled as i32,
        WorkOrderStatus::InProgress => wire::MaintenanceWorkOrderStatus::InProgress as i32,
        WorkOrderStatus::Blocked => wire::MaintenanceWorkOrderStatus::Blocked as i32,
        WorkOrderStatus::Completed => wire::MaintenanceWorkOrderStatus::Completed as i32,
        WorkOrderStatus::Cancelled => wire::MaintenanceWorkOrderStatus::Cancelled as i32,
    }
}
fn decode_status(value: i32) -> Result<WorkOrderStatus, UiError> {
    match wire::MaintenanceWorkOrderStatus::try_from(value).ok() {
        Some(wire::MaintenanceWorkOrderStatus::Planned) => Ok(WorkOrderStatus::Planned),
        Some(wire::MaintenanceWorkOrderStatus::Scheduled) => Ok(WorkOrderStatus::Scheduled),
        Some(wire::MaintenanceWorkOrderStatus::InProgress) => Ok(WorkOrderStatus::InProgress),
        Some(wire::MaintenanceWorkOrderStatus::Blocked) => Ok(WorkOrderStatus::Blocked),
        Some(wire::MaintenanceWorkOrderStatus::Completed) => Ok(WorkOrderStatus::Completed),
        Some(wire::MaintenanceWorkOrderStatus::Cancelled) => Ok(WorkOrderStatus::Cancelled),
        _ => Err(invalid_response("decode maintenance status")),
    }
}
fn decode_condition(value: i32) -> Result<assets::AssetCondition, UiError> {
    match WireAssetCondition::try_from(value).ok() {
        Some(WireAssetCondition::Operational) => Ok(assets::AssetCondition::Operational),
        Some(WireAssetCondition::Degraded) => Ok(assets::AssetCondition::Degraded),
        Some(WireAssetCondition::OutOfService) => Ok(assets::AssetCondition::OutOfService),
        Some(WireAssetCondition::Retired) => Ok(assets::AssetCondition::Retired),
        _ => Err(invalid_response("decode maintenance asset condition")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_values_fail_closed() {
        assert!(decode_status(wire::MaintenanceWorkOrderStatus::Unspecified as i32).is_err());
        assert!(decode_condition(WireAssetCondition::Unspecified as i32).is_err());
    }
}
