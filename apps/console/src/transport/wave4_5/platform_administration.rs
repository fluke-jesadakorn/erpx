use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_platform_administration::{
    AdminContextId, AdminContextStatus, AdminContextView, ConnectorBindingId, ConnectorKind,
    ConnectorScope, ConnectorState, ConnectorTransition, ConnectorView, EnvironmentId,
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformTenant,
    GetPlatformTenantResult, IssueTenantAdminContext, ListConnectors, ListPlatformTenants, Page,
    PlatformAdministrationMutationResult, PlatformRecordView, PlatformTenantId, RegisterConnector,
    RegisterPlatformTenant, RevokeTenantAdminContext, TenantLifecycleAction, TenantLifecycleState,
    TenantView, TransitionConnector, TransitionPlatformTenant,
};
use erpx_proto::{common, platform};
use erpx_ui_foundation::UiError;

use crate::{
    features::platform_administration::gateway::PlatformAdministrationProtoTransport,
    transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    RpcAuthority, command_receipt, decode_scope, encode_scope, invalid_response, query_result, rpc,
    time, uuid,
};

#[derive(Clone)]
pub(crate) struct PlatformAdministrationGrpcTransport {
    command: platform::platform_administration_command_service_client::PlatformAdministrationCommandServiceClient<GrpcChannel>,
    query: platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient<GrpcChannel>,
    authority: RpcAuthority,
}

impl PlatformAdministrationGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command: platform::platform_administration_command_service_client::PlatformAdministrationCommandServiceClient::new(channel.clone()),
            query: platform::platform_administration_query_service_client::PlatformAdministrationQueryServiceClient::new(channel),
            authority: RpcAuthority::platform(access_token)?,
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl PlatformAdministrationProtoTransport for PlatformAdministrationGrpcTransport {
    async fn register_tenant(
        &self,
        envelope: CommandEnvelope<RegisterPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RegisterPlatformTenantRequest {
                metadata: Some(metadata),
                tenant_id: command.tenant_id.as_uuid().to_string(),
                code: command.code,
                name: command.name,
                environment_id: command.environment_id.as_uuid().to_string(),
                customer_id: command.customer_id.to_string(),
                root_holding_id: command.root_holding_id.to_string(),
                holding_ref: command.holding_ref,
            }
        })?;
        let response = self
            .command
            .clone()
            .register_tenant(request)
            .await
            .map_err(|error| rpc(error, "register platform tenant"))?
            .into_inner();
        let record = PlatformRecordView::Tenant(decode_tenant(
            response.tenant.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn transition_tenant(
        &self,
        envelope: CommandEnvelope<TransitionPlatformTenant>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::TransitionPlatformTenantRequest {
                metadata: Some(metadata),
                tenant_id: command.tenant_id.as_uuid().to_string(),
                action: encode_tenant_action(command.action),
            }
        })?;
        let response = self
            .command
            .clone()
            .transition_tenant(request)
            .await
            .map_err(|error| rpc(error, "transition platform tenant"))?
            .into_inner();
        let record = PlatformRecordView::Tenant(decode_tenant(
            response.tenant.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn register_connector(
        &self,
        envelope: CommandEnvelope<RegisterConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            let (kind, other_kind) = encode_connector_kind(&command.kind);
            platform::RegisterPlatformConnectorRequest {
                metadata: Some(metadata),
                connector_id: command.connector_id.as_uuid().to_string(),
                scope: Some(encode_connector_scope(&command.scope)),
                kind,
                other_kind,
                connector_key: command.key,
                configuration_sha256: command.configuration_digest,
                capabilities: command.capabilities.into_iter().collect(),
            }
        })?;
        let response = self
            .command
            .clone()
            .register_connector(request)
            .await
            .map_err(|error| rpc(error, "register platform connector"))?
            .into_inner();
        let record = PlatformRecordView::Connector(decode_connector(
            response.connector.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn transition_connector(
        &self,
        envelope: CommandEnvelope<TransitionConnector>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::TransitionPlatformConnectorRequest {
                metadata: Some(metadata),
                connector_id: command.connector_id.as_uuid().to_string(),
                action: encode_connector_transition(command.action),
            }
        })?;
        let response = self
            .command
            .clone()
            .transition_connector(request)
            .await
            .map_err(|error| rpc(error, "transition platform connector"))?
            .into_inner();
        let record = PlatformRecordView::Connector(decode_connector(
            response.connector.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn issue_admin_context(
        &self,
        envelope: CommandEnvelope<IssueTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::IssuePlatformTenantAdminContextRequest {
                metadata: Some(metadata),
                context_id: command.context_id.as_uuid().to_string(),
                tenant_id: command.tenant_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                device_id: command.device_id.to_string(),
                session_id: command.session_id.to_string(),
                reason: command.reason,
                ttl_seconds: command.ttl_seconds,
            }
        })?;
        let response = self
            .command
            .clone()
            .issue_admin_context(request)
            .await
            .map_err(|error| rpc(error, "issue platform admin context"))?
            .into_inner();
        let record = PlatformRecordView::AdminContext(decode_admin_context(
            response.context.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn revoke_admin_context(
        &self,
        envelope: CommandEnvelope<RevokeTenantAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RevokePlatformTenantAdminContextRequest {
                metadata: Some(metadata),
                context_id: command.context_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .revoke_admin_context(request)
            .await
            .map_err(|error| rpc(error, "revoke platform admin context"))?
            .into_inner();
        let record = PlatformRecordView::AdminContext(decode_admin_context(
            response.context.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn get_tenant(
        &self,
        envelope: QueryEnvelope<GetPlatformTenant>,
    ) -> Result<QueryResult<GetPlatformTenantResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetPlatformAdministrationTenantRequest {
                metadata: Some(metadata),
                tenant_id: query.tenant_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_tenant(request)
            .await
            .map_err(|error| rpc(error, "get platform tenant"))?
            .into_inner();
        let tenant = response.tenant.map(decode_tenant).transpose()?;
        query_result(response.metadata, GetPlatformTenantResult { tenant })
    }

    async fn list_tenants(
        &self,
        envelope: QueryEnvelope<ListPlatformTenants>,
    ) -> Result<QueryResult<Page<TenantView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListPlatformAdministrationTenantsRequest {
                metadata: Some(metadata),
                filter: Some(encode_page(query.filter.page_size, query.filter.cursor)),
            }
        })?;
        let response = self
            .query
            .clone()
            .list_tenants(request)
            .await
            .map_err(|error| rpc(error, "list platform tenants"))?
            .into_inner();
        let records = response
            .tenants
            .into_iter()
            .map(decode_tenant)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            Page {
                records,
                next_cursor: response.next_cursor,
            },
        )
    }

    async fn get_connector(
        &self,
        envelope: QueryEnvelope<GetConnector>,
    ) -> Result<QueryResult<GetConnectorResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetPlatformAdministrationConnectorRequest {
                metadata: Some(metadata),
                connector_id: query.connector_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_connector(request)
            .await
            .map_err(|error| rpc(error, "get platform connector"))?
            .into_inner();
        let connector = response.connector.map(decode_connector).transpose()?;
        query_result(response.metadata, GetConnectorResult { connector })
    }

    async fn list_connectors(
        &self,
        envelope: QueryEnvelope<ListConnectors>,
    ) -> Result<QueryResult<Page<ConnectorView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListPlatformAdministrationConnectorsRequest {
                metadata: Some(metadata),
                filter: Some(encode_page(query.filter.page_size, query.filter.cursor)),
            }
        })?;
        let response = self
            .query
            .clone()
            .list_connectors(request)
            .await
            .map_err(|error| rpc(error, "list platform connectors"))?
            .into_inner();
        let records = response
            .connectors
            .into_iter()
            .map(decode_connector)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            Page {
                records,
                next_cursor: response.next_cursor,
            },
        )
    }

    async fn get_admin_context(
        &self,
        envelope: QueryEnvelope<GetAdminContext>,
    ) -> Result<QueryResult<GetAdminContextResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetPlatformTenantAdminContextRequest {
                metadata: Some(metadata),
                context_id: query.context_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_admin_context(request)
            .await
            .map_err(|error| rpc(error, "get platform admin context"))?
            .into_inner();
        let context = response.context.map(decode_admin_context).transpose()?;
        query_result(response.metadata, GetAdminContextResult { context })
    }
}

fn encode_page(
    page_size: u16,
    cursor: Option<String>,
) -> platform::PlatformAdministrationPageFilter {
    platform::PlatformAdministrationPageFilter {
        page_size: u32::from(page_size),
        cursor,
    }
}

fn encode_tenant_action(value: TenantLifecycleAction) -> i32 {
    match value {
        TenantLifecycleAction::Activate => platform::PlatformTenantLifecycleAction::Activate as i32,
        TenantLifecycleAction::Suspend => platform::PlatformTenantLifecycleAction::Suspend as i32,
        TenantLifecycleAction::EnterRecovery => {
            platform::PlatformTenantLifecycleAction::EnterRecovery as i32
        }
        TenantLifecycleAction::Retire => platform::PlatformTenantLifecycleAction::Retire as i32,
    }
}

fn encode_connector_transition(value: ConnectorTransition) -> i32 {
    match value {
        ConnectorTransition::Activate => platform::PlatformConnectorTransition::Activate as i32,
        ConnectorTransition::Suspend => platform::PlatformConnectorTransition::Suspend as i32,
        ConnectorTransition::Retire => platform::PlatformConnectorTransition::Retire as i32,
    }
}

fn encode_connector_kind(value: &ConnectorKind) -> (i32, Option<String>) {
    match value {
        ConnectorKind::Stripe => (platform::PlatformConnectorKind::Stripe as i32, None),
        ConnectorKind::Minio => (platform::PlatformConnectorKind::Minio as i32, None),
        ConnectorKind::N8n => (platform::PlatformConnectorKind::N8n as i32, None),
        ConnectorKind::Ollama => (platform::PlatformConnectorKind::Ollama as i32, None),
        ConnectorKind::Other(value) => (
            platform::PlatformConnectorKind::Other as i32,
            Some(value.clone()),
        ),
    }
}

fn encode_connector_scope(value: &ConnectorScope) -> platform::PlatformConnectorScope {
    use platform::platform_connector_scope::Target;
    platform::PlatformConnectorScope {
        target: Some(match value {
            ConnectorScope::Platform => Target::Platform(common::PlatformScope {}),
            ConnectorScope::Tenant(scope) => Target::Tenant(encode_scope(scope)),
        }),
    }
}

fn decode_connector_scope(
    value: platform::PlatformConnectorScope,
) -> Result<ConnectorScope, UiError> {
    use platform::platform_connector_scope::Target;
    match value.target.ok_or_else(invalid_response)? {
        Target::Platform(_) => Ok(ConnectorScope::Platform),
        Target::Tenant(value) => Ok(ConnectorScope::Tenant(decode_scope(value)?)),
    }
}

fn decode_tenant(
    value: platform::PlatformAdministrationTenantRecord,
) -> Result<TenantView, UiError> {
    Ok(TenantView {
        tenant_id: PlatformTenantId::try_from_uuid(uuid(&value.tenant_id)?)
            .map_err(|_| invalid_response())?,
        version: value.aggregate_version,
        code: value.code,
        name: value.name,
        environment_id: EnvironmentId::try_from_uuid(uuid(&value.environment_id)?)
            .map_err(|_| invalid_response())?,
        customer_id: uuid(&value.customer_id)?,
        root_holding_id: uuid(&value.root_holding_id)?,
        holding_ref: value.holding_ref,
        status: match platform::PlatformTenantLifecycleState::try_from(value.status).ok() {
            Some(platform::PlatformTenantLifecycleState::Provisioning) => {
                TenantLifecycleState::Provisioning
            }
            Some(platform::PlatformTenantLifecycleState::Active) => TenantLifecycleState::Active,
            Some(platform::PlatformTenantLifecycleState::Suspended) => {
                TenantLifecycleState::Suspended
            }
            Some(platform::PlatformTenantLifecycleState::Recovery) => {
                TenantLifecycleState::Recovery
            }
            Some(platform::PlatformTenantLifecycleState::Retired) => TenantLifecycleState::Retired,
            _ => return Err(invalid_response()),
        },
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_connector(
    value: platform::PlatformAdministrationConnectorRecord,
) -> Result<ConnectorView, UiError> {
    let kind = match platform::PlatformConnectorKind::try_from(value.kind).ok() {
        Some(platform::PlatformConnectorKind::Stripe) => ConnectorKind::Stripe,
        Some(platform::PlatformConnectorKind::Minio) => ConnectorKind::Minio,
        Some(platform::PlatformConnectorKind::N8n) => ConnectorKind::N8n,
        Some(platform::PlatformConnectorKind::Ollama) => ConnectorKind::Ollama,
        Some(platform::PlatformConnectorKind::Other) => ConnectorKind::Other(
            value
                .other_kind
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(invalid_response)?,
        ),
        _ => return Err(invalid_response()),
    };
    let state = match platform::PlatformConnectorState::try_from(value.state).ok() {
        Some(platform::PlatformConnectorState::Pending) => ConnectorState::Pending,
        Some(platform::PlatformConnectorState::Active) => ConnectorState::Active,
        Some(platform::PlatformConnectorState::Suspended) => ConnectorState::Suspended,
        Some(platform::PlatformConnectorState::Retired) => ConnectorState::Retired,
        _ => return Err(invalid_response()),
    };
    Ok(ConnectorView {
        connector_id: ConnectorBindingId::try_from_uuid(uuid(&value.connector_id)?)
            .map_err(|_| invalid_response())?,
        version: value.aggregate_version,
        scope: decode_connector_scope(value.scope.ok_or_else(invalid_response)?)?,
        kind,
        key: value.connector_key,
        configuration_digest: value.configuration_sha256,
        capabilities: value.capabilities.into_iter().collect(),
        state,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_admin_context(
    value: platform::PlatformAdministrationAdminContextRecord,
) -> Result<AdminContextView, UiError> {
    Ok(AdminContextView {
        context_id: AdminContextId::try_from_uuid(uuid(&value.context_id)?)
            .map_err(|_| invalid_response())?,
        version: value.aggregate_version,
        tenant_id: PlatformTenantId::try_from_uuid(uuid(&value.tenant_id)?)
            .map_err(|_| invalid_response())?,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        principal_ref: value.principal_ref,
        reason: value.reason,
        issued_at: time(&value.issued_at)?,
        expires_at: time(&value.expires_at)?,
        revoked_at: value.revoked_at.map(|value| time(&value)).transpose()?,
        status: match platform::PlatformAdminContextStatus::try_from(value.status).ok() {
            Some(platform::PlatformAdminContextStatus::Active) => AdminContextStatus::Active,
            Some(platform::PlatformAdminContextStatus::Revoked) => AdminContextStatus::Revoked,
            Some(platform::PlatformAdminContextStatus::Expired) => AdminContextStatus::Expired,
            _ => return Err(invalid_response()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_other_kind_must_be_explicit() {
        let record = platform::PlatformAdministrationConnectorRecord {
            kind: platform::PlatformConnectorKind::Other as i32,
            other_kind: None,
            ..Default::default()
        };
        assert!(decode_connector(record).is_err());
    }
}
