use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_identity_access::{
    ActivatePrincipal, AssignRole, CreatePrincipal, CreatePrincipalResult, DefineRole,
    DefineRoleResult, DisablePrincipal, GetPrincipal, GetPrincipalResult, GetRole, GetRoleResult,
    ListPrincipals, ListRoles, PrincipalList, PrincipalMutationResult, ReactivatePrincipal,
    RevokeRole, RoleList, RoleMutationResult, SetRolePermissions, SuspendPrincipal,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// Generated messages terminate in the target-specific implementation of this
/// trait and never reach presenters or views.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait IdentityAccessProtoTransport: Send + Sync {
    async fn create_principal(
        &self,
        envelope: CommandEnvelope<CreatePrincipal>,
    ) -> Result<CommandReceipt<CreatePrincipalResult>, UiError>;
    async fn activate_principal(
        &self,
        envelope: CommandEnvelope<ActivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn suspend_principal(
        &self,
        envelope: CommandEnvelope<SuspendPrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn reactivate_principal(
        &self,
        envelope: CommandEnvelope<ReactivatePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn disable_principal(
        &self,
        envelope: CommandEnvelope<DisablePrincipal>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn assign_role(
        &self,
        envelope: CommandEnvelope<AssignRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn revoke_role(
        &self,
        envelope: CommandEnvelope<RevokeRole>,
    ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError>;
    async fn define_role(
        &self,
        envelope: CommandEnvelope<DefineRole>,
    ) -> Result<CommandReceipt<DefineRoleResult>, UiError>;
    async fn set_role_permissions(
        &self,
        envelope: CommandEnvelope<SetRolePermissions>,
    ) -> Result<CommandReceipt<RoleMutationResult>, UiError>;
    async fn get_principal(
        &self,
        envelope: QueryEnvelope<GetPrincipal>,
    ) -> Result<QueryResult<GetPrincipalResult>, UiError>;
    async fn list_principals(
        &self,
        envelope: QueryEnvelope<ListPrincipals>,
    ) -> Result<QueryResult<PrincipalList>, UiError>;
    async fn get_role(
        &self,
        envelope: QueryEnvelope<GetRole>,
    ) -> Result<QueryResult<GetRoleResult>, UiError>;
    async fn list_roles(
        &self,
        envelope: QueryEnvelope<ListRoles>,
    ) -> Result<QueryResult<RoleList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct IdentityAccessGateway<T> {
    transport: T,
}

impl<T> IdentityAccessGateway<T> {
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
        impl<T> CommandGateway<$command> for IdentityAccessGateway<T>
        where
            T: IdentityAccessProtoTransport,
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

command_gateway!(CreatePrincipal, CreatePrincipalResult, create_principal);
command_gateway!(
    ActivatePrincipal,
    PrincipalMutationResult,
    activate_principal
);
command_gateway!(SuspendPrincipal, PrincipalMutationResult, suspend_principal);
command_gateway!(
    ReactivatePrincipal,
    PrincipalMutationResult,
    reactivate_principal
);
command_gateway!(DisablePrincipal, PrincipalMutationResult, disable_principal);
command_gateway!(AssignRole, PrincipalMutationResult, assign_role);
command_gateway!(RevokeRole, PrincipalMutationResult, revoke_role);
command_gateway!(DefineRole, DefineRoleResult, define_role);
command_gateway!(SetRolePermissions, RoleMutationResult, set_role_permissions);

macro_rules! query_gateway {
    ($query:ty, $response:ty, $method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T> QueryGateway<$query> for IdentityAccessGateway<T>
        where
            T: IdentityAccessProtoTransport,
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

query_gateway!(GetPrincipal, GetPrincipalResult, get_principal);
query_gateway!(ListPrincipals, PrincipalList, list_principals);
query_gateway!(GetRole, GetRoleResult, get_role);
query_gateway!(ListRoles, RoleList, list_roles);

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_command<C, R, G>()
    where
        G: CommandGateway<C, Response = R>,
    {
    }

    fn assert_query<Q, R, G>()
    where
        G: QueryGateway<Q, Response = R>,
    {
    }

    struct ContractOnlyTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl IdentityAccessProtoTransport for ContractOnlyTransport {
        async fn create_principal(
            &self,
            _: CommandEnvelope<CreatePrincipal>,
        ) -> Result<CommandReceipt<CreatePrincipalResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn activate_principal(
            &self,
            _: CommandEnvelope<ActivatePrincipal>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn suspend_principal(
            &self,
            _: CommandEnvelope<SuspendPrincipal>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn reactivate_principal(
            &self,
            _: CommandEnvelope<ReactivatePrincipal>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn disable_principal(
            &self,
            _: CommandEnvelope<DisablePrincipal>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn assign_role(
            &self,
            _: CommandEnvelope<AssignRole>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn revoke_role(
            &self,
            _: CommandEnvelope<RevokeRole>,
        ) -> Result<CommandReceipt<PrincipalMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn define_role(
            &self,
            _: CommandEnvelope<DefineRole>,
        ) -> Result<CommandReceipt<DefineRoleResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn set_role_permissions(
            &self,
            _: CommandEnvelope<SetRolePermissions>,
        ) -> Result<CommandReceipt<RoleMutationResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }

        async fn get_principal(
            &self,
            _: QueryEnvelope<GetPrincipal>,
        ) -> Result<QueryResult<GetPrincipalResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }
        async fn list_principals(
            &self,
            _: QueryEnvelope<ListPrincipals>,
        ) -> Result<QueryResult<PrincipalList>, UiError> {
            unreachable!("compile-time gateway contract")
        }
        async fn get_role(
            &self,
            _: QueryEnvelope<GetRole>,
        ) -> Result<QueryResult<GetRoleResult>, UiError> {
            unreachable!("compile-time gateway contract")
        }
        async fn list_roles(
            &self,
            _: QueryEnvelope<ListRoles>,
        ) -> Result<QueryResult<RoleList>, UiError> {
            unreachable!("compile-time gateway contract")
        }
    }

    #[test]
    fn gateway_implements_every_frozen_operation() {
        type Gateway = IdentityAccessGateway<ContractOnlyTransport>;
        assert_command::<CreatePrincipal, CreatePrincipalResult, Gateway>();
        assert_command::<ActivatePrincipal, PrincipalMutationResult, Gateway>();
        assert_command::<SuspendPrincipal, PrincipalMutationResult, Gateway>();
        assert_command::<ReactivatePrincipal, PrincipalMutationResult, Gateway>();
        assert_command::<DisablePrincipal, PrincipalMutationResult, Gateway>();
        assert_command::<AssignRole, PrincipalMutationResult, Gateway>();
        assert_command::<RevokeRole, PrincipalMutationResult, Gateway>();
        assert_command::<DefineRole, DefineRoleResult, Gateway>();
        assert_command::<SetRolePermissions, RoleMutationResult, Gateway>();
        assert_query::<GetPrincipal, GetPrincipalResult, Gateway>();
        assert_query::<ListPrincipals, PrincipalList, Gateway>();
        assert_query::<GetRole, GetRoleResult, Gateway>();
        assert_query::<ListRoles, RoleList, Gateway>();
    }
}
