use async_trait::async_trait;
use erpx_assets::{
    AssetId, AssetLifecycle, AssetList, AssetMutationResult, AssetScope, AssetStatus, AssetView,
    BeginMaintenance, CreateAsset, CreateAssetResult, GetAsset, GetAssetResult, ListAssets,
    MarkAssetDegraded, PlaceAssetInService, RetireAsset, ReturnAssetToService,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::assets::AssetCondition;
use erpx_proto::erpx::assets as wire;
use erpx_ui_foundation::UiError;

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, decode_scope, encode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};
use crate::{features::assets::AssetsProtoTransport, transport::typed_cqrs::GrpcChannel};

type Commands = wire::assets_command_service_client::AssetsCommandServiceClient<GrpcChannel>;
type Queries = wire::assets_query_service_client::AssetsQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct AssetsGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl AssetsGrpcTransport {
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

fn asset_result<R>(
    response: wire::AssetMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation: erpx_kernel::CorrelationId,
    build: impl FnOnce(AssetView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    let asset = decode_asset(
        response
            .asset
            .ok_or_else(|| invalid_response("decode asset"))?,
    )?;
    command_result(response.receipt, build(asset), command_id, correlation)
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl AssetsProtoTransport for AssetsGrpcTransport {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateAsset>,
    ) -> Result<CommandReceipt<CreateAssetResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .create(self.auth.request(
                wire::CreateAssetRequest {
                    metadata: Some(metadata),
                    asset_id: value.asset_id.as_uuid().to_string(),
                    code: value.code,
                    name: value.name,
                    scope: Some(encode_scope(value.scope.active_scope())),
                    location_id: value.location_id.to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("assets create", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            CreateAssetResult { asset }
        })
    }
    async fn place_in_service(
        &self,
        envelope: CommandEnvelope<PlaceAssetInService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .place_in_service(
                self.auth
                    .request(identity(metadata, envelope.command.asset_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("assets place in service", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            AssetMutationResult { asset }
        })
    }
    async fn mark_degraded(
        &self,
        envelope: CommandEnvelope<MarkAssetDegraded>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .mark_degraded(
                self.auth
                    .request(identity(metadata, envelope.command.asset_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("assets mark degraded", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            AssetMutationResult { asset }
        })
    }
    async fn begin_maintenance(
        &self,
        envelope: CommandEnvelope<BeginMaintenance>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let value = envelope.command;
        let response = self
            .commands
            .clone()
            .begin_maintenance(self.auth.request(
                wire::BeginAssetMaintenanceRequest {
                    metadata: Some(metadata),
                    asset_id: value.asset_id.as_uuid().to_string(),
                    request_code: value.request_code,
                    title: value.title,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("assets begin maintenance", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            AssetMutationResult { asset }
        })
    }
    async fn return_to_service(
        &self,
        envelope: CommandEnvelope<ReturnAssetToService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .return_to_service(
                self.auth
                    .request(identity(metadata, envelope.command.asset_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("assets return to service", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            AssetMutationResult { asset }
        })
    }
    async fn retire(
        &self,
        envelope: CommandEnvelope<RetireAsset>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation = envelope.correlation_id;
        let response = self
            .commands
            .clone()
            .retire(
                self.auth
                    .request(identity(metadata, envelope.command.asset_id), &actor)?,
            )
            .await
            .map_err(|status| status_ui("assets retire", status))?
            .into_inner();
        asset_result(response, command_id, correlation, |asset| {
            AssetMutationResult { asset }
        })
    }
    async fn get(
        &self,
        envelope: QueryEnvelope<GetAsset>,
    ) -> Result<QueryResult<GetAssetResult>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let response = self
            .queries
            .clone()
            .get(self.auth.request(
                wire::GetAssetRequest {
                    metadata: Some(metadata),
                    asset_id: envelope.query.asset_id.as_uuid().to_string(),
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("assets get", status))?
            .into_inner();
        query_result(
            response.metadata,
            GetAssetResult {
                asset: response.asset.map(decode_asset).transpose()?,
            },
            correlation,
            consistency,
        )
    }
    async fn list(
        &self,
        envelope: QueryEnvelope<ListAssets>,
    ) -> Result<QueryResult<AssetList>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let filter = envelope.query.filter;
        let response = self
            .queries
            .clone()
            .list(self.auth.request(
                wire::ListAssetsRequest {
                    metadata: Some(metadata),
                    status: filter.status.map(encode_status),
                    location_id: filter.location_id.map(|id| id.to_string()),
                    page_size: u32::from(filter.page_size),
                    cursor: filter.cursor,
                },
                &actor,
            )?)
            .await
            .map_err(|status| status_ui("assets list", status))?
            .into_inner();
        query_result(
            response.metadata,
            AssetList {
                assets: response
                    .assets
                    .into_iter()
                    .map(decode_asset)
                    .collect::<Result<_, _>>()?,
                next_cursor: response.next_cursor,
            },
            correlation,
            consistency,
        )
    }
}

fn identity(
    metadata: erpx_proto::common::CommandRequestMetadata,
    asset_id: AssetId,
) -> wire::AssetIdentityCommandRequest {
    wire::AssetIdentityCommandRequest {
        metadata: Some(metadata),
        asset_id: asset_id.as_uuid().to_string(),
    }
}
fn decode_asset(value: wire::AssetRecord) -> Result<AssetView, UiError> {
    let lifecycle = value
        .lifecycle
        .ok_or_else(|| invalid_response("decode asset lifecycle"))?;
    Ok(AssetView {
        asset_id: AssetId::try_from_uuid(uuid(&value.asset_id, "decode asset")?)
            .map_err(|_| invalid_response("decode asset"))?,
        version: value.source_version,
        code: value.code,
        name: value.name,
        scope: AssetScope::try_new(decode_scope(value.scope)?)
            .map_err(|_| invalid_response("decode asset scope"))?,
        location_id: uuid(&value.location_id, "decode asset")?,
        status: decode_status(value.status)?,
        condition: decode_condition(value.condition)?,
        lifecycle: AssetLifecycle {
            placed_in_service_at: lifecycle
                .placed_in_service_at
                .map(|value| timestamp(&value))
                .transpose()?,
            retired_at: lifecycle
                .retired_at
                .map(|value| timestamp(&value))
                .transpose()?,
        },
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}
fn encode_status(value: AssetStatus) -> i32 {
    match value {
        AssetStatus::Draft => wire::AssetStatus::Draft as i32,
        AssetStatus::InService => wire::AssetStatus::InService as i32,
        AssetStatus::UnderMaintenance => wire::AssetStatus::UnderMaintenance as i32,
        AssetStatus::Retired => wire::AssetStatus::Retired as i32,
    }
}
fn decode_status(value: i32) -> Result<AssetStatus, UiError> {
    match wire::AssetStatus::try_from(value).ok() {
        Some(wire::AssetStatus::Draft) => Ok(AssetStatus::Draft),
        Some(wire::AssetStatus::InService) => Ok(AssetStatus::InService),
        Some(wire::AssetStatus::UnderMaintenance) => Ok(AssetStatus::UnderMaintenance),
        Some(wire::AssetStatus::Retired) => Ok(AssetStatus::Retired),
        _ => Err(invalid_response("decode asset status")),
    }
}
fn decode_condition(value: i32) -> Result<AssetCondition, UiError> {
    match wire::AssetCondition::try_from(value).ok() {
        Some(wire::AssetCondition::Operational) => Ok(AssetCondition::Operational),
        Some(wire::AssetCondition::Degraded) => Ok(AssetCondition::Degraded),
        Some(wire::AssetCondition::OutOfService) => Ok(AssetCondition::OutOfService),
        Some(wire::AssetCondition::Retired) => Ok(AssetCondition::Retired),
        _ => Err(invalid_response("decode asset condition")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_values_fail_closed() {
        assert!(decode_status(wire::AssetStatus::Unspecified as i32).is_err());
        assert!(decode_condition(wire::AssetCondition::Unspecified as i32).is_err());
    }
}
