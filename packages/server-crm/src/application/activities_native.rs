//! Native CRM activity create / complete / cancel use case.
//!
//! Mirrors the TS application gates (lead-open state, owner/manager rules)
//! without SQL. `change_activity` keeps rejecting `reassign` because that
//! action needs the `sales_recipient_eligibility` cross-owner join; the
//! PostgreSQL adapter exposes a separate staged reassign path for the
//! composition to bind to that join, notification and Workflow ledger.
use crate::domain::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityChangePlan, ActivityCreateInput,
    ActivityCreatePlan, require_lead_open, validate_change, validate_change_access, validate_create,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub type ActivitiesFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct LeadFacts {
    pub status: String,
    pub stage: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ActivityFacts {
    pub owner_id: String,
    pub due_date: String,
    pub status: String,
    pub version: i64,
}

pub trait ActivityRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lead<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error>;

    fn activity<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error>;

    fn insert_activity<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        created_by: &'a str,
        plan: &'a ActivityCreatePlan,
        input: &'a ActivityCreateInput,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error>;

    fn update_activity<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a ActivityFacts,
        input: &'a ActivityChangeInput,
        plan: &'a ActivityChangePlan,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error>;
}

pub async fn create_activity<R>(
    actor: &ActivityActor,
    command_key: &str,
    input: &ActivityCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<ActivityCreatePlan, R::Error>
where
    R: ActivityRepository,
{
    if command_key.is_empty() {
        return Err(BusinessError::invalid(
            "CRM_ACTIVITY_INPUT_INVALID",
            "A command key is required.",
        )
        .into());
    }
    let plan = validate_create(actor, input)?;
    let lead = repository
        .lead(actor, &plan.lead_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_LEAD_NOT_FOUND",
                "Lead not found in this scope.",
            )
        })?;
    require_lead_open(&lead.status, lead.stage.as_deref())?;
    if lead.version != plan.expected_lead_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into());
    }
    let record_id = crate::domain::activities_native::create_record_id(command_key);
    repository
        .insert_activity(
            &actor.company_id,
            &record_id,
            command_key,
            &actor.user_id,
            &plan,
            input,
            now,
        )
        .await?;
    Ok(plan)
}

pub async fn change_activity<R>(
    actor: &ActivityActor,
    record_id: &str,
    command_key: &str,
    input: &ActivityChangeInput,
    repository: &mut R,
    now: i64,
) -> Result<ActivityChangePlan, R::Error>
where
    R: ActivityRepository,
{
    if command_key.is_empty() {
        return Err(BusinessError::invalid(
            "CRM_ACTIVITY_INPUT_INVALID",
            "A command key is required.",
        )
        .into());
    }
    validate_change_access(actor, input)?;
    let record = repository
        .activity(actor, record_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_ACTIVITY_NOT_FOUND",
                "CRM activity not found in this scope.",
            )
        })?;
    let plan = validate_change(actor, input, &record.owner_id, &record.status)?;
    if record.version != input.expected_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into());
    }
    repository
        .update_activity(
            &actor.company_id,
            record_id,
            command_key,
            &actor.user_id,
            &record,
            input,
            &plan,
            now,
        )
        .await?;
    Ok(plan)
}
