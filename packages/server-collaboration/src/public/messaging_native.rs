//! Native Messaging owner port. No commit or Company-membership authority here.
pub const CONTRACT_VERSION: &str = "collaboration.messaging.v1";

pub use crate::adapters::messaging_native::{
    CurrentMessagingEffects, MessageRow, RoomView, apply_plan, lock_messaging_sources,
    read_history, read_message, read_overview, read_room, retain_effects,
};
pub use crate::application::messaging_native::{Access, MessagingUnit, authorize_read, execute};
pub use crate::domain::messaging_native::{
    Change, HistoryCursor, MAX_BODY, MAX_MEMBERS, MessageSnapshot, MessagingActor, OwnerPlan,
    Receipt, RoomKind, RoomSnapshot, members, message_body, message_id, plan_receipt,
    require_member, room_name, room_not_found, validate_seq,
};
