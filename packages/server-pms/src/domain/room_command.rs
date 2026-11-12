//! PMS room-configuration and housekeeping command facts.
//!
//! These rules are the retained `room.save`, `room.dnd`, `room.observe` and
//! `task.assign`/`task.start`/`task.complete`/`task.inspect` transitions
//! reduced to owner facts: the property row, the target room or task, the room
//! types/rooms/nonterminal stays/active blocks used by the retained capacity
//! reconciliation, and the tasks of the target room. They never read a client
//! source label, an Account session, a Finance balance or a People/Commercial
//! HTTP projection. A command result is a property receipt version, never a
//! permission or a current authorization proof.
use super::room_board::{
    PmsPropertyRow, PmsRecord, cp_len, ecma_trim, not_found, parse_date, record_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};

/// Bounded reads. The retained engine reads the complete hotel snapshot; these
/// bounds match the landed board/stay readers so a command fails closed instead
/// of loading an unbounded hotel.
pub const MAX_ROOMS: usize = 500;
pub const MAX_ROOM_TYPES: usize = 500;
pub const MAX_STAYS: usize = 4_000;
pub const MAX_BLOCKS: usize = 1_000;
pub const MAX_TASKS: usize = 1_000;
/// Retained `textField` (`.trim().max(2000)`) and `idField` limits.
pub const MAX_TEXT: usize = 2_000;
pub const MAX_ID: usize = 128;
/// Retained `checked` is an unbounded `array(textField)`; ingress stays bounded.
pub const MAX_CHECKED: usize = 200;
/// Retained `dates()` refuses a single range longer than 730 nights.
pub const MAX_NIGHTS: usize = 730;
/// Capacity reconciliation is O(room types × affected nights).
pub const MAX_CAPACITY_EVALUATIONS: usize = 200_000;

/// The retained room/housekeeping command family. Each operation keeps the
/// retained command name for ingress and the retained `pms.<name>` receipt
/// operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsRoomCommandOperation {
    Save,
    Dnd,
    Observe,
    TaskAssign,
    TaskStart,
    TaskComplete,
    TaskInspect,
}

impl PmsRoomCommandOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "room.save" => Some(Self::Save),
            "room.dnd" => Some(Self::Dnd),
            "room.observe" => Some(Self::Observe),
            "task.assign" => Some(Self::TaskAssign),
            "task.start" => Some(Self::TaskStart),
            "task.complete" => Some(Self::TaskComplete),
            "task.inspect" => Some(Self::TaskInspect),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Save => "room.save",
            Self::Dnd => "room.dnd",
            Self::Observe => "room.observe",
            Self::TaskAssign => "task.assign",
            Self::TaskStart => "task.start",
            Self::TaskComplete => "task.complete",
            Self::TaskInspect => "task.inspect",
        }
    }

    /// Retained receipt, audit and outbox operation (`pms.${op}`).
    pub fn receipt_operation(self) -> &'static str {
        match self {
            Self::Save => "pms.room.save",
            Self::Dnd => "pms.room.dnd",
            Self::Observe => "pms.room.observe",
            Self::TaskAssign => "pms.task.assign",
            Self::TaskStart => "pms.task.start",
            Self::TaskComplete => "pms.task.complete",
            Self::TaskInspect => "pms.task.inspect",
        }
    }

    /// Retained `operationPermission`: `room.save` is inventory configuration,
    /// `room.dnd`/`room.observe`/`task.start`/`task.complete` are housekeeping,
    /// and claiming (`task.assign`) or inspecting a task needs inspection
    /// rights. Composition still resolves and revalidates the permission.
    pub fn permission(self) -> &'static str {
        match self {
            Self::Save => "pms.configure",
            Self::Dnd | Self::Observe | Self::TaskStart | Self::TaskComplete => "pms.housekeeping",
            Self::TaskAssign | Self::TaskInspect => "pms.inspect",
        }
    }

    /// Retained `room.save` ends with `.save` and is therefore the only
    /// operation of this family still available before the hotel opens.
    pub fn allowed_in_setup(self) -> bool {
        matches!(self, Self::Save)
    }
}

/// Retained `task.assign` priority enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsTaskPriority {
    Normal,
    Urgent,
}

impl PmsTaskPriority {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Urgent => "urgent",
        }
    }
}

/// Retained `room.save` input. An absent `room_id` means create; the other
/// fields keep the retained Zod defaults (`building`/`floor` empty, `active`
/// true) and are stored trimmed exactly as `.trim()` stores them.
#[derive(Clone, Debug)]
pub struct PmsRoomSaveInput {
    pub room_id: Option<String>,
    pub number: String,
    pub type_id: String,
    pub building: String,
    pub floor: String,
    pub active: bool,
}

/// One typed input per retained operation. The variant must match
/// [`PmsRoomCommand::operation`].
#[derive(Clone, Debug)]
pub enum PmsRoomCommandInput {
    Save(PmsRoomSaveInput),
    Dnd {
        room_id: String,
        dnd: bool,
    },
    Observe {
        room_id: String,
        occupied: bool,
    },
    TaskAssign {
        task_id: String,
        assignee_id: String,
        priority: PmsTaskPriority,
    },
    TaskStart {
        task_id: String,
        checked: Vec<String>,
        notes: String,
    },
    TaskComplete {
        task_id: String,
        checked: Vec<String>,
        notes: String,
    },
    TaskInspect {
        task_id: String,
    },
}

impl PmsRoomCommandInput {
    fn operation(&self) -> PmsRoomCommandOperation {
        match self {
            Self::Save(_) => PmsRoomCommandOperation::Save,
            Self::Dnd { .. } => PmsRoomCommandOperation::Dnd,
            Self::Observe { .. } => PmsRoomCommandOperation::Observe,
            Self::TaskAssign { .. } => PmsRoomCommandOperation::TaskAssign,
            Self::TaskStart { .. } => PmsRoomCommandOperation::TaskStart,
            Self::TaskComplete { .. } => PmsRoomCommandOperation::TaskComplete,
            Self::TaskInspect { .. } => PmsRoomCommandOperation::TaskInspect,
        }
    }

    /// The room or task this command acts on, when the retained input names one.
    pub fn target_id(&self) -> Option<&str> {
        match self {
            Self::Save(input) => input.room_id.as_deref(),
            Self::Dnd { room_id, .. } | Self::Observe { room_id, .. } => Some(room_id),
            Self::TaskAssign { task_id, .. }
            | Self::TaskStart { task_id, .. }
            | Self::TaskComplete { task_id, .. }
            | Self::TaskInspect { task_id } => Some(task_id),
        }
    }

    /// Room id a task operation must load, when the input names a task.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::TaskAssign { task_id, .. }
            | Self::TaskStart { task_id, .. }
            | Self::TaskComplete { task_id, .. }
            | Self::TaskInspect { task_id } => Some(task_id),
            Self::Save(_) | Self::Dnd { .. } | Self::Observe { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsRoomCommand {
    pub operation: PmsRoomCommandOperation,
    pub property_id: String,
    /// Current `pms_properties.version`. Required: this command family never
    /// accepts a stale property snapshot.
    pub expected_version: i64,
    pub input: PmsRoomCommandInput,
}

impl PmsRoomCommand {
    pub fn operation(&self) -> PmsRoomCommandOperation {
        self.operation
    }

    /// Retained `.trim()` boundaries: `number`, `building`, `floor`, `notes`
    /// and each `checked` entry are stored trimmed. Ids are never trimmed.
    pub fn normalize(&mut self) {
        match &mut self.input {
            PmsRoomCommandInput::Save(input) => {
                input.number = ecma_trim(&input.number).to_owned();
                input.building = ecma_trim(&input.building).to_owned();
                input.floor = ecma_trim(&input.floor).to_owned();
            }
            PmsRoomCommandInput::TaskStart { checked, notes, .. }
            | PmsRoomCommandInput::TaskComplete { checked, notes, .. } => {
                *notes = ecma_trim(notes).to_owned();
                for value in checked.iter_mut() {
                    *value = ecma_trim(value).to_owned();
                }
            }
            PmsRoomCommandInput::Dnd { .. }
            | PmsRoomCommandInput::Observe { .. }
            | PmsRoomCommandInput::TaskAssign { .. }
            | PmsRoomCommandInput::TaskInspect { .. } => {}
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.input.operation() != self.operation
            || !valid_id(&self.property_id)
            || self.expected_version < 1
        {
            return Err(invalid());
        }
        match &self.input {
            PmsRoomCommandInput::Save(input) => {
                if input.room_id.as_deref().is_some_and(|id| !valid_id(id))
                    || !(1..=MAX_TEXT).contains(&cp_len(&input.number))
                    || !valid_id(&input.type_id)
                    || cp_len(&input.building) > MAX_TEXT
                    || cp_len(&input.floor) > MAX_TEXT
                {
                    return Err(invalid());
                }
            }
            PmsRoomCommandInput::Dnd { room_id, .. }
            | PmsRoomCommandInput::Observe { room_id, .. } => {
                if !valid_id(room_id) {
                    return Err(invalid());
                }
            }
            PmsRoomCommandInput::TaskAssign {
                task_id,
                assignee_id,
                ..
            } => {
                if !valid_id(task_id) || !valid_id(assignee_id) {
                    return Err(invalid());
                }
            }
            PmsRoomCommandInput::TaskStart {
                task_id,
                checked,
                notes,
            }
            | PmsRoomCommandInput::TaskComplete {
                task_id,
                checked,
                notes,
            } => {
                if !valid_id(task_id)
                    || checked.len() > MAX_CHECKED
                    || checked.iter().any(|value| cp_len(value) > MAX_TEXT)
                    || cp_len(notes) > MAX_TEXT
                {
                    return Err(invalid());
                }
            }
            PmsRoomCommandInput::TaskInspect { task_id } => {
                if !valid_id(task_id) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

/// Retained `idField`: one to 128 Unicode scalars, never trimmed.
fn valid_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_ROOM_COMMAND_REQUEST_INVALID",
        "Use one bounded hotel room action with the current hotel version.",
    )
}

pub fn record_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_ROOM_COMMAND_RECORD_INVALID",
        "The hotel room record could not be read safely.",
    )
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_ROOM_COMMAND_READ_LIMIT",
        message,
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn version_conflict() -> BusinessError {
    conflict(
        "PMS_VERSION_CONFLICT",
        "Hotel data changed. Load the latest version and review again.",
    )
}

/// Owner command context. The board actor supplies Company/branch scope,
/// permissions, module and work-context expiry; receipt/audit facts add the
/// selected assignment identity without granting anything.
#[derive(Clone, Debug)]
pub struct PmsRoomCommandActor {
    pub board: super::room_board::PmsBoardActor,
    pub assignment_id: String,
    pub role: String,
    pub revision: i64,
    pub assignment_version: i64,
}

impl PmsRoomCommandActor {
    /// Mirror of the retained workspace gate (read permission, module, branch
    /// area, unexpired work context) followed by the retained
    /// `operationPermission` for this operation. `room_board`'s workspace gate
    /// is crate-private and its public helpers encode different permissions, so
    /// the gate is repeated here instead of widened there.
    pub fn require_command(
        &self,
        operation: PmsRoomCommandOperation,
        now: i64,
    ) -> Result<(), BusinessError> {
        let board = &self.board;
        if !board.has("pms.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
        if !board.modules.iter().any(|module| module == "pms") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Hotels are unavailable to the selected role.",
            ));
        }
        if board.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PMS_BRANCH_REQUIRED",
                "Select a company or branch area.",
            ));
        }
        if now >= board.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !board.has(operation.permission()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ));
        }
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PmsRoomCommandResult {
    pub id: String,
    pub property_id: String,
    pub version: i64,
}

/// Authoritative owner snapshot loaded under the property row lock. Fields are
/// loaded only for the operations that need them; every list is bounded.
#[derive(Clone, Debug)]
pub struct PmsRoomCommandFacts {
    pub property: PmsPropertyRow,
    /// Existing target room (`room.save` update, `room.dnd`, `room.observe` and
    /// the room a task belongs to).
    pub room: Option<PmsRecord>,
    /// Target `room_type` for `room.save`.
    pub room_type: Option<PmsRecord>,
    /// Complete bounded room set: duplicate-number and capacity facts.
    pub rooms: Vec<PmsRecord>,
    /// Complete bounded room-type set: capacity reconciliation iterates it.
    pub room_types: Vec<PmsRecord>,
    /// Nonterminal stays (`reserved`, `in_house`): assignment and capacity.
    pub stays: Vec<PmsRecord>,
    /// Active blocks: maintenance availability and group holds.
    pub blocks: Vec<PmsRecord>,
    /// Target task for `task.*`.
    pub task: Option<PmsRecord>,
    /// Bounded tasks of the target room; the retained discrepancy dedupe set.
    pub room_tasks: Vec<PmsRecord>,
    /// Whether an in-house stay occupies the target room (`room.observe`).
    pub in_house: bool,
}

#[derive(Clone, Debug)]
pub struct PmsRoomCommandPlan {
    pub result: PmsRoomCommandResult,
    /// Updated room row (never also a created row).
    pub room: Option<PmsRecord>,
    /// New room row (`room.save` create).
    pub new_room: Option<PmsRecord>,
    /// Updated task row.
    pub task: Option<PmsRecord>,
    /// New generated task (`room.observe` discrepancy).
    pub new_task: Option<PmsRecord>,
    /// Property version after this command. Audit/outbox rows use this version.
    pub property_version: i64,
}

fn object(
    data: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, BusinessError> {
    data.as_object().ok_or_else(record_invalid)
}

fn text(data: &serde_json::Value, key: &'static str) -> Result<String, BusinessError> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(record_invalid)
}

fn optional_text(data: &serde_json::Value, key: &'static str) -> Option<String> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
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

fn cleaning_cycle(data: &serde_json::Value) -> Result<i64, BusinessError> {
    match data.get("cleaningCycle") {
        Some(serde_json::Value::Number(value)) => {
            let value = value.as_i64().ok_or_else(record_invalid)?;
            (value >= 0).then_some(value).ok_or_else(record_invalid)
        }
        Some(serde_json::Value::Null) | None => Ok(0),
        Some(_) => Err(record_invalid()),
    }
}

/// Retained room data shape this command family reads and writes.
#[derive(Clone, Debug)]
struct RoomFacts {
    type_id: String,
    cleaning_cycle: i64,
    dnd: bool,
}

impl RoomFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "room" {
            return Err(record_invalid());
        }
        let data = object(&record.data)?;
        let type_id = text(&record.data, "typeId")?;
        let cleanliness = text(&record.data, "cleanliness")?;
        if type_id.is_empty() || !matches!(cleanliness.as_str(), "dirty" | "clean" | "inspected") {
            return Err(record_invalid());
        }
        boolean(&record.data, "active")?;
        Ok(Self {
            type_id,
            cleaning_cycle: cleaning_cycle(&record.data)?,
            dnd: data
                .get("dnd")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// Retained `stay.segments` room references used by the `PMS_ROOM_IN_USE` guard.
fn stay_segments(record: &PmsRecord) -> Result<Vec<String>, BusinessError> {
    let state = text(&record.data, "state")?;
    if !matches!(state.as_str(), "reserved" | "in_house") {
        return Ok(Vec::new());
    }
    match record.data.get("segments") {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(values)) => {
            let mut rooms = Vec::with_capacity(values.len());
            for value in values {
                let room = value
                    .get("roomId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                if room.is_empty() {
                    return Err(record_invalid());
                }
                rooms.push(room.to_owned());
            }
            Ok(rooms)
        }
        Some(_) => Err(record_invalid()),
    }
}

/// Retained `task` data facts this command family reads.
#[derive(Clone, Debug)]
struct TaskFacts {
    kind: String,
    state: String,
    assignee_id: String,
    checklist: Vec<String>,
    room_cycle: i64,
}

impl TaskFacts {
    fn read(record: &PmsRecord) -> Result<Self, BusinessError> {
        if record.kind != "task" {
            return Err(record_invalid());
        }
        let data = object(&record.data)?;
        let kind = text(&record.data, "type")?;
        let state = text(&record.data, "state")?;
        if text(&record.data, "roomId")?.is_empty()
            || !matches!(
                kind.as_str(),
                "departure" | "stayover" | "arrival" | "maintenance" | "discrepancy"
            )
            || !matches!(
                state.as_str(),
                "pending" | "working" | "clean" | "done" | "superseded"
            )
        {
            return Err(record_invalid());
        }
        let checklist = match data.get("checklist") {
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or_else(record_invalid))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(record_invalid()),
            None => Vec::new(),
        };
        Ok(Self {
            kind,
            state,
            assignee_id: optional_text(&record.data, "assigneeId").unwrap_or_default(),
            checklist,
            room_cycle: data
                .get("roomCycle")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
    }
}

#[derive(Clone, Debug)]
struct CapacityStay {
    id: String,
    type_id: String,
    arrival: String,
    departure: String,
    periods: Vec<(String, String, String)>,
}

impl CapacityStay {
    /// Retained `stayTypeOnDate`: the first covering room-type period wins.
    fn type_on(&self, date: &str) -> &str {
        self.periods
            .iter()
            .find(|(_, from, to)| from.as_str() <= date && date < to.as_str())
            .map(|(type_id, _, _)| type_id.as_str())
            .unwrap_or(self.type_id.as_str())
    }
    fn covers(&self, date: &str) -> bool {
        self.arrival.as_str() <= date && date < self.departure.as_str()
    }
}

fn capacity_stay(record: &PmsRecord) -> Result<CapacityStay, BusinessError> {
    let state = text(&record.data, "state")?;
    if !matches!(state.as_str(), "reserved" | "in_house") {
        return Err(record_invalid());
    }
    let type_id = text(&record.data, "typeId")?;
    let arrival = text(&record.data, "arrival")?;
    let departure = text(&record.data, "departure")?;
    if type_id.is_empty()
        || arrival.len() != 10
        || departure.len() != 10
        || parse_date(&arrival).is_none()
        || parse_date(&departure).is_none()
    {
        return Err(record_invalid());
    }
    let periods = match record.data.get("roomTypePeriods") {
        None => Vec::new(),
        Some(serde_json::Value::Array(values)) => {
            let mut periods = Vec::with_capacity(values.len());
            for value in values {
                let type_id = value
                    .get("typeId")
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
                if type_id.is_empty()
                    || from.len() != 10
                    || to.len() != 10
                    || parse_date(from).is_none()
                    || parse_date(to).is_none()
                {
                    return Err(record_invalid());
                }
                periods.push((type_id.to_owned(), from.to_owned(), to.to_owned()));
            }
            periods
        }
        Some(_) => return Err(record_invalid()),
    };
    Ok(CapacityStay {
        id: record.id.clone(),
        type_id,
        arrival,
        departure,
        periods,
    })
}

#[derive(Clone, Debug)]
struct CapacityBlock {
    id: String,
    kind: String,
    state: String,
    room_id: String,
    type_id: String,
    from: String,
    to: String,
    quantity: i64,
    release_date: String,
    quota_only: bool,
    expires_at: Option<i64>,
    picked: Vec<String>,
    booking_change_id: String,
    claims: Vec<(String, String)>,
}

impl CapacityBlock {
    fn covers(&self, date: &str) -> bool {
        self.from.as_str() <= date && date < self.to.as_str()
    }
    fn live_at(&self, as_of: i64) -> bool {
        self.expires_at.is_none_or(|value| value > as_of)
    }
}

fn capacity_block(record: &PmsRecord) -> Result<CapacityBlock, BusinessError> {
    let kind = text(&record.data, "kind")?;
    let state = text(&record.data, "state")?;
    if !matches!(kind.as_str(), "group" | "maintenance")
        || !matches!(state.as_str(), "active" | "released")
    {
        return Err(record_invalid());
    }
    let from = text(&record.data, "from")?;
    let to = text(&record.data, "to")?;
    if from.len() != 10
        || to.len() != 10
        || parse_date(&from).is_none()
        || parse_date(&to).is_none()
    {
        return Err(record_invalid());
    }
    let picked = match record.data.get("pickedStayIds") {
        None => Vec::new(),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .map(|value| value.as_str().map(str::to_owned).ok_or_else(record_invalid))
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(record_invalid()),
    };
    let claims = match record.data.get("groupPickupClaims") {
        None => Vec::new(),
        Some(serde_json::Value::Array(values)) => {
            let mut claims = Vec::with_capacity(values.len());
            for value in values {
                let block_id = value
                    .get("blockId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                let stay_id = value
                    .get("stayId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(record_invalid)?;
                if block_id.is_empty() || stay_id.is_empty() {
                    return Err(record_invalid());
                }
                claims.push((block_id.to_owned(), stay_id.to_owned()));
            }
            claims
        }
        Some(_) => return Err(record_invalid()),
    };
    let expires_at = match record.data.get("expiresAt") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Number(value)) => Some(value.as_i64().ok_or_else(record_invalid)?),
        Some(_) => return Err(record_invalid()),
    };
    Ok(CapacityBlock {
        id: record.id.clone(),
        kind,
        state,
        room_id: optional_text(&record.data, "roomId").unwrap_or_default(),
        type_id: optional_text(&record.data, "typeId").unwrap_or_default(),
        from,
        to,
        quantity: integer(&record.data, "quantity")?,
        release_date: optional_text(&record.data, "releaseDate").unwrap_or_default(),
        quota_only: record
            .data
            .get("quotaOnly")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        expires_at,
        picked,
        booking_change_id: optional_text(&record.data, "bookingChangeId").unwrap_or_default(),
        claims,
    })
}

/// Retained `dates(from, to)`: `from` inclusive to `to` exclusive, with the
/// retained 730-night refusal.
fn day_range(from: &str, to: &str) -> Result<Vec<String>, BusinessError> {
    let (Some(from_day), Some(to_day)) = (parse_date(from), parse_date(to)) else {
        return Err(record_invalid());
    };
    let nights = to_day - from_day;
    if nights < 0 || nights as usize > MAX_NIGHTS {
        return Err(record_invalid());
    }
    Ok((0..nights)
        .map(|offset| format_day(from_day + offset))
        .collect())
}

fn format_day(days: i64) -> String {
    // Civil-from-days, the inverse of `room_board::parse_date`.
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    format!(
        "{:04}-{:02}-{:02}",
        year + i64::from(month <= 2),
        month,
        day
    )
}

/// Retained `groupPickupCount`: stays holding an allotment night plus durable
/// booking-change claims against the same block.
fn group_pickup_count(
    block: &CapacityBlock,
    stays: &[CapacityStay],
    blocks: &[CapacityBlock],
    date: &str,
    as_of: i64,
) -> i64 {
    let picked: std::collections::BTreeSet<&str> =
        block.picked.iter().map(String::as_str).collect();
    let live: Vec<&CapacityStay> = stays
        .iter()
        .filter(|stay| picked.contains(stay.id.as_str()))
        .collect();
    let live_ids: std::collections::BTreeSet<&str> =
        live.iter().map(|stay| stay.id.as_str()).collect();
    let mut used: std::collections::BTreeSet<&str> = live
        .iter()
        .filter(|stay| stay.covers(date) && stay.type_on(date) == block.type_id)
        .map(|stay| stay.id.as_str())
        .collect();
    for hold in blocks {
        if hold.state != "active"
            || hold.booking_change_id.is_empty()
            || !hold.covers(date)
            || !hold.live_at(as_of)
        {
            continue;
        }
        for (claim_block, stay_id) in &hold.claims {
            if claim_block == &block.id && live_ids.contains(stay_id.as_str()) {
                used.insert(stay_id.as_str());
            }
        }
    }
    used.len() as i64
}

/// Retained `availability(s, typeId, date)` for one night.
fn available(
    type_id: &str,
    date: &str,
    rooms: &[PmsRecord],
    stays: &[CapacityStay],
    blocks: &[CapacityBlock],
    as_of: i64,
    business_date: &str,
) -> i64 {
    let unavailable: Vec<&str> = blocks
        .iter()
        .filter(|block| {
            block.kind == "maintenance" && block.state == "active" && block.from.as_str() <= date
        })
        .map(|block| block.room_id.as_str())
        .collect();
    let capacity = rooms
        .iter()
        .filter(|room| {
            room.data.get("typeId").and_then(serde_json::Value::as_str) == Some(type_id)
                && room.data.get("active").and_then(serde_json::Value::as_bool) == Some(true)
                && !unavailable.contains(&room.id.as_str())
        })
        .count() as i64;
    let occupied = stays
        .iter()
        .filter(|stay| stay.type_on(date) == type_id && stay.covers(date))
        .count() as i64;
    let held: i64 = blocks
        .iter()
        .filter(|block| {
            block.kind == "group"
                && block.state == "active"
                && !block.quota_only
                && block.live_at(as_of)
                && block.type_id == type_id
                && block.covers(date)
                && block.release_date.as_str() > business_date
        })
        .map(|block| {
            (block.quantity - group_pickup_count(block, stays, blocks, date, as_of)).max(0)
        })
        .sum();
    capacity - occupied - held
}

/// Plan one retained room/housekeeping command. The property row is already
/// locked and `command.expected_version` must match it. Every stored fact is
/// validated before any row is written, so malformed owner data fails closed
/// instead of inventing availability.
pub fn plan_room_command(
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    facts: &PmsRoomCommandFacts,
    command: &PmsRoomCommand,
    now: i64,
) -> Result<PmsRoomCommandPlan, BusinessError> {
    command.validate()?;
    if facts.property.id != command.property_id
        || facts.property.company_id != actor.board.company_id
        || actor
            .board
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &facts.property.branch_id)
        || facts.property.business_date.len() != 10
        || parse_date(&facts.property.business_date).is_none()
        || facts.property.version < 1
    {
        return Err(not_found());
    }
    if facts.property.version != command.expected_version {
        return Err(version_conflict());
    }
    match facts.property.state.as_str() {
        "open" => {}
        "closing" => {
            return Err(conflict(
                "PMS_DAY_CLOSING",
                "Day close is in progress. Please wait for it to finish",
            ));
        }
        "setup" if command.operation().allowed_in_setup() => {}
        "setup" => {
            return Err(conflict(
                "PMS_SETUP_REQUIRED",
                "Review readiness and open the hotel first",
            ));
        }
        _ => return Err(record_invalid()),
    }
    if facts.rooms.len() > MAX_ROOMS
        || facts.room_types.len() > MAX_ROOM_TYPES
        || facts.stays.len() > MAX_STAYS
        || facts.blocks.len() > MAX_BLOCKS
        || facts.room_tasks.len() > MAX_TASKS
    {
        return Err(read_limit(
            "The hotel has more room facts than this command can read.",
        ));
    }
    let property_version = facts.property.version + 1;
    let mut plan = PmsRoomCommandPlan {
        result: PmsRoomCommandResult {
            id: String::new(),
            property_id: facts.property.id.clone(),
            version: property_version,
        },
        room: None,
        new_room: None,
        task: None,
        new_task: None,
        property_version,
    };
    match &command.input {
        PmsRoomCommandInput::Save(input) => plan_save(actor, key, facts, input, now, &mut plan)?,
        PmsRoomCommandInput::Dnd { room_id, dnd } => {
            let mut room = target_room(facts, room_id)?;
            RoomFacts::read(&room)?;
            object(&room.data)?;
            room.data["dnd"] = serde_json::Value::Bool(*dnd);
            plan.result.id = room.id.clone();
            touch(&mut room, now);
            plan.room = Some(room);
        }
        PmsRoomCommandInput::Observe { room_id, occupied } => {
            plan_observe(facts, room_id, *occupied, now, &mut plan)?
        }
        PmsRoomCommandInput::TaskAssign {
            task_id,
            assignee_id,
            priority,
        } => {
            let (mut task, room) = target_task(facts, task_id)?;
            RoomFacts::read(&room)?;
            let existing = TaskFacts::read(&task)?;
            if matches!(existing.state.as_str(), "done" | "superseded") {
                return Err(conflict("PMS_TASK_DONE", "Task has already been inspected"));
            }
            object(&task.data)?;
            task.data["assigneeId"] = serde_json::Value::String(assignee_id.clone());
            task.data["priority"] = serde_json::Value::String(priority.as_str().to_owned());
            plan.result.id = task.id.clone();
            touch(&mut task, now);
            plan.task = Some(task);
        }
        PmsRoomCommandInput::TaskStart { task_id, notes, .. } => {
            let (task, room) = plan_task_progress(actor, facts, task_id, None, notes, now)?;
            plan.result.id = task.id.clone();
            plan.task = Some(task);
            plan.room = room;
        }
        PmsRoomCommandInput::TaskComplete {
            task_id,
            checked,
            notes,
        } => {
            let (task, room) =
                plan_task_progress(actor, facts, task_id, Some(checked), notes, now)?;
            plan.result.id = task.id.clone();
            plan.task = Some(task);
            plan.room = room;
        }
        PmsRoomCommandInput::TaskInspect { task_id } => {
            let (task, room) = plan_task_inspect(actor, facts, task_id, now)?;
            plan.result.id = task.id.clone();
            plan.task = Some(task);
            plan.room = room;
        }
    }
    if plan.result.id.is_empty() {
        return Err(record_invalid());
    }
    Ok(plan)
}

fn touch(record: &mut PmsRecord, now: i64) {
    record.version += 1;
    record.updated_at = now;
}

fn target_room(facts: &PmsRoomCommandFacts, room_id: &str) -> Result<PmsRecord, BusinessError> {
    match &facts.room {
        Some(room) if room.id == room_id => Ok(room.clone()),
        _ => Err(record_not_found()),
    }
}

fn target_task(
    facts: &PmsRoomCommandFacts,
    task_id: &str,
) -> Result<(PmsRecord, PmsRecord), BusinessError> {
    let task = match &facts.task {
        Some(task) if task.id == task_id => task.clone(),
        _ => return Err(record_not_found()),
    };
    let room_id = text(&task.data, "roomId")?;
    let room = facts
        .room
        .clone()
        .filter(|room| room.id == room_id)
        .ok_or_else(record_not_found)?;
    Ok((task, room))
}

fn plan_save(
    _actor: &PmsRoomCommandActor,
    key: &CommandKey,
    facts: &PmsRoomCommandFacts,
    input: &PmsRoomSaveInput,
    now: i64,
    plan: &mut PmsRoomCommandPlan,
) -> Result<(), BusinessError> {
    let room_type = facts.room_type.as_ref().ok_or_else(record_not_found)?;
    if room_type.kind != "room_type" {
        return Err(record_invalid());
    }
    object(&room_type.data)?;
    if facts.rooms.iter().any(|room| {
        room.kind == "room"
            && Some(room.id.as_str()) != input.room_id.as_deref()
            && optional_text(&room.data, "number").as_deref() == Some(input.number.as_str())
    }) {
        return Err(conflict("PMS_ROOM_NUMBER_EXISTS", "Duplicate room number"));
    }
    match &input.room_id {
        Some(room_id) => {
            let mut room = target_room(facts, room_id)?;
            let existing = RoomFacts::read(&room)?;
            let assigned = facts
                .stays
                .iter()
                .map(stay_segments)
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|rooms| rooms.iter().any(|value| value == room_id));
            if assigned && !(input.active && input.type_id == existing.type_id) {
                return Err(conflict("PMS_ROOM_IN_USE", "Room is assigned to a stay"));
            }
            object(&room.data)?;
            room.data["id"] = serde_json::Value::String(room.id.clone());
            room.data["number"] = serde_json::Value::String(input.number.clone());
            room.data["typeId"] = serde_json::Value::String(input.type_id.clone());
            room.data["building"] = serde_json::Value::String(input.building.clone());
            room.data["floor"] = serde_json::Value::String(input.floor.clone());
            room.data["active"] = serde_json::Value::Bool(input.active);
            plan.result.id = room.id.clone();
            touch(&mut room, now);
            plan.room = Some(room);
        }
        None => {
            let room = PmsRecord {
                id: created_room_id(key, &facts.property.id),
                kind: "room".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "number": input.number,
                    "typeId": input.type_id,
                    "building": input.building,
                    "floor": input.floor,
                    "active": input.active,
                    "cleanliness": "dirty",
                    "dnd": false,
                    "observedOccupied": null
                }),
            };
            plan.result.id = room.id.clone();
            plan.new_room = Some(room);
        }
    }
    // The retained engine reconciles capacity against the mutated snapshot, so
    // the planned room replaces (or joins) the rooms the loader read.
    let mut capacity_rooms = facts.rooms.clone();
    match (&plan.room, &plan.new_room) {
        (Some(room), _) => match capacity_rooms.iter_mut().find(|row| row.id == room.id) {
            Some(slot) => *slot = room.clone(),
            None => capacity_rooms.push(room.clone()),
        },
        (None, Some(room)) => capacity_rooms.push(room.clone()),
        (None, None) => {}
    }
    reconcile_capacity(
        &facts.property,
        &facts.room_types,
        &capacity_rooms,
        &facts.stays,
        &facts.blocks,
        now,
    )
}

/// Deterministic identity for a newly created room. The retained engine mints
/// a UUID; deriving it from the retained command key keeps one key on one room
/// while the duplicate-number guard still rejects a second room with the same
/// number.
fn created_room_id(key: &CommandKey, property_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("pms-room-create:{}:{property_id}", key.as_str()).as_bytes());
    format!("pms-room-{}", hex::encode(&digest[..16]))
}

fn plan_observe(
    facts: &PmsRoomCommandFacts,
    room_id: &str,
    occupied: bool,
    now: i64,
    plan: &mut PmsRoomCommandPlan,
) -> Result<(), BusinessError> {
    let mut room = target_room(facts, room_id)?;
    let room_facts = RoomFacts::read(&room)?;
    object(&room.data)?;
    room.data["observedOccupied"] = serde_json::Value::Bool(occupied);
    plan.result.id = room.id.clone();
    touch(&mut room, now);
    if facts.in_house != occupied {
        let date = facts.property.business_date.clone();
        let cycle = room_facts.cleaning_cycle;
        let existing = facts.room_tasks.iter().any(|task| {
            optional_text(&task.data, "roomId").as_deref() == Some(room.id.as_str())
                && optional_text(&task.data, "type").as_deref() == Some("discrepancy")
                && optional_text(&task.data, "date").as_deref() == Some(date.as_str())
                && optional_text(&task.data, "stayId")
                    .unwrap_or_default()
                    .is_empty()
                && task
                    .data
                    .get("roomCycle")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    == cycle
        });
        if !existing {
            plan.new_task = Some(PmsRecord {
                id: room_task_id(&room.id, "discrepancy", &date, "", cycle),
                kind: "task".to_owned(),
                version: 1,
                updated_at: now,
                data: serde_json::json!({
                    "roomCycle": cycle,
                    "roomId": room.id,
                    "type": "discrepancy",
                    "stayId": "",
                    "date": date,
                    "assigneeId": "",
                    "priority": "normal",
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
    }
    plan.room = Some(room);
    Ok(())
}

/// Retained `task.start`/`task.complete` shared transition. `checked` is `None`
/// for start and `Some` for complete.
fn plan_task_progress(
    actor: &PmsRoomCommandActor,
    facts: &PmsRoomCommandFacts,
    task_id: &str,
    checked: Option<&Vec<String>>,
    notes: &str,
    now: i64,
) -> Result<(PmsRecord, Option<PmsRecord>), BusinessError> {
    let (mut task, mut room) = target_task(facts, task_id)?;
    let room_facts = RoomFacts::read(&room)?;
    let existing = TaskFacts::read(&task)?;
    if existing.room_cycle != room_facts.cleaning_cycle {
        return Err(conflict(
            "PMS_CLEANING_STALE",
            "The room has a newer cleaning cycle. Use the latest cycle",
        ));
    }
    if existing.state == "done"
        || (!existing.assignee_id.is_empty() && existing.assignee_id != actor.board.user_id)
    {
        return Err(conflict(
            "PMS_TASK_ASSIGNMENT",
            "This task is assigned to another employee or is already complete",
        ));
    }
    if room_facts.dnd && existing.kind != "maintenance" {
        return Err(conflict("PMS_DND", "Guest requested Do Not Disturb"));
    }
    object(&task.data)?;
    let mut room_touched = false;
    match checked {
        Some(checked) => {
            if !matches!(existing.state.as_str(), "working" | "pending")
                || existing
                    .checklist
                    .iter()
                    .any(|item| !checked.iter().any(|value| value == item))
            {
                return Err(conflict("PMS_CHECKLIST_REQUIRED", "Complete the checklist"));
            }
            task.data["state"] = serde_json::Value::String("clean".to_owned());
            task.data["completedBy"] = serde_json::Value::String(actor.board.user_id.clone());
            task.data["checked"] = serde_json::Value::Array(
                checked
                    .iter()
                    .map(|value| serde_json::Value::String(value.clone()))
                    .collect(),
            );
            if !matches!(existing.kind.as_str(), "maintenance" | "discrepancy") {
                object(&room.data)?;
                room.data["cleanliness"] = serde_json::Value::String("clean".to_owned());
                touch(&mut room, now);
                room_touched = true;
            }
        }
        None => {
            if existing.state != "pending" {
                return Err(conflict("PMS_TASK_STATE", "Task has already started"));
            }
            task.data["assigneeId"] = serde_json::Value::String(actor.board.user_id.clone());
            task.data["state"] = serde_json::Value::String("working".to_owned());
        }
    }
    task.data["notes"] = serde_json::Value::String(notes.to_owned());
    touch(&mut task, now);
    Ok((task, room_touched.then_some(room)))
}

fn plan_task_inspect(
    actor: &PmsRoomCommandActor,
    facts: &PmsRoomCommandFacts,
    task_id: &str,
    now: i64,
) -> Result<(PmsRecord, Option<PmsRecord>), BusinessError> {
    let (mut task, mut room) = target_task(facts, task_id)?;
    let room_facts = RoomFacts::read(&room)?;
    let existing = TaskFacts::read(&task)?;
    if existing.room_cycle != room_facts.cleaning_cycle {
        return Err(conflict(
            "PMS_CLEANING_STALE",
            "The room has a newer cleaning cycle",
        ));
    }
    let completed_by = optional_text(&task.data, "completedBy").unwrap_or_default();
    if existing.state != "clean" || completed_by.is_empty() || completed_by == actor.board.user_id {
        return Err(conflict(
            "PMS_INDEPENDENT_INSPECTOR",
            "A different supervisor must inspect after the task is complete",
        ));
    }
    object(&task.data)?;
    task.data["state"] = serde_json::Value::String("done".to_owned());
    task.data["inspectedBy"] = serde_json::Value::String(actor.board.user_id.clone());
    touch(&mut task, now);
    let mut room_touched = false;
    if !matches!(existing.kind.as_str(), "maintenance" | "discrepancy") {
        object(&room.data)?;
        room.data["cleanliness"] = serde_json::Value::String("inspected".to_owned());
        touch(&mut room, now);
        room_touched = true;
    }
    Ok((task, room_touched.then_some(room)))
}

/// Retained `task()` identity for a generated room task.
fn room_task_id(room_id: &str, kind: &str, date: &str, stay_id: &str, cycle: i64) -> String {
    use sha2::{Digest, Sha256};
    let digest =
        Sha256::digest(format!("pms-task:{room_id}:{kind}:{date}:{stay_id}:{cycle}").as_bytes());
    format!("pms-task-{}", hex::encode(&digest[..16]))
}

/// Retained `reconcileCapacity`: after a room changes, every room type must
/// still have nonnegative availability on every night any nonterminal stay or
/// active block touches.
fn reconcile_capacity(
    property: &PmsPropertyRow,
    room_types: &[PmsRecord],
    rooms: &[PmsRecord],
    stays: &[PmsRecord],
    blocks: &[PmsRecord],
    as_of: i64,
) -> Result<(), BusinessError> {
    if room_types.len() > MAX_ROOM_TYPES
        || rooms.len() > MAX_ROOMS
        || stays.len() > MAX_STAYS
        || blocks.len() > MAX_BLOCKS
    {
        return Err(read_limit(
            "The hotel has more capacity facts than this command can read.",
        ));
    }
    let business_date = property.business_date.as_str();
    let stays = stays
        .iter()
        .map(capacity_stay)
        .collect::<Result<Vec<_>, _>>()?;
    let blocks = blocks
        .iter()
        .map(capacity_block)
        .collect::<Result<Vec<_>, _>>()?;
    let mut evaluations = 0_usize;
    for room_type in room_types {
        if room_type.kind != "room_type" || room_type.id.is_empty() {
            return Err(record_invalid());
        }
        let mut dates: Vec<String> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for stay in &stays {
            for date in day_range(&stay.arrival, &stay.departure)? {
                if seen.insert(date.clone()) {
                    dates.push(date);
                }
            }
        }
        for block in blocks.iter().filter(|block| {
            block.state == "active" && (block.type_id == room_type.id || !block.room_id.is_empty())
        }) {
            for date in day_range(&block.from, &block.to)? {
                if seen.insert(date.clone()) {
                    dates.push(date);
                }
            }
        }
        for date in dates {
            evaluations += 1;
            if evaluations > MAX_CAPACITY_EVALUATIONS {
                return Err(read_limit(
                    "The hotel has more capacity nights than this command can read.",
                ));
            }
            if available(
                &room_type.id,
                &date,
                rooms,
                &stays,
                &blocks,
                as_of,
                business_date,
            ) < 0
            {
                // The retained engine reports the offending night
                // (`Not enough rooms on ${date}`); the shared command error
                // message is a static string, so the night is not interpolated.
                return Err(conflict("PMS_SOLD_OUT", "Not enough rooms on this date"));
            }
        }
    }
    Ok(())
}
