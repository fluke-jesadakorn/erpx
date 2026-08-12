use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_platform_administration::{
    AdminContextStatus, AdminContextView, ConnectorKind, ConnectorView, GetAdminContext,
    GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformTenant,
    GetPlatformTenantResult, IssueTenantAdminContext, ListConnectors, ListPlatformTenants, Page,
    PlatformAdministrationMutationResult, RegisterConnector, RegisterPlatformTenant,
    RevokeTenantAdminContext, TenantLifecycleState, TenantView, TransitionConnector,
    TransitionPlatformTenant,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

use super::model::{AdminContextItem, ConnectorItem, TenantItem};

pub fn tenant_item(value: &TenantView) -> TenantItem {
    TenantItem {
        id: value.tenant_id.as_uuid().to_string(),
        code: value.code.clone(),
        name: value.name.clone(),
        status: match value.status {
            TenantLifecycleState::Provisioning => "provisioning",
            TenantLifecycleState::Active => "active",
            TenantLifecycleState::Suspended => "suspended",
            TenantLifecycleState::Recovery => "recovery",
            TenantLifecycleState::Retired => "retired",
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
            ConnectorKind::N8n => "n8n",
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
        tenant_id: value.tenant_id.as_uuid().to_string(),
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
    async fn register_tenant(
        &self,
        envelope: CommandEnvelope<RegisterPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn transition_tenant(
        &self,
        envelope: CommandEnvelope<TransitionPlatformTenant>,
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
        envelope: CommandEnvelope<IssueTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn revoke_admin_context(
        &self,
        envelope: CommandEnvelope<RevokeTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError>;
    async fn get_tenant(
        &self,
        envelope: QueryEnvelope<GetPlatformTenant>,
    ) -> Result<QueryResult<GetPlatformTenantResult>, UiError>;
    async fn list_tenants(
        &self,
        envelope: QueryEnvelope<ListPlatformTenants>,
    ) -> Result<QueryResult<Page<TenantView>>, UiError>;
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
command_gateway!(RegisterPlatformTenant, register_tenant);
command_gateway!(TransitionPlatformTenant, transition_tenant);
command_gateway!(RegisterConnector, register_connector);
command_gateway!(TransitionConnector, transition_connector);
command_gateway!(IssueTenantAdminContext, issue_admin_context);
command_gateway!(RevokeTenantAdminContext, revoke_admin_context);
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
query_gateway!(GetPlatformTenant, GetPlatformTenantResult, get_tenant);
query_gateway!(ListPlatformTenants, Page<TenantView>, list_tenants);
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
            "register_tenant",
            "transition_tenant",
            "register_connector",
            "transition_connector",
            "issue_admin_context",
            "revoke_admin_context",
            "get_tenant",
            "list_tenants",
            "get_connector",
            "list_connectors",
            "get_admin_context",
        ] {
            assert!(source.contains(method));
        }
    }
}
