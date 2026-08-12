use async_trait::async_trait;
use erpx_brand::{
    BrandHistory, BrandMutationResult, CreateBrandDraft, CreateBrandDraftResult, GetBrandDraft,
    GetBrandDraftResult, GetResolvedBrand, GetResolvedBrandResult, ListBrandHistory,
    PublishBrandDraft, RollbackBrand, SaveBrandDraft, SetBrandCategoryLocks,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// Typed target adapter boundary. Generated protobuf remains behind this trait
/// and cannot reach Brand presenters or views.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait BrandProtoTransport: Send + Sync {
    async fn create_draft(
        &self,
        envelope: CommandEnvelope<CreateBrandDraft>,
    ) -> Result<CommandReceipt<CreateBrandDraftResult>, UiError>;
    async fn save_draft(
        &self,
        envelope: CommandEnvelope<SaveBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError>;
    async fn publish_draft(
        &self,
        envelope: CommandEnvelope<PublishBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError>;
    async fn rollback(
        &self,
        envelope: CommandEnvelope<RollbackBrand>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError>;
    async fn set_category_locks(
        &self,
        envelope: CommandEnvelope<SetBrandCategoryLocks>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError>;
    async fn get_draft(
        &self,
        envelope: QueryEnvelope<GetBrandDraft>,
    ) -> Result<QueryResult<GetBrandDraftResult>, UiError>;
    async fn get_resolved(
        &self,
        envelope: QueryEnvelope<GetResolvedBrand>,
    ) -> Result<QueryResult<GetResolvedBrandResult>, UiError>;
    async fn list_history(
        &self,
        envelope: QueryEnvelope<ListBrandHistory>,
    ) -> Result<QueryResult<BrandHistory>, UiError>;
}

#[derive(Debug, Clone)]
pub struct BrandGateway<T> {
    transport: T,
}

impl<T> BrandGateway<T> {
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
        impl<T> CommandGateway<$command> for BrandGateway<T>
        where
            T: BrandProtoTransport,
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

command_gateway!(CreateBrandDraft, CreateBrandDraftResult, create_draft);
command_gateway!(SaveBrandDraft, BrandMutationResult, save_draft);
command_gateway!(PublishBrandDraft, BrandMutationResult, publish_draft);
command_gateway!(RollbackBrand, BrandMutationResult, rollback);
command_gateway!(
    SetBrandCategoryLocks,
    BrandMutationResult,
    set_category_locks
);

macro_rules! query_gateway {
    ($query:ty, $response:ty, $method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T> QueryGateway<$query> for BrandGateway<T>
        where
            T: BrandProtoTransport,
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

query_gateway!(GetBrandDraft, GetBrandDraftResult, get_draft);
query_gateway!(GetResolvedBrand, GetResolvedBrandResult, get_resolved);
query_gateway!(ListBrandHistory, BrandHistory, list_history);

#[cfg(test)]
mod tests {
    use super::*;

    struct ContractOnlyTransport;

    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl BrandProtoTransport for ContractOnlyTransport {
        async fn create_draft(
            &self,
            _: CommandEnvelope<CreateBrandDraft>,
        ) -> Result<CommandReceipt<CreateBrandDraftResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn save_draft(
            &self,
            _: CommandEnvelope<SaveBrandDraft>,
        ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn publish_draft(
            &self,
            _: CommandEnvelope<PublishBrandDraft>,
        ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn rollback(
            &self,
            _: CommandEnvelope<RollbackBrand>,
        ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn set_category_locks(
            &self,
            _: CommandEnvelope<SetBrandCategoryLocks>,
        ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn get_draft(
            &self,
            _: QueryEnvelope<GetBrandDraft>,
        ) -> Result<QueryResult<GetBrandDraftResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn get_resolved(
            &self,
            _: QueryEnvelope<GetResolvedBrand>,
        ) -> Result<QueryResult<GetResolvedBrandResult>, UiError> {
            unreachable!("compile-time contract test")
        }
        async fn list_history(
            &self,
            _: QueryEnvelope<ListBrandHistory>,
        ) -> Result<QueryResult<BrandHistory>, UiError> {
            unreachable!("compile-time contract test")
        }
    }

    #[test]
    fn gateway_implements_every_typed_brand_operation() {
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

        type Gateway = BrandGateway<ContractOnlyTransport>;
        command::<CreateBrandDraft, CreateBrandDraftResult, Gateway>();
        command::<SaveBrandDraft, BrandMutationResult, Gateway>();
        command::<PublishBrandDraft, BrandMutationResult, Gateway>();
        command::<RollbackBrand, BrandMutationResult, Gateway>();
        command::<SetBrandCategoryLocks, BrandMutationResult, Gateway>();
        query::<GetBrandDraft, GetBrandDraftResult, Gateway>();
        query::<GetResolvedBrand, GetResolvedBrandResult, Gateway>();
        query::<ListBrandHistory, BrandHistory, Gateway>();
    }

    #[test]
    fn generated_transport_cannot_leak_into_feature_boundary() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("payload_", "json")));
    }
}
