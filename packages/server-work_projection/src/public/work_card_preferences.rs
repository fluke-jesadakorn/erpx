//! Native Work Projection card-preference read and save command.
pub use crate::adapters::work_card_preferences::{
    WorkCardPreferenceEffects, execute_work_card_preferences, read_work_card_preferences,
};
pub use crate::domain::work_card_preferences::{
    MAX_CARD_ID, MAX_PINNED, SaveWorkCardPreferences, WorkCardPreferences, invalid,
    member_required, parse_pinned, pinned_json, result_wire,
};
