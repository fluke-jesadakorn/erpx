use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_organization::{
    ActivateOrganizationNode, ArchiveOrganizationNode, ChangeOrganizationCode,
    CreateOrganizationNode, CreateOrganizationNodeResult, GetOrganizationNode,
    GetOrganizationNodeResult, ListOrganizationNodes, NodeList, NodeView,
    OrganizationMutationResult, OrganizationNodeId, OrganizationNodeKind, OrganizationNodeStatus,
    OrganizationPlacement, RenameOrganizationNode, ReparentOrganizationNode,
    SuspendOrganizationNode,
};
use erpx_proto::platform;
use erpx_ui_foundation::UiError;

use crate::{
    features::organization::gateway::OrganizationProtoTransport, transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    RpcAuthority, command_receipt, decode_scope, encode_scope, invalid_response, query_result, rpc,
    time, uuid,
};

#[derive(Clone)]
pub(crate) struct OrganizationGrpcTransport {
    command: platform::organization_command_service_client::OrganizationCommandServiceClient<
        GrpcChannel,
    >,
    query: platform::organization_query_service_client::OrganizationQueryServiceClient<GrpcChannel>,
    authority: RpcAuthority,
}

impl OrganizationGrpcTransport {
    pub(crate) fn try_active_scope(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command:
                platform::organization_command_service_client::OrganizationCommandServiceClient::new(
                    channel.clone(),
                ),
            query: platform::organization_query_service_client::OrganizationQueryServiceClient::new(
                channel,
            ),
            authority: RpcAuthority::active_scope(access_token)?,
        })
    }

    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
        tenant_id: impl AsRef<str>,
        holding_ref: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command:
                platform::organization_command_service_client::OrganizationCommandServiceClient::new(
                    channel.clone(),
                ),
            query: platform::organization_query_service_client::OrganizationQueryServiceClient::new(
                channel,
            ),
            authority: RpcAuthority::tenant(access_token, tenant_id, holding_ref)?,
        })
    }
}

fn node_receipt<R>(
    response: platform::OrganizationNodeCommandResponse,
    result: impl FnOnce(NodeView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    let node = decode_node(response.node.ok_or_else(invalid_response)?)?;
    command_receipt(response.receipt, result(node))
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl OrganizationProtoTransport for OrganizationGrpcTransport {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateOrganizationNodeCommandRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
                placement: Some(encode_placement(&command.placement, command.node_id)),
                code: command.code,
                display_name: command.name,
            }
        })?;
        let response = self
            .command
            .clone()
            .create_node(request)
            .await
            .map_err(|error| rpc(error, "create organization node"))?
            .into_inner();
        node_receipt(response, |node| CreateOrganizationNodeResult { node })
    }

    async fn rename(
        &self,
        envelope: CommandEnvelope<RenameOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RenameOrganizationNodeRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
                display_name: command.name,
            }
        })?;
        let response = self
            .command
            .clone()
            .rename_node(request)
            .await
            .map_err(|error| rpc(error, "rename organization node"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn change_code(
        &self,
        envelope: CommandEnvelope<ChangeOrganizationCode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::ChangeOrganizationNodeCodeRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
                code: command.code,
            }
        })?;
        let response = self
            .command
            .clone()
            .change_node_code(request)
            .await
            .map_err(|error| rpc(error, "change organization code"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn reparent(
        &self,
        envelope: CommandEnvelope<ReparentOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::ReparentOrganizationNodeRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
                placement: Some(encode_placement(&command.placement, command.node_id)),
            }
        })?;
        let response = self
            .command
            .clone()
            .reparent_node(request)
            .await
            .map_err(|error| rpc(error, "reparent organization node"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn suspend(
        &self,
        envelope: CommandEnvelope<SuspendOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::OrganizationNodeLifecycleRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .suspend_node(request)
            .await
            .map_err(|error| rpc(error, "suspend organization node"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn activate(
        &self,
        envelope: CommandEnvelope<ActivateOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::OrganizationNodeLifecycleRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .activate_node(request)
            .await
            .map_err(|error| rpc(error, "activate organization node"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn archive(
        &self,
        envelope: CommandEnvelope<ArchiveOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::OrganizationNodeLifecycleRequest {
                metadata: Some(metadata),
                node_id: command.node_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .archive_node(request)
            .await
            .map_err(|error| rpc(error, "archive organization node"))?
            .into_inner();
        node_receipt(response, |node| OrganizationMutationResult { node })
    }

    async fn get(
        &self,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<GetOrganizationNodeResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetOrganizationNodeRequest {
                metadata: Some(metadata),
                node_id: query.node_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_node(request)
            .await
            .map_err(|error| rpc(error, "get organization node"))?
            .into_inner();
        let node = response.node.map(decode_node).transpose()?;
        query_result(response.metadata, GetOrganizationNodeResult { node })
    }

    async fn list(
        &self,
        envelope: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<NodeList>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListOrganizationNodeRecordsRequest {
                metadata: Some(metadata),
                kind: query.filter.kind.map(encode_kind),
                status: query.filter.status.map(encode_status),
                parent_node_id: query.filter.parent_id.map(|id| id.as_uuid().to_string()),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_nodes(request)
            .await
            .map_err(|error| rpc(error, "list organization nodes"))?
            .into_inner();
        let nodes = response
            .nodes
            .into_iter()
            .map(decode_node)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            NodeList {
                nodes,
                next_cursor: response.next_cursor,
            },
        )
    }
}

fn encode_placement(
    value: &OrganizationPlacement,
    node_id: OrganizationNodeId,
) -> platform::OrganizationPlacementV1 {
    use platform::organization_placement_v1::Placement;
    let placement = match value {
        OrganizationPlacement::Holding => {
            Placement::Holding(platform::OrganizationHoldingPlacement {})
        }
        OrganizationPlacement::Company { holding_id } => {
            Placement::Company(platform::OrganizationCompanyPlacement {
                holding_id: holding_id.to_string(),
            })
        }
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        } => Placement::Headquarters(platform::OrganizationHeadquartersPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => Placement::Branch(platform::OrganizationBranchPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
        }),
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => Placement::Department(platform::OrganizationDepartmentPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.map(|id| id.to_string()),
        }),
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => Placement::Team(platform::OrganizationTeamPlacement {
            holding_id: holding_id.to_string(),
            company_id: company_id.to_string(),
            branch_id: branch_id.map(|id| id.to_string()),
            department_id: department_id.to_string(),
            team_id: node_id.as_uuid().to_string(),
        }),
    };
    platform::OrganizationPlacementV1 {
        placement: Some(placement),
    }
}

fn decode_placement(
    value: platform::OrganizationPlacementV1,
) -> Result<OrganizationPlacement, UiError> {
    use platform::organization_placement_v1::Placement;
    Ok(match value.placement.ok_or_else(invalid_response)? {
        Placement::Holding(_) => OrganizationPlacement::Holding,
        Placement::Company(value) => OrganizationPlacement::Company {
            holding_id: uuid(&value.holding_id)?,
        },
        Placement::Headquarters(value) => OrganizationPlacement::Headquarters {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
        },
        Placement::Branch(value) => OrganizationPlacement::Branch {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
        },
        Placement::Department(value) => OrganizationPlacement::Department {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
            branch_id: value.branch_id.map(|v| uuid(&v)).transpose()?,
        },
        Placement::Team(value) => OrganizationPlacement::Team {
            holding_id: uuid(&value.holding_id)?,
            company_id: uuid(&value.company_id)?,
            branch_id: value.branch_id.map(|v| uuid(&v)).transpose()?,
            department_id: uuid(&value.department_id)?,
        },
    })
}

fn decode_node(value: platform::OrganizationNodeRecord) -> Result<NodeView, UiError> {
    let node_id =
        OrganizationNodeId::try_from_uuid(uuid(&value.node_id)?).map_err(|_| invalid_response())?;
    let placement = decode_placement(value.placement.ok_or_else(invalid_response)?)?;
    if placement.kind() != decode_kind(value.kind)? {
        return Err(invalid_response());
    }
    let derived_parent = placement.parent_id(node_id);
    let wire_parent = value
        .parent_node_id
        .map(|v| OrganizationNodeId::try_from_uuid(uuid(&v)?).map_err(|_| invalid_response()))
        .transpose()?;
    if derived_parent != wire_parent {
        return Err(invalid_response());
    }
    Ok(NodeView {
        node_id,
        version: value.source_version,
        code: value.code,
        name: value.display_name,
        kind: placement.kind(),
        placement,
        parent_id: wire_parent,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        status: decode_status(value.status)?,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn encode_kind(value: OrganizationNodeKind) -> i32 {
    match value {
        OrganizationNodeKind::Holding => platform::OrganizationNodeKindV1::Holding as i32,
        OrganizationNodeKind::Company => platform::OrganizationNodeKindV1::Company as i32,
        OrganizationNodeKind::Headquarters => platform::OrganizationNodeKindV1::Headquarters as i32,
        OrganizationNodeKind::Branch => platform::OrganizationNodeKindV1::Branch as i32,
        OrganizationNodeKind::Department => platform::OrganizationNodeKindV1::Department as i32,
        OrganizationNodeKind::Team => platform::OrganizationNodeKindV1::Team as i32,
    }
}

fn decode_kind(value: i32) -> Result<OrganizationNodeKind, UiError> {
    match platform::OrganizationNodeKindV1::try_from(value).ok() {
        Some(platform::OrganizationNodeKindV1::Holding) => Ok(OrganizationNodeKind::Holding),
        Some(platform::OrganizationNodeKindV1::Company) => Ok(OrganizationNodeKind::Company),
        Some(platform::OrganizationNodeKindV1::Headquarters) => {
            Ok(OrganizationNodeKind::Headquarters)
        }
        Some(platform::OrganizationNodeKindV1::Branch) => Ok(OrganizationNodeKind::Branch),
        Some(platform::OrganizationNodeKindV1::Department) => Ok(OrganizationNodeKind::Department),
        Some(platform::OrganizationNodeKindV1::Team) => Ok(OrganizationNodeKind::Team),
        _ => Err(invalid_response()),
    }
}

fn encode_status(value: OrganizationNodeStatus) -> i32 {
    match value {
        OrganizationNodeStatus::Active => platform::OrganizationNodeStatusV1::Active as i32,
        OrganizationNodeStatus::Suspended => platform::OrganizationNodeStatusV1::Suspended as i32,
        OrganizationNodeStatus::Archived => platform::OrganizationNodeStatusV1::Archived as i32,
    }
}

fn decode_status(value: i32) -> Result<OrganizationNodeStatus, UiError> {
    match platform::OrganizationNodeStatusV1::try_from(value).ok() {
        Some(platform::OrganizationNodeStatusV1::Active) => Ok(OrganizationNodeStatus::Active),
        Some(platform::OrganizationNodeStatusV1::Suspended) => {
            Ok(OrganizationNodeStatus::Suspended)
        }
        Some(platform::OrganizationNodeStatusV1::Archived) => Ok(OrganizationNodeStatus::Archived),
        _ => Err(invalid_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_node_enums_are_not_accepted() {
        assert!(decode_kind(0).is_err());
        assert!(decode_status(0).is_err());
    }
}
