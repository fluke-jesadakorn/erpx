//! Messaging teams and channels. Current Company membership and team ownership
//! remain composition facts; this module only sequences stored team/channel
//! rows and never grants conversation authority.
use crate::domain::messaging_native::{MAX_MEMBERS, MessagingActor, js_trim, members, utf16_len};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSnapshot {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub owner_id: String,
    pub version: i64,
    pub members: Vec<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelSnapshot {
    pub id: String,
    pub company_id: String,
    pub team_id: String,
    pub name: String,
    pub owner_id: String,
    pub version: i64,
    pub last_seq: i64,
    pub private: bool,
    pub members: Vec<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub id: String,
    pub version: i64,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    CreateTeam {
        name: String,
        members: Vec<String>,
    },
    CreateChannel {
        team_id: String,
        name: String,
        private: bool,
        members: Vec<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerPlan {
    CreateTeam {
        id: String,
        general_room_id: String,
        company_id: String,
        name: String,
        owner_id: String,
        members: Vec<String>,
    },
    CreateChannel {
        id: String,
        company_id: String,
        team_id: String,
        name: String,
        owner_id: String,
        private: bool,
        members: Vec<String>,
    },
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MESSAGING_INPUT_INVALID",
        "Provide a valid team or channel value.",
    )
}

pub fn team_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TEAM_NOT_FOUND",
        "The team is unavailable in this Company.",
    )
}

pub fn team_owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "OWNER_REQUIRED",
        "Only the team owner can add a channel.",
    )
}

pub fn team_name(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value);
    if value.is_empty() || utf16_len(value) > 120 {
        return Err(invalid());
    }
    Ok(value.into())
}

pub fn channel_name(value: &str) -> Result<String, BusinessError> {
    team_name(value)
}

pub fn team_members(actor: &str, requested: &[String]) -> Result<Vec<String>, BusinessError> {
    members(actor, requested)
}

pub fn require_team_owner(
    actor: &MessagingActor,
    team: &TeamSnapshot,
) -> Result<(), BusinessError> {
    if team.company_id != actor.company_id {
        return Err(team_not_found());
    }
    if team.owner_id != actor.user_id {
        return Err(team_owner_required());
    }
    Ok(())
}

pub fn plan_receipt(_key: &CommandKey, plan: &OwnerPlan) -> Result<Receipt, BusinessError> {
    Ok(match plan {
        OwnerPlan::CreateTeam { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "created",
        },
        OwnerPlan::CreateChannel { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "created",
        },
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelCursor {
    pub created_at: i64,
    pub id: String,
}
impl ChannelCursor {
    pub fn encode(&self) -> String {
        format!("{}:{}", self.created_at, self.id)
    }
    pub fn decode(value: &str) -> Result<Self, BusinessError> {
        let (at, id) = value.split_once(':').ok_or_else(channel_cursor_invalid)?;
        let created_at = at.parse::<i64>().map_err(|_| channel_cursor_invalid())?;
        if !(0..=9_007_199_254_740_991).contains(&created_at) || id.is_empty() || id.len() > 512 {
            return Err(channel_cursor_invalid());
        }
        Ok(Self {
            created_at,
            id: id.into(),
        })
    }
}
fn channel_cursor_invalid() -> BusinessError {
    BusinessError::invalid("INVALID_CURSOR", "Restart the channel list.")
}

pub const MAX_CHANNEL_MEMBERS: usize = MAX_MEMBERS;

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> MessagingActor {
        MessagingActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
        }
    }

    fn team() -> TeamSnapshot {
        TeamSnapshot {
            id: "team-1".into(),
            company_id: "company".into(),
            name: "Team".into(),
            owner_id: "user".into(),
            version: 1,
            members: vec!["user".into()],
            created_at: 1,
        }
    }

    #[test]
    fn names_and_members_are_bounded_and_normalized() {
        assert_eq!(team_name("  Team  ").unwrap(), "Team");
        assert!(team_name("   ").is_err());
        assert!(team_name(&"x".repeat(121)).is_err());
        assert_eq!(
            team_members("user", &["b".into(), "a".into()]).unwrap(),
            vec!["a".to_owned(), "b".to_owned(), "user".to_owned()]
        );
    }

    #[test]
    fn only_a_current_team_owner_can_add_channels() {
        let actor = actor();
        assert!(require_team_owner(&actor, &team()).is_ok());
        let mut other = team();
        other.owner_id = "other".into();
        assert_eq!(
            require_team_owner(&actor, &other).unwrap_err().code,
            "OWNER_REQUIRED"
        );
        let mut foreign = team();
        foreign.company_id = "other".into();
        assert_eq!(
            require_team_owner(&actor, &foreign).unwrap_err().code,
            "TEAM_NOT_FOUND"
        );
    }

    #[test]
    fn receipts_and_cursor_round_trip() {
        let key = CommandKey::new("team-key-0001").unwrap();
        let plan = OwnerPlan::CreateTeam {
            id: "team-team-key-0001".into(),
            general_room_id: "general-team-key-0001".into(),
            company_id: "company".into(),
            name: "Team".into(),
            owner_id: "user".into(),
            members: vec!["user".into()],
        };
        assert_eq!(plan_receipt(&key, &plan).unwrap().status, "created");
        let cursor = ChannelCursor {
            created_at: 1_700_000_000_000,
            id: "channel-1".into(),
        };
        assert_eq!(ChannelCursor::decode(&cursor.encode()).unwrap(), cursor);
        assert_eq!(
            ChannelCursor::decode("x").unwrap_err().code,
            "INVALID_CURSOR"
        );
    }
}
