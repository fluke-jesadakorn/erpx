use super::*;
use chrono::Duration;
use erpx_integration_contracts::{CurrencyCode, reference::RecordTypeId};
use erpx_kernel::{ActorAttribution, ActorKind, HumanDecisionProof, SessionId};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn record_id(n: u128) -> Uuid {
    let mut bytes = n.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: id(1),
        company_id: id(2),
        branch_id: id(3),
    }
}
fn command_id(value: u128) -> CommandId {
    CommandId::try_from_uuid(id(value)).unwrap()
}
fn human_decision(principal: Uuid, decision_ref: String, at: DateTime<Utc>) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: principal.to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(900)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(),
        decision_ref,
        at,
    )
    .unwrap()
}
fn money(n: i64) -> MoneyAmount {
    MoneyAmount::new(n, CurrencyCode::try_new("THB").unwrap())
}
fn record(n: u128, context: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(record_id(n)).unwrap(),
        context,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn channel(at: DateTime<Utc>) -> CommerceChannel {
    let mut value = CommerceChannel::draft(
        ChannelId::try_from_uuid(id(4)).unwrap(),
        scope(),
        "web",
        vec!["th-TH".into()],
        vec!["THB".into()],
        id(5),
        at,
    )
    .unwrap();
    let approve = command_id(101);
    value
        .approve(
            id(6),
            1,
            &human_decision(
                id(6),
                channel_approve_decision_ref(approve, value.id(), 1),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    let transition = command_id(102);
    value
        .transition(
            ChannelStatus::Active,
            2,
            &human_decision(
                id(6),
                channel_transition_decision_ref(transition, value.id(), 2, ChannelStatus::Active),
                at,
            ),
            transition,
            at,
        )
        .unwrap();
    value
}
fn listing(at: DateTime<Utc>, channel: &CommerceChannel) -> Listing {
    let mut value = Listing::draft(
        ListingId::try_from_uuid(id(7)).unwrap(),
        scope(),
        channel.id(),
        record(8, ContextId::Inventory, "inventory.document"),
        [1; 5],
        money(100),
        true,
        id(9),
        at,
    )
    .unwrap();
    let approve = command_id(103);
    value
        .approve(
            id(10),
            1,
            &human_decision(
                id(10),
                listing_approve_decision_ref(approve, value.id(), 1),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    let publish = command_id(104);
    value
        .publish(
            channel,
            2,
            &human_decision(
                id(10),
                listing_publish_decision_ref(
                    publish,
                    value.id(),
                    2,
                    channel.id(),
                    channel.version(),
                ),
                at,
            ),
            publish,
            at,
        )
        .unwrap();
    value
}
fn owner() -> CommerceIdentity {
    CommerceIdentity {
        realm: CommerceRealm::Guest,
        principal_id: id(11),
        verified_customer: None,
    }
}

#[test]
fn guest_identity_cannot_smuggle_customer_authority() {
    let invalid = CommerceIdentity {
        realm: CommerceRealm::Guest,
        principal_id: id(1),
        verified_customer: Some(record(2, ContextId::CustomerAuthority, "customer.account")),
    };
    assert_eq!(invalid.validate(), Err(CommerceError::RealmViolation));
}

#[test]
fn listing_requires_maker_checker() {
    let at = Utc::now();
    let draft = CommerceChannel::draft(
        ChannelId::try_from_uuid(id(1)).unwrap(),
        scope(),
        "web",
        vec!["th".into()],
        vec!["THB".into()],
        id(2),
        at,
    )
    .unwrap();
    let mut listing = Listing::draft(
        ListingId::try_from_uuid(id(3)).unwrap(),
        scope(),
        draft.id(),
        record(4, ContextId::Inventory, "inventory.document"),
        [1; 5],
        money(10),
        true,
        id(5),
        at,
    )
    .unwrap();
    let approve = command_id(105);
    assert_eq!(
        listing.approve(
            id(5),
            1,
            &human_decision(
                id(5),
                listing_approve_decision_ref(approve, listing.id(), 1),
                at,
            ),
            approve,
            at,
        ),
        Err(CommerceError::SegregationOfDuties)
    );
}

#[test]
fn channel_rejects_mismatched_human_evidence_before_mutation() {
    let at = Utc::now();
    let mut channel = CommerceChannel::draft(
        ChannelId::try_from_uuid(id(110)).unwrap(),
        scope(),
        "web",
        vec!["th".into()],
        vec!["THB".into()],
        id(111),
        at,
    )
    .unwrap();
    let command = command_id(112);
    let wrong = human_decision(id(113), "ecommerce.channel.approve:wrong".into(), at);
    assert_eq!(
        channel.approve(id(113), 1, &wrong, command, at),
        Err(CommerceError::HumanDecisionRequired)
    );
    assert_eq!(channel.version(), 1);
    assert!(channel.human_decisions().is_empty());
}

#[test]
fn cart_conflict_fails_closed() {
    let at = Utc::now();
    let channel = channel(at);
    let listing = listing(at, &channel);
    let mut cart = Cart::create(
        CartId::try_from_uuid(id(20)).unwrap(),
        scope(),
        &channel,
        owner(),
        false,
        at,
    )
    .unwrap();
    cart.put_line(&listing, 2, 1, at).unwrap();
    assert_eq!(
        cart.put_line(&listing, 1, 1, at),
        Err(CommerceError::VersionConflict)
    );
}

#[test]
fn checkout_revalidates_versions_and_creates_one_order() {
    let at = Utc::now();
    let channel = channel(at);
    let listing = listing(at, &channel);
    let mut cart = Cart::create(
        CartId::try_from_uuid(id(21)).unwrap(),
        scope(),
        &channel,
        owner(),
        false,
        at,
    )
    .unwrap();
    cart.put_line(&listing, 1, 1, at).unwrap();
    cart.lock(2, at).unwrap();
    let mut checkout = Checkout::start(
        CheckoutId::try_from_uuid(id(22)).unwrap(),
        scope(),
        "checkout-1",
        &cart,
        money(7),
        money(20),
        money(127),
        "a".repeat(64),
        1,
        at,
    )
    .unwrap();
    assert_eq!(
        checkout.reconcile_revalidation(true, "rv", &[999], at),
        Err(CommerceError::StaleCheckout)
    );
    checkout
        .reconcile_revalidation(true, "rv", &[listing.version()], at)
        .unwrap();
    checkout
        .reconcile_reservation(DecisionStatus::Accepted, "reserve", 4, at)
        .unwrap();
    checkout
        .reconcile_payment(DecisionStatus::Accepted, "payment", 3, at)
        .unwrap();
    checkout
        .reconcile_order("order-1", record(30, ContextId::Sales, "sales.order"), at)
        .unwrap();
    assert_eq!(checkout.status(), CheckoutStatus::Ordered);
    assert_eq!(
        checkout.reconcile_order(
            "order-2",
            record(31, ContextId::Sales, "sales.order"),
            at + Duration::seconds(1)
        ),
        Err(CommerceError::InvalidTransition)
    );
}

#[test]
fn uncertain_payment_cannot_be_treated_as_ordered() {
    let at = Utc::now();
    let channel = channel(at);
    let listing = listing(at, &channel);
    let mut cart = Cart::create(
        CartId::try_from_uuid(id(40)).unwrap(),
        scope(),
        &channel,
        owner(),
        false,
        at,
    )
    .unwrap();
    cart.put_line(&listing, 1, 1, at).unwrap();
    cart.lock(2, at).unwrap();
    let mut checkout = Checkout::start(
        CheckoutId::try_from_uuid(id(41)).unwrap(),
        scope(),
        "checkout-u",
        &cart,
        money(7),
        money(20),
        money(127),
        "b".repeat(64),
        1,
        at,
    )
    .unwrap();
    checkout
        .reconcile_revalidation(true, "rv", &[listing.version()], at)
        .unwrap();
    checkout
        .reconcile_reservation(DecisionStatus::Accepted, "r", 1, at)
        .unwrap();
    checkout
        .reconcile_payment(DecisionStatus::Uncertain, "p", 1, at)
        .unwrap();
    assert_eq!(checkout.status(), CheckoutStatus::Uncertain);
}
