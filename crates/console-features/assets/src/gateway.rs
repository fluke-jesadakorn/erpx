use async_trait::async_trait;
use erpx_assets::{
    AssetList, AssetMutationResult, AssetStatus, AssetView, BeginMaintenance, CreateAsset,
    CreateAssetResult, GetAsset, GetAssetResult, ListAssets, MarkAssetDegraded,
    PlaceAssetInService, RetireAsset, ReturnAssetToService,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::{
    models::{AssetItem, AssetItems},
    presenter::AssetsContent,
};

pub fn present_asset(value: QueryResult<GetAssetResult>) -> QueryResult<Option<AssetsContent>> {
    QueryResult {
        value: value
            .value
            .asset
            .map(AssetItem::from)
            .map(|value| AssetsContent::Asset(Box::new(value))),
        metadata: value.metadata,
    }
}

pub fn present_assets(value: QueryResult<AssetList>) -> QueryResult<Option<AssetsContent>> {
    let metadata = value.metadata;
    let items = AssetItems::from(value.value);
    QueryResult {
        value: (!items.assets.is_empty())
            .then_some(items)
            .map(AssetsContent::Assets),
        metadata,
    }
}

impl From<AssetView> for AssetItem {
    fn from(value: AssetView) -> Self {
        Self {
            asset_id: value.asset_id.as_uuid().to_string(),
            version: value.version,
            code: value.code,
            name: value.name,
            scope: format!("{:?}", value.scope.active_scope()),
            location_id: value.location_id.to_string(),
            status: match value.status {
                AssetStatus::Draft => "draft",
                AssetStatus::InService => "in_service",
                AssetStatus::UnderMaintenance => "under_maintenance",
                AssetStatus::Retired => "retired",
            }
            .into(),
            condition: format!("{:?}", value.condition).to_ascii_lowercase(),
            placed_in_service_at: value
                .lifecycle
                .placed_in_service_at
                .map(|time| time.to_rfc3339()),
            retired_at: value.lifecycle.retired_at.map(|time| time.to_rfc3339()),
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<AssetList> for AssetItems {
    fn from(value: AssetList) -> Self {
        Self {
            assets: value.assets.into_iter().map(AssetItem::from).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait AssetsProtoTransport: Send + Sync {
    async fn create(
        &self,
        value: CommandEnvelope<CreateAsset>,
    ) -> Result<CommandReceipt<CreateAssetResult>, UiError>;
    async fn place_in_service(
        &self,
        value: CommandEnvelope<PlaceAssetInService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError>;
    async fn mark_degraded(
        &self,
        value: CommandEnvelope<MarkAssetDegraded>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError>;
    async fn begin_maintenance(
        &self,
        value: CommandEnvelope<BeginMaintenance>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError>;
    async fn return_to_service(
        &self,
        value: CommandEnvelope<ReturnAssetToService>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError>;
    async fn retire(
        &self,
        value: CommandEnvelope<RetireAsset>,
    ) -> Result<CommandReceipt<AssetMutationResult>, UiError>;
    async fn get(
        &self,
        value: QueryEnvelope<GetAsset>,
    ) -> Result<QueryResult<GetAssetResult>, UiError>;
    async fn list(
        &self,
        value: QueryEnvelope<ListAssets>,
    ) -> Result<QueryResult<AssetList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct AssetsGateway<T> {
    transport: T,
}

impl<T> AssetsGateway<T> {
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
        impl<T: AssetsProtoTransport> CommandGateway<$command> for AssetsGateway<T> {
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

command_gateway!(CreateAsset, CreateAssetResult, create);
command_gateway!(PlaceAssetInService, AssetMutationResult, place_in_service);
command_gateway!(MarkAssetDegraded, AssetMutationResult, mark_degraded);
command_gateway!(BeginMaintenance, AssetMutationResult, begin_maintenance);
command_gateway!(ReturnAssetToService, AssetMutationResult, return_to_service);
command_gateway!(RetireAsset, AssetMutationResult, retire);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: AssetsProtoTransport> QueryGateway<GetAsset> for AssetsGateway<T> {
    type Response = GetAssetResult;
    async fn execute(
        &self,
        value: QueryEnvelope<GetAsset>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(value).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: AssetsProtoTransport> QueryGateway<ListAssets> for AssetsGateway<T> {
    type Response = AssetList;
    async fn execute(
        &self,
        value: QueryEnvelope<ListAssets>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.list(value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_command<C, R, G: CommandGateway<C, Response = R>>() {}
    fn assert_query<Q, R, G: QueryGateway<Q, Response = R>>() {}

    struct ContractTransport;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl AssetsProtoTransport for ContractTransport {
        async fn create(
            &self,
            _: CommandEnvelope<CreateAsset>,
        ) -> Result<CommandReceipt<CreateAssetResult>, UiError> {
            unreachable!()
        }
        async fn place_in_service(
            &self,
            _: CommandEnvelope<PlaceAssetInService>,
        ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
            unreachable!()
        }
        async fn mark_degraded(
            &self,
            _: CommandEnvelope<MarkAssetDegraded>,
        ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
            unreachable!()
        }
        async fn begin_maintenance(
            &self,
            _: CommandEnvelope<BeginMaintenance>,
        ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
            unreachable!()
        }
        async fn return_to_service(
            &self,
            _: CommandEnvelope<ReturnAssetToService>,
        ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
            unreachable!()
        }
        async fn retire(
            &self,
            _: CommandEnvelope<RetireAsset>,
        ) -> Result<CommandReceipt<AssetMutationResult>, UiError> {
            unreachable!()
        }
        async fn get(
            &self,
            _: QueryEnvelope<GetAsset>,
        ) -> Result<QueryResult<GetAssetResult>, UiError> {
            unreachable!()
        }
        async fn list(
            &self,
            _: QueryEnvelope<ListAssets>,
        ) -> Result<QueryResult<AssetList>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_covers_every_typed_operation() {
        type Gateway = AssetsGateway<ContractTransport>;
        assert_command::<CreateAsset, CreateAssetResult, Gateway>();
        assert_command::<PlaceAssetInService, AssetMutationResult, Gateway>();
        assert_command::<MarkAssetDegraded, AssetMutationResult, Gateway>();
        assert_command::<BeginMaintenance, AssetMutationResult, Gateway>();
        assert_command::<ReturnAssetToService, AssetMutationResult, Gateway>();
        assert_command::<RetireAsset, AssetMutationResult, Gateway>();
        assert_query::<GetAsset, GetAssetResult, Gateway>();
        assert_query::<ListAssets, AssetList, Gateway>();
    }
}
