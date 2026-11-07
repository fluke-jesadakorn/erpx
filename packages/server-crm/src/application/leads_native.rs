//! Native CRM lead / opportunity command use cases.
use crate::domain::leads_native::{
    LeadActor, LeadChangeInput, LeadChangePlan, LeadCreateInput, LeadCreatePlan,
    OpportunityChangeInput, OpportunityChangePlan, require_lead_state_for_change,
    require_opportunity_state, validate_lead_change, validate_lead_create,
    validate_opportunity_change,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub type LeadsFuture<'a, T, E> = std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct LeadFacts {
    pub owner_id: String,
    pub status: String,
    pub stage: Option<String>,
    pub version: i64,
    pub branch_id: String,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OpportunityFacts {
    pub id: String,
    pub lead_id: String,
    pub stage: String,
    pub version: i64,
    pub customer_id: String,
    pub currency: String,
    pub quote_id: Option<String>,
    pub quote_version: Option<i64>,
    pub sales_id: Option<String>,
}

pub trait LeadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lead<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> LeadsFuture<'a, Option<LeadFacts>, Self::Error>;

    fn opportunity<'a>(
        &'a mut self,
        company_id: &'a str,
        opportunity_id: &'a str,
    ) -> LeadsFuture<'a, Option<OpportunityFacts>, Self::Error>;

    fn insert_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a LeadCreatePlan,
        input: &'a LeadCreateInput,
        account_id: &'a str,
        contact_id: &'a str,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error>;

    fn update_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a LeadFacts,
        input: &'a LeadChangeInput,
        plan: &'a LeadChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error>;

    fn update_opportunity<'a>(
        &'a mut self,
        company_id: &'a str,
        opportunity_id: &'a str,
        lead_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a OpportunityFacts,
        lead: &'a LeadFacts,
        input: &'a OpportunityChangeInput,
        plan: &'a OpportunityChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error>;
}

pub async fn create_lead<R>(
    actor: &LeadActor,
    command_key: &str,
    input: &LeadCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<LeadCreatePlan, R::Error>
where
    R: LeadRepository,
{
    if command_key.is_empty() {
        return Err(
            BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "A command key is required.").into(),
        );
    }
    let plan = validate_lead_create(actor, input)?;
    let account_id = plan
        .account_id
        .clone()
        .unwrap_or_else(|| format!("account-{command_key}"));
    let contact_id = plan
        .contact_id
        .clone()
        .unwrap_or_else(|| format!("contact-{command_key}"));
    let record_id = format!("lead-{command_key}");
    repository
        .insert_lead(
            &actor.company_id,
            &record_id,
            &actor.user_id,
            &plan,
            input,
            &account_id,
            &contact_id,
            now,
        )
        .await?;
    Ok(plan)
}

pub async fn change_lead<R>(
    actor: &LeadActor,
    record_id: &str,
    command_key: &str,
    input: &LeadChangeInput,
    repository: &mut R,
    now: i64,
) -> Result<LeadChangePlan, R::Error>
where
    R: LeadRepository,
{
    if command_key.is_empty() {
        return Err(
            BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "A command key is required.").into(),
        );
    }
    let plan = validate_lead_change(actor, input)?;
    let record = repository
        .lead(&actor.company_id, record_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_LEAD_NOT_FOUND",
                "Lead not found in this scope.",
            )
        })?;
    if record.version != plan.expected_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CRM_LEAD_VERSION_CHANGED",
            "The lead changed. Reload and retry.",
        )
        .into());
    }
    let is_owner = actor.user_id == record.owner_id || actor.is_same_person(&record.owner_id);
    require_lead_state_for_change(
        &plan.action,
        &record.status,
        record.stage.as_deref(),
        is_owner,
        actor.can_manage,
    )?;
    repository
        .update_lead(
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

pub async fn change_opportunity<R>(
    actor: &LeadActor,
    record_id: &str,
    opportunity_id: &str,
    command_key: &str,
    input: &OpportunityChangeInput,
    repository: &mut R,
    now: i64,
) -> Result<OpportunityChangePlan, R::Error>
where
    R: LeadRepository,
{
    if command_key.is_empty() {
        return Err(BusinessError::invalid(
            "CRM_OPPORTUNITY_INPUT_INVALID",
            "A command key is required.",
        )
        .into());
    }
    let plan = validate_opportunity_change(actor, input)?;
    let lead = repository
        .lead(&actor.company_id, record_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_LEAD_NOT_FOUND",
                "Lead not found in this scope.",
            )
        })?;
    if lead.version != plan.lead_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CRM_LEAD_VERSION_CHANGED",
            "The lead changed. Reload and retry.",
        )
        .into());
    }
    let opportunity = repository
        .opportunity(&actor.company_id, opportunity_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "CRM_OPPORTUNITY_NOT_FOUND",
                "Opportunity not found in this scope.",
            )
        })?;
    if opportunity.version != plan.expected_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "CRM_OPPORTUNITY_VERSION_CHANGED",
            "The opportunity changed. Reload and retry.",
        )
        .into());
    }
    if opportunity.lead_id != record_id {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "CRM_OPPORTUNITY_NOT_FOUND",
            "Opportunity not found in this scope.",
        )
        .into());
    }
    let is_owner = actor.user_id == lead.owner_id || actor.is_same_person(&lead.owner_id);
    require_opportunity_state(&plan.action, &opportunity.stage, is_owner, actor.can_manage)?;
    repository
        .update_opportunity(
            &actor.company_id,
            opportunity_id,
            record_id,
            command_key,
            &actor.user_id,
            &opportunity,
            &lead,
            input,
            &plan,
            now,
        )
        .await?;
    Ok(plan)
}
