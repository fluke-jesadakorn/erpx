// Native PMS room/housekeeping command domain/application facts. No database,
// browser, adapter, persisted receipt or HTTP acceptance is inferred.
mod domain {
    #[path = "../../../../packages/server-pms/src/domain/room_board.rs"]
    pub mod room_board;
    #[path = "../../../../packages/server-pms/src/domain/room_command.rs"]
    pub mod room_command;
}
mod application {
    #[path = "../../../../packages/server-pms/src/application/room_board.rs"]
    pub mod room_board;
    #[path = "../../../../packages/server-pms/src/application/room_command.rs"]
    pub mod room_command;
}

use application::room_command::room_command_request_hash;
use domain::room_board::{PmsBoardActor, PmsPropertyRow, PmsRecord};
use domain::room_command::{
    PmsBlockCreateInput, PmsMaintenanceOpenInput, PmsRoomCommand, PmsRoomCommandActor,
    PmsRoomCommandFacts, PmsRoomCommandInput, PmsRoomCommandOperation, PmsRoomSaveInput,
    PmsRoomTypeSaveInput, PmsTaskCreateInput, PmsTaskKind, PmsTaskPriority, plan_room_command,
};
use erpx_server_shared::public::CommandKey;
use serde_json::json;

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec![
            "pms.read".into(),
            "pms.configure".into(),
            "pms.housekeeping".into(),
            "pms.inspect".into(),
        ],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn room_actor() -> PmsRoomCommandActor {
    PmsRoomCommandActor {
        board: actor(),
        assignment_id: "assignment".into(),
        role: "housekeeping".into(),
        revision: 1,
        assignment_version: 1,
    }
}

fn property_row() -> PmsPropertyRow {
    PmsPropertyRow {
        id: "property".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        name: "Hotel".into(),
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

fn room(id: &str, number: &str, type_id: &str, cleanliness: &str, cycle: i64) -> PmsRecord {
    record(
        id,
        "room",
        json!({
            "number": number,
            "typeId": type_id,
            "building": "Main",
            "floor": "1",
            "active": true,
            "cleanliness": cleanliness,
            "cleaningCycle": cycle,
            "dnd": false,
            "observedOccupied": null
        }),
    )
}

fn room_type(id: &str) -> PmsRecord {
    record(
        id,
        "room_type",
        json!({"name": format!("Type {id}"), "capacity": 2, "amenities": ""}),
    )
}

fn base_facts() -> PmsRoomCommandFacts {
    PmsRoomCommandFacts {
        property: property_row(),
        room: None,
        room_type: Some(room_type("type-1")),
        block: None,
        rooms: vec![room("room-a", "101", "type-1", "clean", 0)],
        room_types: vec![room_type("type-1")],
        stays: Vec::new(),
        blocks: Vec::new(),
        task: None,
        room_tasks: Vec::new(),
        in_house: false,
    }
}

fn save(room_id: Option<&str>, number: &str, active: bool) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Save,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::Save(PmsRoomSaveInput {
            room_id: room_id.map(str::to_owned),
            number: number.into(),
            type_id: "type-1".into(),
            building: "Main".into(),
            floor: "2".into(),
            active,
        }),
    }
}

fn dnd(room_id: &str, value: bool) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Dnd,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::Dnd {
            room_id: room_id.into(),
            dnd: value,
        },
    }
}

fn observe(room_id: &str, occupied: bool) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::Observe,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::Observe {
            room_id: room_id.into(),
            occupied,
        },
    }
}

fn task_command(
    operation: PmsRoomCommandOperation,
    task_id: &str,
    checked: Vec<&str>,
) -> PmsRoomCommand {
    let input = match operation {
        PmsRoomCommandOperation::TaskAssign => PmsRoomCommandInput::TaskAssign {
            task_id: task_id.into(),
            assignee_id: "assignee".into(),
            priority: PmsTaskPriority::Urgent,
        },
        PmsRoomCommandOperation::TaskStart => PmsRoomCommandInput::TaskStart {
            task_id: task_id.into(),
            checked: checked.iter().map(|value| (*value).to_owned()).collect(),
            notes: " started ".into(),
        },
        PmsRoomCommandOperation::TaskComplete => PmsRoomCommandInput::TaskComplete {
            task_id: task_id.into(),
            checked: checked.iter().map(|value| (*value).to_owned()).collect(),
            notes: " done ".into(),
        },
        PmsRoomCommandOperation::TaskInspect => PmsRoomCommandInput::TaskInspect {
            task_id: task_id.into(),
        },
        PmsRoomCommandOperation::RoomTypeSave
        | PmsRoomCommandOperation::Save
        | PmsRoomCommandOperation::Dnd
        | PmsRoomCommandOperation::Observe
        | PmsRoomCommandOperation::TaskCreate
        | PmsRoomCommandOperation::MaintenanceOpen
        | PmsRoomCommandOperation::MaintenanceClose
        | PmsRoomCommandOperation::BlockCreate
        | PmsRoomCommandOperation::BlockRelease => unreachable!(),
    };
    PmsRoomCommand {
        operation,
        property_id: "property".into(),
        expected_version: 1,
        input,
    }
}

fn room_type_save(
    type_id: Option<&str>,
    name: &str,
    capacity: i64,
    amenities: &str,
) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::RoomTypeSave,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::RoomTypeSave(PmsRoomTypeSaveInput {
            type_id: type_id.map(str::to_owned),
            name: name.into(),
            capacity,
            amenities: amenities.into(),
        }),
    }
}

fn task_create(room_id: &str, kind: PmsTaskKind, date: &str) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::TaskCreate,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::TaskCreate(PmsTaskCreateInput {
            room_id: room_id.into(),
            kind,
            date: date.into(),
        }),
    }
}

fn maintenance_open(room_id: &str, from: &str, to: &str, reason: &str) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::MaintenanceOpen,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::MaintenanceOpen(PmsMaintenanceOpenInput {
            room_id: room_id.into(),
            from: from.into(),
            to: to.into(),
            reason: reason.into(),
        }),
    }
}

fn block_ref(operation: PmsRoomCommandOperation, block_id: &str) -> PmsRoomCommand {
    let input = match operation {
        PmsRoomCommandOperation::MaintenanceClose => PmsRoomCommandInput::MaintenanceClose {
            block_id: block_id.into(),
        },
        PmsRoomCommandOperation::BlockRelease => PmsRoomCommandInput::BlockRelease {
            block_id: block_id.into(),
        },
        _ => unreachable!(),
    };
    PmsRoomCommand {
        operation,
        property_id: "property".into(),
        expected_version: 1,
        input,
    }
}

fn block_create(
    name: &str,
    type_id: &str,
    from: &str,
    to: &str,
    release_date: &str,
    quantity: i64,
) -> PmsRoomCommand {
    PmsRoomCommand {
        operation: PmsRoomCommandOperation::BlockCreate,
        property_id: "property".into(),
        expected_version: 1,
        input: PmsRoomCommandInput::BlockCreate(PmsBlockCreateInput {
            name: name.into(),
            type_id: type_id.into(),
            from: from.into(),
            to: to.into(),
            release_date: release_date.into(),
            quantity,
        }),
    }
}

fn task(id: &str, room_id: &str, kind: &str, state: &str, cycle: i64) -> PmsRecord {
    record(
        id,
        "task",
        json!({
            "roomCycle": cycle,
            "roomId": room_id,
            "type": kind,
            "stayId": "stay",
            "date": "2026-11-04",
            "assigneeId": "",
            "priority": "normal",
            "state": state,
            "checklist": ["Bathroom", "Bed and linen"],
            "checked": [],
            "notes": "",
            "completedBy": "",
            "inspectedBy": ""
        }),
    )
}

fn key() -> CommandKey {
    CommandKey::new("room-command-0001").unwrap()
}

#[test]
fn room_command_validation_uses_retained_limits() {
    let mut command = save(None, " 108 ", true);
    command.normalize();
    assert!(command.validate().is_ok());
    assert_eq!(
        match &command.input {
            PmsRoomCommandInput::Save(input) => input.number.clone(),
            _ => unreachable!(),
        },
        "108"
    );

    let mut command = save(None, "", true);
    command.normalize();
    assert!(command.validate().is_err());

    let mut command = save(None, "108", true);
    command.expected_version = 0;
    assert!(command.validate().is_err());

    let mut command = save(None, "108", true);
    command.property_id = "😀".repeat(128);
    assert!(
        command.validate().is_ok(),
        "128 scalars is the retained id limit"
    );
    command.property_id = "😀".repeat(129);
    assert!(command.validate().is_err());

    let command = save(Some(&"a".repeat(129)), "108", true);
    assert!(command.validate().is_err());

    let mut command = task_command(
        PmsRoomCommandOperation::TaskComplete,
        "task-1",
        vec!["Bathroom"],
    );

    command.normalize();
    assert!(command.validate().is_ok());
    let PmsRoomCommandInput::TaskComplete { notes, .. } = &command.input else {
        unreachable!()
    };
    assert_eq!(notes, "done");

    let mut command = task_command(
        PmsRoomCommandOperation::TaskStart,
        "task-1",
        vec![&"a".repeat(2001)],
    );
    assert!(command.validate().is_err());

    let mut command = dnd("room-a", true);
    command.operation = PmsRoomCommandOperation::Save;
    assert!(
        command.validate().is_err(),
        "the operation and the input shape must agree"
    );
}

#[test]
fn room_command_actor_selects_the_retained_permission() {
    let mut frontdesk = room_actor();
    frontdesk.board.permissions = vec!["pms.read".into(), "pms.frontdesk".into()];
    let error = frontdesk
        .require_command(PmsRoomCommandOperation::Save, 0)
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    let error = frontdesk
        .require_command(PmsRoomCommandOperation::Observe, 0)
        .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");

    let mut housekeeping = room_actor();
    housekeeping.board.permissions = vec!["pms.read".into(), "pms.housekeeping".into()];
    assert!(
        housekeeping
            .require_command(PmsRoomCommandOperation::Dnd, 0)
            .is_ok()
    );
    assert!(
        housekeeping
            .require_command(PmsRoomCommandOperation::Save, 0)
            .is_err(),
        "room.save is the retained pms.configure operation"
    );

    let error = room_actor()
        .require_command(PmsRoomCommandOperation::Dnd, 10_000)
        .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    let mut expired = room_actor();
    expired.assignment_id = String::new();
    assert!(
        expired
            .require_command(PmsRoomCommandOperation::Dnd, 0)
            .is_err()
    );
}

#[test]
fn plan_room_save_creates_a_dirty_room_and_versions_the_property() {
    let facts = base_facts();
    let command = save(None, "108", true);
    let plan = plan_room_command(&room_actor(), &key(), &facts, &command, 5).unwrap();
    assert_eq!(plan.property_version, 2);
    assert_eq!(plan.result.version, 2);
    let room = plan.new_room.unwrap();
    assert_eq!(room.version, 1);
    assert_eq!(plan.result.id, room.id);
    assert_eq!(room.data["number"], "108");
    assert_eq!(room.data["typeId"], "type-1");
    assert_eq!(room.data["cleanliness"], "dirty");
    assert_eq!(room.data["dnd"], false);
    assert!(room.data["observedOccupied"].is_null());
    assert!(plan.room.is_none());

    let replayed = plan_room_command(&room_actor(), &key(), &facts, &command, 9).unwrap();
    assert_eq!(
        replayed.result.id, plan.result.id,
        "the created identity is derived from the retained command key"
    );
}

#[test]
fn plan_room_save_rejects_a_duplicate_number() {
    let facts = base_facts();
    let error =
        plan_room_command(&room_actor(), &key(), &facts, &save(None, "101", true), 5).unwrap_err();
    assert_eq!(error.code, "PMS_ROOM_NUMBER_EXISTS");
    assert_eq!(error.message, "Duplicate room number");
}

#[test]
fn plan_room_save_guards_the_room_assigned_to_a_stay() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    facts.stays = vec![record(
        "stay-1",
        "stay",
        json!({
            "state": "reserved",
            "typeId": "type-1",
            "roomId": "room-a",
            "arrival": "2026-11-04",
            "departure": "2026-11-06",
            "segments": [{"roomId": "room-a", "from": "2026-11-04", "to": "2026-11-06"}]
        }),
    )];
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &save(Some("room-a"), "101", false),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_ROOM_IN_USE");

    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &save(Some("room-a"), "101", true),
        5,
    )
    .unwrap();
    let room = plan.room.unwrap();
    assert_eq!(room.version, 2);
    assert_eq!(room.data["floor"], "2");
    assert_eq!(room.data["cleanliness"], "clean");
    assert_eq!(room.data["cleaningCycle"], 0);
    assert!(plan.new_room.is_none());
}

#[test]
fn plan_room_save_rejects_a_stale_property_version() {
    let facts = base_facts();
    let mut command = save(None, "108", true);
    command.expected_version = 2;
    let error = plan_room_command(&room_actor(), &key(), &facts, &command, 5).unwrap_err();
    assert_eq!(error.code, "PMS_VERSION_CONFLICT");
}

#[test]
fn plan_room_command_matches_retained_property_state_gates() {
    let mut facts = base_facts();
    facts.property.state = "closing".into();
    let error =
        plan_room_command(&room_actor(), &key(), &facts, &save(None, "108", true), 5).unwrap_err();
    assert_eq!(error.code, "PMS_DAY_CLOSING");
    assert_eq!(
        error.message,
        "Day close is in progress. Please wait for it to finish"
    );

    let mut facts = base_facts();
    facts.property.state = "setup".into();
    let error =
        plan_room_command(&room_actor(), &key(), &facts, &dnd("room-a", true), 5).unwrap_err();
    assert_eq!(error.code, "PMS_SETUP_REQUIRED");
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    let plan =
        plan_room_command(&room_actor(), &key(), &facts, &save(None, "108", true), 5).unwrap();
    assert_eq!(
        plan.property_version, 2,
        "room.save stays available in setup"
    );
}

#[test]
fn plan_room_save_reconciles_capacity() {
    let mut facts = base_facts();
    facts.stays = vec![
        record(
            "stay-1",
            "stay",
            json!({
                "state": "reserved",
                "typeId": "type-1",
                "roomId": "",
                "arrival": "2026-11-04",
                "departure": "2026-11-05",
                "segments": []
            }),
        ),
        record(
            "stay-2",
            "stay",
            json!({
                "state": "reserved",
                "typeId": "type-1",
                "roomId": "",
                "arrival": "2026-11-04",
                "departure": "2026-11-05",
                "segments": []
            }),
        ),
        record(
            "stay-3",
            "stay",
            json!({
                "state": "reserved",
                "typeId": "type-1",
                "roomId": "",
                "arrival": "2026-11-04",
                "departure": "2026-11-05",
                "segments": []
            }),
        ),
    ];
    let error =
        plan_room_command(&room_actor(), &key(), &facts, &save(None, "108", true), 5).unwrap_err();
    assert_eq!(error.code, "PMS_SOLD_OUT");
    assert_eq!(error.message, "Not enough rooms on this date");
}

#[test]
fn plan_room_save_capacity_sees_the_planned_room() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    facts.stays = vec![
        record(
            "stay-1",
            "stay",
            json!({
                "state": "reserved",
                "typeId": "type-1",
                "roomId": "",
                "arrival": "2026-11-04",
                "departure": "2026-11-05",
                "segments": []
            }),
        ),
        record(
            "stay-2",
            "stay",
            json!({
                "state": "reserved",
                "typeId": "type-1",
                "roomId": "",
                "arrival": "2026-11-04",
                "departure": "2026-11-05",
                "segments": []
            }),
        ),
    ];
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &save(Some("room-a"), "101", false),
        5,
    )
    .unwrap_err();
    assert_eq!(
        error.code, "PMS_SOLD_OUT",
        "deactivating the last room must be reconciled against the planned row"
    );
}

#[test]
fn plan_room_dnd_sets_the_flag() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    let plan = plan_room_command(&room_actor(), &key(), &facts, &dnd("room-a", true), 5).unwrap();
    assert_eq!(plan.result.id, "room-a");
    let room = plan.room.unwrap();
    assert_eq!(room.version, 2);
    assert_eq!(room.data["dnd"], true);
}

#[test]
fn plan_room_observe_creates_one_discrepancy_task() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 3));
    facts.in_house = true;
    let plan =
        plan_room_command(&room_actor(), &key(), &facts, &observe("room-a", false), 5).unwrap();
    assert_eq!(plan.result.id, "room-a");
    assert_eq!(plan.room.as_ref().unwrap().data["observedOccupied"], false);
    let task = plan.new_task.clone().unwrap();
    assert_eq!(task.data["type"], "discrepancy");
    assert_eq!(task.data["roomId"], "room-a");
    assert_eq!(task.data["roomCycle"], 3);
    assert_eq!(task.data["stayId"], "");
    assert_eq!(task.data["date"], "2026-11-04");
    assert_eq!(task.data["priority"], "normal");
    assert_eq!(task.data["state"], "pending");

    facts.room_tasks = vec![task];
    let repeated =
        plan_room_command(&room_actor(), &key(), &facts, &observe("room-a", false), 9).unwrap();
    assert!(
        repeated.new_task.is_none(),
        "the retained task() dedupe reuses the same room/type/date/cycle task"
    );

    facts.in_house = false;
    let matched =
        plan_room_command(&room_actor(), &key(), &facts, &observe("room-a", false), 9).unwrap();
    assert!(matched.new_task.is_none(), "observed occupancy matches");
}

#[test]
fn plan_room_task_assign_and_start_follow_retained_guards() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "dirty", 0));
    facts.task = Some(task("task-1", "room-a", "stayover", "pending", 0));
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskAssign, "task-1", vec![]),
        5,
    )
    .unwrap();
    let assigned = plan.task.unwrap();
    assert_eq!(assigned.data["assigneeId"], "assignee");
    assert_eq!(assigned.data["priority"], "urgent");
    assert_eq!(assigned.version, 2);

    let mut start = task_command(PmsRoomCommandOperation::TaskStart, "task-1", vec![]);
    start.normalize();
    let plan = plan_room_command(&room_actor(), &key(), &facts, &start, 5).unwrap();
    let started = plan.task.unwrap();
    assert_eq!(started.data["state"], "working");
    assert_eq!(started.data["assigneeId"], "user");
    assert_eq!(started.data["notes"], "started");
    assert!(
        plan.room.is_none(),
        "start does not dirty or clean the room"
    );

    facts.task = Some(task("task-1", "room-a", "stayover", "done", 0));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskAssign, "task-1", vec![]),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_TASK_DONE");
}

#[test]
fn plan_room_task_complete_and_inspect_follow_retained_guards() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "dirty", 0));
    facts.task = Some(task("task-1", "room-a", "stayover", "working", 0));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(
            PmsRoomCommandOperation::TaskComplete,
            "task-1",
            vec!["Bathroom"],
        ),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_CHECKLIST_REQUIRED");

    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(
            PmsRoomCommandOperation::TaskComplete,
            "task-1",
            vec!["Bathroom", "Bed and linen"],
        ),
        5,
    )
    .unwrap();
    let task = plan.task.unwrap();
    assert_eq!(task.data["state"], "clean");
    assert_eq!(task.data["completedBy"], "user");
    assert_eq!(plan.room.as_ref().unwrap().data["cleanliness"], "clean");

    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    facts.task = Some(task);
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskInspect, "task-1", vec![]),
        5,
    )
    .unwrap_err();
    assert_eq!(
        error.code, "PMS_INDEPENDENT_INSPECTOR",
        "the same person cannot complete and inspect"
    );

    let mut other = facts.clone();
    let mut completed = other.task.clone().unwrap();
    completed.data["completedBy"] = json!("supervisor");
    other.task = Some(completed);
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &other,
        &task_command(PmsRoomCommandOperation::TaskInspect, "task-1", vec![]),
        5,
    )
    .unwrap();
    assert_eq!(plan.task.as_ref().unwrap().data["state"], "done");
    assert_eq!(plan.task.as_ref().unwrap().data["inspectedBy"], "user");
    assert_eq!(plan.room.as_ref().unwrap().data["cleanliness"], "inspected");
}

#[test]
fn plan_room_task_progress_respects_cycle_dnd_and_assignment() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "dirty", 2));
    facts.task = Some(task("task-1", "room-a", "stayover", "pending", 1));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskStart, "task-1", vec![]),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_CLEANING_STALE");

    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "dirty", 0));
    facts.room.as_mut().unwrap().data["dnd"] = json!(true);
    facts.task = Some(task("task-1", "room-a", "stayover", "pending", 0));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskStart, "task-1", vec![]),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_DND");

    let mut facts = facts.clone();
    facts.room.as_mut().unwrap().data["dnd"] = json!(false);
    let mut assigned = facts.task.clone().unwrap();
    assigned.data["assigneeId"] = json!("another-employee");
    facts.task = Some(assigned);
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_command(PmsRoomCommandOperation::TaskStart, "task-1", vec![]),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_TASK_ASSIGNMENT");
}

#[test]
fn room_command_request_hash_is_deterministic_and_operation_scoped() {
    let actor = room_actor();
    let command = save(None, "108", true);
    let first = room_command_request_hash(&actor, &command).unwrap();
    assert_eq!(first, room_command_request_hash(&actor, &command).unwrap());
    let other = dnd("room-a", true);
    assert_ne!(first, room_command_request_hash(&actor, &other).unwrap());
    let mut renamed = save(None, "109", true);
    renamed.normalize();
    assert_ne!(first, room_command_request_hash(&actor, &renamed).unwrap());
    let maintenance = maintenance_open("room-a", "2026-11-10", "2026-11-12", "Leak");
    assert_ne!(
        first,
        room_command_request_hash(&actor, &maintenance).unwrap()
    );
    let mut relabelled = maintenance_open("room-a", "2026-11-10", "2026-11-12", "Leak fixed");
    relabelled.normalize();
    assert_ne!(
        room_command_request_hash(&actor, &maintenance).unwrap(),
        room_command_request_hash(&actor, &relabelled).unwrap()
    );
}

fn stay(
    id: &str,
    type_id: &str,
    state: &str,
    arrival: &str,
    departure: &str,
    adults: i64,
    child_ages: Vec<i64>,
    segments: serde_json::Value,
) -> PmsRecord {
    record(
        id,
        "stay",
        json!({
            "state": state,
            "typeId": type_id,
            "roomId": "",
            "arrival": arrival,
            "departure": departure,
            "adults": adults,
            "childAges": child_ages,
            "segments": segments
        }),
    )
}

fn block(id: &str, kind: &str, state: &str, room_id: &str, extra: serde_json::Value) -> PmsRecord {
    let mut data = json!({
        "kind": kind,
        "state": state,
        "roomId": room_id,
        "typeId": "type-1",
        "from": "2026-11-10",
        "to": "2026-11-12",
        "name": "Block",
        "quantity": 1,
        "releaseDate": "2026-11-10",
        "pickedStayIds": []
    });
    if let (Some(target), Some(source)) = (data.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    record(id, "block", data)
}

#[test]
fn room_command_validation_covers_the_extended_operations() {
    let mut command = room_type_save(None, "  Deluxe  ", 2, " Wifi ");
    command.normalize();
    assert!(command.validate().is_ok());
    let PmsRoomCommandInput::RoomTypeSave(input) = &command.input else {
        unreachable!()
    };
    assert_eq!(input.name, "Deluxe");
    assert_eq!(input.amenities, "Wifi");

    for mut command in [
        room_type_save(None, "  ", 2, ""),
        room_type_save(None, "Deluxe", 0, ""),
        room_type_save(None, "Deluxe", 51, ""),
        room_type_save(None, "Deluxe", 2, &"a".repeat(2001)),
        room_type_save(Some(&"a".repeat(129)), "Deluxe", 2, ""),
    ] {
        command.normalize();
        assert!(command.validate().is_err(), "{command:?}");
    }

    let command = task_create("room-a", PmsTaskKind::Arrival, "2026-11-04");
    assert!(command.validate().is_ok());
    assert!(
        task_create("room-a", PmsTaskKind::Stayover, "2026-11-31")
            .validate()
            .is_err()
    );

    let mut command = maintenance_open("room-a", "2026-11-10", "2026-11-12", " Fix leak ");
    command.normalize();
    assert!(command.validate().is_ok());
    let PmsRoomCommandInput::MaintenanceOpen(input) = &command.input else {
        unreachable!()
    };
    assert_eq!(input.reason, "Fix leak");
    assert!(
        maintenance_open("room-a", "2026-11-12", "2026-11-10", "Leak")
            .validate()
            .is_ok(),
        "dateField rejects non-calendar dates; period ordering stays a command guard"
    );
    let mut blank_reason = maintenance_open("room-a", "2026-11-10", "2026-11-12", " ");
    blank_reason.normalize();
    assert!(blank_reason.validate().is_err());

    let mut command = block_create(
        " Group ",
        "type-1",
        "2026-11-10",
        "2026-11-12",
        "2026-11-05",
        4,
    );
    command.normalize();
    assert!(command.validate().is_ok());
    let PmsRoomCommandInput::BlockCreate(input) = &command.input else {
        unreachable!()
    };
    assert_eq!(input.name, "Group");
    for mut command in [
        block_create("", "type-1", "2026-11-10", "2026-11-12", "2026-11-05", 1),
        block_create(
            "Group",
            "type-1",
            "2026-11-10",
            "2026-11-12",
            "2026-11-05",
            0,
        ),
        block_create(
            "Group",
            "type-1",
            "2026-11-10",
            "2026-11-12",
            "2026-11-05",
            1_001,
        ),
        block_create(
            "Group",
            "type-1",
            "2026-02-30",
            "2026-11-12",
            "2026-11-05",
            1,
        ),
    ] {
        command.normalize();
        assert!(command.validate().is_err(), "{command:?}");
    }

    let command = block_ref(PmsRoomCommandOperation::BlockRelease, "block-1");
    assert!(command.validate().is_ok());
    let mut command = block_ref(PmsRoomCommandOperation::MaintenanceClose, "block-1");
    command.operation = PmsRoomCommandOperation::BlockRelease;
    assert!(
        command.validate().is_err(),
        "the operation and the input shape must agree"
    );
}

#[test]
fn room_command_permissions_cover_the_extended_operations() {
    let mut configure = room_actor();
    configure.board.permissions = vec!["pms.read".into(), "pms.configure".into()];
    assert!(
        configure
            .require_command(PmsRoomCommandOperation::RoomTypeSave, 0)
            .is_ok()
    );
    assert!(
        configure
            .require_command(PmsRoomCommandOperation::TaskCreate, 0)
            .is_err()
    );

    let mut housekeeping = room_actor();
    housekeeping.board.permissions = vec!["pms.read".into(), "pms.housekeeping".into()];
    assert!(
        housekeeping
            .require_command(PmsRoomCommandOperation::TaskCreate, 0)
            .is_ok()
    );
    assert!(
        housekeeping
            .require_command(PmsRoomCommandOperation::MaintenanceOpen, 0)
            .is_err()
    );

    let mut maintenance = room_actor();
    maintenance.board.permissions = vec!["pms.read".into(), "pms.maintenance".into()];
    assert!(
        maintenance
            .require_command(PmsRoomCommandOperation::MaintenanceOpen, 0)
            .is_ok()
    );
    assert!(
        maintenance
            .require_command(PmsRoomCommandOperation::MaintenanceClose, 0)
            .is_ok()
    );
    assert!(
        maintenance
            .require_command(PmsRoomCommandOperation::BlockCreate, 0)
            .is_err()
    );

    let mut reservations = room_actor();
    reservations.board.permissions = vec!["pms.read".into(), "pms.reservations".into()];
    assert!(
        reservations
            .require_command(PmsRoomCommandOperation::BlockCreate, 0)
            .is_ok()
    );
    assert!(
        reservations
            .require_command(PmsRoomCommandOperation::BlockRelease, 0)
            .is_ok()
    );
}

#[test]
fn plan_room_type_save_creates_updates_and_guards_booked_guests() {
    let facts = base_facts();
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &room_type_save(None, "Garden", 2, "Fan"),
        5,
    )
    .unwrap();
    assert_eq!(plan.property_version, 2);
    let created = plan.new_room_type.clone().unwrap();
    assert_eq!(created.version, 1);
    assert_eq!(plan.result.id, created.id);
    assert!(created.id.starts_with("pms-room-type-"));
    assert_eq!(created.data["name"], "Garden");
    assert_eq!(created.data["capacity"], 2);
    assert_eq!(created.data["amenities"], "Fan");
    assert!(
        created.data.get("id").is_none(),
        "retained create stores no id"
    );
    assert!(plan.room_type.is_none());

    let mut facts = base_facts();
    facts.room_type = Some(room_type("type-1"));
    facts.stays = vec![stay(
        "stay-1",
        "type-1",
        "reserved",
        "2026-11-04",
        "2026-11-06",
        2,
        vec![7],
        json!([]),
    )];
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &room_type_save(Some("type-1"), "Deluxe", 2, ""),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_CAPACITY_IN_USE");
    assert_eq!(
        error.message,
        "Capacity is lower than the number of booked guests"
    );

    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &room_type_save(Some("type-1"), "Deluxe", 3, "Wifi"),
        5,
    )
    .unwrap();
    let updated = plan.room_type.clone().unwrap();
    assert_eq!(updated.version, 2);
    assert_eq!(updated.data["id"], "type-1");
    assert_eq!(updated.data["name"], "Deluxe");
    assert_eq!(updated.data["capacity"], 3);
    assert_eq!(updated.data["amenities"], "Wifi");
    assert!(plan.new_room_type.is_none());

    let mut setup = base_facts();
    setup.property.state = "setup".into();
    assert!(
        plan_room_command(
            &room_actor(),
            &key(),
            &setup,
            &room_type_save(None, "Garden", 2, ""),
            5,
        )
        .is_ok(),
        "room_type.save stays available in setup"
    );
}

#[test]
fn plan_task_create_creates_dedupes_and_prioritises() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "dirty", 3));
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_create("room-a", PmsTaskKind::Departure, "2026-11-04"),
        5,
    )
    .unwrap();
    let task = plan.new_task.clone().unwrap();
    assert_eq!(plan.result.id, task.id);
    assert!(task.id.starts_with("pms-task-"));
    assert_eq!(task.data["type"], "departure");
    assert_eq!(task.data["priority"], "urgent");
    assert_eq!(task.data["state"], "pending");
    assert_eq!(task.data["date"], "2026-11-04");
    assert_eq!(task.data["stayId"], "");
    assert_eq!(task.data["roomCycle"], 3);
    assert_eq!(
        task.data["checklist"],
        json!([
            "Bathroom",
            "Bed and linen",
            "Floor and dust",
            "Minibar",
            "Equipment and safety"
        ])
    );

    facts.room_tasks = vec![task];
    let task_id = facts.room_tasks[0].id.clone();
    let repeated = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_create("room-a", PmsTaskKind::Departure, "2026-11-04"),
        9,
    )
    .unwrap();
    assert_eq!(
        repeated.result.id, task_id,
        "the retained task() dedupe returns the existing task"
    );
    assert!(
        repeated.new_task.is_none(),
        "an existing room/type/date/cycle task is reused, not duplicated"
    );

    let other_kind = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_create("room-a", PmsTaskKind::Stayover, "2026-11-04"),
        9,
    )
    .unwrap();
    assert!(
        other_kind.new_task.is_some(),
        "a different type is a new task"
    );
    assert_eq!(
        other_kind.new_task.as_ref().unwrap().data["priority"],
        "normal"
    );

    facts.room_tasks.clear();
    let missing = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &task_create("room-b", PmsTaskKind::Stayover, "2026-11-04"),
        9,
    )
    .unwrap_err();
    assert_eq!(missing.code, "PMS_RECORD_NOT_FOUND");
}

#[test]
fn plan_maintenance_open_guards_the_period_and_current_assignment() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &maintenance_open("room-a", "2026-11-03", "2026-11-10", "Leak"),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_INVALID_DATES");
    assert_eq!(error.message, "Invalid maintenance period");

    assert_eq!(
        plan_room_command(
            &room_actor(),
            &key(),
            &facts,
            &maintenance_open("room-a", "2026-11-10", "2026-11-10", "Leak"),
            5,
        )
        .unwrap_err()
        .code,
        "PMS_INVALID_DATES"
    );

    facts.stays = vec![stay(
        "stay-1",
        "type-1",
        "in_house",
        "2026-11-03",
        "2026-11-06",
        1,
        vec![],
        json!([{"roomId": "room-a", "from": "2026-11-03", "to": "2026-11-06"}]),
    )];
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &maintenance_open("room-a", "2026-11-05", "2026-11-08", "Leak"),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_ROOM_IN_USE");
    assert_eq!(
        error.message,
        "Move the booking/guest out of the room before maintenance"
    );

    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &maintenance_open("room-a", "2026-11-10", "2026-11-12", "Leak"),
        5,
    )
    .unwrap();
    let block = plan.new_block.clone().unwrap();
    assert_eq!(plan.result.id, block.id);
    assert!(block.id.starts_with("pms-block-"));
    assert_eq!(block.data["kind"], "maintenance");
    assert_eq!(block.data["state"], "active");
    assert_eq!(block.data["roomId"], "room-a");
    assert_eq!(block.data["typeId"], "type-1");
    assert_eq!(block.data["name"], "Leak");
    assert_eq!(block.data["quantity"], 1);
    assert_eq!(block.data["releaseDate"], "2026-11-12");
    assert_eq!(block.data["pickedStayIds"], json!([]));
    let task = plan.new_task.clone().unwrap();
    assert_eq!(task.data["type"], "maintenance");
    assert_eq!(task.data["stayId"], block.id);
    assert_eq!(task.data["date"], "2026-11-04");
    assert_eq!(
        task.data["checklist"],
        json!(["Repair complete", "Safety test"])
    );
    assert_eq!(task.data["priority"], "normal");
    assert!(
        plan.room.is_none(),
        "maintenance.open does not rewrite the room"
    );
}

#[test]
fn plan_maintenance_open_reconciles_availability() {
    let mut facts = base_facts();
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    facts.stays = vec![stay(
        "stay-1",
        "type-1",
        "reserved",
        "2026-11-10",
        "2026-11-12",
        1,
        vec![],
        json!([]),
    )];
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &maintenance_open("room-a", "2026-11-10", "2026-11-12", "Leak"),
        5,
    )
    .unwrap_err();
    assert_eq!(
        error.code, "PMS_SOLD_OUT",
        "blocking the only room of the type would oversell the night"
    );
    assert_eq!(error.message, "Not enough rooms on this date");
}

#[test]
fn plan_maintenance_close_requires_inspection_then_dirties_the_room() {
    let mut facts = base_facts();
    facts.block = Some(block(
        "block-1",
        "maintenance",
        "active",
        "room-a",
        json!({}),
    ));
    facts.room = Some(room("room-a", "101", "type-1", "clean", 4));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::MaintenanceClose, "block-1"),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_INSPECTION_REQUIRED");
    assert_eq!(error.message, "Repair and inspect before opening the room");

    facts.room_tasks = vec![
        task("task-maintenance", "room-a", "maintenance", "clean", 4),
        task("task-stayover", "room-a", "stayover", "pending", 4),
        task("task-done", "room-a", "arrival", "done", 4),
    ];
    facts.room_tasks[0].data["stayId"] = json!("block-1");
    facts.room_tasks[0].data["state"] = json!("done");
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::MaintenanceClose, "block-1"),
        5,
    )
    .unwrap();
    let released = plan.block.clone().unwrap();
    assert_eq!(released.version, 2);
    assert_eq!(released.data["state"], "released");
    let room = plan.room.clone().unwrap();
    assert_eq!(room.version, 2);
    assert_eq!(room.data["cleanliness"], "dirty");
    assert_eq!(room.data["cleaningCycle"], 5);
    assert_eq!(room.data["dnd"], false);
    let superseded: Vec<&str> = plan
        .superseded_tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect();
    assert_eq!(
        superseded,
        ["task-stayover"],
        "done and maintenance tasks are not superseded"
    );
    assert_eq!(plan.superseded_tasks[0].data["state"], "superseded");
    let arrival = plan.new_task.clone().unwrap();
    assert_eq!(arrival.data["type"], "arrival");
    assert_eq!(arrival.data["roomCycle"], 5);
    assert_eq!(arrival.data["date"], "2026-11-04");
    assert_eq!(arrival.data["priority"], "normal");
}

#[test]
fn plan_maintenance_close_rejects_a_non_maintenance_block() {
    let mut facts = base_facts();
    facts.block = Some(block("block-1", "group", "active", "", json!({})));
    facts.room = Some(room("room-a", "101", "type-1", "clean", 0));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::MaintenanceClose, "block-1"),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_BLOCK_TYPE");
    assert_eq!(error.message, "This is not a maintenance room");

    facts.block = Some(block(
        "block-1",
        "maintenance",
        "released",
        "room-a",
        json!({}),
    ));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::MaintenanceClose, "block-1"),
        5,
    )
    .unwrap_err();
    assert_eq!(
        error.code, "PMS_BLOCK_TYPE",
        "only an active maintenance block can be closed"
    );
}

#[test]
fn plan_block_create_and_release_follow_retained_guards() {
    let facts = base_facts();
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_create(
            "Allotment",
            "type-1",
            "2026-11-10",
            "2026-11-12",
            "2026-11-05",
            1,
        ),
        5,
    )
    .unwrap();
    let created = plan.new_block.clone().unwrap();
    assert_eq!(plan.result.id, created.id);
    assert!(created.id.starts_with("pms-block-"));
    assert_eq!(created.data["kind"], "group");
    assert_eq!(created.data["state"], "active");
    assert_eq!(created.data["roomId"], "");
    assert_eq!(created.data["name"], "Allotment");
    assert_eq!(created.data["quantity"], 1);
    assert_eq!(created.data["releaseDate"], "2026-11-05");
    assert_eq!(created.data["pickedStayIds"], json!([]));

    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_create(
            "Allotment",
            "type-1",
            "2026-11-10",
            "2026-11-12",
            "2026-11-11",
            1,
        ),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_INVALID_DATES");
    assert_eq!(
        error.message,
        "Quota return date cannot be after arrival or before the business date"
    );
    assert_eq!(
        plan_room_command(
            &room_actor(),
            &key(),
            &facts,
            &block_create(
                "Allotment",
                "type-1",
                "2026-11-03",
                "2026-11-12",
                "2026-11-03",
                1
            ),
            5,
        )
        .unwrap_err()
        .code,
        "PMS_INVALID_DATES"
    );
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_create(
            "Allotment",
            "type-1",
            "2026-11-10",
            "2026-11-12",
            "2026-11-05",
            2,
        ),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_SOLD_OUT");

    let mut facts = base_facts();
    facts.block = Some(block("block-1", "group", "active", "", json!({})));
    let plan = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::BlockRelease, "block-1"),
        5,
    )
    .unwrap();
    let released = plan.block.clone().unwrap();
    assert_eq!(released.version, 2);
    assert_eq!(released.data["state"], "released");
    assert!(plan.new_block.is_none());

    facts.block = Some(block(
        "block-1",
        "group",
        "active",
        "",
        json!({"bookingChangeId":"change-1"}),
    ));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::BlockRelease, "block-1"),
        5,
    )
    .unwrap_err();
    assert_eq!(error.code, "PMS_BLOCK_TYPE");
    assert_eq!(
        error.message,
        "Only ordinary group allotments can be released here"
    );

    facts.block = Some(block(
        "block-1",
        "maintenance",
        "active",
        "room-a",
        json!({}),
    ));
    let error = plan_room_command(
        &room_actor(),
        &key(),
        &facts,
        &block_ref(PmsRoomCommandOperation::BlockRelease, "block-1"),
        5,
    )
    .unwrap_err();
    assert_eq!(
        error.code, "PMS_BLOCK_TYPE",
        "maintenance blocks close through maintenance.close"
    );
}
