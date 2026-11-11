//! Team/channel command sequencing on one caller-owned owner unit.
use crate::domain::messaging_native::MessagingActor;
use crate::domain::messaging_teams::{
    Change, OwnerPlan, Receipt, TeamSnapshot, channel_name, plan_receipt, require_team_owner,
    team_members, team_name, team_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}

pub trait MessagingTeamUnit: Send {
    type Error: From<BusinessError> + Send;
    fn authorize(
        &mut self,
        access: Access,
    ) -> impl Future<Output = Result<MessagingActor, Self::Error>> + Send;
    fn replay(
        &mut self,
        change: &Change,
    ) -> impl Future<Output = Result<Option<Receipt>, Self::Error>> + Send;
    fn team(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<TeamSnapshot>, Self::Error>> + Send;
    fn is_company_member(
        &mut self,
        user_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn apply(
        &mut self,
        actor: &MessagingActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> impl Future<Output = Result<Receipt, Self::Error>> + Send;
    fn flush_deferred(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn require_unit_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn participant_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MESSAGING_MEMBER_REQUIRED",
        "Choose current Company members for this team.",
    )
}

pub async fn execute<P: MessagingTeamUnit>(
    p: &mut P,
    key: &CommandKey,
    change: Change,
) -> Result<Receipt, P::Error> {
    let actor = p.authorize(Access::Write).await?;
    let prior = p.replay(&change).await?;
    let mut plan = None;
    if prior.is_none() {
        match change {
            Change::CreateTeam {
                name,
                members: requested,
            } => {
                let members = team_members(&actor.user_id, &requested)?;
                for member in &members {
                    if !p.is_company_member(member).await? {
                        return Err(participant_required().into());
                    }
                }
                let name = team_name(&name)?;
                let suffix = key.as_str();
                plan = Some(OwnerPlan::CreateTeam {
                    id: format!("team-{suffix}"),
                    general_room_id: format!("general-{suffix}"),
                    company_id: actor.company_id.clone(),
                    name,
                    owner_id: actor.user_id.clone(),
                    members,
                });
            }
            Change::CreateChannel {
                team_id,
                name,
                private,
                members: requested,
            } => {
                let team = p.team(&team_id).await?.ok_or_else(team_not_found)?;
                require_team_owner(&actor, &team)?;
                let members = team_members(&actor.user_id, &requested)?;
                for member in &members {
                    if !p.is_company_member(member).await? {
                        return Err(participant_required().into());
                    }
                }
                let name = channel_name(&name)?;
                plan = Some(OwnerPlan::CreateChannel {
                    id: format!("channel-{}", key.as_str()),
                    company_id: actor.company_id.clone(),
                    team_id,
                    name,
                    owner_id: actor.user_id.clone(),
                    private,
                    members,
                });
            }
        }
    }
    let result = match prior {
        Some(receipt) => receipt,
        None => {
            let plan = plan.ok_or_else(|| {
                BusinessError::invalid(
                    "MESSAGING_INPUT_INVALID",
                    "Provide a valid team or channel value.",
                )
            })?;
            plan_receipt(key, &plan)?;
            p.apply(&actor, key, plan).await?
        }
    };
    p.flush_deferred().await?;
    p.require_unit_current().await?;
    Ok(result)
}

pub async fn authorize_read<P: MessagingTeamUnit>(p: &mut P) -> Result<MessagingActor, P::Error> {
    let actor = p.authorize(Access::Read).await?;
    if actor.company_id.is_empty() {
        return Err(team_not_found().into());
    }
    Ok(actor)
}
