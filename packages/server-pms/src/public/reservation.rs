//! Native PMS staff reservation command ports and the PostgreSQL owner
//! adapter. Composition supplies the current Account/assignment authority and
//! the serializable unit; this module mints no authority. Public booking,
//! payment providers, email delivery and Finance posting are not exported.
pub use crate::adapters::reservation_http::{
    PmsReservationHttpBackend, PmsReservationHttpCommandRequest, PmsReservationHttpContext,
    PmsReservationHttpFuture, PmsReservationHttpResponse, PmsReservationHttpTarget,
    pms_reservation_router,
};
pub use crate::adapters::reservation_postgres::{
    PgPmsReservationRepository, lock_reservation_sources,
};
pub use crate::application::reservation::{
    PmsReservationRepository, pms_reservation, reservation_request_hash,
};
pub use crate::application::room_board::PmsBoardFuture;
pub use crate::domain::reservation::{
    MAX_ACCOUNTS, MAX_BLOCKS, MAX_CAMPAIGN, MAX_CHILD_AGES, MAX_GUEST_EMAIL, MAX_GUEST_NAME,
    MAX_GUEST_PHONE, MAX_ID, MAX_LINES, MAX_NIGHTS, MAX_RATES, MAX_REASON, MAX_ROOM_TYPES,
    MAX_ROOMS, MAX_STAYS, MAX_TEXT, PmsReservationActor, PmsReservationCancelInput,
    PmsReservationChangeInput, PmsReservationCommand, PmsReservationCommandInput,
    PmsReservationCreateInput, PmsReservationFacts, PmsReservationGuestInput, PmsReservationLine,
    PmsReservationOperation, PmsReservationPlan, PmsReservationResult, invalid,
    plan_reservation_command, read_limit, record_invalid,
};
