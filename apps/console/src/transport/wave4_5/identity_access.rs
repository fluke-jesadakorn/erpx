use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, CreatePrincipal, CreatePrincipalResult, DefineRole,
    DefineRoleResult, DisablePrincipal, GetPrincipal, GetPrincipalResult, GetRole, GetRoleResult,
    ListPrincipals, ListRoles, PermissionKey, PrincipalId, PrincipalList, PrincipalMutationResult,
    PrincipalStatus, PrincipalView, ReactivatePrincipal, RevokeRole, RiskTier, RoleAssignmentId,
    RoleAssignmentView, RoleCode, RoleId, RoleList, RoleMutationResult, RoleView,
    SetRolePermissions, SuspendPrincipal,
};
use erpx_proto::erpx::identity_access as platform;
use erpx_ui_foundation::UiError;

use crate::{
    features::identity_access::gateway::IdentityAccessProtoTransport,
    transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    RpcAuthority, command_receipt, decode_scope, encode_scope, invalid_response, query_result, rpc,
    time, uuid,
};

#[derive(Clone)]
pub(crate) struct IdentityAccessGrpcTransport {
    command: platform::identity_access_command_service_client::IdentityAccessCommandServiceClient<
        GrpcChannel,
    >,
    query: platform::identity_access_query_service_client::IdentityAccessQueryServiceClient<
        GrpcChannel,
    >,
    authority: RpcAuthority,
}

impl IdentityAccessGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
        holding_id: impl AsRef<str>,
        data_profile_key: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command: platform::identity_access_command_service_client::IdentityAccessCommandServiceClient::new(channel.clone()),
            query: platform::identity_access_query_service_client::IdentityAccessQueryServiceClient::new(channel),
            authority: RpcAuthority::holding(access_token, holding_id, data_profile_key)?,
        })
    }
}

fn principal_mutation(
    metadata: erpx_proto::common::CommandRequestMetadata,
    principal_id: PrincipalId,
) -> platform::IdentityAccessPrincipalMutationRequest {
    platform::IdentityAccessPrincipalMutationRequest {
        metadata: Some(metadata),
        principal_id: principal_id.as_uuid().to_string(),
    }
}

fn principal_receipt<R>(
    response: platform::IdentityAccessPrincipalCommandResponse,
    result: impl FnOnce(PrincipalView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    let principal = decode_principal(response.principal.ok_or_else(invalid_response)?)?;
    command_receipt(response.receipt, result(principal))
}

fn role_receipt<R>(
    response: platform::IdentityAccessRoleCommandResponse,
    result: impl FnOnce(RoleView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    let role = decode_role(response.role.ok_or_else(invalid_response)?)?;
    command_receipt(response.receipt, result(role))
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl IdentityAccessProtoTransport for IdentityAccessGrpcTransport {
    async fn create_principal(
        &self,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<CreatePrincipalResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateIdentityAccessPrincipalRequest {
                metadata: Some(metadata),
                principal_id: command.principal_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                display_name: command.display_name,
            }
        })?;
        let response = self
            .command
            .clone()
            .create_principal(request)
            .await
            .map_err(|error| rpc(error, "create identity principal"))?
            .into_inner();
        principal_receipt(response, |principal| CreatePrincipalResult { principal })
    }

    async fn activate_principal(
        &self,
        envelope: CommandEnvelope<ActivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            principal_mutation(metadata, command.principal_id)
        })?;
        let response = self
            .command
            .clone()
            .activate_principal(request)
            .await
            .map_err(|error| rpc(error, "activate identity principal"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn suspend_principal(
        &self,
        envelope: CommandEnvelope<SuspendPrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            principal_mutation(metadata, command.principal_id)
        })?;
        let response = self
            .command
            .clone()
            .suspend_principal(request)
            .await
            .map_err(|error| rpc(error, "suspend identity principal"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn reactivate_principal(
        &self,
        envelope: CommandEnvelope<ReactivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            principal_mutation(metadata, command.principal_id)
        })?;
        let response = self
            .command
            .clone()
            .reactivate_principal(request)
            .await
            .map_err(|error| rpc(error, "reactivate identity principal"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn disable_principal(
        &self,
        envelope: CommandEnvelope<DisablePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            principal_mutation(metadata, command.principal_id)
        })?;
        let response = self
            .command
            .clone()
            .disable_principal(request)
            .await
            .map_err(|error| rpc(error, "disable identity principal"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn assign_role(
        &self,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::AssignIdentityAccessRoleRequest {
                metadata: Some(metadata),
                principal_id: command.principal_id.as_uuid().to_string(),
                assignment_id: command.assignment_id.as_uuid().to_string(),
                role_id: command.role_id.as_uuid().to_string(),
                assignment_scope: Some(encode_scope(&command.scope)),
                risk_tier: encode_risk(command.risk_tier),
                valid_from: command.valid_from.to_rfc3339(),
                valid_until: command.valid_until.to_rfc3339(),
                grant_reason: command.grant_reason,
                ticket_reference: command.ticket_reference,
                grantor_principal_ref: command.grantor_ref,
                approver_principal_ref: command.approver_ref,
            }
        })?;
        let response = self
            .command
            .clone()
            .assign_role(request)
            .await
            .map_err(|error| rpc(error, "assign identity role"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn revoke_role(
        &self,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RevokeIdentityAccessRoleRequest {
                metadata: Some(metadata),
                principal_id: command.principal_id.as_uuid().to_string(),
                assignment_id: command.assignment_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .revoke_role(request)
            .await
            .map_err(|error| rpc(error, "revoke identity role"))?
            .into_inner();
        principal_receipt(response, |principal| PrincipalMutationResult { principal })
    }

    async fn define_role(
        &self,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<DefineRoleResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::DefineIdentityAccessRoleRequest {
                metadata: Some(metadata),
                role_id: command.role_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                code: command.code.as_str().into(),
                display_name: command.display_name,
                risk_tier: encode_risk(command.risk_tier),
                permissions: command
                    .permissions
                    .iter()
                    .map(|v| v.as_str().into())
                    .collect(),
            }
        })?;
        let response = self
            .command
            .clone()
            .define_role(request)
            .await
            .map_err(|error| rpc(error, "define identity role"))?
            .into_inner();
        role_receipt(response, |role| DefineRoleResult { role })
    }

    async fn set_role_permissions(
        &self,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<RoleMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::SetIdentityAccessRolePermissionsRequest {
                metadata: Some(metadata),
                role_id: command.role_id.as_uuid().to_string(),
                permissions: command
                    .permissions
                    .iter()
                    .map(|v| v.as_str().into())
                    .collect(),
            }
        })?;
        let response = self
            .command
            .clone()
            .set_role_permissions(request)
            .await
            .map_err(|error| rpc(error, "set identity role permissions"))?
            .into_inner();
        role_receipt(response, |role| RoleMutationResult { role })
    }

    async fn get_principal(
        &self,
        envelope: QueryEnvelope<GetPrincipal>,
    ) -> Result<QueryResult<GetPrincipalResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetIdentityAccessPrincipalRequest {
                metadata: Some(metadata),
                principal_id: query.principal_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_principal(request)
            .await
            .map_err(|error| rpc(error, "get identity principal"))?
            .into_inner();
        let principal = response.principal.map(decode_principal).transpose()?;
        query_result(response.metadata, GetPrincipalResult { principal })
    }

    async fn list_principals(
        &self,
        envelope: QueryEnvelope<ListPrincipals>,
    ) -> Result<QueryResult<PrincipalList>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListIdentityAccessPrincipalsRequest {
                metadata: Some(metadata),
                status: query.filter.status.map(encode_status),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_principals(request)
            .await
            .map_err(|error| rpc(error, "list identity principals"))?
            .into_inner();
        let principals = response
            .principals
            .into_iter()
            .map(decode_principal)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            PrincipalList {
                principals,
                next_cursor: response.next_cursor,
            },
        )
    }

    async fn get_role(
        &self,
        envelope: QueryEnvelope<GetRole>,
    ) -> Result<QueryResult<GetRoleResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetIdentityAccessRoleRequest {
                metadata: Some(metadata),
                role_id: query.role_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_role(request)
            .await
            .map_err(|error| rpc(error, "get identity role"))?
            .into_inner();
        let role = response.role.map(decode_role).transpose()?;
        query_result(response.metadata, GetRoleResult { role })
    }

    async fn list_roles(
        &self,
        envelope: QueryEnvelope<ListRoles>,
    ) -> Result<QueryResult<RoleList>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListIdentityAccessRolesRequest {
                metadata: Some(metadata),
                risk_tier: query.filter.risk_tier.map(encode_risk),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_roles(request)
            .await
            .map_err(|error| rpc(error, "list identity roles"))?
            .into_inner();
        let roles = response
            .roles
            .into_iter()
            .map(decode_role)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            RoleList {
                roles,
                next_cursor: response.next_cursor,
            },
        )
    }
}

fn decode_principal(
    value: platform::IdentityAccessPrincipalView,
) -> Result<PrincipalView, UiError> {
    Ok(PrincipalView {
        principal_id: PrincipalId::try_from_uuid(uuid(&value.principal_id)?)
            .map_err(|_| invalid_response())?,
        version: value.source_version,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        display_name: value.display_name,
        status: decode_status(value.status)?,
        assignments: value
            .assignments
            .into_iter()
            .map(decode_assignment)
            .collect::<Result<_, _>>()?,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_assignment(
    value: platform::IdentityAccessRoleAssignmentView,
) -> Result<RoleAssignmentView, UiError> {
    Ok(RoleAssignmentView {
        assignment_id: RoleAssignmentId::try_from_uuid(uuid(&value.assignment_id)?)
            .map_err(|_| invalid_response())?,
        role_id: RoleId::try_from_uuid(uuid(&value.role_id)?).map_err(|_| invalid_response())?,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        risk_tier: decode_risk(value.risk_tier)?,
        valid_from: time(&value.valid_from)?,
        valid_until: time(&value.valid_until)?,
        grant_reason: value.grant_reason,
        ticket_reference: value.ticket_reference,
        grantor_ref: value.grantor_principal_ref,
        approver_ref: value.approver_principal_ref,
        revoked_at: value.revoked_at.map(|value| time(&value)).transpose()?,
    })
}

fn decode_role(value: platform::IdentityAccessRoleView) -> Result<RoleView, UiError> {
    Ok(RoleView {
        role_id: RoleId::try_from_uuid(uuid(&value.role_id)?).map_err(|_| invalid_response())?,
        version: value.source_version,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        code: RoleCode::try_new(value.code).map_err(|_| invalid_response())?,
        display_name: value.display_name,
        risk_tier: decode_risk(value.risk_tier)?,
        permissions: value
            .permissions
            .into_iter()
            .map(|v| PermissionKey::try_new(v).map_err(|_| invalid_response()))
            .collect::<Result<_, _>>()?,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn encode_status(value: PrincipalStatus) -> i32 {
    match value {
        PrincipalStatus::Invited => platform::IdentityAccessPrincipalStatus::Invited as i32,
        PrincipalStatus::Active => platform::IdentityAccessPrincipalStatus::Active as i32,
        PrincipalStatus::Suspended => platform::IdentityAccessPrincipalStatus::Suspended as i32,
        PrincipalStatus::Disabled => platform::IdentityAccessPrincipalStatus::Disabled as i32,
    }
}

fn decode_status(value: i32) -> Result<PrincipalStatus, UiError> {
    match platform::IdentityAccessPrincipalStatus::try_from(value).ok() {
        Some(platform::IdentityAccessPrincipalStatus::Invited) => Ok(PrincipalStatus::Invited),
        Some(platform::IdentityAccessPrincipalStatus::Active) => Ok(PrincipalStatus::Active),
        Some(platform::IdentityAccessPrincipalStatus::Suspended) => Ok(PrincipalStatus::Suspended),
        Some(platform::IdentityAccessPrincipalStatus::Disabled) => Ok(PrincipalStatus::Disabled),
        _ => Err(invalid_response()),
    }
}

fn encode_risk(value: RiskTier) -> i32 {
    match value {
        RiskTier::PlatformRoot => platform::IdentityAccessRoleRiskTier::PlatformRoot as i32,
        RiskTier::Administrator => platform::IdentityAccessRoleRiskTier::Administrator as i32,
        RiskTier::High => platform::IdentityAccessRoleRiskTier::High as i32,
        RiskTier::Standard => platform::IdentityAccessRoleRiskTier::Standard as i32,
        RiskTier::Contractor => platform::IdentityAccessRoleRiskTier::Contractor as i32,
        RiskTier::SupportDiagnostic => {
            platform::IdentityAccessRoleRiskTier::SupportDiagnostic as i32
        }
        RiskTier::SupportMutation => platform::IdentityAccessRoleRiskTier::SupportMutation as i32,
        RiskTier::Emergency => platform::IdentityAccessRoleRiskTier::Emergency as i32,
    }
}

fn decode_risk(value: i32) -> Result<RiskTier, UiError> {
    match platform::IdentityAccessRoleRiskTier::try_from(value).ok() {
        Some(platform::IdentityAccessRoleRiskTier::PlatformRoot) => Ok(RiskTier::PlatformRoot),
        Some(platform::IdentityAccessRoleRiskTier::Administrator) => Ok(RiskTier::Administrator),
        Some(platform::IdentityAccessRoleRiskTier::High) => Ok(RiskTier::High),
        Some(platform::IdentityAccessRoleRiskTier::Standard) => Ok(RiskTier::Standard),
        Some(platform::IdentityAccessRoleRiskTier::Contractor) => Ok(RiskTier::Contractor),
        Some(platform::IdentityAccessRoleRiskTier::SupportDiagnostic) => {
            Ok(RiskTier::SupportDiagnostic)
        }
        Some(platform::IdentityAccessRoleRiskTier::SupportMutation) => {
            Ok(RiskTier::SupportMutation)
        }
        Some(platform::IdentityAccessRoleRiskTier::Emergency) => Ok(RiskTier::Emergency),
        _ => Err(invalid_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_wire_enums_are_rejected() {
        assert!(decode_status(0).is_err());
        assert!(decode_risk(0).is_err());
    }
}
