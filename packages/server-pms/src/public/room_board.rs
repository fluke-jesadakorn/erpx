//! Native PMS room-board read ports and the PostgreSQL owner adapter.
pub use crate::adapters::room_board::{
    PgPmsBoardRead, lock_room_board_sources, read_room_board, read_room_properties,
};
pub use crate::application::room_board::{
    PmsBoardEntitlementPort, PmsBoardFuture, PmsBoardRepository, PmsStaffDirectory, pms_room_board,
};
pub use crate::domain::room_board::{
    PAGE_SIZE, PMS_PERMISSIONS, PmsBoardActor, PmsBoardData, PmsBoardEntitlement, PmsBoardSection,
    PmsBoardSelection, PmsBoardSetup, PmsBoardVisibility, PmsBookingReadiness, PmsListMeta,
    PmsProperty, PmsPropertyRow, PmsPropertySummary, PmsRecord, PmsRoomSummary, PmsSnapshot,
    PmsStaff, PmsWorkspace, booking_readiness, default_date, not_found, read_invalid, read_limit,
    readiness, readiness_from_facts, record_not_found, redact_config, staff, visible_records,
};
