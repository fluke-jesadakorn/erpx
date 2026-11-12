//! Native PMS room/housekeeping command ports and the PostgreSQL owner adapter.
//! Composition supplies the current Account/assignment authority, the People
//! assignment scope and the serializable unit; this module mints no authority.
pub use crate::adapters::room_command_http::{
    PmsRoomCommandHttpBackend, PmsRoomCommandHttpCommandRequest, PmsRoomCommandHttpContext,
    PmsRoomCommandHttpFuture, PmsRoomCommandHttpResponse, PmsRoomCommandHttpTarget,
    pms_room_command_router,
};
pub use crate::adapters::room_command_postgres::{
    PgPmsRoomCommandRepository, lock_room_command_sources,
};
pub use crate::application::room_board::PmsBoardFuture;
pub use crate::application::room_command::{
    PmsRoomAssigneeScopePort, PmsRoomCommandRepository, pms_room_command, room_command_request_hash,
};
pub use crate::domain::room_command::{
    MAX_BLOCKS, MAX_CHECKED, MAX_ID, MAX_NIGHTS, MAX_ROOM_TYPES, MAX_ROOMS, MAX_STAYS, MAX_TASKS,
    MAX_TEXT, PmsBlockCreateInput, PmsMaintenanceOpenInput, PmsRoomCommand, PmsRoomCommandActor,
    PmsRoomCommandFacts, PmsRoomCommandInput, PmsRoomCommandOperation, PmsRoomCommandPlan,
    PmsRoomCommandResult, PmsRoomSaveInput, PmsRoomTypeSaveInput, PmsTaskCreateInput, PmsTaskKind,
    PmsTaskPriority, invalid as room_command_invalid, plan_room_command,
    read_limit as room_command_read_limit, record_invalid as room_command_record_invalid,
};
