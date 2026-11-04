//! Native Messaging sequencing with a caller-owned unit. PostgreSQL rooms,
//! messages, read state and Company isolation live in the integration target.
use erpx_server_collaboration::public::messaging_native::{
    Access, Change, HistoryCursor, MAX_BODY, MessageSnapshot, MessagingActor, MessagingUnit,
    OwnerPlan, Receipt, RoomKind, RoomSnapshot, execute, message_body, members, plan_receipt,
    room_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::{BTreeMap, BTreeSet};

struct Unit {
    actor: MessagingActor,
    replay: Option<Receipt>,
    rooms: BTreeMap<String, RoomSnapshot>,
    company_members: BTreeSet<String>,
    applied: Option<OwnerPlan>,
    drift: bool,
}
impl Unit {
    fn new() -> Self {
        Self {
            actor: MessagingActor {
                user_id: "user".into(),
                display_name: "User".into(),
                company_id: "company".into(),
            },
            replay: None,
            rooms: BTreeMap::new(),
            company_members: ["user", "person-1", "person-2"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            applied: None,
            drift: false,
        }
    }
}
impl MessagingUnit for Unit {
    type Error = BusinessError;
    async fn authorize(&mut self, _: Access) -> Result<MessagingActor, BusinessError> {
        Ok(self.actor.clone())
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, BusinessError> {
        Ok(self.replay.clone())
    }
    async fn room(&mut self, id: &str) -> Result<Option<RoomSnapshot>, BusinessError> {
        Ok(self.rooms.get(id).cloned())
    }
    async fn is_company_member(&mut self, user_id: &str) -> Result<bool, BusinessError> {
        Ok(self.company_members.contains(user_id))
    }
    async fn direct_room_id(&mut self, members: &[String]) -> Result<String, BusinessError> {
        Ok(format!("dm-{}", members.join("-")))
    }
    async fn apply(
        &mut self,
        _: &MessagingActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, BusinessError> {
        let receipt = plan_receipt(key, &plan)?;
        self.applied = Some(plan);
        Ok(receipt)
    }
    async fn flush_deferred(&mut self) -> Result<(), BusinessError> {
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), BusinessError> {
        if self.drift {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "MESSAGING_SOURCE_CHANGED",
                "changed",
            ));
        }
        Ok(())
    }
}
fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

#[test]
fn public_port_bounds_members_body_and_cursor() {
    assert_eq!(
        members("b", &["a".into(), "b".into()]).unwrap(),
        vec!["a".to_owned(), "b".to_owned()]
    );
    assert_eq!(message_body("  hello  ").unwrap(), "hello");
    assert!(message_body(&"x".repeat(MAX_BODY + 1)).is_err());
    assert_eq!(
        HistoryCursor::decode(&HistoryCursor { seq: 2 }.encode()).unwrap(),
        HistoryCursor { seq: 2 }
    );
    assert_eq!(
        RoomKind::parse("direct").unwrap(),
        RoomKind::Direct
    );
}

#[tokio::test]
async fn direct_rooms_add_the_actor_and_reuse_the_stable_key() {
    let mut unit = Unit::new();
    let receipt = execute(
        &mut unit,
        &key("messaging-room-0001"),
        Change::CreateRoom {
            kind: RoomKind::Direct,
            name: "ignored".into(),
            members: vec!["person-1".into(), "user".into()],
        },
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "dm-person-1-user");
    assert_eq!(receipt.status, "created");
    match unit.applied.as_ref().unwrap() {
        OwnerPlan::CreateRoom {
            members, name, kind, ..
        } => {
            assert_eq!(*kind, RoomKind::Direct);
            assert_eq!(members, &vec!["person-1".to_owned(), "user".to_owned()]);
            assert_eq!(name, "");
        }
        other => panic!("unexpected plan {other:?}"),
    }
    let existing = RoomSnapshot {
        id: receipt.id.clone(),
        company_id: "company".into(),
        kind: RoomKind::Direct,
        name: String::new(),
        owner_id: "user".into(),
        version: 1,
        last_seq: 3,
        members: vec!["person-1".into(), "user".into()],
        created_at: 1,
    };
    unit.rooms.insert(receipt.id.clone(), existing);
    unit.applied = None;
    let replay = execute(
        &mut unit,
        &key("messaging-room-0002"),
        Change::CreateRoom {
            kind: RoomKind::Direct,
            name: String::new(),
            members: vec!["person-1".into()],
        },
    )
    .await
    .unwrap();
    assert_eq!(replay.seq, 3);
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn direct_rooms_require_two_people_and_current_company_members() {
    let mut unit = Unit::new();
    let one = execute(
        &mut unit,
        &key("messaging-room-0003"),
        Change::CreateRoom {
            kind: RoomKind::Direct,
            name: String::new(),
            members: vec![],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(one.code, "MESSAGING_INPUT_INVALID");
    let outsider = execute(
        &mut unit,
        &key("messaging-room-0004"),
        Change::CreateRoom {
            kind: RoomKind::Direct,
            name: String::new(),
            members: vec!["outsider".into()],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(outsider.code, "MESSAGING_MEMBER_REQUIRED");
    assert!(unit.applied.is_none());
}

#[tokio::test]
async fn messages_increment_sequence_and_deny_non_members_before_apply() {
    let mut unit = Unit::new();
    unit.rooms.insert(
        "room-1".into(),
        RoomSnapshot {
            id: "room-1".into(),
            company_id: "company".into(),
            kind: RoomKind::Group,
            name: "Team".into(),
            owner_id: "user".into(),
            version: 1,
            last_seq: 4,
            members: vec!["user".into()],
            created_at: 1,
        },
    );
    let receipt = execute(
        &mut unit,
        &key("messaging-message-0001"),
        Change::SendMessage {
            room_id: "room-1".into(),
            body: "  hello  ".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(receipt.seq, 5);
    match unit.applied.as_ref().unwrap() {
        OwnerPlan::SendMessage { seq, body, .. } => {
            assert_eq!(*seq, 5);
            assert_eq!(body, "hello");
        }
        other => panic!("unexpected plan {other:?}"),
    }
    unit.rooms.get_mut("room-1").unwrap().members.clear();
    unit.applied = None;
    let denied = execute(
        &mut unit,
        &key("messaging-message-0002"),
        Change::SendMessage {
            room_id: "room-1".into(),
            body: "hello".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "ROOM_MEMBER_REQUIRED");
    assert!(unit.applied.is_none());
    let missing = execute(
        &mut unit,
        &key("messaging-message-0003"),
        Change::SendMessage {
            room_id: "absent".into(),
            body: "hello".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, "ROOM_NOT_FOUND");
    assert!(unit.applied.is_none());
    assert_eq!(room_not_found().code, "ROOM_NOT_FOUND");
}

#[tokio::test]
async fn read_state_clamps_to_the_current_last_sequence_and_replay_skips_apply() {
    let mut unit = Unit::new();
    unit.rooms.insert(
        "room-1".into(),
        RoomSnapshot {
            id: "room-1".into(),
            company_id: "company".into(),
            kind: RoomKind::Group,
            name: "Team".into(),
            owner_id: "user".into(),
            version: 1,
            last_seq: 3,
            members: vec!["user".into()],
            created_at: 1,
        },
    );
    let read = execute(
        &mut unit,
        &key("messaging-read-0001"),
        Change::MarkRead {
            room_id: "room-1".into(),
            seq: 99,
        },
    )
    .await
    .unwrap();
    assert_eq!(read.seq, 3);
    assert!(matches!(
        unit.applied,
        Some(OwnerPlan::MarkRead { seq: 3, .. })
    ));
    unit.replay = Some(Receipt {
        id: "room-1".into(),
        version: 1,
        status: "read",
        seq: 2,
    });
    unit.applied = None;
    let replay = execute(
        &mut unit,
        &key("messaging-read-0002"),
        Change::MarkRead {
            room_id: "room-1".into(),
            seq: 3,
        },
    )
    .await
    .unwrap();
    assert_eq!(replay.seq, 2);
    assert!(unit.applied.is_none());
    assert!(MessageSnapshot {
        id: "m".into(),
        room_id: "room-1".into(),
        seq: 1,
        sender_id: "user".into(),
        sender_name: "User".into(),
        body: "hello".into(),
        created_at: 1,
    }
    .body
    .eq("hello"));
}
