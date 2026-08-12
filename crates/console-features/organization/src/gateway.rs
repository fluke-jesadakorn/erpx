use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_organization::{
    ActivateOrganizationNode, ArchiveOrganizationNode, ChangeOrganizationCode,
    CreateOrganizationNode, CreateOrganizationNodeResult, GetOrganizationNode,
    GetOrganizationNodeResult, ListOrganizationNodes, NodeList, OrganizationMutationResult,
    RenameOrganizationNode, ReparentOrganizationNode, SuspendOrganizationNode,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// Typed target adapter boundary. Generated protobuf is confined behind this
/// trait and cannot reach presenters or views.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait OrganizationProtoTransport: Send + Sync {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateOrganizationNode>,
    ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, UiError>;
    async fn rename(
        &self,
        envelope: CommandEnvelope<RenameOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn change_code(
        &self,
        envelope: CommandEnvelope<ChangeOrganizationCode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn reparent(
        &self,
        envelope: CommandEnvelope<ReparentOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn suspend(
        &self,
        envelope: CommandEnvelope<SuspendOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn activate(
        &self,
        envelope: CommandEnvelope<ActivateOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn archive(
        &self,
        envelope: CommandEnvelope<ArchiveOrganizationNode>,
    ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError>;
    async fn get(
        &self,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<GetOrganizationNodeResult>, UiError>;
    async fn list(
        &self,
        envelope: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<NodeList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct OrganizationGateway<T> {
    transport: T,
}

impl<T> OrganizationGateway<T> {
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
        impl<T> CommandGateway<$command> for OrganizationGateway<T>
        where
            T: OrganizationProtoTransport,
        {
            type Response = $response;

            async fn execute(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}

command_gateway!(CreateOrganizationNode, CreateOrganizationNodeResult, create);
command_gateway!(RenameOrganizationNode, OrganizationMutationResult, rename);
command_gateway!(
    ChangeOrganizationCode,
    OrganizationMutationResult,
    change_code
);
command_gateway!(
    ReparentOrganizationNode,
    OrganizationMutationResult,
    reparent
);
command_gateway!(SuspendOrganizationNode, OrganizationMutationResult, suspend);
command_gateway!(
    ActivateOrganizationNode,
    OrganizationMutationResult,
    activate
);
command_gateway!(ArchiveOrganizationNode, OrganizationMutationResult, archive);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<GetOrganizationNode> for OrganizationGateway<T>
where
    T: OrganizationProtoTransport,
{
    type Response = GetOrganizationNodeResult;

    async fn execute(
        &self,
        envelope: QueryEnvelope<GetOrganizationNode>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<ListOrganizationNodes> for OrganizationGateway<T>
where
    T: OrganizationProtoTransport,
{
    type Response = NodeList;

    async fn execute(
        &self,
        envelope: QueryEnvelope<ListOrganizationNodes>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.list(envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ContractOnlyTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl OrganizationProtoTransport for ContractOnlyTransport {
        async fn create(
            &self,
            _: CommandEnvelope<CreateOrganizationNode>,
        ) -> Result<CommandReceipt<CreateOrganizationNodeResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn rename(
            &self,
            _: CommandEnvelope<RenameOrganizationNode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn change_code(
            &self,
            _: CommandEnvelope<ChangeOrganizationCode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn reparent(
            &self,
            _: CommandEnvelope<ReparentOrganizationNode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn suspend(
            &self,
            _: CommandEnvelope<SuspendOrganizationNode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn activate(
            &self,
            _: CommandEnvelope<ActivateOrganizationNode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn archive(
            &self,
            _: CommandEnvelope<ArchiveOrganizationNode>,
        ) -> Result<CommandReceipt<OrganizationMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn get(
            &self,
            _: QueryEnvelope<GetOrganizationNode>,
        ) -> Result<QueryResult<GetOrganizationNodeResult>, UiError> {
            unreachable!("compile-time contract test")
        }

        async fn list(
            &self,
            _: QueryEnvelope<ListOrganizationNodes>,
        ) -> Result<QueryResult<NodeList>, UiError> {
            unreachable!("compile-time contract test")
        }
    }

    #[test]
    fn gateway_implements_every_typed_organization_operation() {
        fn command<C, R, G>()
        where
            G: CommandGateway<C, Response = R>,
        {
        }
        fn query<Q, R, G>()
        where
            G: QueryGateway<Q, Response = R>,
        {
        }

        type Gateway = OrganizationGateway<ContractOnlyTransport>;
        command::<CreateOrganizationNode, CreateOrganizationNodeResult, Gateway>();
        command::<RenameOrganizationNode, OrganizationMutationResult, Gateway>();
        command::<ChangeOrganizationCode, OrganizationMutationResult, Gateway>();
        command::<ReparentOrganizationNode, OrganizationMutationResult, Gateway>();
        command::<SuspendOrganizationNode, OrganizationMutationResult, Gateway>();
        command::<ActivateOrganizationNode, OrganizationMutationResult, Gateway>();
        command::<ArchiveOrganizationNode, OrganizationMutationResult, Gateway>();
        query::<GetOrganizationNode, GetOrganizationNodeResult, Gateway>();
        query::<ListOrganizationNodes, NodeList, Gateway>();
    }
}
