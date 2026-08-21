//! Authorization-first target transport for the E-commerce bounded context.

use async_trait::async_trait;
use chrono::Utc;
use erpx_ecommerce as commerce;
use erpx_ecommerce_sqlx as commerce_sqlx;
use erpx_identity::AuthenticatedRealm;
use erpx_infrastructure::RoutedDatabaseSide;
use erpx_integration_contracts::{
    CurrencyCode, MoneyAmount, RecordTypeId, StableRecordRef, catalog::ContextId,
};
use erpx_kernel::{AuthorityScope, CommandId, CorrelationId, RecordId};
use erpx_proto::{common, erpx::ecommerce as wire};
use prost::Message;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::future_auth::{self, FutureInvocation};
use crate::AppState;

const COMMERCE_WRITE: &str = "ecommerce.write";
const COMMERCE_READ: &str = "ecommerce.read";

#[derive(Clone)]
pub(crate) struct EcommerceCommandRpc {
    state: AppState,
}

#[derive(Clone)]
pub(crate) struct EcommerceQueryRpc {
    state: AppState,
}

impl EcommerceCommandRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl EcommerceQueryRpc {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[derive(Debug, Clone)]
struct CommerceAccess {
    actor: commerce::CommerceActor,
    write: bool,
}

#[async_trait]
impl commerce::AuthorizationPort for CommerceAccess {
    async fn authorize(
        &self,
        principal_id: Uuid,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: commerce::CommerceAction,
    ) -> Result<(), commerce::CommercePortError> {
        let writes = action != commerce::CommerceAction::ViewOwnJourney;
        if principal_id != self.actor.principal_id
            || authority_scope != &self.actor.authority_scope
            || writes != self.write
            || !authority_scope.contains(target_scope)
        {
            return Err(commerce::CommercePortError::Denied);
        }
        Ok(())
    }

    async fn authorize_owner(
        &self,
        principal: &commerce::CommerceIdentity,
        stored_owner: &commerce::CommerceIdentity,
        action: commerce::CommerceAction,
    ) -> Result<(), commerce::CommercePortError> {
        let writes = action != commerce::CommerceAction::ViewOwnJourney;
        if self.actor.external_identity.as_ref() != Some(principal)
            || principal != stored_owner
            || writes != self.write
        {
            return Err(commerce::CommercePortError::Denied);
        }
        Ok(())
    }
}

fn actor(invocation: &FutureInvocation) -> Result<commerce::CommerceActor, Status> {
    let principal_id = invocation.principal_id.as_uuid();
    let external_identity = match invocation.realm {
        AuthenticatedRealm::InternalWorkforce | AuthenticatedRealm::PlatformAdministration => None,
        AuthenticatedRealm::CommercePortal => Some(commerce::CommerceIdentity {
            realm: commerce::CommerceRealm::Customer,
            principal_id,
            verified_customer: None,
        }),
        _ => {
            return Err(Status::permission_denied(
                "authenticated realm is not allowed for E-commerce",
            ));
        }
    };
    Ok(commerce::CommerceActor {
        holding_id: invocation.holding_id,
        principal_id,
        authority_scope: invocation.scope.clone(),
        external_identity,
    })
}

fn command_context(
    value: Option<&common::CommandContext>,
) -> Result<&common::CommandContext, Status> {
    value.ok_or_else(|| Status::invalid_argument("command context is required"))
}

fn query_context(value: Option<&common::QueryContext>) -> Result<&common::QueryContext, Status> {
    value.ok_or_else(|| Status::invalid_argument("query context is required"))
}

fn expected(context: &common::CommandContext) -> Result<u64, Status> {
    (context.expected_aggregate_version > 0)
        .then_some(context.expected_aggregate_version)
        .ok_or_else(|| Status::failed_precondition("exact expected aggregate version is required"))
}

fn identity<M: Message>(
    context: &common::CommandContext,
    request: &M,
) -> Result<commerce::MutationIdentity, Status> {
    Ok(commerce::MutationIdentity {
        command_id: CommandId::try_from_uuid(uuid(&context.command_id, "command_id")?)
            .map_err(|_| Status::invalid_argument("command_id is invalid"))?,
        correlation_id: CorrelationId::try_from_uuid(uuid(
            &context.correlation_id,
            "correlation_id",
        )?)
        .map_err(|_| Status::invalid_argument("correlation_id is invalid"))?,
        idempotency_key: context.idempotency_key.clone(),
        request_digest: hex::encode(Sha256::digest(request.encode_to_vec())),
    })
}

fn uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} is invalid")))
}

fn money(value: Option<wire::MoneyAmount>) -> Result<MoneyAmount, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("money amount is required"))?;
    Ok(MoneyAmount::new(
        value.minor_units,
        CurrencyCode::try_new(&value.currency)
            .map_err(|_| Status::invalid_argument("currency is invalid"))?,
    ))
}

fn stable_ref(value: Option<wire::RecordRef>) -> Result<StableRecordRef, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("record reference is required"))?;
    let owner = ContextId::ALL
        .into_iter()
        .find(|candidate| candidate.slug() == value.owner_context)
        .ok_or_else(|| Status::invalid_argument("record owner context is invalid"))?;
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(&value.record_id, "record_id")?)
            .map_err(|_| Status::invalid_argument("record_id is invalid"))?,
        owner,
        RecordTypeId::try_new(value.record_type)
            .map_err(|_| Status::invalid_argument("record type is invalid"))?,
    )
    .map_err(|_| Status::invalid_argument("record reference is invalid"))
}

fn owner(
    value: Option<wire::CommerceOwnerRef>,
    invocation: &FutureInvocation,
) -> Result<commerce::CommerceIdentity, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("commerce owner is required"))?;
    let principal_id = uuid(&value.principal_id, "owner principal_id")?;
    if principal_id != invocation.principal_id.as_uuid() {
        return Err(Status::permission_denied(
            "commerce owner must be the authenticated principal",
        ));
    }
    let realm = match wire::CommerceRealm::try_from(value.realm) {
        Ok(wire::CommerceRealm::Guest) => commerce::CommerceRealm::Guest,
        Ok(wire::CommerceRealm::Customer) => commerce::CommerceRealm::Customer,
        _ => return Err(Status::invalid_argument("commerce owner realm is invalid")),
    };
    let authenticated_realm = match invocation.realm {
        AuthenticatedRealm::CommercePortal => commerce::CommerceRealm::Customer,
        AuthenticatedRealm::InternalWorkforce | AuthenticatedRealm::PlatformAdministration => realm,
        _ => return Err(Status::permission_denied("commerce owner realm is denied")),
    };
    if realm != authenticated_realm {
        return Err(Status::permission_denied(
            "commerce owner realm does not match the authenticated session",
        ));
    }
    let verified_customer = value
        .verified_customer
        .map(|record| stable_ref(Some(record)))
        .transpose()?;
    let result = commerce::CommerceIdentity {
        realm,
        principal_id,
        verified_customer,
    };
    result
        .validate()
        .map_err(|_| Status::invalid_argument("commerce owner is invalid"))?;
    Ok(result)
}

fn decode_scope(value: Option<&common::AuthorityScope>) -> Result<AuthorityScope, Status> {
    use common::authority_scope::Scope;
    match value.and_then(|value| value.scope.as_ref()) {
        Some(Scope::Company(value)) => Ok(AuthorityScope::Company {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
        }),
        Some(Scope::Branch(value)) => Ok(AuthorityScope::Branch {
            holding_id: uuid(&value.holding_id, "holding_id")?,
            company_id: uuid(&value.company_id, "company_id")?,
            branch_id: uuid(&value.branch_id, "branch_id")?,
        }),
        _ => Err(Status::invalid_argument(
            "E-commerce record scope must be an exact company or branch",
        )),
    }
}

fn encode_scope(value: &AuthorityScope) -> common::AuthorityScope {
    use common::authority_scope::Scope;
    common::AuthorityScope {
        scope: Some(match value {
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            _ => unreachable!("E-commerce domain stores only exact company/branch scope"),
        }),
    }
}

fn decision(value: i32) -> Result<commerce::DecisionStatus, Status> {
    match wire::CommerceDecisionStatus::try_from(value) {
        Ok(wire::CommerceDecisionStatus::Accepted) => Ok(commerce::DecisionStatus::Accepted),
        Ok(wire::CommerceDecisionStatus::Rejected) => Ok(commerce::DecisionStatus::Rejected),
        Ok(wire::CommerceDecisionStatus::Uncertain) => Ok(commerce::DecisionStatus::Uncertain),
        _ => Err(Status::invalid_argument("commerce decision is invalid")),
    }
}

fn channel_status(value: i32) -> Result<commerce::ChannelStatus, Status> {
    match wire::CommerceChannelStatus::try_from(value) {
        Ok(wire::CommerceChannelStatus::Active) => Ok(commerce::ChannelStatus::Active),
        Ok(wire::CommerceChannelStatus::Suspended) => Ok(commerce::ChannelStatus::Suspended),
        Ok(wire::CommerceChannelStatus::Retired) => Ok(commerce::ChannelStatus::Retired),
        _ => Err(Status::invalid_argument("channel target status is invalid")),
    }
}

fn status(error: commerce::CommercePortError) -> Status {
    match error {
        commerce::CommercePortError::Denied | commerce::CommercePortError::NotFound => {
            Status::not_found("item unavailable")
        }
        commerce::CommercePortError::Conflict
        | commerce::CommercePortError::IdempotencyConflict => {
            Status::aborted("version or idempotency conflict")
        }
        commerce::CommercePortError::Stale => Status::unavailable("commerce projection is stale"),
        commerce::CommercePortError::Unavailable => {
            Status::unavailable("commerce dependency is unavailable")
        }
        commerce::CommercePortError::Invalid => {
            Status::invalid_argument("commerce request is invalid")
        }
        commerce::CommercePortError::Internal => Status::internal("commerce operation failed"),
    }
}

fn response(
    context: &common::CommandContext,
    receipt: commerce::CommerceCommitReceipt,
    record_type: &str,
    lifecycle_status: &str,
) -> wire::EcommerceMutationResponse {
    wire::EcommerceMutationResponse {
        receipt: Some(common::CommandReceipt {
            command_id: context.command_id.clone(),
            correlation_id: context.correlation_id.clone(),
            aggregate: Some(common::AggregateVersion {
                context: "ecommerce".into(),
                aggregate_type: record_type.into(),
                aggregate_id: receipt.aggregate_id.to_string(),
                version: receipt.aggregate_version,
            }),
            committed_events: vec![common::EventPosition {
                event_id: receipt.event_id.to_string(),
                global_sequence: receipt.global_sequence,
            }],
            committed_at: receipt.committed_at.to_rfc3339(),
            replayed: receipt.replayed,
        }),
        record_type: record_type.into(),
        record_id: receipt.aggregate_id.to_string(),
        lifecycle_status: lifecycle_status.into(),
    }
}

async fn command_service(
    state: &AppState,
    invocation: &FutureInvocation,
) -> Result<
    commerce::CommerceCommandService<CommerceAccess, commerce_sqlx::SqlxCommerceWriteRepository>,
    Status,
> {
    let actor = actor(invocation)?;
    let pool = future_auth::pool(state, invocation, RoutedDatabaseSide::Write).await?;
    Ok(commerce::CommerceCommandService::new(
        CommerceAccess { actor, write: true },
        commerce_sqlx::SqlxCommerceWriteRepository::new(pool),
    ))
}

#[tonic::async_trait]
impl wire::ecommerce_command_service_server::EcommerceCommandService for EcommerceCommandRpc {
    async fn create_channel(
        &self,
        request: Request<wire::CreateCommerceChannelRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let scope = decode_scope(body.scope.as_ref())?;
        if !invocation.scope.contains(&scope) {
            return Err(Status::permission_denied(
                "channel scope is outside authority",
            ));
        }
        let proposer = uuid(&body.proposed_by_principal_id, "proposed_by_principal_id")?;
        if proposer != invocation.principal_id.as_uuid() {
            return Err(Status::permission_denied(
                "channel proposer must be the authenticated principal",
            ));
        }
        let channel = commerce::CommerceChannel::draft(
            commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            scope,
            body.code.clone(),
            body.locales.clone(),
            body.currencies.clone(),
            proposer,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let service = command_service(&self.state, &invocation).await?;
        let receipt = service
            .create_channel(actor(&invocation)?, identity(context, body)?, channel)
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.channel",
            "draft",
        )))
    }

    async fn approve_channel(
        &self,
        request: Request<wire::ApproveCommerceChannelRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let decided_at = Utc::now();
        let _human_decision =
            future_auth::human_decision(&invocation, "ecommerce.channel.approve", decided_at)?;
        let approver = uuid(&body.approved_by_principal_id, "approved_by_principal_id")?;
        if approver != invocation.principal_id.as_uuid() {
            return Err(Status::permission_denied(
                "channel approver must be the authenticated principal",
            ));
        }
        let receipt = command_service(&self.state, &invocation)
            .await?
            .approve_channel(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                approver,
                decided_at,
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.channel",
            "approved",
        )))
    }

    async fn transition_channel(
        &self,
        request: Request<wire::TransitionCommerceChannelRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let target = channel_status(body.target_status)?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .transition_channel(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                target,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.channel",
            token(target)?,
        )))
    }

    async fn create_listing(
        &self,
        request: Request<wire::CreateCommerceListingRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let scope = decode_scope(body.scope.as_ref())?;
        if !invocation.scope.contains(&scope) {
            return Err(Status::permission_denied(
                "listing scope is outside authority",
            ));
        }
        let proposer = uuid(&body.proposed_by_principal_id, "proposed_by_principal_id")?;
        if proposer != invocation.principal_id.as_uuid() {
            return Err(Status::permission_denied(
                "listing proposer must be the authenticated principal",
            ));
        }
        let listing = commerce::Listing::draft(
            commerce::ListingId::try_from_uuid(uuid(&body.listing_id, "listing_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            scope,
            commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                .map_err(|error| Status::invalid_argument(error.to_string()))?,
            stable_ref(body.product.clone())?,
            [
                body.product_version,
                body.catalog_version,
                body.price_version,
                body.availability_version,
                body.policy_version,
            ],
            money(body.unit_price.clone())?,
            body.available,
            proposer,
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .create_listing(actor(&invocation)?, identity(context, body)?, listing)
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.catalog-publication",
            "draft",
        )))
    }

    async fn approve_listing(
        &self,
        request: Request<wire::ApproveCommerceListingRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let decided_at = Utc::now();
        let _human_decision =
            future_auth::human_decision(&invocation, "ecommerce.listing.approve", decided_at)?;
        let approver = uuid(&body.approved_by_principal_id, "approved_by_principal_id")?;
        if approver != invocation.principal_id.as_uuid() {
            return Err(Status::permission_denied(
                "listing approver must be the authenticated principal",
            ));
        }
        let receipt = command_service(&self.state, &invocation)
            .await?
            .approve_listing(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ListingId::try_from_uuid(uuid(&body.listing_id, "listing_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                approver,
                decided_at,
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.catalog-publication",
            "approved",
        )))
    }

    async fn publish_listing(
        &self,
        request: Request<wire::PublishCommerceListingRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .publish_listing(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ListingId::try_from_uuid(uuid(&body.listing_id, "listing_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                body.channel_version,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.catalog-publication",
            "published",
        )))
    }

    async fn withdraw_listing(
        &self,
        request: Request<wire::ListingIdentityRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .withdraw_listing(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ListingId::try_from_uuid(uuid(&body.listing_id, "listing_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.catalog-publication",
            "withdrawn",
        )))
    }

    async fn create_cart(
        &self,
        request: Request<wire::CreateCommerceCartRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let scope = decode_scope(body.scope.as_ref())?;
        if !invocation.scope.contains(&scope) {
            return Err(Status::permission_denied("cart scope is outside authority"));
        }
        let receipt = command_service(&self.state, &invocation)
            .await?
            .create_cart(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CartId::try_from_uuid(uuid(&body.cart_id, "cart_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                scope,
                commerce::ChannelId::try_from_uuid(uuid(&body.channel_id, "channel_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.channel_version,
                owner(body.owner.clone(), &invocation)?,
                body.marketing_consent,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.cart",
            "active",
        )))
    }

    async fn put_cart_line(
        &self,
        request: Request<wire::PutCommerceCartLineRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .put_cart_line(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CartId::try_from_uuid(uuid(&body.cart_id, "cart_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                commerce::ListingId::try_from_uuid(uuid(&body.listing_id, "listing_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                body.listing_version,
                body.quantity,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.cart",
            "active",
        )))
    }

    async fn lock_cart(
        &self,
        request: Request<wire::CartIdentityRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .lock_cart(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CartId::try_from_uuid(uuid(&body.cart_id, "cart_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.cart",
            "checkout_locked",
        )))
    }

    async fn start_checkout(
        &self,
        request: Request<wire::StartCommerceCheckoutRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let scope = decode_scope(body.scope.as_ref())?;
        if !invocation.scope.contains(&scope) {
            return Err(Status::permission_denied(
                "checkout scope is outside authority",
            ));
        }
        let receipt = command_service(&self.state, &invocation)
            .await?
            .start_checkout(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CheckoutId::try_from_uuid(uuid(&body.checkout_id, "checkout_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                scope,
                commerce::CartId::try_from_uuid(uuid(&body.cart_id, "cart_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.cart_version,
                money(body.tax.clone())?,
                money(body.shipping.clone())?,
                money(body.total.clone())?,
                body.address_digest.clone(),
                body.terms_version,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.order",
            "revalidating",
        )))
    }

    async fn reconcile_checkout_revalidation(
        &self,
        request: Request<wire::ReconcileCheckoutRevalidationRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let lifecycle = if body.accepted {
            "reserving"
        } else {
            "rejected"
        };
        let receipt = command_service(&self.state, &invocation)
            .await?
            .reconcile_checkout_revalidation(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CheckoutId::try_from_uuid(uuid(&body.checkout_id, "checkout_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                body.accepted,
                body.owner_receipt_id.clone(),
                body.exact_listing_versions.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.order",
            lifecycle,
        )))
    }

    async fn reconcile_checkout_reservation(
        &self,
        request: Request<wire::ReconcileCheckoutDecisionRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        self.reconcile_checkout_decision(request, true).await
    }

    async fn reconcile_checkout_payment(
        &self,
        request: Request<wire::ReconcileCheckoutDecisionRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        self.reconcile_checkout_decision(request, false).await
    }

    async fn reconcile_checkout_order(
        &self,
        request: Request<wire::ReconcileCheckoutOrderRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let order = stable_ref(Some(wire::RecordRef {
            record_id: body.order_record_id.clone(),
            owner_context: body.order_owner_context.clone(),
            record_type: body.order_record_type.clone(),
        }))?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .reconcile_checkout_order(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::CheckoutId::try_from_uuid(uuid(&body.checkout_id, "checkout_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                body.owner_receipt_id.clone(),
                order,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.order",
            "ordered",
        )))
    }

    async fn request_return(
        &self,
        request: Request<wire::RequestCommerceReturnRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let receipt = command_service(&self.state, &invocation)
            .await?
            .request_return(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ReturnId::try_from_uuid(uuid(&body.return_id, "return_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                commerce::CheckoutId::try_from_uuid(uuid(&body.checkout_id, "checkout_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                body.checkout_version,
                owner(body.owner.clone(), &invocation)?,
                body.reason.clone(),
                body.evidence_digest.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.return",
            "requested",
        )))
    }

    async fn reconcile_return_disposition(
        &self,
        request: Request<wire::ReconcileReturnDispositionRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let lifecycle = if body.accepted {
            "refund_pending"
        } else {
            "rejected"
        };
        let receipt = command_service(&self.state, &invocation)
            .await?
            .reconcile_return_disposition(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ReturnId::try_from_uuid(uuid(&body.return_id, "return_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                body.accepted,
                body.owner_receipt_id.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.return",
            lifecycle,
        )))
    }

    async fn reconcile_return_refund(
        &self,
        request: Request<wire::ReconcileReturnRefundRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let target = decision(body.decision)?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .reconcile_return_refund(
                actor(&invocation)?,
                identity(context, body)?,
                commerce::ReturnId::try_from_uuid(uuid(&body.return_id, "return_id")?)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?,
                expected(context)?,
                target,
                body.owner_receipt_id.clone(),
                Utc::now(),
            )
            .await
            .map_err(status)?;
        let lifecycle = match target {
            commerce::DecisionStatus::Accepted => "refunded",
            commerce::DecisionStatus::Rejected => "rejected",
            commerce::DecisionStatus::Uncertain => "uncertain",
            commerce::DecisionStatus::Pending => unreachable!(),
        };
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.return",
            lifecycle,
        )))
    }

    async fn propose_fulfillment(
        &self,
        request: Request<wire::ProposeCommerceFulfillmentRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        if context.expected_aggregate_version != 0 {
            return Err(Status::failed_precondition(
                "create cannot carry an expected version",
            ));
        }
        let scope = decode_scope(body.scope.as_ref())?;
        if !invocation.scope.contains(&scope) {
            return Err(Status::permission_denied(
                "fulfillment scope is outside authority",
            ));
        }
        let checkout = stable_ref(body.checkout.clone())?;
        if checkout.owner_context() != ContextId::Ecommerce
            || checkout.record_type().as_str() != "commerce.order"
        {
            return Err(Status::invalid_argument("checkout reference is invalid"));
        }
        let request = commerce::FulfillmentRequest::propose(
            uuid(&body.fulfillment_id, "fulfillment_id")?,
            scope,
            owner(body.owner.clone(), &invocation)?,
            checkout,
            stable_ref(body.order.clone())?,
            body.reservation_digest.clone(),
            Utc::now(),
        )
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let receipt = command_service(&self.state, &invocation)
            .await?
            .propose_fulfillment(actor(&invocation)?, identity(context, body)?, request)
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.fulfillment-request",
            "proposed",
        )))
    }

    async fn transition_fulfillment(
        &self,
        request: Request<wire::TransitionCommerceFulfillmentRequest>,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let target = match body.target_status.as_str() {
            "accepted" => commerce::FulfillmentStatus::Accepted,
            "dispatched" => commerce::FulfillmentStatus::Dispatched,
            "completed" => commerce::FulfillmentStatus::Completed,
            "rejected" => commerce::FulfillmentStatus::Rejected,
            "cancelled" => commerce::FulfillmentStatus::Cancelled,
            _ => {
                return Err(Status::invalid_argument(
                    "fulfillment target status is invalid",
                ));
            }
        };
        let receipt = command_service(&self.state, &invocation)
            .await?
            .transition_fulfillment(
                actor(&invocation)?,
                identity(context, body)?,
                uuid(&body.fulfillment_id, "fulfillment_id")?,
                expected(context)?,
                target,
                Utc::now(),
            )
            .await
            .map_err(status)?;
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.fulfillment-request",
            token(target)?,
        )))
    }
}

impl EcommerceCommandRpc {
    async fn reconcile_checkout_decision(
        &self,
        request: Request<wire::ReconcileCheckoutDecisionRequest>,
        reservation: bool,
    ) -> Result<Response<wire::EcommerceMutationResponse>, Status> {
        let body = request.get_ref();
        let context = command_context(body.context.as_ref())?;
        let invocation = future_auth::command(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_WRITE,
        )
        .await?;
        let target = decision(body.decision)?;
        let service = command_service(&self.state, &invocation).await?;
        let actor = actor(&invocation)?;
        let identity = identity(context, body)?;
        let id = commerce::CheckoutId::try_from_uuid(uuid(&body.checkout_id, "checkout_id")?)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let receipt = if reservation {
            service
                .reconcile_checkout_reservation(
                    actor,
                    identity,
                    id,
                    expected(context)?,
                    target,
                    body.owner_receipt_id.clone(),
                    body.owner_record_version,
                    Utc::now(),
                )
                .await
        } else {
            service
                .reconcile_checkout_payment(
                    actor,
                    identity,
                    id,
                    expected(context)?,
                    target,
                    body.owner_receipt_id.clone(),
                    body.owner_record_version,
                    Utc::now(),
                )
                .await
        }
        .map_err(status)?;
        let lifecycle = match (reservation, target) {
            (true, commerce::DecisionStatus::Accepted) => "paying",
            (false, commerce::DecisionStatus::Accepted) => "ready_for_order",
            (_, commerce::DecisionStatus::Rejected) => "rejected",
            (_, commerce::DecisionStatus::Uncertain) => "uncertain",
            (_, commerce::DecisionStatus::Pending) => unreachable!(),
        };
        Ok(Response::new(response(
            context,
            receipt,
            "commerce.order",
            lifecycle,
        )))
    }
}

fn token(value: impl serde::Serialize) -> Result<&'static str, Status> {
    let encoded = serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| Status::internal("lifecycle encoding failed"))?;
    match encoded.as_str() {
        "active" => Ok("active"),
        "suspended" => Ok("suspended"),
        "retired" => Ok("retired"),
        "accepted" => Ok("accepted"),
        "dispatched" => Ok("dispatched"),
        "completed" => Ok("completed"),
        "rejected" => Ok("rejected"),
        "cancelled" => Ok("cancelled"),
        _ => Err(Status::internal("unsupported lifecycle encoding")),
    }
}

#[tonic::async_trait]
impl wire::ecommerce_query_service_server::EcommerceQueryService for EcommerceQueryRpc {
    async fn get_channel(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(request, "commerce_channel", "commerce.channel")
            .await
    }

    async fn get_listing(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(request, "commerce_listing", "commerce.catalog-publication")
            .await
    }

    async fn get_cart(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(request, "commerce_cart", "commerce.cart").await
    }

    async fn get_checkout(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(request, "commerce_checkout", "commerce.order")
            .await
    }

    async fn get_return(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(request, "commerce_return", "commerce.return")
            .await
    }

    async fn get_fulfillment(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        self.get(
            request,
            "commerce_fulfillment_request",
            "commerce.fulfillment-request",
        )
        .await
    }
}

impl EcommerceQueryRpc {
    async fn get(
        &self,
        request: Request<wire::GetCommerceRecordRequest>,
        aggregate_type: &'static str,
        record_type: &'static str,
    ) -> Result<Response<wire::GetCommerceRecordResponse>, Status> {
        let body = request.get_ref();
        let context = query_context(body.context.as_ref())?;
        let invocation = future_auth::query(
            &self.state,
            &request,
            Some(context),
            "ecommerce",
            COMMERCE_READ,
        )
        .await?;
        let pool = future_auth::pool(&self.state, &invocation, RoutedDatabaseSide::Read).await?;
        let actor = actor(&invocation)?;
        let service = commerce::CommerceQueryService::new(
            CommerceAccess {
                actor: actor.clone(),
                write: false,
            },
            commerce_sqlx::SqlxCommerceReadRepository::new(pool, invocation.holding_id)
                .map_err(status)?,
        );
        let projection = service
            .get(&actor, aggregate_type, uuid(&body.record_id, "record_id")?)
            .await
            .map_err(status)?;
        Ok(Response::new(wire::GetCommerceRecordResponse {
            record: Some(wire::CommerceRecordProjection {
                record_id: projection.aggregate_id.to_string(),
                source_version: projection.aggregate_version,
                scope: Some(encode_scope(&projection.scope)),
                record_type: record_type.into(),
                lifecycle_status: projection.lifecycle_status,
                owner_principal_id: projection.owner.map(|value| value.principal_id.to_string()),
                channel_id: None,
                checkout_id: None,
                order_record_id: None,
                source_position: projection.source_position,
                projected_at: projection.projected_at.to_rfc3339(),
            }),
            projection: Some(common::ProjectionMetadata {
                checkpoint: None,
                freshness: common::ProjectionFreshnessState::Current as i32,
                lag_millis: u64::try_from(
                    Utc::now()
                        .signed_duration_since(projection.projected_at)
                        .num_milliseconds()
                        .max(0),
                )
                .ok(),
            }),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denial_and_absence_share_the_same_neutral_wire_status() {
        assert_eq!(
            status(commerce::CommercePortError::Denied).code(),
            tonic::Code::NotFound
        );
        assert_eq!(
            status(commerce::CommercePortError::NotFound).code(),
            tonic::Code::NotFound
        );
    }

    #[test]
    fn external_realm_is_never_derived_from_owner_request_text() {
        let source = include_str!("future_ecommerce.rs");
        assert!(source.contains("invocation.realm"));
        assert!(source.contains("CommercePortal"));
        assert!(!source.contains(concat!("Guest", "Portal")));
    }
}
