use async_trait::async_trait;
use erpx_commercial::{
    CatalogPlanId, CatalogPlanStatus, CatalogPlanView, CommercialMutationResult,
    CommercialRecordView, CreateCatalogPlan, CreateEntitlement, CreateSubscription,
    EntitlementCapacity, EntitlementId, EntitlementView, GetCatalogPlan, GetCatalogPlanResult,
    GetEntitlement, GetEntitlementResult, GetSubscription, GetSubscriptionResult, ListCatalogPlans,
    ListEntitlements, ListSubscriptions, Page, PublishCatalogPlan, SubscriptionId,
    SubscriptionStatus, SubscriptionView,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_integration_contracts::CurrencyCode;
use erpx_proto::erpx::commercial as platform;
use erpx_ui_foundation::UiError;

use crate::{
    features::commercial::gateway::CommercialProtoTransport, transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    RpcAuthority, command_receipt, decode_scope, encode_scope, invalid_response, query_result, rpc,
    time, uuid,
};

#[derive(Clone)]
pub(crate) struct CommercialGrpcTransport {
    command:
        platform::commercial_command_service_client::CommercialCommandServiceClient<GrpcChannel>,
    query: platform::commercial_query_service_client::CommercialQueryServiceClient<GrpcChannel>,
    authority: RpcAuthority,
}

impl CommercialGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
        holding_id: impl AsRef<str>,
        data_profile_key: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command:
                platform::commercial_command_service_client::CommercialCommandServiceClient::new(
                    channel.clone(),
                ),
            query: platform::commercial_query_service_client::CommercialQueryServiceClient::new(
                channel,
            ),
            authority: RpcAuthority::holding(access_token, holding_id, data_profile_key)?,
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CommercialProtoTransport for CommercialGrpcTransport {
    async fn create_catalog_plan(
        &self,
        envelope: CommandEnvelope<CreateCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateCommercialCatalogPlanRequest {
                metadata: Some(metadata),
                catalog_plan_id: command.plan_id.as_uuid().to_string(),
                plan_key: command.key,
                display_name: command.display_name,
                amount_minor: command.amount_minor,
                currency: command.currency.to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .create_catalog_plan(request)
            .await
            .map_err(|error| rpc(error, "create commercial catalog plan"))?
            .into_inner();
        let record = CommercialRecordView::CatalogPlan(decode_catalog(
            response.catalog_plan.ok_or_else(invalid_response)?,
        )?);
        command_receipt(response.receipt, CommercialMutationResult { record })
    }

    async fn publish_catalog_plan(
        &self,
        envelope: CommandEnvelope<PublishCatalogPlan>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::PublishCommercialCatalogPlanRequest {
                metadata: Some(metadata),
                catalog_plan_id: command.plan_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .publish_catalog_plan(request)
            .await
            .map_err(|error| rpc(error, "publish commercial catalog plan"))?
            .into_inner();
        let record = CommercialRecordView::CatalogPlan(decode_catalog(
            response.catalog_plan.ok_or_else(invalid_response)?,
        )?);
        command_receipt(response.receipt, CommercialMutationResult { record })
    }

    async fn create_subscription(
        &self,
        envelope: CommandEnvelope<CreateSubscription>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateCommercialSubscriptionRequest {
                metadata: Some(metadata),
                subscription_id: command.subscription_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                customer_id: command.customer_id.to_string(),
                catalog_plan_id: command.plan_id.as_uuid().to_string(),
                amount_minor: command.amount_minor,
                currency: command.currency.to_string(),
                starts_in_trial: command.starts_in_trial,
            }
        })?;
        let response = self
            .command
            .clone()
            .create_subscription(request)
            .await
            .map_err(|error| rpc(error, "create commercial subscription"))?
            .into_inner();
        let record = CommercialRecordView::Subscription(decode_subscription(
            response.subscription.ok_or_else(invalid_response)?,
        )?);
        command_receipt(response.receipt, CommercialMutationResult { record })
    }

    async fn create_entitlement(
        &self,
        envelope: CommandEnvelope<CreateEntitlement>,
    ) -> Result<CommandReceipt<CommercialMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateCommercialEntitlementRequest {
                metadata: Some(metadata),
                entitlement_id: command.entitlement_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                subscription_id: command.subscription_id.as_uuid().to_string(),
                capability: command.capability,
                enabled: command.enabled,
                capacity: Some(encode_capacity(command.capacity)),
            }
        })?;
        let response = self
            .command
            .clone()
            .create_entitlement(request)
            .await
            .map_err(|error| rpc(error, "create commercial entitlement"))?
            .into_inner();
        let record = CommercialRecordView::Entitlement(decode_entitlement(
            response.entitlement.ok_or_else(invalid_response)?,
        )?);
        command_receipt(response.receipt, CommercialMutationResult { record })
    }

    async fn get_catalog_plan(
        &self,
        envelope: QueryEnvelope<GetCatalogPlan>,
    ) -> Result<QueryResult<GetCatalogPlanResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetCommercialCatalogPlanRequest {
                metadata: Some(metadata),
                catalog_plan_id: query.plan_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_catalog_plan(request)
            .await
            .map_err(|error| rpc(error, "get commercial catalog plan"))?
            .into_inner();
        let plan = response.catalog_plan.map(decode_catalog).transpose()?;
        query_result(response.metadata, GetCatalogPlanResult { plan })
    }

    async fn list_catalog_plans(
        &self,
        envelope: QueryEnvelope<ListCatalogPlans>,
    ) -> Result<QueryResult<Page<CatalogPlanView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListCommercialCatalogPlansRequest {
                metadata: Some(metadata),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_catalog_plans(request)
            .await
            .map_err(|error| rpc(error, "list commercial catalog plans"))?
            .into_inner();
        let records = response
            .catalog_plans
            .into_iter()
            .map(decode_catalog)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            Page {
                records,
                next_cursor: response.next_cursor,
            },
        )
    }

    async fn get_subscription(
        &self,
        envelope: QueryEnvelope<GetSubscription>,
    ) -> Result<QueryResult<GetSubscriptionResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetCommercialSubscriptionRequest {
                metadata: Some(metadata),
                subscription_id: query.subscription_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_subscription(request)
            .await
            .map_err(|error| rpc(error, "get commercial subscription"))?
            .into_inner();
        let subscription = response.subscription.map(decode_subscription).transpose()?;
        query_result(response.metadata, GetSubscriptionResult { subscription })
    }

    async fn list_subscriptions(
        &self,
        envelope: QueryEnvelope<ListSubscriptions>,
    ) -> Result<QueryResult<Page<SubscriptionView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListCommercialSubscriptionsRequest {
                metadata: Some(metadata),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_subscriptions(request)
            .await
            .map_err(|error| rpc(error, "list commercial subscriptions"))?
            .into_inner();
        let records = response
            .subscriptions
            .into_iter()
            .map(decode_subscription)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            Page {
                records,
                next_cursor: response.next_cursor,
            },
        )
    }

    async fn get_entitlement(
        &self,
        envelope: QueryEnvelope<GetEntitlement>,
    ) -> Result<QueryResult<GetEntitlementResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetCommercialEntitlementRequest {
                metadata: Some(metadata),
                entitlement_id: query.entitlement_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_entitlement(request)
            .await
            .map_err(|error| rpc(error, "get commercial entitlement"))?
            .into_inner();
        let entitlement = response.entitlement.map(decode_entitlement).transpose()?;
        query_result(response.metadata, GetEntitlementResult { entitlement })
    }

    async fn list_entitlements(
        &self,
        envelope: QueryEnvelope<ListEntitlements>,
    ) -> Result<QueryResult<Page<EntitlementView>>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListCommercialEntitlementsRequest {
                metadata: Some(metadata),
                page_size: u32::from(query.filter.page_size),
                cursor: query.filter.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_entitlements(request)
            .await
            .map_err(|error| rpc(error, "list commercial entitlements"))?
            .into_inner();
        let records = response
            .entitlements
            .into_iter()
            .map(decode_entitlement)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            Page {
                records,
                next_cursor: response.next_cursor,
            },
        )
    }
}

fn encode_capacity(value: EntitlementCapacity) -> platform::CommercialEntitlementCapacity {
    platform::CommercialEntitlementCapacity {
        seats: value.seats,
        companies: value.companies,
        branches: value.branches,
    }
}

fn decode_capacity(
    value: Option<platform::CommercialEntitlementCapacity>,
) -> Result<EntitlementCapacity, UiError> {
    let value = value.ok_or_else(invalid_response)?;
    Ok(EntitlementCapacity {
        seats: value.seats,
        companies: value.companies,
        branches: value.branches,
    })
}

fn decode_catalog(value: platform::CommercialCatalogPlanView) -> Result<CatalogPlanView, UiError> {
    Ok(CatalogPlanView {
        id: CatalogPlanId::try_from_uuid(uuid(&value.catalog_plan_id)?)
            .map_err(|_| invalid_response())?,
        version: value.source_version,
        key: value.plan_key,
        display_name: value.display_name,
        amount_minor: value.amount_minor,
        currency: CurrencyCode::try_new(&value.currency).map_err(|_| invalid_response())?,
        status: match platform::CommercialCatalogPlanStatus::try_from(value.status).ok() {
            Some(platform::CommercialCatalogPlanStatus::Draft) => CatalogPlanStatus::Draft,
            Some(platform::CommercialCatalogPlanStatus::Published) => CatalogPlanStatus::Published,
            _ => return Err(invalid_response()),
        },
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_subscription(
    value: platform::CommercialSubscriptionView,
) -> Result<SubscriptionView, UiError> {
    Ok(SubscriptionView {
        id: SubscriptionId::try_from_uuid(uuid(&value.subscription_id)?)
            .map_err(|_| invalid_response())?,
        version: value.source_version,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        customer_id: uuid(&value.customer_id)?,
        plan_id: CatalogPlanId::try_from_uuid(uuid(&value.catalog_plan_id)?)
            .map_err(|_| invalid_response())?,
        status: match platform::CommercialSubscriptionStatus::try_from(value.status).ok() {
            Some(platform::CommercialSubscriptionStatus::Trial) => SubscriptionStatus::Trial,
            Some(platform::CommercialSubscriptionStatus::Active) => SubscriptionStatus::Active,
            Some(platform::CommercialSubscriptionStatus::PastDue) => SubscriptionStatus::PastDue,
            Some(platform::CommercialSubscriptionStatus::Suspended) => {
                SubscriptionStatus::Suspended
            }
            Some(platform::CommercialSubscriptionStatus::Ended) => SubscriptionStatus::Ended,
            _ => return Err(invalid_response()),
        },
        amount_minor: value.amount_minor,
        currency: CurrencyCode::try_new(&value.currency).map_err(|_| invalid_response())?,
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_entitlement(
    value: platform::CommercialEntitlementView,
) -> Result<EntitlementView, UiError> {
    Ok(EntitlementView {
        id: EntitlementId::try_from_uuid(uuid(&value.entitlement_id)?)
            .map_err(|_| invalid_response())?,
        version: value.source_version,
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        subscription_id: SubscriptionId::try_from_uuid(uuid(&value.subscription_id)?)
            .map_err(|_| invalid_response())?,
        capability: value.capability,
        enabled: value.enabled,
        capacity: decode_capacity(value.capacity)?,
        allocated: decode_capacity(value.allocated)?,
        updated_at: time(&value.updated_at)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_catalog_state_is_rejected() {
        let record = platform::CommercialCatalogPlanView {
            status: 0,
            ..Default::default()
        };
        assert!(decode_catalog(record).is_err());
    }
}
