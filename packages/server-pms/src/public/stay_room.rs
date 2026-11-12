//! Native PMS `stay.assign`/`stay.move` command ports and the PostgreSQL owner
//! adapter. Composition supplies the current Account/Actor, Commercial
//! entitlement and the serializable unit; this module mints no authority.
pub use crate::adapters::stay_room_http::{
    PmsStayRoomHttpBackend, PmsStayRoomHttpCommandRequest, PmsStayRoomHttpContext,
    PmsStayRoomHttpFuture, PmsStayRoomHttpResponse, PmsStayRoomHttpTarget, pms_stay_room_router,
};
pub use crate::adapters::stay_room_postgres::{PgPmsStayRoomRepository, lock_stay_room_sources};
pub use crate::application::room_board::PmsBoardFuture;
pub use crate::application::stay_room::{
    PmsStayRoomEntitlementPort, PmsStayRoomRepository, pms_stay_room, stay_room_request_hash,
};
pub use crate::domain::stay_room::{
    MAX_BLOCKS, MAX_REASON, MAX_SEGMENTS, MAX_STAYS, MAX_TASKS, PmsStayRoomActor,
    PmsStayRoomCommand, PmsStayRoomEntitlement, PmsStayRoomFacts, PmsStayRoomOperation,
    PmsStayRoomPlan, PmsStayRoomResult, invalid as stay_room_invalid, plan_stay_room,
    read_limit as stay_room_read_limit, record_invalid as stay_room_record_invalid,
};
