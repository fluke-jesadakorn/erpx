// Native PMS stay-room command domain/application facts. No database, browser,
// adapter, persisted receipt or HTTP acceptance is inferred.
#[path = "domain/mod.rs"]
mod domain;
#[path = "application/mod.rs"]
mod application;

use application::stay_room::{assert_stay_record, stay_room_request_hash};
use domain::room_board::{PmsBoardActor, PmsPropertyRow, PmsRecord};
use domain::stay_room::{
    PmsStayRoomActor, PmsStayRoomCommand, PmsStayRoomEntitlement, PmsStayRoomFacts,
    PmsStayRoomOperation, plan_stay_room,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.frontdesk".into()],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn stay_actor() -> PmsStayRoomActor {
    PmsStayRoomActor {
        board: actor(),
        assignment_id: "assignment".into(),
        role: "frontdesk".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn property_row(id: &str) -> PmsPropertyRow {
    PmsPropertyRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        name: format!("Hotel {id}"),
        business_date: "2026-11-04".into(),
        state: "open".into(),
        config_json: "{}".into(),
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

#[test]
fn stay_room_command_validation_rejects_invalid_input() {
    let mut command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "test".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    assert!(command.validate().is_ok());
    command.expected_version = 0;
    assert!(command.validate().is_err());
    command.expected_version = 1;
    command.property_id = "".into();
    assert!(command.validate().is_err());
}

#[test]
fn stay_room_reason_and_ids_use_chars_after_ecma_trim() {
    let base = || PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "test".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    let mut command = base();
    command.reason = format!("{}😀", "a".repeat(1998));
    assert!(command.validate().is_ok(), "reason is textField .trim().max(2000)");
    command.reason = format!("{}😀", "a".repeat(1999));
    assert!(command.validate().is_ok(), "1999 a + emoji is 2000 chars");
    command.reason = format!("{}😀", "a".repeat(2000));
    assert!(command.validate().is_err(), "2000 a + emoji is 2001 chars");
    let mut command = base();
    command.reason = format!("  \u{00A0}{}{} ", "a".repeat(2000), "\u{3000}");
    assert!(
        command.validate().is_ok(),
        "ECMAScript whitespace around 2000 chars still passes"
    );
    let mut command = base();
    command.reason = format!("{}{}", "\u{0085}", "a".repeat(2000));
    assert!(
        command.validate().is_err(),
        "U+0085 NEL is not in the ECMAScript trim set"
    );
    let mut command = base();
    command.reason = format!("{}{}", "\u{FEFF}", "a".repeat(2000));
    assert!(
        command.validate().is_ok(),
        "BOM (ZWNBSP) is in the ECMAScript trim set"
    );
    let mut command = base();
    command.property_id = "😀".repeat(201);
    assert!(
        command.validate().is_err(),
        "ids are untrimmed idField: 201 emoji are 201 chars"
    );
    command.property_id = "😀".repeat(200);
    assert!(command.validate().is_ok());
    let mut command = base();
    command.property_id = "中".repeat(150);
    assert!(
        command.validate().is_ok(),
        "150 CJK are 150 chars but 450 UTF-8 bytes"
    );
}

#[test]
fn stay_room_actor_requires_command_context() {
    let mut a = stay_actor();
    assert!(a.require_command(0).is_ok());
    a.assignment_id = "".into();
    assert!(a.require_command(0).is_err());
}

#[test]
fn stay_room_entitlement_requires_settle() {
    let allowed = PmsStayRoomEntitlement {
        enforced: true,
        settle_existing_allowed: true,
    };
    assert!(allowed.require_settle_existing().is_ok());
    let denied = PmsStayRoomEntitlement {
        enforced: true,
        settle_existing_allowed: false,
    };
    assert!(denied.require_settle_existing().is_err());
}

#[test]
fn stay_room_assert_stay_record_accepts_stay_object() {
    let r = record("stay", "stay", json!({"state":"reserved"}));
    assert!(assert_stay_record(&r).is_ok());
    let bad = record("stay", "room", json!({"state":"reserved"}));
    assert!(assert_stay_record(&bad).is_err());
}

#[test]
fn stay_room_request_hash_is_deterministic() {
    let actor = stay_actor();
    let command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "test".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    let a = stay_room_request_hash(&actor, &command).unwrap();
    let b = stay_room_request_hash(&actor, &command).unwrap();
    assert_eq!(a, b);
    assert!(!a.is_empty());
}

#[test]
fn plan_stay_room_assigns_reserved_stay_and_versions_property() {
    let actor = stay_actor();
    let property = property_row("property");
    let stay = record(
        "stay",
        "stay",
        json!({
            "state": "reserved",
            "typeId": "type-1",
            "roomId": "",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "reservationId": "reservation"
        }),
    );
    let reservation = record(
        "reservation",
        "reservation",
        json!({"history": []}),
    );
    let room = record(
        "room",
        "room",
        json!({
            "active": true,
            "typeId": "type-1",
            "cleanliness": "clean",
            "cleaningCycle": 0
        }),
    );
    let facts = PmsStayRoomFacts {
        property,
        stay,
        reservation: Some(reservation),
        room,
        old_room: None,
        stays: vec![],
        blocks: vec![],
        tasks: vec![],
    };
    let command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "assign".into(),
        expected_version: 1,
        expected_stay_version: Some(1),
    };
    let plan = plan_stay_room(&actor, &facts, &command, 5).unwrap();
    assert_eq!(plan.property_version, 2);
    assert_eq!(plan.result.version, 2);
    assert_eq!(plan.stay.data["roomId"], "room");
    assert_eq!(plan.stay.data["segments"][0]["roomId"], "room");
    assert_eq!(plan.stay.version, 2);
    let reservation = plan.reservation.unwrap();
    assert_eq!(reservation.data["history"].as_array().unwrap().len(), 1);
    assert!(plan.old_room.is_none());
    assert!(plan.departure_task.is_none());
}

#[test]
fn plan_stay_room_rejects_stale_property_version() {
    let actor = stay_actor();
    let mut property = property_row("property");
    property.version = 2;
    let stay = record(
        "stay",
        "stay",
        json!({
            "state": "reserved",
            "typeId": "type-1",
            "roomId": "",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "reservationId": "reservation"
        }),
    );
    let room = record(
        "room",
        "room",
        json!({
            "active": true,
            "typeId": "type-1",
            "cleanliness": "clean",
            "cleaningCycle": 0
        }),
    );
    let facts = PmsStayRoomFacts {
        property,
        stay,
        reservation: None,
        room,
        old_room: None,
        stays: vec![],
        blocks: vec![],
        tasks: vec![],
    };
    let command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "assign".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    let error = plan_stay_room(&actor, &facts, &command, 5).unwrap_err();
    assert_eq!(error.code, "PMS_VERSION_CONFLICT");
}

#[test]
fn plan_stay_room_moves_in_house_stay_and_marks_old_room_dirty() {
    let actor = stay_actor();
    let property = property_row("property");
    let stay = record(
        "stay",
        "stay",
        json!({
            "state": "in_house",
            "typeId": "type-1",
            "roomId": "old-room",
            "arrival": "2026-11-03",
            "departure": "2026-11-06",
            "reservationId": "reservation",
            "segments": [
                {"roomId": "old-room", "from": "2026-11-03", "to": "2026-11-06"}
            ]
        }),
    );
    let old_room = record(
        "old-room",
        "room",
        json!({
            "active": true,
            "typeId": "type-1",
            "cleanliness": "clean",
            "cleaningCycle": 3
        }),
    );
    let new_room = record(
        "new-room",
        "room",
        json!({
            "active": true,
            "typeId": "type-1",
            "cleanliness": "inspected",
            "cleaningCycle": 0
        }),
    );
    let facts = PmsStayRoomFacts {
        property,
        stay,
        reservation: None,
        room: new_room,
        old_room: Some(old_room),
        stays: vec![],
        blocks: vec![],
        tasks: vec![],
    };
    let command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Move,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "new-room".into(),
        reason: "move".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    let plan = plan_stay_room(&actor, &facts, &command, 5).unwrap();
    assert_eq!(plan.stay.data["roomId"], "new-room");
    let old = plan.old_room.unwrap();
    assert_eq!(old.data["cleanliness"], "dirty");
    assert_eq!(old.data["cleaningCycle"], 4);
    assert!(plan.departure_task.is_some());
    let task = plan.departure_task.unwrap();
    assert_eq!(task.data["roomId"], "old-room");
    assert_eq!(task.data["type"], "departure");
}

#[test]
fn plan_stay_room_rejects_room_type_mismatch() {
    let actor = stay_actor();
    let property = property_row("property");
    let stay = record(
        "stay",
        "stay",
        json!({
            "state": "reserved",
            "typeId": "type-1",
            "roomId": "",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "reservationId": "reservation"
        }),
    );
    let room = record(
        "room",
        "room",
        json!({
            "active": true,
            "typeId": "type-2",
            "cleanliness": "clean",
            "cleaningCycle": 0
        }),
    );
    let facts = PmsStayRoomFacts {
        property,
        stay,
        reservation: None,
        room,
        old_room: None,
        stays: vec![],
        blocks: vec![],
        tasks: vec![],
    };
    let command = PmsStayRoomCommand {
        operation: PmsStayRoomOperation::Assign,
        property_id: "property".into(),
        stay_id: "stay".into(),
        room_id: "room".into(),
        reason: "assign".into(),
        expected_version: 1,
        expected_stay_version: None,
    };
    let error = plan_stay_room(&actor, &facts, &command, 5).unwrap_err();
    assert_eq!(error.code, "PMS_ROOM_TYPE");
}
