use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::{AuthorityScope, HumanDecisionProof};
use uuid::Uuid;

use crate::{
    AggregateRepository, AuthorizationPort, Cart, CartId, ChannelId, ChannelStatus, Checkout,
    CheckoutId, CommerceAction, CommerceActor, CommerceAggregate, CommerceChannel, CommerceCommit,
    CommerceCommitReceipt, CommerceIdentity, CommercePortError, CommerceProjection,
    CommerceReadRepository, DecisionStatus, FulfillmentRequest, FulfillmentStatus, Listing,
    ListingId, MutationIdentity, ReturnId, ReturnRequest,
};

pub struct CommercePolicyService<A> {
    authorization: A,
}

pub struct CommerceCommandService<A, R> {
    authorization: A,
    repository: R,
}
impl<A, R> CommerceCommandService<A, R>
where
    A: AuthorizationPort,
    R: AggregateRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }

    pub async fn create_channel(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        channel: CommerceChannel,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.commit_new(
            actor,
            identity,
            CommerceAction::GovernChannel,
            CommerceAggregate::Channel(channel),
            "ecommerce.channel.created",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_channel(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ChannelId,
        expected: u64,
        approver: Uuid,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        if decision.principal_ref() != actor.principal_id.to_string() {
            return Err(CommercePortError::Denied);
        }
        crate::validate_commerce_human_decision(
            &decision,
            &actor.authority_scope,
            &crate::channel_approve_decision_ref(identity.command_id, id, expected),
            at,
        )
        .map_err(domain_error)?;
        let command_id = identity.command_id;
        self.mutate(
            actor,
            identity,
            CommerceAction::GovernChannel,
            "commerce_channel",
            id.as_uuid(),
            expected,
            "ecommerce.channel.approved",
            move |aggregate| match aggregate {
                CommerceAggregate::Channel(value) => {
                    value.approve(approver, expected, &decision, command_id, at)
                }
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition_channel(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ChannelId,
        expected: u64,
        target: ChannelStatus,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        if decision.principal_ref() != actor.principal_id.to_string() {
            return Err(CommercePortError::Denied);
        }
        crate::validate_commerce_human_decision(
            &decision,
            &actor.authority_scope,
            &crate::channel_transition_decision_ref(identity.command_id, id, expected, target),
            at,
        )
        .map_err(domain_error)?;
        let command_id = identity.command_id;
        self.mutate(
            actor,
            identity,
            CommerceAction::GovernChannel,
            "commerce_channel",
            id.as_uuid(),
            expected,
            "ecommerce.channel.transitioned",
            move |aggregate| match aggregate {
                CommerceAggregate::Channel(value) => {
                    value.transition(target, expected, &decision, command_id, at)
                }
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    pub async fn create_listing(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        listing: Listing,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.commit_new(
            actor,
            identity,
            CommerceAction::GovernListing,
            CommerceAggregate::Listing(listing),
            "ecommerce.listing.created",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_listing(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ListingId,
        expected: u64,
        approver: Uuid,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        if decision.principal_ref() != actor.principal_id.to_string() {
            return Err(CommercePortError::Denied);
        }
        crate::validate_commerce_human_decision(
            &decision,
            &actor.authority_scope,
            &crate::listing_approve_decision_ref(identity.command_id, id, expected),
            at,
        )
        .map_err(domain_error)?;
        let command_id = identity.command_id;
        self.mutate(
            actor,
            identity,
            CommerceAction::GovernListing,
            "commerce_listing",
            id.as_uuid(),
            expected,
            "ecommerce.listing.approved",
            move |aggregate| match aggregate {
                CommerceAggregate::Listing(value) => {
                    value.approve(approver, expected, &decision, command_id, at)
                }
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn publish_listing(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        listing_id: ListingId,
        channel_id: ChannelId,
        expected_listing_version: u64,
        exact_channel_version: u64,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        if decision.principal_ref() != actor.principal_id.to_string() {
            return Err(CommercePortError::Denied);
        }
        crate::validate_commerce_human_decision(
            &decision,
            &actor.authority_scope,
            &crate::listing_publish_decision_ref(
                identity.command_id,
                listing_id,
                expected_listing_version,
                channel_id,
                exact_channel_version,
            ),
            at,
        )
        .map_err(domain_error)?;
        let command_id = identity.command_id;
        let channel = self
            .repository
            .load(
                actor.holding_id,
                &actor.authority_scope,
                "commerce_channel",
                channel_id.as_uuid(),
            )
            .await?
            .ok_or(CommercePortError::NotFound)?;
        let CommerceAggregate::Channel(channel) = channel else {
            return Err(CommercePortError::Internal);
        };
        if channel.version() != exact_channel_version {
            return Err(CommercePortError::Conflict);
        }
        self.mutate(
            actor,
            identity,
            CommerceAction::GovernListing,
            "commerce_listing",
            listing_id.as_uuid(),
            expected_listing_version,
            "ecommerce.listing.published",
            move |aggregate| match aggregate {
                CommerceAggregate::Listing(value) => value.publish(
                    &channel,
                    expected_listing_version,
                    &decision,
                    command_id,
                    at,
                ),
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    pub async fn withdraw_listing(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ListingId,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate(
            actor,
            identity,
            CommerceAction::GovernListing,
            "commerce_listing",
            id.as_uuid(),
            expected,
            "ecommerce.listing.withdrawn",
            move |aggregate| match aggregate {
                CommerceAggregate::Listing(value) => value.withdraw(expected, at),
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_cart(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CartId,
        scope: AuthorityScope,
        channel_id: ChannelId,
        exact_channel_version: u64,
        owner: CommerceIdentity,
        marketing_consent: bool,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        let channel = self
            .repository
            .load(
                actor.holding_id,
                &actor.authority_scope,
                "commerce_channel",
                channel_id.as_uuid(),
            )
            .await?
            .ok_or(CommercePortError::NotFound)?;
        let CommerceAggregate::Channel(channel) = channel else {
            return Err(CommercePortError::Internal);
        };
        if channel.version() != exact_channel_version {
            return Err(CommercePortError::Conflict);
        }
        let cart = Cart::create(id, scope, &channel, owner, marketing_consent, at)
            .map_err(domain_error)?;
        self.commit_new(
            actor,
            identity,
            CommerceAction::ChangeOwnCart,
            CommerceAggregate::Cart(cart),
            "ecommerce.cart.created",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn put_cart_line(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        cart_id: CartId,
        listing_id: ListingId,
        expected_cart_version: u64,
        exact_listing_version: u64,
        quantity: u32,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        let listing = self
            .repository
            .load(
                actor.holding_id,
                &actor.authority_scope,
                "commerce_listing",
                listing_id.as_uuid(),
            )
            .await?
            .ok_or(CommercePortError::NotFound)?;
        let CommerceAggregate::Listing(listing) = listing else {
            return Err(CommercePortError::Internal);
        };
        if listing.version() != exact_listing_version {
            return Err(CommercePortError::Conflict);
        }
        self.mutate(
            actor,
            identity,
            CommerceAction::ChangeOwnCart,
            "commerce_cart",
            cart_id.as_uuid(),
            expected_cart_version,
            "ecommerce.cart.line_put",
            move |aggregate| match aggregate {
                CommerceAggregate::Cart(value) => {
                    value.put_line(&listing, quantity, expected_cart_version, at)
                }
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    pub async fn lock_cart(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CartId,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate(
            actor,
            identity,
            CommerceAction::ChangeOwnCart,
            "commerce_cart",
            id.as_uuid(),
            expected,
            "ecommerce.cart.locked",
            move |aggregate| match aggregate {
                CommerceAggregate::Cart(value) => value.lock(expected, at),
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_checkout(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        scope: AuthorityScope,
        cart_id: CartId,
        exact_cart_version: u64,
        tax: erpx_integration_contracts::MoneyAmount,
        shipping: erpx_integration_contracts::MoneyAmount,
        total: erpx_integration_contracts::MoneyAmount,
        address_digest: String,
        terms_version: u64,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        let cart = self
            .repository
            .load(
                actor.holding_id,
                &actor.authority_scope,
                "commerce_cart",
                cart_id.as_uuid(),
            )
            .await?
            .ok_or(CommercePortError::NotFound)?;
        let CommerceAggregate::Cart(cart) = cart else {
            return Err(CommercePortError::Internal);
        };
        if cart.version() != exact_cart_version {
            return Err(CommercePortError::Conflict);
        }
        let checkout = Checkout::start(
            id,
            scope,
            identity.idempotency_key.clone(),
            &cart,
            tax,
            shipping,
            total,
            address_digest,
            terms_version,
            at,
        )
        .map_err(domain_error)?;
        self.commit_new(
            actor,
            identity,
            CommerceAction::Checkout,
            CommerceAggregate::Checkout(Box::new(checkout)),
            "ecommerce.checkout.started",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_checkout_revalidation(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        expected: u64,
        accepted: bool,
        receipt: String,
        exact_listing_versions: Vec<u64>,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_checkout(
            actor,
            identity,
            id,
            expected,
            "ecommerce.checkout.revalidation_reconciled",
            move |value| {
                value.reconcile_revalidation(accepted, receipt, &exact_listing_versions, at)
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_checkout_reservation(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        expected: u64,
        decision: DecisionStatus,
        receipt: String,
        source_version: u64,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_checkout(
            actor,
            identity,
            id,
            expected,
            "ecommerce.checkout.reservation_reconciled",
            move |value| value.reconcile_reservation(decision, receipt, source_version, at),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_checkout_payment(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        expected: u64,
        decision: DecisionStatus,
        receipt: String,
        source_version: u64,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_checkout(
            actor,
            identity,
            id,
            expected,
            "ecommerce.checkout.payment_reconciled",
            move |value| value.reconcile_payment(decision, receipt, source_version, at),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_checkout_order(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        expected: u64,
        receipt: String,
        order: StableRecordRef,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_checkout(
            actor,
            identity,
            id,
            expected,
            "ecommerce.checkout.order_reconciled",
            move |value| value.reconcile_order(receipt, order, at),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_return(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ReturnId,
        checkout_id: CheckoutId,
        exact_checkout_version: u64,
        owner: CommerceIdentity,
        reason: String,
        evidence_digest: String,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        let checkout = self
            .repository
            .load(
                actor.holding_id,
                &actor.authority_scope,
                "commerce_checkout",
                checkout_id.as_uuid(),
            )
            .await?
            .ok_or(CommercePortError::NotFound)?;
        let CommerceAggregate::Checkout(checkout) = checkout else {
            return Err(CommercePortError::Internal);
        };
        if checkout.version() != exact_checkout_version {
            return Err(CommercePortError::Conflict);
        }
        let request = ReturnRequest::request(id, &checkout, owner, reason, evidence_digest, at)
            .map_err(domain_error)?;
        self.commit_new(
            actor,
            identity,
            CommerceAction::RequestReturn,
            CommerceAggregate::Return(request),
            "ecommerce.return.requested",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_return_disposition(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ReturnId,
        expected: u64,
        accepted: bool,
        receipt: String,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_return(
            actor,
            identity,
            id,
            expected,
            "ecommerce.return.disposition_reconciled",
            move |value| value.reconcile_disposition(accepted, receipt, at),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_return_refund(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ReturnId,
        expected: u64,
        decision: DecisionStatus,
        receipt: String,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate_return(
            actor,
            identity,
            id,
            expected,
            "ecommerce.return.refund_reconciled",
            move |value| value.reconcile_refund(decision, receipt, at),
        )
        .await
    }

    pub async fn propose_fulfillment(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        request: FulfillmentRequest,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.commit_new(
            actor,
            identity,
            CommerceAction::Checkout,
            CommerceAggregate::Fulfillment(request),
            "ecommerce.fulfillment.proposed",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition_fulfillment(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: Uuid,
        expected: u64,
        target: FulfillmentStatus,
        at: DateTime<Utc>,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        self.mutate(
            actor,
            identity,
            CommerceAction::Checkout,
            "commerce_fulfillment_request",
            id,
            expected,
            "ecommerce.fulfillment.transitioned",
            move |aggregate| match aggregate {
                CommerceAggregate::Fulfillment(value) => {
                    if value.version() != expected {
                        return Err(crate::CommerceError::VersionConflict);
                    }
                    match target {
                        FulfillmentStatus::Accepted => value.accept(at),
                        FulfillmentStatus::Dispatched => value.dispatch(at),
                        FulfillmentStatus::Completed => value.complete(at),
                        FulfillmentStatus::Rejected => value.reject(at),
                        FulfillmentStatus::Cancelled => value.cancel(at),
                        FulfillmentStatus::Proposed => Err(crate::CommerceError::InvalidTransition),
                    }
                }
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    async fn mutate_checkout<F>(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: CheckoutId,
        expected: u64,
        event_type: &'static str,
        change: F,
    ) -> Result<CommerceCommitReceipt, CommercePortError>
    where
        F: FnOnce(&mut Checkout) -> Result<(), crate::CommerceError>,
    {
        self.mutate(
            actor,
            identity,
            CommerceAction::Checkout,
            "commerce_checkout",
            id.as_uuid(),
            expected,
            event_type,
            move |aggregate| match aggregate {
                CommerceAggregate::Checkout(value) => change(value),
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    async fn mutate_return<F>(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        id: ReturnId,
        expected: u64,
        event_type: &'static str,
        change: F,
    ) -> Result<CommerceCommitReceipt, CommercePortError>
    where
        F: FnOnce(&mut ReturnRequest) -> Result<(), crate::CommerceError>,
    {
        self.mutate(
            actor,
            identity,
            CommerceAction::RequestReturn,
            "commerce_return",
            id.as_uuid(),
            expected,
            event_type,
            move |aggregate| match aggregate {
                CommerceAggregate::Return(value) => change(value),
                _ => unreachable!("repository aggregate type is fixed"),
            },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn mutate<F>(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        action: CommerceAction,
        aggregate_type: &'static str,
        id: Uuid,
        expected: u64,
        event_type: &'static str,
        change: F,
    ) -> Result<CommerceCommitReceipt, CommercePortError>
    where
        F: FnOnce(&mut CommerceAggregate) -> Result<(), crate::CommerceError>,
    {
        let mut aggregate = self
            .repository
            .load(actor.holding_id, &actor.authority_scope, aggregate_type, id)
            .await?
            .ok_or(CommercePortError::NotFound)?;
        if aggregate.version() != expected {
            return Err(CommercePortError::Conflict);
        }
        change(&mut aggregate).map_err(domain_error)?;
        self.commit(
            actor,
            identity,
            action,
            Some(expected),
            aggregate,
            event_type,
            serde_json::json!({"aggregate_id": id, "expected_version": expected}),
        )
        .await
    }

    async fn commit_new(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        action: CommerceAction,
        aggregate: CommerceAggregate,
        event_type: &'static str,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        let id = aggregate.id_uuid();
        self.commit(
            actor,
            identity,
            action,
            None,
            aggregate,
            event_type,
            serde_json::json!({"aggregate_id": id}),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn commit(
        &self,
        actor: CommerceActor,
        identity: MutationIdentity,
        action: CommerceAction,
        expected_version: Option<u64>,
        aggregate: CommerceAggregate,
        event_type: impl Into<String>,
        event_payload: serde_json::Value,
    ) -> Result<CommerceCommitReceipt, CommercePortError> {
        identity.validate()?;
        if !actor.authority_scope.contains(aggregate.scope()) || !action_matches(action, &aggregate)
        {
            return Err(CommercePortError::Denied);
        }
        match (&actor.external_identity, aggregate.owner()) {
            (Some(principal), Some(owner)) => {
                if !matches!(
                    action,
                    CommerceAction::ChangeOwnCart
                        | CommerceAction::Checkout
                        | CommerceAction::RequestReturn
                        | CommerceAction::ViewOwnJourney
                ) {
                    return Err(CommercePortError::Denied);
                }
                self.authorization
                    .authorize_owner(principal, owner, action)
                    .await?;
            }
            (Some(_), None) => return Err(CommercePortError::Denied),
            (None, _) => {
                self.authorization
                    .authorize(
                        actor.principal_id,
                        &actor.authority_scope,
                        aggregate.scope(),
                        action,
                    )
                    .await?;
            }
        }
        self.repository
            .commit(CommerceCommit {
                actor,
                action,
                identity,
                expected_version,
                aggregate,
                event_type: event_type.into(),
                event_payload,
            })
            .await
    }
}

pub struct CommerceQueryService<A, R> {
    authorization: A,
    repository: R,
}
impl<A, R> CommerceQueryService<A, R>
where
    A: AuthorizationPort,
    R: CommerceReadRepository,
{
    pub const fn new(authorization: A, repository: R) -> Self {
        Self {
            authorization,
            repository,
        }
    }
    pub async fn get(
        &self,
        actor: &CommerceActor,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<CommerceProjection, CommercePortError> {
        let projection = self
            .repository
            .get(actor.holding_id, aggregate_type, id)
            .await?
            .ok_or(CommercePortError::NotFound)?;
        if !actor.authority_scope.contains(&projection.scope) {
            return Err(CommercePortError::NotFound);
        }
        if let Some(principal) = &actor.external_identity {
            let owner = projection
                .owner
                .as_ref()
                .ok_or(CommercePortError::NotFound)?;
            self.authorization
                .authorize_owner(principal, owner, CommerceAction::ViewOwnJourney)
                .await
                .map_err(|_| CommercePortError::NotFound)?;
        } else {
            self.authorization
                .authorize(
                    actor.principal_id,
                    &actor.authority_scope,
                    &projection.scope,
                    CommerceAction::ViewOwnJourney,
                )
                .await
                .map_err(|_| CommercePortError::NotFound)?;
        }
        Ok(projection)
    }
}

fn action_matches(action: CommerceAction, aggregate: &CommerceAggregate) -> bool {
    matches!(
        (action, aggregate),
        (CommerceAction::GovernChannel, CommerceAggregate::Channel(_))
            | (CommerceAction::GovernListing, CommerceAggregate::Listing(_))
            | (CommerceAction::ChangeOwnCart, CommerceAggregate::Cart(_))
            | (CommerceAction::Checkout, CommerceAggregate::Checkout(_))
            | (CommerceAction::RequestReturn, CommerceAggregate::Return(_))
            | (CommerceAction::Checkout, CommerceAggregate::Fulfillment(_))
            | (CommerceAction::ViewOwnJourney, CommerceAggregate::Cart(_))
            | (
                CommerceAction::ViewOwnJourney,
                CommerceAggregate::Checkout(_)
            )
            | (CommerceAction::ViewOwnJourney, CommerceAggregate::Return(_))
            | (
                CommerceAction::ViewOwnJourney,
                CommerceAggregate::Fulfillment(_)
            )
    )
}

fn domain_error(error: crate::CommerceError) -> CommercePortError {
    match error {
        crate::CommerceError::VersionConflict => CommercePortError::Conflict,
        _ => CommercePortError::Invalid,
    }
}
impl<A> CommercePolicyService<A> {
    pub const fn new(authorization: A) -> Self {
        Self { authorization }
    }
}
impl<A: AuthorizationPort> CommercePolicyService<A> {
    pub async fn authorize_internal(
        &self,
        principal_id: Uuid,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: CommerceAction,
    ) -> Result<(), CommercePortError> {
        if !authority_scope.contains(target_scope) {
            return Err(CommercePortError::Denied);
        }
        self.authorization
            .authorize(principal_id, authority_scope, target_scope, action)
            .await
    }
}

#[cfg(test)]
mod human_decision_order_tests {
    fn method<'a>(source: &'a str, name: &str, next: &str) -> &'a str {
        let start = source.find(name).expect("decision method remains present");
        let end = source[start..]
            .find(next)
            .map_or(source.len(), |offset| start + offset);
        &source[start..end]
    }

    #[test]
    fn human_decisions_are_rejected_before_owner_repository_access() {
        let source = include_str!("application.rs");
        for body in [
            method(
                source,
                "pub async fn approve_channel",
                "pub async fn transition_channel",
            ),
            method(
                source,
                "pub async fn transition_channel",
                "pub async fn create_listing",
            ),
            method(
                source,
                "pub async fn approve_listing",
                "pub async fn publish_listing",
            ),
            method(
                source,
                "pub async fn publish_listing",
                "pub async fn withdraw_listing",
            ),
        ] {
            let proof = body
                .find("validate_commerce_human_decision")
                .expect("exact proof is validated");
            let repository = [body.find("self.mutate"), body.find(".repository")]
                .into_iter()
                .flatten()
                .min()
                .expect("owner repository path remains present");
            assert!(proof < repository);
        }
    }
}
