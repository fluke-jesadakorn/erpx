//! Messaging teams/channels owner port. No Company membership authority here.
pub const CONTRACT_VERSION: &str = "collaboration.messaging.teams.v1";

pub use crate::adapters::messaging_teams::{
    ChannelView, CurrentTeamEffects, apply_plan, lock_messaging_team_sources, read_channel,
    read_team, read_team_channels, retain_effects,
};
pub use crate::application::messaging_teams::{Access, MessagingTeamUnit, authorize_read, execute};
pub use crate::domain::messaging_teams::{
    Change, ChannelCursor, ChannelSnapshot, MAX_CHANNEL_MEMBERS, OwnerPlan, Receipt, TeamSnapshot,
    channel_name, plan_receipt, require_team_owner, team_members, team_name, team_not_found,
    team_owner_required,
};
