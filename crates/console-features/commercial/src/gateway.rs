use async_trait::async_trait;
use erpx_commercial::{
    CatalogPlanView, CommercialMutationResult, CreateCatalogPlan, CreateEntitlement,
    CreateSubscription, EntitlementView, GetCatalogPlan, GetCatalogPlanResult, GetEntitlement,
    GetEntitlementResult, GetSubscription, GetSubscriptionResult, ListCatalogPlans,
    ListEntitlements, ListSubscriptions, Page, PublishCatalogPlan, SubscriptionView,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

/// User-facing typed transport. Provider-originated billing and entitlement
/// evidence is deliberately absent: webhooks/internal jobs own those commands.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CommercialProtoTransport: Send + Sync {
    async fn create_catalog_plan(
        &self,
        envelope: CommandEnvelope<CreateCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError>;
    async fn publish_catalog_plan(
        &self,
        envelope: CommandEnvelope<PublishCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError>;
    async fn create_subscription(
        &self,
        envelope: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError>;
    async fn create_entitlement(
        &self,
        envelope: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError>;
    async fn get_catalog_plan(
        &self,
        envelope: QueryEnvelope<GetCatalogPlan>,
    ) -> Result<QueryResult<GetCatalogPlanResult>, UiError>;
    async fn list_catalog_plans(
        &self,
        envelope: QueryEnvelope<ListCatalogPlans>,
    ) -> Result<QueryResult<Page<CatalogPlanView>>, UiError>;
    async fn get_subscription(
        &self,
        envelope: QueryEnvelope<GetSubscription>,
    ) -> Result<QueryResult<GetSubscriptionResult>, UiError>;
    async fn list_subscriptions(
        &self,
        envelope: QueryEnvelope<ListSubscriptions>,
    ) -> Result<QueryResult<Page<SubscriptionView>>, UiError>;
    async fn get_entitlement(
        &self,
        envelope: QueryEnvelope<GetEntitlement>,
    ) -> Result<QueryResult<GetEntitlementResult>, UiError>;
    async fn list_entitlements(
        &self,
        envelope: QueryEnvelope<ListEntitlements>,
    ) -> Result<QueryResult<Page<EntitlementView>>, UiError>;
}

#[derive(Debug, Clone)]
pub struct CommercialGateway<T> {
    transport: T,
}
impl<T> CommercialGateway<T> {
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
        impl<T: CommercialProtoTransport> CommandGateway<$command> for CommercialGateway<T> {
            type Response = CommercialMutationResult;
            async fn execute(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, UiError> {
                self.transport.$method(envelope).await
            }
        }
    };
}
command_gateway!(CreateCatalogPlan, create_catalog_plan);
command_gateway!(PublishCatalogPlan, publish_catalog_plan);
command_gateway!(CreateSubscription, create_subscription);
command_gateway!(CreateEntitlement, create_entitlement);

macro_rules! query_gateway {
    ($query:ty,$response:ty,$method:ident) => {
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        impl<T: CommercialProtoTransport> QueryGateway<$query> for CommercialGateway<T> {
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
query_gateway!(GetCatalogPlan, GetCatalogPlanResult, get_catalog_plan);
query_gateway!(ListCatalogPlans, Page<CatalogPlanView>, list_catalog_plans);
query_gateway!(GetSubscription, GetSubscriptionResult, get_subscription);
query_gateway!(
    ListSubscriptions,
    Page<SubscriptionView>,
    list_subscriptions
);
query_gateway!(GetEntitlement, GetEntitlementResult, get_entitlement);
query_gateway!(ListEntitlements, Page<EntitlementView>, list_entitlements);

#[cfg(test)]
mod tests {
    use super::*;
    struct ContractOnlyTransport;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl CommercialProtoTransport for ContractOnlyTransport {
        async fn create_catalog_plan(
            &self,
            _: CommandEnvelope<CreateCatalogPlan>,
        ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
            unreachable!()
        }
        async fn publish_catalog_plan(
            &self,
            _: CommandEnvelope<PublishCatalogPlan>,
        ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
            unreachable!()
        }
        async fn create_subscription(
            &self,
            _: CommandEnvelope<CreateSubscription>,
        ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
            unreachable!()
        }
        async fn create_entitlement(
            &self,
            _: CommandEnvelope<CreateEntitlement>,
        ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
            unreachable!()
        }
        async fn get_catalog_plan(
            &self,
            _: QueryEnvelope<GetCatalogPlan>,
        ) -> Result<QueryResult<GetCatalogPlanResult>, UiError> {
            unreachable!()
        }
        async fn list_catalog_plans(
            &self,
            _: QueryEnvelope<ListCatalogPlans>,
        ) -> Result<QueryResult<Page<CatalogPlanView>>, UiError> {
            unreachable!()
        }
        async fn get_subscription(
            &self,
            _: QueryEnvelope<GetSubscription>,
        ) -> Result<QueryResult<GetSubscriptionResult>, UiError> {
            unreachable!()
        }
        async fn list_subscriptions(
            &self,
            _: QueryEnvelope<ListSubscriptions>,
        ) -> Result<QueryResult<Page<SubscriptionView>>, UiError> {
            unreachable!()
        }
        async fn get_entitlement(
            &self,
            _: QueryEnvelope<GetEntitlement>,
        ) -> Result<QueryResult<GetEntitlementResult>, UiError> {
            unreachable!()
        }
        async fn list_entitlements(
            &self,
            _: QueryEnvelope<ListEntitlements>,
        ) -> Result<QueryResult<Page<EntitlementView>>, UiError> {
            unreachable!()
        }
    }

    #[test]
    fn gateway_implements_all_user_facing_typed_operations() {
        fn command<C, G: CommandGateway<C, Response = CommercialMutationResult>>() {}
        fn query<Q, R, G: QueryGateway<Q, Response = R>>() {}
        type G = CommercialGateway<ContractOnlyTransport>;
        command::<CreateCatalogPlan, G>();
        command::<PublishCatalogPlan, G>();
        command::<CreateSubscription, G>();
        command::<CreateEntitlement, G>();
        query::<GetCatalogPlan, GetCatalogPlanResult, G>();
        query::<ListCatalogPlans, Page<CatalogPlanView>, G>();
        query::<GetSubscription, GetSubscriptionResult, G>();
        query::<ListSubscriptions, Page<SubscriptionView>, G>();
        query::<GetEntitlement, GetEntitlementResult, G>();
        query::<ListEntitlements, Page<EntitlementView>, G>();
    }

    #[test]
    fn provider_evidence_is_not_a_console_contract() {
        let source = include_str!("gateway.rs");
        for forbidden in [
            concat!("Apply", "BillingEvent"),
            concat!("Apply", "EntitlementChange"),
            concat!("verification", "_token"),
            concat!("BillingEvent", "Evidence"),
            concat!("EntitlementChange", "Evidence"),
        ] {
            assert!(
                !source.contains(forbidden),
                "Console must not expose {forbidden}"
            );
        }
    }
}
