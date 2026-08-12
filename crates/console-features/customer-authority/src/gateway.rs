use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_customer_authority::{
    ChangeCustomerStatus, CreateCustomer, CreateCustomerResult, CustomerAuthorityMutationResult,
    CustomerList, GetCustomer, GetCustomerResult, LinkCustomerAuthority, ListCustomers,
    MergeCustomer, UnlinkCustomerAuthority, UpdateCustomerIdentity,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// Typed target boundary. Generated transport contracts remain behind this
/// trait and cannot reach presenters or views.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CustomerAuthorityProtoTransport: Send + Sync {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateCustomer>,
    ) -> Result<CommandReceipt<CreateCustomerResult>, UiError>;
    async fn update_identity(
        &self,
        envelope: CommandEnvelope<UpdateCustomerIdentity>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError>;
    async fn change_status(
        &self,
        envelope: CommandEnvelope<ChangeCustomerStatus>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError>;
    async fn link_authority(
        &self,
        envelope: CommandEnvelope<LinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError>;
    async fn unlink_authority(
        &self,
        envelope: CommandEnvelope<UnlinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError>;
    async fn merge(
        &self,
        envelope: CommandEnvelope<MergeCustomer>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError>;
    async fn get(
        &self,
        envelope: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<GetCustomerResult>, UiError>;
    async fn list(
        &self,
        envelope: QueryEnvelope<ListCustomers>,
    ) -> Result<QueryResult<CustomerList>, UiError>;
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityGateway<T> {
    transport: T,
}

impl<T> CustomerAuthorityGateway<T> {
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
        impl<T> CommandGateway<$command> for CustomerAuthorityGateway<T>
        where
            T: CustomerAuthorityProtoTransport,
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

command_gateway!(CreateCustomer, CreateCustomerResult, create);
command_gateway!(
    UpdateCustomerIdentity,
    CustomerAuthorityMutationResult,
    update_identity
);
command_gateway!(
    ChangeCustomerStatus,
    CustomerAuthorityMutationResult,
    change_status
);
command_gateway!(
    LinkCustomerAuthority,
    CustomerAuthorityMutationResult,
    link_authority
);
command_gateway!(
    UnlinkCustomerAuthority,
    CustomerAuthorityMutationResult,
    unlink_authority
);
command_gateway!(MergeCustomer, CustomerAuthorityMutationResult, merge);

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<GetCustomer> for CustomerAuthorityGateway<T>
where
    T: CustomerAuthorityProtoTransport,
{
    type Response = GetCustomerResult;

    async fn execute(
        &self,
        envelope: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.get(envelope).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T> QueryGateway<ListCustomers> for CustomerAuthorityGateway<T>
where
    T: CustomerAuthorityProtoTransport,
{
    type Response = CustomerList;

    async fn execute(
        &self,
        envelope: QueryEnvelope<ListCustomers>,
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
    impl CustomerAuthorityProtoTransport for ContractOnlyTransport {
        async fn create(
            &self,
            _: CommandEnvelope<CreateCustomer>,
        ) -> Result<CommandReceipt<CreateCustomerResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn update_identity(
            &self,
            _: CommandEnvelope<UpdateCustomerIdentity>,
        ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn change_status(
            &self,
            _: CommandEnvelope<ChangeCustomerStatus>,
        ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn link_authority(
            &self,
            _: CommandEnvelope<LinkCustomerAuthority>,
        ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn unlink_authority(
            &self,
            _: CommandEnvelope<UnlinkCustomerAuthority>,
        ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn merge(
            &self,
            _: CommandEnvelope<MergeCustomer>,
        ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn get(
            &self,
            _: QueryEnvelope<GetCustomer>,
        ) -> Result<QueryResult<GetCustomerResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn list(
            &self,
            _: QueryEnvelope<ListCustomers>,
        ) -> Result<QueryResult<CustomerList>, UiError> {
            unreachable!("compile-time contract test")
        }
    }

    #[test]
    fn gateway_implements_every_customer_authority_operation() {
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

        type Gateway = CustomerAuthorityGateway<ContractOnlyTransport>;
        command::<CreateCustomer, CreateCustomerResult, Gateway>();
        command::<UpdateCustomerIdentity, CustomerAuthorityMutationResult, Gateway>();
        command::<ChangeCustomerStatus, CustomerAuthorityMutationResult, Gateway>();
        command::<LinkCustomerAuthority, CustomerAuthorityMutationResult, Gateway>();
        command::<UnlinkCustomerAuthority, CustomerAuthorityMutationResult, Gateway>();
        command::<MergeCustomer, CustomerAuthorityMutationResult, Gateway>();
        query::<GetCustomer, GetCustomerResult, Gateway>();
        query::<ListCustomers, CustomerList, Gateway>();
    }
}
