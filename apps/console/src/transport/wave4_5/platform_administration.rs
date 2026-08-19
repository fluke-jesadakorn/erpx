use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_platform_administration::{
    AdminContextId, AdminContextStatus, AdminContextView, ConnectorBindingId, ConnectorKind,
    ConnectorScope, ConnectorState, ConnectorTransition, ConnectorView, EnvironmentId,
    GetAdminContext, GetAdminContextResult, GetConnector, GetConnectorResult, GetPlatformHolding,
    GetPlatformHoldingResult, HoldingLifecycleAction, HoldingLifecycleState, HoldingView,
    IssueHoldingAdminContext, ListConnectors, ListPlatformHoldings, Page,
    PlatformAdministrationMutationResult, PlatformHoldingId, PlatformRecordView, RegisterConnector,
    RegisterPlatformHolding, RevokeHoldingAdminContext, TransitionConnector,
    TransitionPlatformHolding,
};
use erpx_proto::{common, erpx::platform_administration as platform};
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
    async fn register_holding(
        &self,
        envelope: CommandEnvelope<RegisterPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RegisterPlatformHoldingRequest {
                metadata: Some(metadata),
                holding_id: command.holding_id.as_uuid().to_string(),
                code: command.code,
                name: command.name,
                environment_id: command.environment_id.as_uuid().to_string(),
                customer_id: command.customer_id.to_string(),
                canonical_holding_id: command.canonical_holding_id.to_string(),
                data_profile_key: command.data_profile_key,
            }
        })?;
        let response = self
            .command
            .clone()
            .register_holding(request)
            .await
            .map_err(|error| rpc(error, "register platform holding"))?
            .into_inner();
        let record = PlatformRecordView::Holding(decode_holding(
            response.holding.ok_or_else(invalid_response)?,
        )?);
        command_receipt(
            response.receipt,
            PlatformAdministrationMutationResult { record },
        )
    }

    async fn transition_holding(
        &self,
        envelope: CommandEnvelope<TransitionPlatformHolding>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::TransitionPlatformHoldingRequest {
                metadata: Some(metadata),
                holding_id: command.holding_id.as_uuid().to_string(),
                action: encode_holding_action(command.action),
            }
        })?;
        let response = self
            .command
            .clone()
            .transition_holding(request)
            .await
            .map_err(|error| rpc(error, "transition platform holding"))?
            .into_inner();
        let record = PlatformRecordView::Holding(decode_holding(
            response.holding.ok_or_else(invalid_response)?,
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
        envelope: CommandEnvelope<IssueHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::IssuePlatformHoldingAdminContextRequest {
                metadata: Some(metadata),
                context_id: command.context_id.as_uuid().to_string(),
                holding_id: command.holding_id.as_uuid().to_string(),
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
        envelope: CommandEnvelope<RevokeHoldingAdminContext>,
    ) -> Result<CommandReceipt<PlatformAdministrationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RevokePlatformHoldingAdminContextRequest {
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

    async fn get_holding(
        &self,
        envelope: QueryEnvelope<GetPlatformHolding>,
    ) -> Result<QueryResult<GetPlatformHoldingResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetPlatformAdministrationHoldingRequest {
                metadata: Some(metadata),
                holding_id: query.holding_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_holding(request)
            .await
            .map_err(|error| rpc(error, "get platform holding"))?
            .into_inner();
        let holding = response.holding.map(decode_holding).transpose()?;
        query_result(response.metadata, GetPlatformHoldingResult { holding })
    }

    async fn list_holdings(
        &self,
        envelope: QueryEnvelope<ListPlatformHoldings>,
    ) -> Result<QueryResult<Page<HoldingView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListPlatformAdministrationHoldingsRequest {
                metadata: Some(metadata),
                filter: Some(encode_page(query.filter.page_size, query.filter.cursor)),
            }
        })?;
        let response = self
            .query
            .clone()
            .list_holdings(request)
            .await
            .map_err(|error| rpc(error, "list platform holdings"))?
            .into_inner();
        let records = response
            .holdings
            .into_iter()
            .map(decode_holding)
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
            platform::GetPlatformHoldingAdminContextRequest {
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

fn encode_holding_action(value: HoldingLifecycleAction) -> i32 {
    match value {
        HoldingLifecycleAction::Activate => {
            platform::PlatformHoldingLifecycleAction::Activate as i32
        }
        HoldingLifecycleAction::Suspend => platform::PlatformHoldingLifecycleAction::Suspend as i32,
        HoldingLifecycleAction::EnterRecovery => {
            platform::PlatformHoldingLifecycleAction::EnterRecovery as i32
        }
        HoldingLifecycleAction::Retire => platform::PlatformHoldingLifecycleAction::Retire as i32,
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
            ConnectorScope::Holding(scope) => Target::Holding(encode_scope(scope)),
        }),
    }
}

fn decode_connector_scope(
    value: platform::PlatformConnectorScope,
) -> Result<ConnectorScope, UiError> {
    use platform::platform_connector_scope::Target;
    match value.target.ok_or_else(invalid_response)? {
        Target::Platform(_) => Ok(ConnectorScope::Platform),
        Target::Holding(value) => Ok(ConnectorScope::Holding(decode_scope(value)?)),
    }
}

fn decode_holding(
    value: platform::PlatformAdministrationHoldingRecord,
) -> Result<HoldingView, UiError> {
    Ok(HoldingView {
        holding_id: PlatformHoldingId::try_from_uuid(uuid(&value.holding_id)?)
            .map_err(|_| invalid_response())?,
        version: value.aggregate_version,
        code: value.code,
        name: value.name,
        environment_id: EnvironmentId::try_from_uuid(uuid(&value.environment_id)?)
            .map_err(|_| invalid_response())?,
        customer_id: uuid(&value.customer_id)?,
        canonical_holding_id: uuid(&value.canonical_holding_id)?,
        data_profile_key: value.data_profile_key,
        status: match platform::PlatformHoldingLifecycleState::try_from(value.status).ok() {
            Some(platform::PlatformHoldingLifecycleState::PendingSubscription) => {
                HoldingLifecycleState::PendingSubscription
            }
            Some(platform::PlatformHoldingLifecycleState::Provisioning) => {
                HoldingLifecycleState::Provisioning
            }
            Some(platform::PlatformHoldingLifecycleState::ProvisioningFailed) => {
                HoldingLifecycleState::ProvisioningFailed
            }
            Some(platform::PlatformHoldingLifecycleState::Active) => HoldingLifecycleState::Active,
            Some(platform::PlatformHoldingLifecycleState::Suspended) => {
                HoldingLifecycleState::Suspended
            }
            Some(platform::PlatformHoldingLifecycleState::Recovery) => {
                HoldingLifecycleState::Recovery
            }
            Some(platform::PlatformHoldingLifecycleState::Retired) => {
                HoldingLifecycleState::Retired
            }
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
        holding_id: PlatformHoldingId::try_from_uuid(uuid(&value.holding_id)?)
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
