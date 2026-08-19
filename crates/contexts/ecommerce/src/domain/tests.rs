use super::*;
use chrono::Duration;
use erpx_integration_contracts::{CurrencyCode, reference::RecordTypeId};

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
    value.approve(id(6), 1, at).unwrap();
    value.transition(ChannelStatus::Active, 2, at).unwrap();
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
    value.approve(id(10), 1, at).unwrap();
    value.publish(channel, 2, at).unwrap();
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
    assert_eq!(
        listing.approve(id(5), 1, at),
        Err(CommerceError::SegregationOfDuties)
    );
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
