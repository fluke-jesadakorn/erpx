//! PMS room-assignment and room-move command facts.
//!
//! These rules are the retained `stay.assign`/`stay.move` lifecycle checks
//! reduced to owner facts: the stay, its reservation history, the target room,
//! other nonterminal stays, active maintenance blocks and the replaced room's
//! open tasks. They never read a client source label, a Finance balance or a
//! People/Commercial HTTP projection. A command result is a property receipt
//! version, never a permission or a current authorization proof.
use super::room_board::{
    PmsPropertyRow, PmsRecord, cp_len, ecma_trim, not_found, parse_date, record_not_found,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_STAYS: usize = 4_000;
pub const MAX_BLOCKS: usize = 1_000;
pub const MAX_TASKS: usize = 1_000;
pub const MAX_SEGMENTS: usize = 200;
pub const MAX_REASON: usize = 2_000;

/// One of the two bounded room lifecycle commands. `stay.assign` applies to a
/// reserved stay and `stay.move` to an in-house stay; the retained engine
/// deliberately shares one availability implementation for both.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsStayRoomOperation {
    Assign,
    Move,
}

impl PmsStayRoomOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stay.assign" => Some(Self::Assign),
            "stay.move" => Some(Self::Move),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assign => "stay.assign",
            Self::Move => "stay.move",
        }
    }

    fn history_action(self) -> &'static str {
        match self {
            Self::Assign => "stay assign",
            Self::Move => "stay move",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsStayRoomCommand {
    pub operation: PmsStayRoomOperation,
    pub property_id: String,
    pub stay_id: String,
    pub room_id: String,
    pub reason: String,
    /// Current `pms_properties.version`. Required: this command family never
    /// accepts a stale property snapshot.
    pub expected_version: i64,
    /// Optional current `pms_records.version` for the stay itself.
    pub expected_stay_version: Option<i64>,
}

impl PmsStayRoomCommand {
    pub fn operation(&self) -> PmsStayRoomOperation {
        self.operation
    }

    /// Retained Zod `reason` uses `.trim().max(2000)`. Normalize so the
    /// trimmed canonical value is hashed, replayed, loaded and committed.
    pub fn normalize(&mut self) {
        self.reason = ecma_trim(&self.reason).to_owned();
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.property_id)
            || !valid_id(&self.stay_id)
            || !valid_id(&self.room_id)
            || cp_len(ecma_trim(&self.reason)) > MAX_REASON
            || self.expected_version < 1
            || self
                .expected_stay_version
                .is_some_and(|version| version < 1)
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && cp_len(value) <= 200
        && !value.chars().any(char::is_control)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_STAY_ROOM_REQUEST_INVALID",
        "Use one bounded room assignment with the current hotel version.",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_STAY_ROOM_RECORD_INVALID",
        "The hotel stay could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_STAY_ROOM_READ_LIMIT",
        message,
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Owner command context. The board actor supplies Company/branch scope,
/// permissions, module and work-context expiry; receipt/audit facts add the
/// selected assignment identity without granting anything.
#[derive(Clone, Debug)]
pub struct PmsStayRoomActor {
    pub board: super::room_board::PmsBoardActor,
    pub assignment_id: String,
    pub role: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl PmsStayRoomActor {
    pub fn require_command(&self, now: i64) -> Result<(), BusinessError> {
        self.board.require_front_desk(now)?;
        if self.assignment_id.is_empty()
            || cp_len(&self.assignment_id) > 200
            || self.assignment_id.chars().any(char::is_control)
            || self.role.is_empty()
            || cp_len(&self.role) > 200
            || self.role.chars().any(char::is_control)
            || self.revision < 0
            || self.assignment_version < 1
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_COMMAND_CONTEXT_INVALID",
                "Your selected work context cannot submit this hotel action.",
            ));
        }
        Ok(())
    }
}

/// Current Commercial `pms.front-desk` settlement entitlement projected
/// through the public owner boundary. The command never invents a package row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmsStayRoomEntitlement {
    pub enforced: bool,
    pub settle_existing_allowed: bool,
}

impl PmsStayRoomEntitlement {
    pub fn require_settle_existing(self) -> Result<(), BusinessError> {
        if self.enforced && !self.settle_existing_allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "This package does not permit the requested Hotel workspace.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsStayRoomResult {
    pub id: String,
    pub property_id: String,
    pub version: i64,
}

/// Authoritative owner snapshot loaded under the property row lock. Reserved
/// and in-house stays are the complete availability set for this property.
#[derive(Clone, Debug)]
pub struct PmsStayRoomFacts {
    pub property: PmsPropertyRow,
    pub stay: PmsRecord,
    pub reservation: Option<PmsRecord>,
    pub room: PmsRecord,
    /// The replaced room only when an in-house stay moves to another room.
    pub old_room: Option<PmsRecord>,
    pub stays: Vec<PmsRecord>,
    pub blocks: Vec<PmsRecord>,
    /// Open and completed tasks of the replaced room, the complete set used by
    /// the retained supersede/departure behavior.
    pub tasks: Vec<PmsRecord>,
}

#[derive(Clone, Debug)]
pub struct PmsStayRoomPlan {
    pub stay: PmsRecord,
    pub reservation: Option<PmsRecord>,
    pub old_room: Option<PmsRecord>,
    pub superseded_tasks: Vec<PmsRecord>,
    pub departure_task: Option<PmsRecord>,
    /// Property version after this command. Audit/outbox rows use this version.
    pub property_version: i64,
    pub result: PmsStayRoomResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Segment {
    room_id: String,
    from: String,
    to: String,
}

#[derive(Clone, Debug)]
struct StayFacts {
    state: String,
    type_id: String,
    room_id: String,
    arrival: String,
    departure: String,
    reservation_id: String,
    segments: Vec<Segment>,
}

#[derive(Clone, Debug)]
struct RoomFacts {
    active: bool,
    type_id: String,
    cleanliness: String,
    cleaning_cycle: i64,
}

fn text(data: &serde_json::Value, key: &'static str) -> Result<String, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(record_invalid)
}

fn boolean(data: &serde_json::Value, key: &'static str) -> Result<bool, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(record_invalid)
}

fn integer(data: &serde_json::Value, key: &'static str) -> Result<i64, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(record_invalid)
}

fn object(data: &serde_json::Value) -> Result<&serde_json::Map<String, serde_json::Value>, BusinessError> {
    data.as_object().ok_or_else(record_invalid)
}

fn stay_facts(record: &PmsRecord) -> Result<StayFacts, BusinessError> {
    let data = object(&record.data)?;
    let state = text(&record.data, "state")?;
    let type_id = text(&record.data, "typeId")?;
    let room_id = text(&record.data, "roomId")?;
    let arrival = text(&record.data, "arrival")?;
    let departure = text(&record.data, "departure")?;
    let reservation_id = text(&record.data, "reservationId")?;
    if !matches!(
        state.as_str(),
        "reserved" | "in_house" | "checked_out" | "cancelled" | "no_show"
    ) || type_id.is_empty()
        || arrival.len() != 10
        || departure.len() != 10
        || parse_date(&arrival).is_none()
        || parse_date(&departure).is_none()
    {
        return Err(record_invalid());
    }
    let segments = match data.get("segments") {
        Some(serde_json::Value::Array(values)) => {
            if values.len() > MAX_SEGMENTS {
                return Err(record_invalid());
            }
            let mut segments = Vec::with_capacity(values.len());
            for value in values {
                let room = value
                    .get("roomId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                let from = value
                    .get("from")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                let to = value
                    .get("to")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                if room.is_empty() || from.len() != 10 || to.len() != 10 {
                    return Err(record_invalid());
                }
                if parse_date(from).is_none() || parse_date(to).is_none() || from > to {
                    return Err(record_invalid());
                }
                segments.push(Segment {
                    room_id: room.to_owned(),
                    from: from.to_owned(),
                    to: to.to_owned(),
                });
            }
            segments
        }
        Some(_) => return Err(record_invalid()),
        None => Vec::new(),
    };
    Ok(StayFacts {
        state,
        type_id,
        room_id,
        arrival,
        departure,
        reservation_id,
        segments,
    })
}

fn room_facts(record: &PmsRecord) -> Result<RoomFacts, BusinessError> {
    let active = boolean(&record.data, "active")?;
    let type_id = text(&record.data, "typeId")?;
    let cleanliness = text(&record.data, "cleanliness")?;
    let cleaning_cycle = match record.data.get("cleaningCycle") {
        Some(serde_json::Value::Number(value)) => value.as_i64().ok_or_else(record_invalid)?,
        Some(serde_json::Value::Null) | None => 0,
        Some(_) => return Err(record_invalid()),
    };
    if type_id.is_empty()
        || !matches!(cleanliness.as_str(), "dirty" | "clean" | "inspected")
        || cleaning_cycle < 0
    {
        return Err(record_invalid());
    }
    Ok(RoomFacts {
        active,
        type_id,
        cleanliness,
        cleaning_cycle,
    })
}

fn block_room(value: &serde_json::Value) -> Result<(String, String, String), BusinessError> {
    let kind = text(value, "kind")?;
    let state = text(value, "state")?;
    let room_id = text(value, "roomId")?;
    let from = text(value, "from")?;
    if from.len() != 10 || parse_date(&from).is_none() {
        return Err(record_invalid());
    }
    Ok((kind, state, room_id))
}

/// Plan the retained room assignment/move transition. The property row is
/// already locked and `command.expected_version` must match it. Every stored
/// fact is validated before any row is written, so malformed owner data fails
/// closed instead of inventing availability.
pub fn plan_stay_room(
    actor: &PmsStayRoomActor,
    facts: &PmsStayRoomFacts,
    command: &PmsStayRoomCommand,
    now: i64,
) -> Result<PmsStayRoomPlan, BusinessError> {
    command.validate()?;
    if facts.property.id != command.property_id
        || facts.property.company_id != actor.board.company_id
        || actor
            .board
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &facts.property.branch_id)
    {
        return Err(not_found());
    }
    if facts.property.business_date.len() != 10
        || parse_date(&facts.property.business_date).is_none()
        || facts.property.version < 1
    {
        return Err(record_invalid());
    }
    if facts.property.version != command.expected_version {
        return Err(conflict(
            "PMS_VERSION_CONFLICT",
            "Hotel data changed. Load the latest version and review again.",
        ));
    }
    if facts.stay.id != command.stay_id {
        return Err(record_not_found());
    }
    if facts.stay.version < 1
        || command
            .expected_stay_version
            .is_some_and(|version| version != facts.stay.version)
    {
        return Err(conflict(
            "PMS_VERSION_CONFLICT",
            "Hotel data changed. Load the latest version and review again.",
        ));
    }
    match facts.property.state.as_str() {
        "open" => {}
        "closing" => {
            return Err(conflict(
                "PMS_DAY_CLOSING",
                "Day close is in progress. Please wait for it to finish.",
            ));
        }
        "setup" => {
            return Err(conflict(
                "PMS_SETUP_REQUIRED",
                "Review readiness and open the hotel first.",
            ));
        }
        _ => return Err(record_invalid()),
    }
    let stay = stay_facts(&facts.stay)?;
    if !matches!(stay.state.as_str(), "reserved" | "in_house") {
        return Err(conflict("PMS_STAY_CLOSED", "The stay is closed."));
    }
    let target = room_facts(&facts.room)?;
    let business_date = facts.property.business_date.as_str();
    let from = if stay.state == "in_house" {
        business_date.to_owned()
    } else {
        stay.arrival.clone()
    };
    if stay.departure <= from {
        return Err(conflict(
            "PMS_DEPARTURE_DUE",
            "Post the night before moving rooms.",
        ));
    }
    if facts.stays.len() > MAX_STAYS || facts.blocks.len() > MAX_BLOCKS || facts.tasks.len() > MAX_TASKS
    {
        return Err(read_limit("The hotel has more stay facts than this command can read."));
    }
    if !target.active || target.type_id != stay.type_id {
        return Err(conflict(
            "PMS_ROOM_TYPE",
            "Room type does not match or the room is inactive.",
        ));
    }
    for block in &facts.blocks {
        let (kind, state, room_id) = block_room(&block.data)?;
        if kind == "maintenance"
            && state == "active"
            && room_id == command.room_id
            && block
                .data
                .get("from")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value < stay.departure.as_str())
        {
            return Err(conflict("PMS_ROOM_BLOCKED", "Room is under maintenance."));
        }
    }
    for other in &facts.stays {
        if other.id == facts.stay.id {
            continue;
        }
        let other_state = text(&other.data, "state")?;
        if !matches!(other_state.as_str(), "reserved" | "in_house") {
            continue;
        }
        for segment in stay_facts(other)?.segments {
            if segment.room_id == command.room_id
                && segment.from < stay.departure
                && segment.to > from
            {
                return Err(conflict(
                    "PMS_ROOM_OVERLAP",
                    "This room is assigned to another stay.",
                ));
            }
        }
    }
    if stay.state == "in_house" {
        if target.cleanliness != "inspected" {
            return Err(conflict(
                "PMS_ROOM_NOT_READY",
                "Room has not passed inspection or has a guest.",
            ));
        }
        for other in &facts.stays {
            if other.id == facts.stay.id {
                continue;
            }
            if text(&other.data, "state")? == "in_house"
                && text(&other.data, "roomId")? == command.room_id
            {
                return Err(conflict(
                    "PMS_ROOM_NOT_READY",
                    "Room has not passed inspection or has a guest.",
                ));
            }
        }
    }

    let mut stay_record = facts.stay.clone();
    let mut segments = stay.segments.clone();
    let moving_in_house = stay.state == "in_house"
        && !stay.room_id.is_empty()
        && stay.room_id != command.room_id;
    if moving_in_house {
        segments = segments
            .into_iter()
            .filter_map(|segment| {
                let to = if segment.to > from {
                    from.clone()
                } else {
                    segment.to
                };
                (to > segment.from).then_some(Segment {
                    room_id: segment.room_id,
                    from: segment.from,
                    to,
                })
            })
            .collect();
    } else if stay.state == "reserved" {
        segments.clear();
    }
    if stay.room_id != command.room_id || segments.is_empty() {
        segments.push(Segment {
            room_id: command.room_id.clone(),
            from: from.clone(),
            to: stay.departure.clone(),
        });
    }
    if segments.len() > MAX_SEGMENTS {
        return Err(read_limit(
            "The hotel stay has more room segments than this command can write.",
        ));
    }
    object(&stay_record.data)?;
    stay_record.data["segments"] = serde_json::Value::Array(
        segments
            .iter()
            .map(|segment| {
                serde_json::json!({
                    "roomId": segment.room_id,
                    "from": segment.from,
                    "to": segment.to,
                })
            })
            .collect(),
    );
    stay_record.data["roomId"] = serde_json::Value::String(command.room_id.clone());
    stay_record.version += 1;
    stay_record.updated_at = now;

    let reservation = match &facts.reservation {
        Some(record) => {
            let mut record = record.clone();
            let mut history = match record.data.get("history") {
                Some(serde_json::Value::Array(values)) => values.clone(),
                Some(_) => return Err(record_invalid()),
                None => Vec::new(),
            };
            history.push(serde_json::json!({
                "at": now,
                "actor": actor.board.user_id,
                "action": command.operation().history_action(),
            }));
            record.data["history"] = serde_json::Value::Array(history);
            record.version += 1;
            record.updated_at = now;
            Some(record)
        }
        None => None,
    };

    let mut old_room = None;
    let mut superseded_tasks = Vec::new();
    let mut departure_task = None;
    if moving_in_house {
        let mut room = facts
            .old_room
            .clone()
            .ok_or_else(|| record_not_found())?;
        let old = room_facts(&room)?;
        room.data["cleanliness"] = serde_json::Value::String("dirty".to_owned());
        room.data["cleaningCycle"] = serde_json::Value::from(old.cleaning_cycle + 1);
        room.data["dnd"] = serde_json::Value::Bool(false);
        room.version += 1;
        room.updated_at = now;
        for task in &facts.tasks {
            let state = text(&task.data, "state")?;
            let kind = text(&task.data, "type")?;
            if !matches!(state.as_str(), "done" | "superseded") && kind != "maintenance" {
                let mut task = task.clone();
                task.data["state"] = serde_json::Value::String("superseded".to_owned());
                task.version += 1;
                task.updated_at = now;
                superseded_tasks.push(task);
            }
        }
        let cycle = old.cleaning_cycle + 1;
        let existing = facts.tasks.iter().any(|task| {
            task.data.get("roomId").and_then(serde_json::Value::as_str) == Some(&command.room_id)
                && task.data.get("type").and_then(serde_json::Value::as_str) == Some("departure")
                && task.data.get("date").and_then(serde_json::Value::as_str)
                    == Some(business_date)
                && task.data.get("stayId").and_then(serde_json::Value::as_str)
                    == Some(facts.stay.id.as_str())
                && task
                    .data
                    .get("roomCycle")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    == cycle
        });
        if !existing {
            departure_task = Some(PmsRecord {
                id: departure_task_id(&facts.stay.id, business_date, cycle),
                kind: "task".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "roomCycle": cycle,
                    "roomId": command.room_id,
                    "type": "departure",
                    "stayId": facts.stay.id,
                    "date": business_date,
                    "assigneeId": "",
                    "priority": "urgent",
                    "state": "pending",
                    "checklist": [
                        "Bathroom",
                        "Bed and linen",
                        "Floor and dust",
                        "Minibar",
                        "Equipment and safety"
                    ],
                    "checked": [],
                    "notes": "",
                    "completedBy": "",
                    "inspectedBy": ""
                }),
            });
        }
        old_room = Some(room);
    }

    let property_version = facts.property.version + 1;
    Ok(PmsStayRoomPlan {
        result: PmsStayRoomResult {
            id: facts.stay.id.clone(),
            property_id: facts.property.id.clone(),
            version: property_version,
        },
        stay: stay_record,
        reservation,
        old_room,
        superseded_tasks,
        departure_task,
        property_version,
    })
}

fn departure_task_id(stay: &str, business_date: &str, cycle: i64) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        format!("pms-departure-task:{stay}:{business_date}:{cycle}").as_bytes(),
    );
    format!("pms-task-{}", hex::encode(&digest[..16]))
}
