//! Messaging HTTP/command JSON normalization. No membership or team-owner
//! authority is granted here.
use crate::domain::messaging_native::{Change as RoomChange, RoomKind};
use crate::domain::messaging_teams::Change as TeamChange;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MESSAGING_INPUT_INVALID",
        "Provide a valid conversation value.",
    )
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)
}

fn boolean(value: &Value, key: &str) -> Result<bool, BusinessError> {
    value.get(key).and_then(Value::as_bool).ok_or_else(invalid)
}

fn unsigned(value: &Value, key: &str) -> Result<i64, BusinessError> {
    let n = value.get(key).and_then(Value::as_f64).ok_or_else(invalid)?;
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err(invalid());
    }
    Ok(n as i64)
}

fn members(value: &Value) -> Result<Vec<String>, BusinessError> {
    match value.get("members") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect(),
        _ => Err(invalid()),
    }
}

fn name(value: &Value) -> Result<String, BusinessError> {
    value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(invalid)
}

pub enum DecodedMessagingCommand {
    Room(RoomChange),
    Team(TeamChange),
}

/// `operation` is selected by the registered route; the room id comes from the
/// path for message/read commands.
pub fn decode_room_command(
    operation: &str,
    room_id: Option<&str>,
    raw: &Value,
) -> Result<DecodedMessagingCommand, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    let room = || room_id.map(str::to_owned).ok_or_else(invalid);
    let change = match operation {
        "messaging.room.create" => {
            let kind = RoomKind::parse(text(raw, "kind")?)?;
            if kind == RoomKind::Channel {
                return Err(invalid());
            }
            RoomChange::CreateRoom {
                kind,
                name: name(raw)?,
                members: members(raw)?,
            }
        }
        "messaging.send" => RoomChange::SendMessage {
            room_id: room()?,
            body: text(raw, "body")?.to_owned(),
        },
        "messaging.read" => RoomChange::MarkRead {
            room_id: room()?,
            seq: unsigned(raw, "seq")?,
        },
        "messaging.manage" => RoomChange::ManageRoom {
            room_id: room()?,
            name: name(raw)?,
            members: members(raw)?,
            expected_version: unsigned(raw, "expectedVersion")?,
        },
        _ => return Err(invalid()),
    };
    Ok(DecodedMessagingCommand::Room(change))
}

pub fn decode_team_command(
    operation: &str,
    team_id: Option<&str>,
    raw: &Value,
) -> Result<DecodedMessagingCommand, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    let change = match operation {
        "messaging.team.create" => TeamChange::CreateTeam {
            name: name(raw)?,
            members: members(raw)?,
        },
        "messaging.channel.create" => TeamChange::CreateChannel {
            team_id: team_id.map(str::to_owned).ok_or_else(invalid)?,
            name: name(raw)?,
            private: boolean(raw, "private")?,
            members: members(raw)?,
        },
        "messaging.team.manage" => TeamChange::ManageTeam {
            team_id: team_id.map(str::to_owned).ok_or_else(invalid)?,
            name: name(raw)?,
            members: members(raw)?,
            expected_version: unsigned(raw, "expectedVersion")?,
        },
        _ => return Err(invalid()),
    };
    Ok(DecodedMessagingCommand::Team(change))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn room_commands_select_the_path_room_and_keep_bounds_for_the_owner() {
        let decoded =
            decode_room_command("messaging.send", Some("room-1"), &json!({"body":" Hello "}))
                .unwrap();
        match decoded {
            DecodedMessagingCommand::Room(RoomChange::SendMessage { room_id, body }) => {
                assert_eq!(room_id, "room-1");
                assert_eq!(body, " Hello ");
            }
            _ => panic!("unexpected decode"),
        }
        let decoded = decode_room_command(
            "messaging.room.create",
            None,
            &json!({"kind":"group","name":"Team","members":["u1"]}),
        )
        .unwrap();
        assert!(matches!(
            decoded,
            DecodedMessagingCommand::Room(RoomChange::CreateRoom {
                kind: RoomKind::Group,
                ..
            })
        ));
    }

    #[test]
    fn team_and_channel_commands_require_the_expected_fields() {
        let decoded = decode_team_command(
            "messaging.channel.create",
            Some("team-1"),
            &json!({"name":"Ops","private":true,"members":["u1"]}),
        )
        .unwrap();
        assert!(matches!(
            decoded,
            DecodedMessagingCommand::Team(TeamChange::CreateChannel { private: true, .. })
        ));
        for (operation, team, value) in [
            (
                "messaging.channel.create",
                None,
                json!({"name":"Ops","private":true}),
            ),
            ("messaging.channel.create", Some("t"), json!({"name":"Ops"})),
            ("messaging.team.create", None, json!({"members":[]})),
            ("messaging.read", Some("r"), json!({"seq":-1})),
            ("bogus", None, json!({})),
        ] {
            assert!(
                decode_team_command(operation, team, &value)
                    .or_else(|_| decode_room_command(operation, team, &value))
                    .is_err()
            );
        }
    }

    #[test]
    fn manage_commands_require_the_owner_version_and_member_list() {
        let decoded = decode_room_command(
            "messaging.manage",
            Some("room-1"),
            &json!({"name":"Ops","members":["u1","u2"],"expectedVersion":3}),
        )
        .unwrap();
        match decoded {
            DecodedMessagingCommand::Room(RoomChange::ManageRoom {
                room_id,
                name,
                members,
                expected_version,
            }) => {
                assert_eq!(room_id, "room-1");
                assert_eq!(name, "Ops");
                assert_eq!(members, vec!["u1".to_owned(), "u2".to_owned()]);
                assert_eq!(expected_version, 3);
            }
            _ => panic!("unexpected decode"),
        }
        let decoded = decode_team_command(
            "messaging.team.manage",
            Some("team-1"),
            &json!({"name":"Ops","members":["u1"],"expectedVersion":2}),
        )
        .unwrap();
        assert!(matches!(
            decoded,
            DecodedMessagingCommand::Team(TeamChange::ManageTeam {
                expected_version: 2,
                ..
            })
        ));
        assert!(
            decode_room_command(
                "messaging.manage",
                Some("x"),
                &json!({"name":"Ops","members":[]})
            )
            .is_err()
        );
        assert!(
            decode_team_command(
                "messaging.team.manage",
                Some("x"),
                &json!({"name":"Ops","members":[],"expectedVersion":-1})
            )
            .is_err()
        );
    }
}
