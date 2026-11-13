//! Conversation command sequencing on one caller-owned owner unit.
use crate::domain::messaging_native::{
    Change, MessagingActor, OwnerPlan, Receipt, RoomKind, RoomSnapshot, changed, message_body,
    members, next_seq, owner_required, plan_receipt, require_member, room_name, room_not_found,
    validate_seq,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}

pub trait MessagingUnit: Send {
    type Error: From<BusinessError> + Send;
    fn authorize(
        &mut self,
        access: Access,
    ) -> impl Future<Output = Result<MessagingActor, Self::Error>> + Send;
    fn replay(
        &mut self,
        change: &Change,
    ) -> impl Future<Output = Result<Option<Receipt>, Self::Error>> + Send;
    fn room(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<RoomSnapshot>, Self::Error>> + Send;
    fn is_company_member(
        &mut self,
        user_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Composition computes the original stable direct-conversation key.
    fn direct_room_id(
        &mut self,
        members: &[String],
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
    fn apply(
        &mut self,
        actor: &MessagingActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> impl Future<Output = Result<Receipt, Self::Error>> + Send;
    fn flush_deferred(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn require_unit_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn invalid() -> BusinessError {
    BusinessError::invalid("MESSAGING_INPUT_INVALID", "Provide a valid conversation value.")
}

fn participant_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MESSAGING_MEMBER_REQUIRED",
        "Choose current Company members for this conversation.",
    )
}

pub async fn execute<P: MessagingUnit>(
    p: &mut P,
    key: &CommandKey,
    change: Change,
) -> Result<Receipt, P::Error> {
    let actor = p.authorize(Access::Write).await?;
    let prior = p.replay(&change).await?;
    let mut plan = None;
    if prior.is_none() {
        match change {
            Change::CreateRoom {
                kind,
                name,
                members: requested,
            } => {
                let members = members(&actor.user_id, &requested)?;
                for member in &members {
                    if !p.is_company_member(member).await? {
                        return Err(participant_required().into());
                    }
                }
                if kind == crate::domain::messaging_native::RoomKind::Direct && members.len() != 2 {
                    return Err(invalid().into());
                }
                let name = room_name(&name, kind)?;
                let id = if kind == crate::domain::messaging_native::RoomKind::Direct {
                    p.direct_room_id(&members).await?
                } else {
                    format!("room-{}", key.as_str())
                };
                if let Some(existing) = p.room(&id).await? {
                    require_member(&actor, &existing)?;
                    return Ok(Receipt {
                        id,
                        version: existing.version,
                        status: "created",
                        seq: existing.last_seq,
                    });
                }
                plan = Some(OwnerPlan::CreateRoom {
                    id,
                    company_id: actor.company_id.clone(),
                    kind,
                    name,
                    owner_id: actor.user_id.clone(),
                    members,
                });
            }
            Change::SendMessage { room_id, body } => {
                let room = p.room(&room_id).await?.ok_or_else(room_not_found)?;
                require_member(&actor, &room)?;
                let body = message_body(&body)?;
                let seq = next_seq(room.last_seq)?;
                plan = Some(OwnerPlan::SendMessage {
                    id: crate::domain::messaging_native::message_id(key),
                    room_id,
                    seq,
                    sender_id: actor.user_id.clone(),
                    sender_name: actor.display_name.clone(),
                    body,
                });
            }
            Change::MarkRead { room_id, seq } => {
                validate_seq(seq)?;
                let room = p.room(&room_id).await?.ok_or_else(room_not_found)?;
                require_member(&actor, &room)?;
                plan = Some(OwnerPlan::MarkRead {
                    room_id,
                    user_id: actor.user_id.clone(),
                    seq: seq.min(room.last_seq),
                });
            }
            Change::ManageRoom {
                room_id,
                name,
                members: requested,
                expected_version,
            } => {
                let room = p.room(&room_id).await?.ok_or_else(room_not_found)?;
                require_member(&actor, &room)?;
                if room.owner_id != actor.user_id || room.kind == RoomKind::Direct {
                    return Err(owner_required().into());
                }
                if expected_version <= 0 || room.version != expected_version {
                    return Err(changed().into());
                }
                let members = members(&actor.user_id, &requested)?;
                for member in &members {
                    if !p.is_company_member(member).await? {
                        return Err(participant_required().into());
                    }
                }
                let name = room_name(&name, room.kind)?;
                let reconcile =
                    room.kind == RoomKind::Group || (room.kind == RoomKind::Channel && room.private);
                plan = Some(OwnerPlan::ManageRoom {
                    room_id,
                    company_id: actor.company_id.clone(),
                    name,
                    owner_id: actor.user_id.clone(),
                    expected_version,
                    members: reconcile.then_some(members),
                });
            }
        }
    }
    let result = match prior {
        Some(receipt) => receipt,
        None => {
            let plan = plan.ok_or_else(invalid)?;
            plan_receipt(key, &plan)?;
            p.apply(&actor, key, plan).await?
        }
    };
    p.flush_deferred().await?;
    p.require_unit_current().await?;
    Ok(result)
}

pub async fn authorize_read<P: MessagingUnit>(p: &mut P) -> Result<MessagingActor, P::Error> {
    let actor = p.authorize(Access::Read).await?;
    if actor.company_id.is_empty() {
        return Err(room_not_found().into());
    }
    Ok(actor)
}
