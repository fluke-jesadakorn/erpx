//! Direct/group conversation decisions. Current Company membership and room
//! membership remain composition/owner facts; this module only sequences the
//! stored conversation and never grants message or participant authority.
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};

pub const MAX_SAFE: i64 = 9_007_199_254_740_991;
pub const MAX_MEMBERS: usize = 100;
pub const MAX_BODY: usize = 5000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomKind {
    Direct,
    Group,
    /// A room owned by a team. Team membership and ownership come from the
    /// `messaging_teams` owner rows, never from this value.
    Channel,
}
impl RoomKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "direct" => Ok(Self::Direct),
            "group" => Ok(Self::Group),
            "channel" => Ok(Self::Channel),
            _ => Err(invalid()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
            Self::Channel => "channel",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingActor {
    pub user_id: String,
    pub display_name: String,
    pub company_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomSnapshot {
    pub id: String,
    pub company_id: String,
    pub kind: RoomKind,
    pub name: String,
    pub owner_id: String,
    pub version: i64,
    pub last_seq: i64,
    pub members: Vec<String>,
    /// True only for a private team channel; group/direct rooms are explicit.
    pub private: bool,
    pub team_id: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageSnapshot {
    pub id: String,
    pub room_id: String,
    pub seq: i64,
    pub sender_id: String,
    pub sender_name: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub id: String,
    pub version: i64,
    pub status: &'static str,
    pub seq: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    CreateRoom {
        kind: RoomKind,
        name: String,
        members: Vec<String>,
    },
    SendMessage {
        room_id: String,
        body: String,
    },
    MarkRead {
        room_id: String,
        seq: i64,
    },
    ManageRoom {
        room_id: String,
        name: String,
        members: Vec<String>,
        expected_version: i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerPlan {
    CreateRoom {
        id: String,
        company_id: String,
        kind: RoomKind,
        name: String,
        owner_id: String,
        members: Vec<String>,
    },
    SendMessage {
        id: String,
        room_id: String,
        seq: i64,
        sender_id: String,
        sender_name: String,
        body: String,
    },
    MarkRead {
        room_id: String,
        user_id: String,
        seq: i64,
    },
    ManageRoom {
        room_id: String,
        company_id: String,
        name: String,
        owner_id: String,
        expected_version: i64,
        /// `Some` records explicit room membership (group/private channel);
        /// `None` leaves public channel membership on the owning team.
        members: Option<Vec<String>>,
    },
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MESSAGING_INPUT_INVALID",
        "Provide a valid conversation value.",
    )
}

pub fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_SOURCE_CHANGED",
        "The conversation changed. Refresh and retry.",
    )
}

pub fn room_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ROOM_NOT_FOUND",
        "The conversation is unavailable in this Company.",
    )
}

pub fn user_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ROOM_MEMBER_REQUIRED",
        "Current conversation membership is required.",
    )
}

pub fn owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "OWNER_REQUIRED",
        "Only the owner can manage this conversation.",
    )
}

pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

pub fn members(actor: &str, requested: &[String]) -> Result<Vec<String>, BusinessError> {
    let mut unique = Vec::new();
    for member in std::iter::once(actor.to_owned()).chain(requested.iter().cloned()) {
        let member = js_trim(&member).to_owned();
        if member.is_empty() || utf16_len(&member) > 160 {
            return Err(invalid());
        }
        if !unique.contains(&member) {
            unique.push(member);
        }
    }
    if unique.len() > MAX_MEMBERS {
        return Err(invalid());
    }
    unique.sort();
    Ok(unique)
}

pub fn room_name(value: &str, kind: RoomKind) -> Result<String, BusinessError> {
    let value = js_trim(value);
    if kind == RoomKind::Direct {
        return Ok(String::new());
    }
    if utf16_len(value) > 120 {
        return Err(invalid());
    }
    Ok(value.into())
}

pub fn message_body(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value);
    if value.is_empty() || utf16_len(value) > MAX_BODY {
        return Err(invalid());
    }
    Ok(value.into())
}

pub fn validate_seq(value: i64) -> Result<(), BusinessError> {
    if !(0..=MAX_SAFE).contains(&value) {
        return Err(invalid());
    }
    Ok(())
}

pub fn require_member(actor: &MessagingActor, room: &RoomSnapshot) -> Result<(), BusinessError> {
    if room.company_id != actor.company_id || !room.members.contains(&actor.user_id) {
        return Err(user_required());
    }
    Ok(())
}

pub fn next_seq(last_seq: i64) -> Result<i64, BusinessError> {
    last_seq
        .checked_add(1)
        .filter(|next| last_seq >= 0 && *next <= MAX_SAFE)
        .ok_or_else(changed)
}

pub fn plan_receipt(_key: &CommandKey, plan: &OwnerPlan) -> Result<Receipt, BusinessError> {
    Ok(match plan {
        OwnerPlan::CreateRoom { id, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "created",
            seq: 0,
        },
        OwnerPlan::SendMessage { id, seq, .. } => Receipt {
            id: id.clone(),
            version: 1,
            status: "sent",
            seq: *seq,
        },
        OwnerPlan::MarkRead { room_id, seq, .. } => Receipt {
            id: room_id.clone(),
            version: 1,
            status: "read",
            seq: *seq,
        },
        OwnerPlan::ManageRoom {
            room_id,
            expected_version,
            ..
        } => Receipt {
            id: room_id.clone(),
            version: expected_version + 1,
            status: "updated",
            seq: 0,
        },
    })
}

pub fn message_id(key: &CommandKey) -> String {
    format!("message-{}", key.as_str())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryCursor {
    pub seq: i64,
}
impl HistoryCursor {
    pub fn encode(&self) -> String {
        self.seq.to_string()
    }
    pub fn decode(value: &str) -> Result<Self, BusinessError> {
        let seq = value.parse::<i64>().map_err(|_| invalid())?;
        validate_seq(seq)?;
        Ok(Self { seq })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_include_the_actor_dedupe_sort_and_bound() {
        assert_eq!(
            members("b", &["a".into(), "b".into(), "a".into()]).unwrap(),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert!(members("", &[]).is_err());
        assert!(members("a", &[" ".into()]).is_err());
        let many = (0..=MAX_MEMBERS)
            .map(|i| format!("u{i}"))
            .collect::<Vec<_>>();
        assert!(members("a", &many).is_err());
    }

    #[test]
    fn body_name_and_sequence_are_bounded() {
        assert_eq!(message_body("  hello  ").unwrap(), "hello");
        assert!(message_body("   ").is_err());
        assert!(message_body(&"x".repeat(MAX_BODY + 1)).is_err());
        assert_eq!(room_name("  Team  ", RoomKind::Group).unwrap(), "Team");
        assert_eq!(room_name("ignored", RoomKind::Direct).unwrap(), "");
        assert!(validate_seq(-1).is_err());
        assert_eq!(
            next_seq(MAX_SAFE).unwrap_err().code,
            "MESSAGING_SOURCE_CHANGED"
        );
    }

    #[test]
    fn room_membership_and_company_are_required() {
        let actor = MessagingActor {
            user_id: "user".into(),
            display_name: "User".into(),
            company_id: "company".into(),
        };
        let mut room = RoomSnapshot {
            id: "room".into(),
            company_id: "company".into(),
            kind: RoomKind::Group,
            name: "Team".into(),
            owner_id: "user".into(),
            version: 1,
            last_seq: 0,
            members: vec!["user".into()],
            private: false,
            team_id: None,
            created_at: 1,
        };
        assert!(require_member(&actor, &room).is_ok());
        room.members.clear();
        assert_eq!(
            require_member(&actor, &room).unwrap_err().code,
            "ROOM_MEMBER_REQUIRED"
        );
        room.members.push("user".into());
        room.company_id = "other".into();
        assert_eq!(
            require_member(&actor, &room).unwrap_err().code,
            "ROOM_MEMBER_REQUIRED"
        );
    }

    #[test]
    fn cursor_round_trips() {
        assert_eq!(
            HistoryCursor::decode(&HistoryCursor { seq: 12 }.encode()).unwrap(),
            HistoryCursor { seq: 12 }
        );
        assert!(HistoryCursor::decode("-1").is_err());
        assert!(HistoryCursor::decode("x").is_err());
    }
}
