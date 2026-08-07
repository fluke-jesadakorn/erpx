use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_customer_authority::{
    ChangeCustomerStatus, CreateCustomer, CreateCustomerResult, CustomerAuthorityMutationResult,
    CustomerAuthorityView, CustomerCode, CustomerId, CustomerIdentity, CustomerList,
    CustomerStatus, GetCustomer, GetCustomerResult, LinkCustomerAuthority, ListCustomers,
    MergeCustomer, UnlinkCustomerAuthority, UpdateCustomerIdentity,
};
use erpx_proto::platform;
use erpx_ui_foundation::UiError;

use crate::{
    features::customer_authority::gateway::CustomerAuthorityProtoTransport,
    transport::typed_cqrs::GrpcChannel,
};

use super::common::{
    RpcAuthority, command_receipt, decode_scope, encode_scope, invalid_response, query_result, rpc,
    time, uuid,
};

#[derive(Clone)]
pub(crate) struct CustomerAuthorityGrpcTransport {
    command:
        platform::customer_authority_command_service_client::CustomerAuthorityCommandServiceClient<
            GrpcChannel,
        >,
    query: platform::customer_authority_query_service_client::CustomerAuthorityQueryServiceClient<
        GrpcChannel,
    >,
    authority: RpcAuthority,
}

impl CustomerAuthorityGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
        tenant_id: impl AsRef<str>,
        holding_ref: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command: platform::customer_authority_command_service_client::CustomerAuthorityCommandServiceClient::new(channel.clone()),
            query: platform::customer_authority_query_service_client::CustomerAuthorityQueryServiceClient::new(channel),
            authority: RpcAuthority::tenant(access_token, tenant_id, holding_ref)?,
        })
    }
}

fn mutation<R>(
    receipt: Option<erpx_proto::common::CommandReceipt>,
    customer: Option<platform::CustomerAuthorityRecord>,
    result: impl FnOnce(CustomerAuthorityView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    command_receipt(
        receipt,
        result(decode_customer(customer.ok_or_else(invalid_response)?)?),
    )
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CustomerAuthorityProtoTransport for CustomerAuthorityGrpcTransport {
    async fn create(
        &self,
        envelope: CommandEnvelope<CreateCustomer>,
    ) -> Result<CommandReceipt<CreateCustomerResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateCustomerAuthorityRequest {
                metadata: Some(metadata),
                customer_id: command.customer_id.as_uuid().to_string(),
                customer_code: command.code.as_str().into(),
                identity: Some(encode_identity(&command.identity)),
                primary_authority: Some(encode_scope(&command.primary_authority)),
            }
        })?;
        let response = self
            .command
            .clone()
            .create(request)
            .await
            .map_err(|error| rpc(error, "create customer authority"))?
            .into_inner();
        mutation(response.receipt, response.customer, |customer| {
            CreateCustomerResult { customer }
        })
    }

    async fn update_identity(
        &self,
        envelope: CommandEnvelope<UpdateCustomerIdentity>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::UpdateCustomerAuthorityIdentityRequest {
                metadata: Some(metadata),
                customer_id: command.customer_id.as_uuid().to_string(),
                identity: Some(encode_identity(&command.identity)),
            }
        })?;
        let response = self
            .command
            .clone()
            .update_identity(request)
            .await
            .map_err(|error| rpc(error, "update customer identity"))?
            .into_inner();
        mutation(response.receipt, response.customer, |customer| {
            CustomerAuthorityMutationResult { customer }
        })
    }

    async fn change_status(
        &self,
        envelope: CommandEnvelope<ChangeCustomerStatus>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::ChangeCustomerAuthorityStatusRequest {
                metadata: Some(metadata),
                customer_id: command.customer_id.as_uuid().to_string(),
                status: encode_status(command.status),
            }
        })?;
        let response = self
            .command
            .clone()
            .change_status(request)
            .await
            .map_err(|error| rpc(error, "change customer status"))?
            .into_inner();
        mutation(response.receipt, response.customer, |customer| {
            CustomerAuthorityMutationResult { customer }
        })
    }

    async fn link_authority(
        &self,
        envelope: CommandEnvelope<LinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::LinkCustomerAuthorityRequest {
                metadata: Some(metadata),
                customer_id: command.customer_id.as_uuid().to_string(),
                authority: Some(encode_scope(&command.scope)),
            }
        })?;
        let response = self
            .command
            .clone()
            .link_authority(request)
            .await
            .map_err(|error| rpc(error, "link customer authority"))?
            .into_inner();
        mutation(response.receipt, response.customer, |customer| {
            CustomerAuthorityMutationResult { customer }
        })
    }

    async fn unlink_authority(
        &self,
        envelope: CommandEnvelope<UnlinkCustomerAuthority>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::UnlinkCustomerAuthorityRequest {
                metadata: Some(metadata),
                customer_id: command.customer_id.as_uuid().to_string(),
                authority: Some(encode_scope(&command.scope)),
            }
        })?;
        let response = self
            .command
            .clone()
            .unlink_authority(request)
            .await
            .map_err(|error| rpc(error, "unlink customer authority"))?
            .into_inner();
        mutation(response.receipt, response.customer, |customer| {
            CustomerAuthorityMutationResult { customer }
        })
    }

    async fn merge(
        &self,
        envelope: CommandEnvelope<MergeCustomer>,
    ) -> Result<CommandReceipt<CustomerAuthorityMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::MergeCustomerAuthorityRequest {
                metadata: Some(metadata),
                retired_customer_id: command.retired_customer_id.as_uuid().to_string(),
                surviving_customer_id: command.surviving_customer_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .command
            .clone()
            .merge(request)
            .await
            .map_err(|error| rpc(error, "merge customer authority"))?
            .into_inner();
        mutation(response.receipt, response.retired_customer, |customer| {
            CustomerAuthorityMutationResult { customer }
        })
    }

    async fn get(
        &self,
        envelope: QueryEnvelope<GetCustomer>,
    ) -> Result<QueryResult<GetCustomerResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetCustomerAuthorityRequest {
                metadata: Some(metadata),
                customer_id: query.customer_id.as_uuid().to_string(),
            }
        })?;
        let response = self
            .query
            .clone()
            .get(request)
            .await
            .map_err(|error| rpc(error, "get customer authority"))?
            .into_inner();
        let customer = response.customer.map(decode_customer).transpose()?;
        query_result(response.metadata, GetCustomerResult { customer })
    }

    async fn list(
        &self,
        envelope: QueryEnvelope<ListCustomers>,
    ) -> Result<QueryResult<CustomerList>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListCustomerAuthoritiesRequest {
                metadata: Some(metadata),
                filter: Some(platform::CustomerAuthorityListFilter {
                    search: query.filter.search,
                    status: query.filter.status.map(encode_status),
                    include_merged: query.filter.include_merged,
                    page_size: u32::from(query.filter.page_size),
                    after_customer_code: query.filter.after_code.map(|value| value.as_str().into()),
                }),
            }
        })?;
        let response = self
            .query
            .clone()
            .list(request)
            .await
            .map_err(|error| rpc(error, "list customer authorities"))?
            .into_inner();
        let customers = response
            .customers
            .into_iter()
            .map(decode_customer)
            .collect::<Result<_, _>>()?;
        let next_cursor = response
            .next_cursor
            .map(CustomerCode::try_new)
            .transpose()
            .map_err(|_| invalid_response())?;
        query_result(
            response.metadata,
            CustomerList {
                customers,
                next_cursor,
            },
        )
    }
}

fn encode_identity(value: &CustomerIdentity) -> platform::CustomerAuthorityIdentityInput {
    platform::CustomerAuthorityIdentityInput {
        trading_name: value.trading_name().into(),
        legal_name: value.legal_name().map(str::to_owned),
        registration_number: value.registration_number().map(str::to_owned),
        tax_identifier: value.tax_identifier().map(str::to_owned),
        country: value.country().into(),
    }
}

fn decode_customer(
    value: platform::CustomerAuthorityRecord,
) -> Result<CustomerAuthorityView, UiError> {
    Ok(CustomerAuthorityView {
        customer_id: CustomerId::try_from_uuid(uuid(&value.customer_id)?)
            .map_err(|_| invalid_response())?,
        version: value.aggregate_version,
        code: CustomerCode::try_new(value.customer_code).map_err(|_| invalid_response())?,
        trading_name: value.trading_name,
        legal_name: value.legal_name,
        country: value.country,
        status: decode_status(value.status)?,
        primary_authority: decode_scope(value.primary_authority.ok_or_else(invalid_response)?)?,
        authority_links: value
            .authority_links
            .into_iter()
            .map(decode_scope)
            .collect::<Result<_, _>>()?,
        merged_into: value
            .merged_into_customer_id
            .map(|value| CustomerId::try_from_uuid(uuid(&value)?).map_err(|_| invalid_response()))
            .transpose()?,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn encode_status(value: CustomerStatus) -> i32 {
    match value {
        CustomerStatus::Prospect => platform::CustomerAuthorityStatus::Prospect as i32,
        CustomerStatus::Onboarding => platform::CustomerAuthorityStatus::Onboarding as i32,
        CustomerStatus::Active => platform::CustomerAuthorityStatus::Active as i32,
        CustomerStatus::Suspended => platform::CustomerAuthorityStatus::Suspended as i32,
        CustomerStatus::Churned => platform::CustomerAuthorityStatus::Churned as i32,
    }
}

fn decode_status(value: i32) -> Result<CustomerStatus, UiError> {
    match platform::CustomerAuthorityStatus::try_from(value).ok() {
        Some(platform::CustomerAuthorityStatus::Prospect) => Ok(CustomerStatus::Prospect),
        Some(platform::CustomerAuthorityStatus::Onboarding) => Ok(CustomerStatus::Onboarding),
        Some(platform::CustomerAuthorityStatus::Active) => Ok(CustomerStatus::Active),
        Some(platform::CustomerAuthorityStatus::Suspended) => Ok(CustomerStatus::Suspended),
        Some(platform::CustomerAuthorityStatus::Churned) => Ok(CustomerStatus::Churned),
        _ => Err(invalid_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_customer_status_is_rejected() {
        assert!(decode_status(0).is_err());
    }
}
