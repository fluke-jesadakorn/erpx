//! Native CRM activity create / complete / cancel use case.
//!
//!mirrors the TS application gates (lead-open state, owner/manager rules)
//! without SQL. `reassign` is rejected here; it remains TS-owned because it
//! needs the `sales_recipient_eligibility` cross-owner join.
use crate::domain::activities_native::{
    ActivityActor, ActivityChangeInput, ActivityChangePlan, ActivityCreateInput,
    ActivityCreatePlan, require_lead_open, validate_change, validate_create,
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
    pub status: String,
    pub version: i64,
}

pub trait ActivityRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lead<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error>;

    fn activity<'a>(
        &'a mut self,
        company_id: &'a str,
        activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error>;

    fn insert_activity<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a ActivityCreatePlan,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error>;

    fn update_activity<'a>(
        &'a mut self,
        record_id: &'a str,
        plan: &'a ActivityChangePlan,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error>;
}

pub async fn create_activity<R>(
    actor: &ActivityActor,
    input: &ActivityCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<ActivityCreatePlan, R::Error>
where
    R: ActivityRepository,
{
    let plan = validate_create(actor, input)?;
    let lead = repository
        .lead(&actor.company_id, &plan.lead_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_LEAD_NOT_FOUND",
                "Lead not found in this scope.",
            )
        })?;
    if lead.version != plan.expected_lead_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CRM_LEAD_VERSION_CHANGED",
            "The lead changed. Reload and retry.",
        )
        .into());
    }
    require_lead_open(&lead.status, lead.stage.as_deref())?;
    let record_id = crate::domain::activities_native::create_record_id("__application__");
    repository
        .insert_activity(
            &actor.company_id,
            &record_id,
            &actor.user_id,
            &plan,
            now,
        )
        .await?;
    Ok(plan)
}

pub async fn change_activity<R>(
    actor: &ActivityActor,
    record_id: &str,
    input: &ActivityChangeInput,
    repository: &mut R,
    now: i64,
) -> Result<ActivityChangePlan, R::Error>
where
    R: ActivityRepository,
{
    let record = repository
        .activity(&actor.company_id, record_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_ACTIVITY_NOT_FOUND",
                "CRM activity not found in this scope.",
            )
        })?;
    if record.version != input.expected_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CRM_ACTIVITY_VERSION_CHANGED",
            "This activity changed. Reload and retry.",
        )
        .into());
    }
    let plan = validate_change(actor, input, &record.owner_id, &record.status)?;
    repository
        .update_activity(record_id, &plan, now)
        .await?;
    Ok(plan)
}
