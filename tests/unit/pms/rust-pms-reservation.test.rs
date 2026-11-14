// Native PMS staff reservation command domain/application facts. No database,
// browser, adapter, persisted receipt or HTTP acceptance is inferred.
#[path = "../../../packages/server-pms/src/application/reservation.rs"]
pub mod application_reservation;
#[path = "../../../packages/server-pms/src/application/room_board.rs"]
pub mod application_room_board;
#[path = "../../../packages/server-pms/src/domain/reservation.rs"]
pub mod reservation;
#[path = "../../../packages/server-pms/src/domain/room_board.rs"]
pub mod room_board;

pub mod domain {
    pub use crate::{reservation, room_board};
}
pub mod application {
    pub use crate::{application_reservation as reservation, application_room_board as room_board};
}

use application::reservation::reservation_request_hash;
use domain::reservation::{
    MAX_STAYS, PmsReservationActor, PmsReservationCancelInput, PmsReservationChangeInput,
    PmsReservationCommand, PmsReservationCommandInput, PmsReservationCreateInput,
    PmsReservationFacts, PmsReservationGuestInput, PmsReservationLine, PmsReservationOperation,
    plan_reservation_command,
};
use domain::room_board::{PmsBoardActor, PmsPropertyRow, PmsRecord};
use erpx_server_shared::public::{CommandKey, FailureKind};
use serde_json::json;

fn board() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.reservations".into()],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn actor() -> PmsReservationActor {
    PmsReservationActor {
        board: board(),
        assignment_id: "assignment".into(),
        role: "pms_reservation".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn property() -> PmsPropertyRow {
    PmsPropertyRow {
        id: "property".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        name: "Hotel".into(),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        config_json: json!({
            "taxBps": 700,
            "serviceBps": 1000,
            "taxInclusive": false
        })
        .to_string(),
        version: 1,
    }
}

fn record(id: &str, kind: &str, data: serde_json::Value) -> PmsRecord {
    PmsRecord {
        id: id.into(),
        kind: kind.into(),
        version: 1,
        updated_at: 1,
        data,
    }
}

fn line() -> PmsReservationLine {
    PmsReservationLine {
        type_id: "type-1".into(),
        rate_id: "rate-1".into(),
        arrival: "2026-11-04".into(),
        departure: "2026-11-06".into(),
        adults: 2,
        child_ages: vec![],
        quantity: 1,
    }
}

fn rate() -> PmsRecord {
    record(
        "rate-1",
        "rate",
        json!({
            "name": "Standard",
            "typeId": "type-1",
            "from": "2026-11-01",
            "to": "2026-11-30",
            "price": 100000,
            "extraAdult": 0,
            "includedGuests": 2,
            "minNights": 1,
            "maxNights": 30,
            "stopSell": false,
            "closedArrival": false,
            "closedDeparture": false,
            "market": "public",
            "partyId": "",
            "package": "",
            "active": true,
            "childBands": [],
            "calendar": []
        }),
    )
}

fn base_facts() -> PmsReservationFacts {
    let mut facts = PmsReservationFacts::empty(property());
    facts.room_types = vec![record(
        "type-1",
        "room_type",
        json!({"name": "Standard", "capacity": 2, "amenities": ""}),
    )];
    facts.rooms = vec![record(
        "room-1",
        "room",
        json!({
            "number": "101", "typeId": "type-1", "active": true,
            "cleanliness": "clean", "cleaningCycle": 0, "dnd": false
        }),
    )];
    facts.rates = vec![rate()];
    facts
}

fn create_command() -> PmsReservationCommand {
    PmsReservationCommand {
        operation: PmsReservationOperation::Create,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsReservationCommandInput::Create(PmsReservationCreateInput {
            guest_id: None,
            guest: Some(PmsReservationGuestInput {
                name: "Guest".into(),
                email: "guest@example.test".into(),
                phone: "0800000000".into(),
            }),
            source: "front desk".into(),
            campaign_code: String::new(),
            external_ref: String::new(),
            group_name: String::new(),
            notes: String::new(),
            block_id: String::new(),
            rooms: vec![line()],
        }),
    }
}

fn change_command() -> PmsReservationCommand {
    let mut changed = line();
    changed.arrival = "2026-11-05".into();
    changed.departure = "2026-11-07".into();
    PmsReservationCommand {
        operation: PmsReservationOperation::Change,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsReservationCommandInput::Change(PmsReservationChangeInput {
            reservation_id: "reservation-1".into(),
            expected_reservation_version: Some(1),
            reason: "Guest changed dates".into(),
            notes: None,
            group_name: None,
            campaign_code: None,
            rooms: vec![changed],
        }),
    }
}

fn cancel_command() -> PmsReservationCommand {
    PmsReservationCommand {
        operation: PmsReservationOperation::Cancel,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsReservationCommandInput::Cancel(PmsReservationCancelInput {
            reservation_id: "reservation-1".into(),
            expected_reservation_version: Some(1),
            reason: "Guest cancelled".into(),
            fee: 0,
        }),
    }
}

fn with_reservation(facts: &mut PmsReservationFacts) {
    facts.reservation = Some(record(
        "reservation-1",
        "reservation",
        json!({
            "number": "PMS-20261104-00000001",
            "guestId": "guest-1",
            "source": "direct",
            "campaignCode": "",
            "externalRef": "",
            "groupName": "",
            "notes": "",
            "state": "confirmed",
            "createdBy": "user",
            "history": []
        }),
    ));
    facts.stays = vec![record(
        "stay-1",
        "stay",
        json!({
            "reservationId": "reservation-1",
            "guestId": "guest-1",
            "typeId": "type-1",
            "rateId": "rate-1",
            "roomId": "",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "adults": 2,
            "childAges": [],
            "state": "reserved",
            "guestAccountId": "account-1",
            "prices": [],
            "segments": []
        }),
    )];
}

fn code(error: &erpx_server_shared::public::BusinessError) -> &'static str {
    error.code
}

#[test]
fn reservation_validation_rejects_invalid_input() {
    let mut command = create_command();
    assert!(command.validate().is_ok());
    command.expected_version = 0;
    assert!(command.validate().is_err());
    command.expected_version = 1;
    command.property_id = String::new();
    assert!(command.validate().is_err());

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms.clear();
    }
    assert!(command.validate().is_err(), "at least one room line");

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms[0].adults = 51;
    }
    assert!(command.validate().is_err());

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms[0].child_ages = vec![18];
    }
    assert!(command.validate().is_err());

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms[0].arrival = "2026-02-30".into();
    }
    assert!(command.validate().is_err());

    let mut command = cancel_command();
    if let PmsReservationCommandInput::Cancel(input) = &mut command.input {
        input.reason = "no".into();
    }
    assert!(command.validate().is_err(), "retained reason min 3");

    let mut command = cancel_command();
    if let PmsReservationCommandInput::Cancel(input) = &mut command.input {
        input.fee = -1;
    }
    assert!(command.validate().is_err());

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.guest = Some(PmsReservationGuestInput {
            name: "Guest".into(),
            email: "not-an-email".into(),
            phone: "0800000000".into(),
        });
    }
    assert!(command.validate().is_err());
}

#[test]
fn reservation_normalization_trims_text_but_not_ids() {
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.source = "  direct\u{3000}".into();
        input.campaign_code = " spring ".into();
        input.external_ref = " ref ".into();
        input.group_name = " group ".into();
        input.notes = " note ".into();
        input.guest = Some(PmsReservationGuestInput {
            name: " Guest ".into(),
            email: " guest@example.test ".into(),
            phone: " 0800000000 ".into(),
        });
        input.block_id = "block".into();
    }
    command.normalize();
    if let PmsReservationCommandInput::Create(input) = &command.input {
        assert_eq!(input.source, "direct");
        assert_eq!(input.campaign_code, "spring");
        assert_eq!(input.external_ref, "ref");
        assert_eq!(input.group_name, "group");
        assert_eq!(input.notes, "note");
        assert_eq!(input.block_id, "block", "ids are never trimmed");
        let guest = input.guest.as_ref().unwrap();
        assert_eq!(guest.name, "Guest");
        assert_eq!(guest.email, "guest@example.test");
        assert_eq!(guest.phone, "0800000000");
    }
}

#[test]
fn reservation_actor_requires_context_and_permission() {
    let a = actor();
    assert!(
        a.require_command(PmsReservationOperation::Create, 0)
            .is_ok()
    );
    let mut denied = a.clone();
    denied.board.permissions = vec!["pms.read".into(), "pms.frontdesk".into()];
    assert!(
        denied
            .require_command(PmsReservationOperation::Create, 0)
            .is_err(),
        "reservation.* requires pms.reservations"
    );
    let mut denied = a.clone();
    denied.board.department_id = Some("department".into());
    assert!(
        denied
            .require_command(PmsReservationOperation::Create, 0)
            .is_err()
    );
    let mut expired = a;
    expired.board.expires_at = 0;
    assert!(
        expired
            .require_command(PmsReservationOperation::Create, 0)
            .is_err()
    );
}

#[test]
fn reservation_request_hash_is_deterministic_and_bounded() {
    let a = actor();
    let command = create_command();
    let first = reservation_request_hash(&a, &command).unwrap();
    let second = reservation_request_hash(&a, &command).unwrap();
    assert_eq!(first, second);
    let mut other = command.clone();
    if let PmsReservationCommandInput::Create(input) = &mut other.input {
        input.source = "website".into();
    }
    assert_ne!(first, reservation_request_hash(&a, &other).unwrap());
    assert_eq!(
        reservation_request_hash(&a, &cancel_command())
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn create_plans_reservation_stay_account_and_retained_prices() {
    let key = CommandKey::new("reservation-create-0001").unwrap();
    let facts = base_facts();
    let plan = plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap();
    assert_eq!(plan.result.property_id, "property");
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.reservation.id, plan.result.id);
    assert_eq!(plan.reservation.version, 1);
    assert_eq!(plan.reservation.data["state"], "confirmed");
    assert_eq!(plan.reservation.data["source"], "front desk");
    assert_eq!(plan.reservation.data["createdBy"], "user");
    assert_eq!(
        plan.reservation.data["history"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        plan.reservation.data["history"][0]["action"],
        "Reservation confirmed"
    );
    assert!(
        plan.reservation.data["number"]
            .as_str()
            .unwrap()
            .starts_with("PMS-20261104-")
    );
    assert_eq!(plan.stays.len(), 1);
    let stay = &plan.stays[0];
    assert_eq!(stay.version, 1);
    assert_eq!(stay.data["reservationId"], plan.reservation.id);
    assert_eq!(stay.data["state"], "reserved");
    assert_eq!(stay.data["roomId"], "");
    let prices = stay.data["prices"].as_array().unwrap();
    assert_eq!(prices.len(), 2, "two nights");
    assert_eq!(
        prices[0],
        json!({"date":"2026-11-04","net":100000,"service":10000,"tax":7700,"total":117700})
    );
    assert_eq!(plan.accounts.len(), 1);
    assert_eq!(plan.accounts[0].version, 1);
    assert_eq!(plan.accounts[0].data["state"], "open");
    assert_eq!(plan.accounts[0].data["due"], 0);
    assert_eq!(plan.accounts[0].data["reservationId"], plan.reservation.id);
    assert_eq!(plan.guest.as_ref().unwrap().kind, "guest");
    assert_eq!(
        plan.guest.as_ref().unwrap().data["email"],
        "guest@example.test"
    );
    assert!(plan.source_key.is_none());
    assert!(plan.block.is_none());
}

#[test]
fn create_gives_each_line_a_distinct_stay_and_account() {
    let key = CommandKey::new("reservation-create-0007").unwrap();
    let mut facts = base_facts();
    facts.rooms.push(record(
        "room-2",
        "room",
        json!({
            "number": "102", "typeId": "type-1", "active": true,
            "cleanliness": "clean", "cleaningCycle": 0, "dnd": false
        }),
    ));
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        let mut second = line();
        second.arrival = "2026-11-05".into();
        second.departure = "2026-11-07".into();
        input.rooms.push(second);
    }
    let plan = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap();
    assert_eq!(plan.stays.len(), 2);
    assert_ne!(plan.stays[0].id, plan.stays[1].id);
    assert_ne!(plan.accounts[0].id, plan.accounts[1].id);
    assert_eq!(plan.accounts.len(), 2);
    assert!(
        plan.stays
            .iter()
            .all(|stay| stay.data["reservationId"] == plan.reservation.id)
    );
}

#[test]
fn create_rejects_missing_guest_past_dates_sold_out_and_duplicate_reference() {
    let key = CommandKey::new("reservation-create-0002").unwrap();
    let mut facts = base_facts();
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.guest = None;
    }
    let error = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_GUEST_REQUIRED");
    assert_eq!(error.kind, FailureKind::InvalidInput);

    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms[0].arrival = "2026-11-03".into();
        input.rooms[0].departure = "2026-11-05".into();
    }
    let error = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_INVALID_DATES");

    facts.stays = vec![record(
        "stay-existing",
        "stay",
        json!({
            "reservationId": "reservation-2", "guestId": "guest-2",
            "typeId": "type-1", "rateId": "rate-1", "roomId": "",
            "arrival": "2026-11-04", "departure": "2026-11-05", "adults": 1,
            "state": "reserved", "guestAccountId": "account-2",
            "prices": [], "segments": []
        }),
    )];
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_SOLD_OUT");

    let mut facts = base_facts();
    facts.external_ref_exists = true;
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.external_ref = "OTA-1".into();
    }
    let error = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_EXTERNAL_REF_EXISTS");
}

#[test]
fn create_retains_rate_occupancy_child_and_party_guards() {
    let key = CommandKey::new("reservation-create-0003").unwrap();
    let mut facts = base_facts();
    facts.rates[0].data["active"] = json!(false);
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_RATE_UNAVAILABLE");

    let mut facts = base_facts();
    facts.rates[0].data["stopSell"] = json!(true);
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_RATE_UNAVAILABLE");

    let mut facts = base_facts();
    facts.room_types[0].data["capacity"] = json!(1);
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_RATE_RESTRICTION");

    let mut facts = base_facts();
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.rooms[0].adults = 1;
        input.rooms[0].child_ages = vec![8];
    }
    let error = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_CHILD_RATE_REQUIRED");

    facts.rates[0].data["childBands"] = json!([{"minAge":5,"maxAge":10,"price":0}]);
    let plan = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap();
    assert_eq!(plan.stays.len(), 1);

    let mut facts = base_facts();
    facts.rates[0].data["partyId"] = json!("party-1");
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert_eq!(code(&error), "PMS_RESERVATION_FINANCE_PARTY_PORT_REQUIRED");
}

#[test]
fn create_retains_group_quota_and_calendar_overrides() {
    let key = CommandKey::new("reservation-create-0004").unwrap();
    let mut facts = base_facts();
    let block = record(
        "block-1",
        "block",
        json!({
            "name": "Group", "typeId": "type-1", "roomId": "", "from": "2026-11-04",
            "to": "2026-11-08", "quantity": 1, "releaseDate": "2026-11-05",
            "state": "active", "kind": "group", "pickedStayIds": ["stay-picked"]
        }),
    );
    facts.block = Some(block.clone());
    facts.blocks = vec![block];
    facts.stays = vec![record(
        "stay-picked",
        "stay",
        json!({
            "reservationId": "reservation-2", "guestId": "guest-2",
            "typeId": "type-1", "rateId": "rate-1", "roomId": "",
            "arrival": "2026-11-04", "departure": "2026-11-06", "adults": 1,
            "state": "reserved", "guestAccountId": "account-2",
            "prices": [], "segments": []
        }),
    )];
    let mut command = create_command();
    if let PmsReservationCommandInput::Create(input) = &mut command.input {
        input.block_id = "block-1".into();
    }
    let error = plan_reservation_command(&actor(), &key, &facts, &command, 5_000).unwrap_err();
    assert_eq!(
        code(&error),
        "PMS_BLOCK_UNAVAILABLE",
        "the only quota night is already picked"
    );

    let mut facts = base_facts();
    facts.rates[0].data["calendar"] = json!([
        {"date": "2026-11-04", "price": 120000, "stopSell": false, "closedArrival": false, "closedDeparture": false}
    ]);
    let plan = plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap();
    let prices = plan.stays[0].data["prices"].as_array().unwrap();
    assert_eq!(prices[0]["net"], 120000);
    assert_eq!(prices[1]["net"], 100000);

    facts.rates[0].data["calendar"] = json!([
        {"date": "2026-11-05", "price": 100000, "stopSell": true, "closedArrival": false, "closedDeparture": false}
    ]);
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_RATE_UNAVAILABLE");

    facts.rates[0].data["calendar"] = json!([
        {"date": "2026-11-04", "price": 100000, "stopSell": false, "closedArrival": true, "closedDeparture": false}
    ]);
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(code(&error), "PMS_RATE_RESTRICTION");
}

#[test]
fn create_requires_open_hotel_and_property_scope() {
    let key = CommandKey::new("reservation-create-0005").unwrap();
    let mut facts = base_facts();
    facts.property.state = "setup".into();
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000)
                .unwrap_err()
        ),
        "PMS_SETUP_REQUIRED"
    );
    facts.property.state = "closing".into();
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000)
                .unwrap_err()
        ),
        "PMS_DAY_CLOSING"
    );
    facts.property.state = "open".into();
    facts.property.company_id = "other".into();
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000)
                .unwrap_err()
        ),
        "PMS_NOT_FOUND"
    );
    facts.property.company_id = "company".into();
    facts.property.version = 2;
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000)
                .unwrap_err()
        ),
        "PMS_VERSION_CONFLICT"
    );
}

#[test]
fn create_rejects_unbounded_stay_facts() {
    let key = CommandKey::new("reservation-create-0006").unwrap();
    let mut facts = base_facts();
    facts.stays = (0..=MAX_STAYS)
        .map(|index| record(&format!("stay-{index}"), "stay", json!({})))
        .collect();
    let error =
        plan_reservation_command(&actor(), &key, &facts, &create_command(), 5_000).unwrap_err();
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert_eq!(code(&error), "PMS_RESERVATION_READ_LIMIT");
}

#[test]
fn change_replaces_reserved_stay_and_appends_history() {
    let key = CommandKey::new("reservation-change-0001").unwrap();
    let mut facts = base_facts();
    with_reservation(&mut facts);
    let plan = plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000).unwrap();
    assert_eq!(plan.result.id, "reservation-1");
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.reservation.version, 2);
    assert_eq!(plan.reservation.data["state"], "confirmed");
    assert_eq!(
        plan.reservation.data["history"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        plan.reservation.data["history"][0]["action"],
        "Reservation changed"
    );
    assert_eq!(
        plan.reservation.data["history"][0]["reason"],
        "Guest changed dates"
    );
    assert_eq!(plan.stays.len(), 1);
    let stay = &plan.stays[0];
    assert_eq!(stay.id, "stay-1");
    assert_eq!(stay.version, 2);
    assert_eq!(stay.data["arrival"], "2026-11-05");
    assert_eq!(stay.data["departure"], "2026-11-07");
    assert_eq!(stay.data["roomId"], "");
    assert_eq!(stay.data["segments"], json!([]));
    assert_eq!(stay.data["prices"].as_array().unwrap().len(), 2);
}

#[test]
fn change_refuses_financial_activity_checked_in_and_stale_snapshots() {
    let key = CommandKey::new("reservation-change-0002").unwrap();
    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.financial_activity = true;
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000)
                .unwrap_err()
        ),
        "PMS_CONTACT_HOTEL"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.stays[0].data["state"] = json!("in_house");
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000)
                .unwrap_err()
        ),
        "PMS_ALREADY_CHECKED_IN"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    let mut command = change_command();
    if let PmsReservationCommandInput::Change(input) = &mut command.input {
        input.expected_reservation_version = Some(2);
    }
    assert_eq!(
        code(&plan_reservation_command(&actor(), &key, &facts, &command, 6_000).unwrap_err()),
        "PMS_VERSION_CONFLICT"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    let mut command = change_command();
    if let PmsReservationCommandInput::Change(input) = &mut command.input {
        input.rooms.push(line());
    }
    assert_eq!(
        code(&plan_reservation_command(&actor(), &key, &facts, &command, 6_000).unwrap_err()),
        "PMS_CONTACT_HOTEL"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.reservation.as_mut().unwrap().data["state"] = json!("cancelled");
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000)
                .unwrap_err()
        ),
        "PMS_RESERVATION_CLOSED"
    );
}

#[test]
fn change_capacity_excludes_the_changed_stay() {
    let key = CommandKey::new("reservation-change-0003").unwrap();
    let mut facts = base_facts();
    with_reservation(&mut facts);
    // The hotel has one room and the sibling stay occupies it; changing the same
    // reservation must not count its own occupancy twice.
    let plan = plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000).unwrap();
    assert_eq!(plan.stays.len(), 1);

    // A different reservation occupying that room blocks the move.
    facts.stays.push(record(
        "stay-other",
        "stay",
        json!({
            "reservationId": "reservation-2", "guestId": "guest-2",
            "typeId": "type-1", "rateId": "rate-1", "roomId": "",
            "arrival": "2026-11-05", "departure": "2026-11-07", "adults": 1,
            "state": "reserved", "guestAccountId": "account-2",
            "prices": [], "segments": []
        }),
    ));
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &change_command(), 6_000)
                .unwrap_err()
        ),
        "PMS_SOLD_OUT"
    );
}

#[test]
fn cancel_marks_stays_and_reservation_and_refuses_fees_or_checked_in_stays() {
    let key = CommandKey::new("reservation-cancel-0001").unwrap();
    let mut facts = base_facts();
    with_reservation(&mut facts);
    let plan = plan_reservation_command(&actor(), &key, &facts, &cancel_command(), 7_000).unwrap();
    assert_eq!(plan.result.id, "reservation-1");
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.reservation.data["state"], "cancelled");
    assert_eq!(plan.reservation.data["notes"], "\nGuest cancelled");
    assert_eq!(
        plan.reservation.data["history"][0]["action"],
        "Reservation cancelled"
    );
    assert_eq!(plan.stays[0].data["state"], "cancelled");
    assert_eq!(plan.stays[0].version, 2);

    let mut facts = base_facts();
    with_reservation(&mut facts);
    let mut command = cancel_command();
    if let PmsReservationCommandInput::Cancel(input) = &mut command.input {
        input.fee = 5000;
    }
    assert_eq!(
        code(&plan_reservation_command(&actor(), &key, &facts, &command, 7_000).unwrap_err()),
        "PMS_FEE_ACCOUNT_REQUIRED"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.stays[0].data["state"] = json!("in_house");
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &cancel_command(), 7_000)
                .unwrap_err()
        ),
        "PMS_ALREADY_CHECKED_IN"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.reservation.as_mut().unwrap().data["state"] = json!("payment_pending");
    assert_eq!(
        code(
            &plan_reservation_command(&actor(), &key, &facts, &cancel_command(), 7_000)
                .unwrap_err()
        ),
        "PMS_RESERVATION_CLOSED"
    );

    let mut facts = base_facts();
    with_reservation(&mut facts);
    facts.property.state = "closing".into();
    assert!(
        plan_reservation_command(&actor(), &key, &facts, &cancel_command(), 7_000).is_ok(),
        "retained reservation.cancel is not blocked by day close"
    );
}
