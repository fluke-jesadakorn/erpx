use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_platform_administration::{
    AdminContextStatus, AdminContextView, ConnectorKind, ConnectorView, GetAdminContext,
    GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformHolding,
    GetPlatformHoldingResult, HoldingLifecycleState, HoldingView, IssueHoldingAdminContext,
    ListConnectors, ListPlatformHoldings, Page, PlatformAdministrationMutationResult,
    RegisterConnector, RegisterPlatformHolding, RevokeHoldingAdminContext, TransitionConnector,
    TransitionPlatformHolding,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::model::{AdminContextItem, ConnectorItem, HoldingItem};

pub fn holding_item(value: &HoldingView) -> HoldingItem {
    HoldingItem {
        id: value.holding_id.as_uuid().to_string(),
        code: value.code.clone(),
        name: value.name.clone(),
        status: match value.status {
            HoldingLifecycleState::PendingSubscription => "pending_subscription",
            HoldingLifecycleState::Provisioning => "provisioning",
            HoldingLifecycleState::ProvisioningFailed => "provisioning_failed",
            HoldingLifecycleState::Active => "active",
            HoldingLifecycleState::Suspended => "suspended",
            HoldingLifecycleState::Recovery => "recovery",
            HoldingLifecycleState::Retired => "retired",
        }
        .into(),
        environment_id: value.environment_id.as_uuid().to_string(),
        version: value.version,
    }
}

pub fn connector_item(value: &ConnectorView) -> ConnectorItem {
    ConnectorItem {
        id: value.connector_id.as_uuid().to_string(),
        key: value.key.clone(),
        kind: match &value.kind {
            ConnectorKind::Stripe => "stripe",
            ConnectorKind::Minio => "minio",
            ConnectorKind::Ollama => "ollama",
            ConnectorKind::Other(value) => value,
        }
        .into(),
        state: format!("{:?}", value.state).to_ascii_lowercase(),
        capabilities: value.capabilities.iter().cloned().collect(),
        version: value.version,
    }
}

pub fn admin_context_item(value: &AdminContextView) -> AdminContextItem {
    AdminContextItem {
        id: value.context_id.as_uuid().to_string(),
        holding_id: value.holding_id.as_uuid().to_string(),
        principal_ref: value.principal_ref.clone(),
        reason: value.reason.clone(),
        status: match value.status {
            AdminContextStatus::Active => "active",
            AdminContextStatus::Revoked => "revoked",
            AdminContextStatus::Expired => "expired",
        }
        .into(),
        expires_at: value.expires_at.to_rfc3339(),
        version: value.version,
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PlatformAdministrationProtoTransport: Send + Sync {
    async fn register_holding(
        &self,
        envelope: CommandEnvelope<RegisterPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn transition_holding(
        &self,
        envelope: CommandEnvelope<TransitionPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn register_connector(
        &self,
        envelope: CommandEnvelope<RegisterConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn transition_connector(
        &self,
        envelope: CommandEnvelope<TransitionConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn issue_admin_context(
        &self,
        envelope: CommandEnvelope<IssueHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn revoke_admin_context(
        &self,
        envelope: CommandEnvelope<RevokeHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn get_holding(
        &self,
        envelope: QueryEnvelope<GetPlatformHolding>,
    ) -> Result<QueryResult<GetPlatformHoldingResult>, UiError>;
    async fn list_holdings(
        &self,
        envelope: QueryEnvelope<ListPlatformHoldings>,
    ) -> Result<QueryResult<Page<HoldingView>>, UiError>;
    async fn get_connector(
        &self,
        envelope: QueryEnvelope<GetConnector>,
    ) -> Result<QueryResult<GetConnectorResult>, UiError>;
    async fn list_connectors(
        &self,
        envelope: QueryEnvelope<ListConnectors>,
    ) -> Result<QueryResult<Page<ConnectorView>>, UiError>;
    async fn get_admin_context(
        &self,
        envelope: QueryEnvelope<GetAdminContext>,
    ) -> Result<QueryResult<GetAdminContextResult>, UiError>;
}

#[derive(Debug, Clone)]
pub struct PlatformAdministrationGateway<T> {
    transport: T,
}
impl<T> PlatformAdministrationGateway<T> {
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
    ($command:ty,$method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T: PlatformAdministrationProtoTransport> CommandGateway<$command>
            for PlatformAdministrationGateway<T>
        {
            type Response = PlatformAdministrationMutationResult;
            async fn execute(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}
command_gateway!(RegisterPlatformHolding, register_holding);
command_gateway!(TransitionPlatformHolding, transition_holding);
command_gateway!(RegisterConnector, register_connector);
command_gateway!(TransitionConnector, transition_connector);
command_gateway!(IssueHoldingAdminContext, issue_admin_context);
command_gateway!(RevokeHoldingAdminContext, revoke_admin_context);
macro_rules! query_gateway {
    ($query:ty,$response:ty,$method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T: PlatformAdministrationProtoTransport> QueryGateway<$query>
            for PlatformAdministrationGateway<T>
        {
            type Response = $response;
            async fn execute(
                &self,
                envelope: QueryEnvelope<$query>,
            ) -> Result<QueryResult<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}
query_gateway!(GetPlatformHolding, GetPlatformHoldingResult, get_holding);
query_gateway!(ListPlatformHoldings, Page<HoldingView>, list_holdings);
query_gateway!(GetConnector, GetConnectorResult, get_connector);
query_gateway!(ListConnectors, Page<ConnectorView>, list_connectors);
query_gateway!(GetAdminContext, GetAdminContextResult, get_admin_context);

#[cfg(test)]
mod tests {
    #[test]
    fn gateway_source_is_only_transport_owner() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("dio", "xus")));
        assert!(!source.contains(concat!("Pg", "Pool")));
    }
    #[test]
    fn all_typed_operations_are_registered() {
        let source = include_str!("gateway.rs");
        for method in [
            "register_holding",
            "transition_holding",
            "register_connector",
            "transition_connector",
            "issue_admin_context",
            "revoke_admin_context",
            "get_holding",
            "list_holdings",
            "get_connector",
            "list_connectors",
            "get_admin_context",
        ] {
            assert!(source.contains(method));
        }
    }
}
