//! PostgreSQL adapter for native CRM lead / opportunity commands.
use crate::application::leads_native::{LeadFacts, LeadRepository, LeadsFuture, OpportunityFacts};
use crate::domain::leads_native::{
    LeadChangeInput, LeadChangePlan, LeadCreateInput, OpportunityChangeInput,
    OpportunityChangePlan, native_lead_action, native_opportunity_action, not_native,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

use crate::adapters::activity_source::CurrentCrmReviewedAccountLink;
use crate::domain::leads_native::{LeadActor, validate_lead_change};

type Error = PgTransactionError<BusinessError>;

/// Persisted CRM lead facts for a downstream owner effect; these are not an
/// authorization, entitlement, Finance or currency witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualifiedCrmLeadFacts {
    pub company_id: String,
    pub lead_id: String,
    pub owner_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub version: i64,
    pub updated_at: i64,
    pub opportunity_id: String,
    pub command_key: String,
}

/// Opaque retained after-image. The account link binds it to the original unit
/// and source locks. Coordinator must retain all external owner proofs too.
pub struct RetainedCrmQualification<'a> {
    link: &'a CurrentCrmReviewedAccountLink,
    facts: QualifiedCrmLeadFacts,
    image: String,
}
impl RetainedCrmQualification<'_> {
    pub fn facts(&self) -> &QualifiedCrmLeadFacts { &self.facts }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.link.require_current(c).await?;
        let current = qualification_image(c, &self.facts).await?;
        if current.as_deref() != Some(self.image.as_str()) {
            return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CRM_QUALIFICATION_CHANGED", "The original qualification changed.").into());
        }
        Ok(())
    }
}

async fn qualification_image(c: &mut PgConnection, f: &QualifiedCrmLeadFacts) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_array(to_jsonb(l),to_jsonb(o),to_jsonb(deal),to_jsonb(lead_event))::text,'UTF8')),'hex') FROM public.crm_leads l JOIN public.crm_opportunities o ON o.id=$3 AND o.lead_id=l.id JOIN public.crm_case_events deal ON deal.id=$4 AND deal.lead_id=l.id AND deal.opportunity_id=o.id JOIN public.crm_case_events lead_event ON lead_event.id=$5 AND lead_event.lead_id=l.id AND lead_event.opportunity_id IS NULL WHERE l.company_id=$1 AND l.id=$2 AND l.owner_id=$6 AND l.status='qualified' AND l.version=$7 AND l.updated_at=$8")
        .bind(&f.company_id).bind(&f.lead_id).bind(&f.opportunity_id).bind(format!("crm-event-{}-deal",f.command_key)).bind(format!("crm-event-{}-lead",f.command_key)).bind(&f.owner_id).bind(f.version).bind(f.updated_at).fetch_optional(c).await?)
}

async fn qualification_lead_preimage(c: &mut PgConnection, company: &str, lead: &str) -> Result<Option<String>, Error> {
    // Qualify may change only these three fields on the lead. Bind every other
    // persisted field, including future schema fields, across the effect.
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(l)-'status'-'version'-'updated_at')::text,'UTF8')),'hex') FROM public.crm_leads l WHERE l.company_id=$1 AND l.id=$2")
        .bind(company).bind(lead).fetch_optional(c).await?)
}

/// Stage the owner effect and capture the actual after-image atomically. The
/// caller resolves Workflow replay first and retains external proofs separately.
pub async fn apply_crm_qualification_retained<'a>(
    c: &mut PgConnection, actor: &LeadActor, lead_id: &str, command_key: &str,
    input: &LeadChangeInput, currency: &str, link: &'a CurrentCrmReviewedAccountLink, now: i64,
) -> Result<RetainedCrmQualification<'a>, Error> {
    serializable(c).await?;
    let mut savepoint = c.begin().await?;
    let staged: Result<RetainedCrmQualification<'a>, Error> = async {
        link.require_current(&mut savepoint).await?;
        let preimage = qualification_lead_preimage(&mut savepoint, &actor.company_id, lead_id).await?
            .ok_or_else(|| BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"CRM_SOURCE_CHANGED","The original CRM source changed."))?;
        let opportunity_id = apply_crm_qualification_inner(&mut savepoint, actor, lead_id, command_key, input, currency, link, now).await?;
        let (branch_id, department_id): (String, Option<String>) = sqlx::query_as(
            "SELECT branch_id,department_id FROM public.crm_leads WHERE id=$1 AND company_id=$2",
        )
        .bind(lead_id)
        .bind(&actor.company_id)
        .fetch_one(&mut *savepoint)
        .await?;
        if qualification_lead_preimage(&mut savepoint, &actor.company_id, lead_id).await?.as_deref() != Some(preimage.as_str()) {
            return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"CRM_QUALIFICATION_CHANGED","The original qualification changed.").into());
        }
        let plan = validate_lead_change(actor, input)?;
        let party = plan.customer_id.as_deref().ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "A customer is required."))?;
        let expected = plan.expected_value.as_deref().ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "An expected value is required."))?;
        let (whole, fraction) = expected.split_once('.').unwrap_or((expected, ""));
        let whole: i64 = whole.parse().map_err(|_| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use a valid expected value."))?;
        let fraction: i64 = if fraction.is_empty() { 0 } else { fraction.parse::<i64>().map_err(|_| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use a valid expected value."))? * if fraction.len()==1 { 10 } else { 1 } };
        let amount=whole.checked_mul(100).and_then(|v|v.checked_add(fraction)).ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Expected value is too large."))?;
        let version = input.expected_version.checked_add(1).ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Lead version is too large."))?;
        let snapshot = serde_json::json!({"action":"qualify","expectedVersion":input.expected_version,"customerId":party,"expectedValue":expected,"closeDate":plan.close_date,"reason":plan.reason});
        let exact: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.crm_leads l JOIN public.crm_opportunities o ON o.lead_id=l.id JOIN public.crm_case_events deal ON deal.lead_id=l.id AND deal.opportunity_id=o.id JOIN public.crm_case_events le ON le.lead_id=l.id AND le.opportunity_id IS NULL WHERE l.id=$1 AND l.company_id=$2 AND l.owner_id=$3 AND l.status='qualified' AND l.version=$4 AND l.updated_at=$5 AND o.id=$6 AND o.customer_id=$7 AND o.expected_minor=$8 AND o.currency=$9 AND o.close_date=$10 AND o.stage='discovery' AND o.version=1 AND o.quote_id IS NULL AND o.quote_version IS NULL AND o.sales_id IS NULL AND o.reservation_id IS NULL AND o.win_source IS NULL AND o.won_at IS NULL AND o.created_by=$3 AND o.created_at=$5 AND o.updated_at=$5 AND deal.id=$11 AND deal.version=1 AND deal.action='qualify' AND deal.actor_id=$3 AND deal.reason=$12 AND deal.snapshot_json=$13 AND deal.created_at=$5 AND le.id=$14 AND le.version=$4 AND le.action='qualify' AND le.actor_id=$3 AND le.reason=$12 AND le.snapshot_json=$13 AND le.created_at=$5)")
            .bind(lead_id).bind(&actor.company_id).bind(&actor.user_id).bind(version).bind(now).bind(&opportunity_id).bind(party).bind(amount).bind(currency).bind(plan.close_date.as_deref()).bind(format!("crm-event-{command_key}-deal")).bind(&plan.reason).bind(&snapshot).bind(format!("crm-event-{command_key}-lead")).fetch_one(&mut *savepoint).await?;
        if !exact { return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"CRM_QUALIFICATION_CHANGED","The original qualification changed.").into()); }
        let facts = QualifiedCrmLeadFacts { company_id: actor.company_id.clone(), lead_id: lead_id.into(), owner_id: actor.user_id.clone(), branch_id, department_id, version, updated_at: now, opportunity_id, command_key: command_key.into() };
        let image = qualification_image(&mut savepoint, &facts).await?.ok_or_else(|| BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"CRM_QUALIFICATION_CHANGED","The original qualification changed."))?;
        Ok(RetainedCrmQualification { link, facts, image })
    }.await;
    match staged {
        Ok(retained) => { savepoint.commit().await?; Ok(retained) }
        Err(error) => { savepoint.rollback().await?; Err(error) }
    }
}

/// CRM-owned qualification effect only. The coordinator must independently
/// retain current selected authority, Sales eligibility, Finance customer,
/// Commerce currency, purchase, Workflow receipt and Collaboration effects in
/// this same transaction. Neither this function nor its inputs grant access.
pub async fn apply_crm_qualification(
    c: &mut PgConnection, actor: &LeadActor, lead_id: &str, command_key: &str,
    input: &LeadChangeInput, currency: &str, link: &CurrentCrmReviewedAccountLink, now: i64,
) -> Result<String, Error> {
    Ok(apply_crm_qualification_retained(c, actor, lead_id, command_key, input, currency, link, now).await?.facts.opportunity_id.clone())
}

async fn apply_crm_qualification_inner(
    c: &mut PgConnection, actor: &LeadActor, lead_id: &str, command_key: &str,
    input: &LeadChangeInput, currency: &str, link: &CurrentCrmReviewedAccountLink, now: i64,
) -> Result<String, Error> {
    let plan = validate_lead_change(actor, input)?;
    if plan.action != "qualify" || command_key.is_empty() {
        return Err(BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "A qualification command and key are required.").into());
    }
    let party = plan.customer_id.as_deref().ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "A customer is required."))?;
    if !link.matches(&actor.company_id, lead_id, party) {
        return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CRM_SOURCE_CHANGED", "The reviewed account link does not match this command.").into());
    }
    link.require_current(c).await?;
    let expected = plan.expected_value.as_deref().ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "An expected value is required."))?;
    let (whole, fraction) = expected.split_once('.').unwrap_or((expected, ""));
    let whole: i64 = whole.parse().map_err(|_| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use a valid expected value."))?;
    let fractional: i64 = if fraction.is_empty() { 0 } else { fraction.parse::<i64>().map_err(|_| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use a valid expected value."))? * if fraction.len() == 1 { 10 } else { 1 } };
    let amount = whole.checked_mul(100).and_then(|v| v.checked_add(fractional)).filter(|v| *v <= 9_007_199_254_740_991).ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Expected value is too large."))?;
    let row = sqlx::query("SELECT owner_id,status,version FROM public.crm_leads WHERE id=$1 AND company_id=$2 FOR UPDATE").bind(lead_id).bind(&actor.company_id).fetch_optional(&mut *c).await?.ok_or_else(|| BusinessError::new(erpx_server_shared::public::FailureKind::NotFound, "CRM_LEAD_NOT_FOUND", "Lead not found in this scope."))?;
    let version: i64 = row.try_get("version")?;
    let owner: String = row.try_get("owner_id")?;
    let status: String = row.try_get("status")?;
    if version != input.expected_version { return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CRM_LEAD_VERSION_CHANGED", "The lead changed. Reload and retry.").into()); }
    if owner != actor.user_id { return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Forbidden, "CRM_OWNER_REQUIRED", "The assigned salesperson must qualify this lead.").into()); }
    if status != "open" { return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CRM_STATE_CHANGED", "Review the current lead or opportunity state.").into()); }
    let next_version = version.checked_add(1).ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Lead version is too large."))?;
    let opportunity_id = format!("opportunity-{command_key}");
    sqlx::query("INSERT INTO public.crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$8)").bind(&opportunity_id).bind(lead_id).bind(party).bind(amount).bind(currency).bind(plan.close_date.as_deref()).bind(&actor.user_id).bind(now).execute(&mut *c).await?;
    let updated = sqlx::query("UPDATE public.crm_leads SET status='qualified',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND owner_id=$4 AND status='open' AND version=$5").bind(now).bind(lead_id).bind(&actor.company_id).bind(&actor.user_id).bind(version).execute(&mut *c).await?.rows_affected();
    if updated != 1 { return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CRM_SOURCE_CHANGED", "The original CRM source changed.").into()); }
    let snapshot = serde_json::json!({"action":"qualify","expectedVersion":input.expected_version,"customerId":party,"expectedValue":expected,"closeDate":plan.close_date,"reason":plan.reason});
    for (suffix, opportunity, event_version) in [("deal", Some(opportunity_id.as_str()), 1_i64), ("lead", None, next_version)] {
        sqlx::query("INSERT INTO public.crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES($1,$2,$3,$4,'qualify',$5,$6,$7,$8)").bind(format!("crm-event-{command_key}-{suffix}")).bind(lead_id).bind(opportunity).bind(event_version).bind(&actor.user_id).bind(&plan.reason).bind(&snapshot).bind(now).execute(&mut *c).await?;
    }
    link.require_current(c).await?;
    Ok(opportunity_id)
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_LEAD_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_LEAD_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Validated, locked lead command input. `snapshot` is the exact canonical
/// command object written to `crm_case_events`, matching the retained
/// Sales/CRM contract.
pub struct PreparedLeadChange {
    pub plan: LeadChangePlan,
    pub record: LeadFacts,
    pub owner_is_actor: bool,
    pub snapshot: serde_json::Value,
}

/// Read-only qualification preparation for the Workflow `prepare` phase. The
/// caller must inspect the reviewed account link and Finance/Commerce
/// witnesses, then call `apply_crm_qualification_retained` in `apply`.
pub struct PreparedCrmQualification {
    pub plan: LeadChangePlan,
    pub record: LeadFacts,
}

/// Validated, locked opportunity command input.
pub struct PreparedOpportunityChange {
    pub plan: OpportunityChangePlan,
    pub lead: LeadFacts,
    pub opportunity: OpportunityFacts,
    pub owner_is_actor: bool,
    pub snapshot: serde_json::Value,
    /// Current Commerce subtotal for `link-quote`/`win`, supplied by the
    /// composition after it validates the exact owner order witness.
    pub reference_minor: Option<i64>,
}

/// Persisted owner facts after a lead change. Composition supplies these to
/// Collaboration; they are not an authorization or entitlement witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedCrmLeadFacts {
    pub company_id: String,
    pub lead_id: String,
    pub owner_id: String,
    pub status: String,
    pub version: i64,
    pub updated_at: i64,
    pub event_id: String,
    pub action: String,
    pub command_key: String,
    pub actor_id: String,
}

/// Persisted owner facts after an opportunity change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedCrmOpportunityFacts {
    pub company_id: String,
    pub lead_id: String,
    pub opportunity_id: String,
    pub owner_id: String,
    pub lead_version: i64,
    pub opportunity_version: i64,
    pub stage: String,
    pub updated_at: i64,
    pub lead_event_id: String,
    pub lead_action: String,
    pub deal_event_id: String,
    pub deal_action: String,
    pub command_key: String,
    pub actor_id: String,
}

pub struct RetainedCrmLeadChange {
    lead_image: String,
    facts: ChangedCrmLeadFacts,
    snapshot: serde_json::Value,
}

impl RetainedCrmLeadChange {
    pub fn facts(&self) -> &ChangedCrmLeadFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if lead_image(c, &self.facts.company_id, &self.facts.lead_id).await?
            != Some(self.lead_image.clone())
            || lead_event(
                c,
                &self.facts.lead_id,
                &self.facts.event_id,
                None,
                self.facts.version,
                &self.facts.action,
                &self.facts.actor_id,
            )
            .await?
                != Some(self.snapshot.clone())
        {
            return Err(changed("CRM_LEAD_CHANGED"));
        }
        Ok(())
    }
}

pub struct RetainedCrmOpportunityChange {
    lead_image: String,
    opportunity_image: String,
    facts: ChangedCrmOpportunityFacts,
    lead_snapshot: serde_json::Value,
    deal_snapshot: serde_json::Value,
}

impl RetainedCrmOpportunityChange {
    pub fn facts(&self) -> &ChangedCrmOpportunityFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if lead_image(c, &self.facts.company_id, &self.facts.lead_id).await?
            != Some(self.lead_image.clone())
            || opportunity_image(c, &self.facts.company_id, &self.facts.opportunity_id).await?
                != Some(self.opportunity_image.clone())
            || lead_event(
                c,
                &self.facts.lead_id,
                &self.facts.lead_event_id,
                None,
                self.facts.lead_version,
                &self.facts.lead_action,
                &self.facts.actor_id,
            )
            .await?
                != Some(self.lead_snapshot.clone())
            || lead_event(
                c,
                &self.facts.lead_id,
                &self.facts.deal_event_id,
                Some(&self.facts.opportunity_id),
                self.facts.opportunity_version,
                &self.facts.deal_action,
                &self.facts.actor_id,
            )
            .await?
                != Some(self.deal_snapshot.clone())
        {
            return Err(changed("CRM_OPPORTUNITY_CHANGED"));
        }
        Ok(())
    }
}

fn changed(code: &'static str) -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        code,
        "The CRM command changed. Reload and review before submitting.",
    )
    .into()
}

fn not_found() -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::NotFound,
        "CRM_LEAD_NOT_FOUND",
        "Lead not found in this scope.",
    )
    .into()
}

async fn lead_image(
    c: &mut PgConnection,
    company: &str,
    lead: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to(to_jsonb(l)::text,'UTF8')),'hex') FROM crm_leads l WHERE l.company_id=$1 AND l.id=$2",
    )
    .bind(company)
    .bind(lead)
    .fetch_optional(&mut *c)
    .await?)
}

async fn opportunity_image(
    c: &mut PgConnection,
    company: &str,
    opportunity: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE l.company_id=$1 AND o.id=$2",
    )
    .bind(company)
    .bind(opportunity)
    .fetch_optional(&mut *c)
    .await?)
}

#[allow(clippy::too_many_arguments)]
async fn lead_event(
    c: &mut PgConnection,
    lead: &str,
    event_id: &str,
    opportunity: Option<&str>,
    version: i64,
    action: &str,
    actor_id: &str,
) -> Result<Option<serde_json::Value>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT snapshot_json FROM crm_case_events WHERE id=$1 AND lead_id=$2 AND opportunity_id IS NOT DISTINCT FROM $3 AND version=$4 AND action=$5 AND actor_id=$6",
    )
    .bind(event_id)
    .bind(lead)
    .bind(opportunity)
    .bind(version)
    .bind(action)
    .bind(actor_id)
    .fetch_optional(&mut *c)
    .await?)
}

pub fn lead_event_id(command_key: &str) -> String {
    format!("crm-event-{command_key}-lead")
}

pub fn deal_event_id(command_key: &str) -> String {
    format!("crm-event-{command_key}-deal")
}

fn snapshot<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(|_| {
        BusinessError::invalid(
            "CRM_LEAD_INPUT_INVALID",
            "Use a valid CRM command payload.",
        )
        .into()
    })
}

fn create_conflict() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

/// Canonical trimmed create payload for the owner event. Optional keys are
/// omitted exactly as the Zod object output omits them; serializing
/// `LeadCreateInput` directly would emit nulls. The coordinator must encode
/// the same fields in the retained TypeScript property order for a Workflow
/// request/fingerprint when historical receipt replay is required.
pub fn canonical_lead_create_input(
    plan: &crate::domain::leads_native::LeadCreatePlan,
) -> serde_json::Value {
    let mut snapshot = serde_json::json!({
        "branchId": &plan.branch_id,
        "title": &plan.title,
        "contactName": &plan.contact_name,
        "contactDetail": &plan.contact_detail,
        "source": &plan.source,
        "brief": &plan.brief,
        "dueDate": &plan.due_date,
    });
    if let Some(object) = snapshot.as_object_mut() {
        for (key, value) in [
            ("sourceEvidence", plan.source_evidence.as_deref()),
            ("sourceKind", plan.source_kind.as_deref()),
            ("sourceId", plan.source_id.as_deref()),
            ("campaignId", plan.campaign_id.as_deref()),
            ("accountId", plan.account_id.as_deref()),
            ("contactId", plan.contact_id.as_deref()),
        ] {
            if let Some(value) = value {
                object.insert(key.into(), serde_json::Value::String(value.into()));
            }
        }
    }
    snapshot
}

pub struct PgCrmLeadCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmLeadCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn lead(
        &mut self,
        company_id: &str,
        lead_id: &str,
    ) -> Result<Option<LeadFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT l.owner_id,l.status,l.version,l.branch_id,l.department_id,v.stage \
             FROM public.crm_leads l LEFT JOIN public.crm_opportunities v ON v.lead_id=l.id \
             WHERE l.id=$1 AND l.company_id=$2 FOR UPDATE OF l",
        )
        .bind(lead_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(LeadFacts {
                owner_id: row.try_get("owner_id")?,
                status: row.try_get("status")?,
                stage: row.try_get("stage").ok(),
                version: row.try_get("version")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id").ok(),
            })
        })
        .transpose()
    }

    pub async fn opportunity(
        &mut self,
        company_id: &str,
        opportunity_id: &str,
    ) -> Result<Option<OpportunityFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT o.id,o.lead_id,o.stage,o.version,o.customer_id,o.currency,o.quote_id,o.quote_version,o.sales_id FROM public.crm_opportunities o JOIN public.crm_leads l ON l.id=o.lead_id WHERE o.id=$1 AND l.company_id=$2 FOR UPDATE OF o",
        )
        .bind(opportunity_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(OpportunityFacts {
                id: row.try_get("id")?,
                lead_id: row.try_get("lead_id")?,
                stage: row.try_get("stage")?,
                version: row.try_get("version")?,
                customer_id: row.try_get("customer_id")?,
                currency: row.try_get("currency")?,
                quote_id: row.try_get::<Option<String>, _>("quote_id").ok().flatten(),
                quote_version: row.try_get::<Option<i64>, _>("quote_version").ok().flatten(),
                sales_id: row.try_get::<Option<String>, _>("sales_id").ok().flatten(),
            })
        })
        .transpose()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_lead(
        &mut self,
        company_id: &str,
        record_id: &str,
        command_key: &str,
        created_by: &str,
        plan: &crate::domain::leads_native::LeadCreatePlan,
        _input: &LeadCreateInput,
        account_id: &str,
        contact_id: &str,
        department_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        // The retained TypeScript writer creates the CRM identity and contact
        // for an unnamed prospect, or verifies caller-selected references.
        if plan.account_id.is_some() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM public.crm_accounts WHERE id=$1 AND company_id=$2)",
            )
            .bind(account_id)
            .bind(company_id)
            .fetch_one(&mut *self.connection)
            .await?;
            if !exists {
                return Err(create_conflict().into());
            }
        } else {
            sqlx::query(
                "INSERT INTO public.crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES($1,$2,'individual',$3,$4,$5,$5)",
            )
            .bind(account_id)
            .bind(company_id)
            .bind(&plan.contact_name)
            .bind(created_by)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
        }
        if plan.contact_id.is_some() {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM public.crm_contacts WHERE id=$1 AND company_id=$2 AND account_id=$3)",
            )
            .bind(contact_id)
            .bind(company_id)
            .bind(account_id)
            .fetch_one(&mut *self.connection)
            .await?;
            if !exists {
                return Err(create_conflict().into());
            }
        } else {
            sqlx::query(
                "INSERT INTO public.crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$7)",
            )
            .bind(contact_id)
            .bind(company_id)
            .bind(account_id)
            .bind(&plan.contact_name)
            .bind(&plan.contact_detail)
            .bind(created_by)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
        }
        sqlx::query(
            "INSERT INTO public.crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at,account_id,contact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
        )
        .bind(record_id)
        .bind(company_id)
        .bind(&plan.branch_id)
        .bind(department_id)
        .bind(&plan.title)
        .bind(&plan.contact_name)
        .bind(&plan.contact_detail)
        .bind(&plan.source)
        .bind(&plan.brief)
        .bind(created_by)
        .bind(&plan.due_date)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .bind(account_id)
        .bind(contact_id)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO public.crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES($1,$2,NULL,1,'create',$3,$4,$5,$6)",
        )
        .bind(lead_event_id(command_key))
        .bind(record_id)
        .bind(created_by)
        .bind(&plan.brief)
        .bind(canonical_lead_create_input(plan))
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    pub async fn update_lead(
        &mut self,
        company_id: &str,
        record_id: &str,
        command_key: &str,
        actor_id: &str,
        record: &LeadFacts,
        input: &LeadChangeInput,
        plan: &crate::domain::leads_native::LeadChangePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        match plan.action.as_str() {
            "reassign" => {
                sqlx::query("UPDATE crm_leads SET owner_id=$1,due_date=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6")
                    .bind(plan.owner_id.as_deref())
                    .bind(plan.due_date.as_deref())
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "disqualify" => {
                sqlx::query("UPDATE crm_leads SET status='disqualified',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "reopen" => {
                sqlx::query("UPDATE crm_leads SET status='open',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "qualify" => {
                return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Unavailable, "CRM_QUALIFICATION_COMPOSITION_REQUIRED", "Qualification requires its complete transactional composition.").into());
            }
            _ => {}
        }
        let _ = (command_key, actor_id, record, input);
        Ok(())
    }

    pub async fn update_opportunity(
        &mut self,
        company_id: &str,
        opportunity_id: &str,
        lead_id: &str,
        command_key: &str,
        actor_id: &str,
        record: &OpportunityFacts,
        lead: &LeadFacts,
        input: &OpportunityChangeInput,
        plan: &crate::domain::leads_native::OpportunityChangePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        match plan.action.as_str() {
            "link-quote" => {
                sqlx::query("UPDATE crm_opportunities SET quote_id=$1,quote_version=$2,stage='proposal',version=version+1,updated_at=$3 WHERE id=$4")
                    .bind(plan.quote_id.as_deref())
                    .bind(plan.quote_version)
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "negotiate" => {
                sqlx::query("UPDATE crm_opportunities SET stage='negotiation',version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "lose" => {
                sqlx::query("UPDATE crm_opportunities SET stage='lost',version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "reopen" => {
                sqlx::query("UPDATE crm_opportunities SET stage='discovery',quote_id=NULL,quote_version=NULL,version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "win" => {
                sqlx::query("UPDATE crm_opportunities SET sales_id=$1,win_source='sales',won_at=$2,stage='won',version=version+1,updated_at=$2 WHERE id=$3")
                    .bind(plan.sales_id.as_deref())
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "win-pms" => {
                sqlx::query("UPDATE crm_opportunities SET reservation_id=$1,win_source='pms',won_at=$2,stage='won',version=version+1,updated_at=$2 WHERE id=$3")
                    .bind(plan.reservation_id.as_deref())
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            _ => {}
        }
        sqlx::query(
            "UPDATE crm_leads SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
        )
        .bind(now)
        .bind(lead_id)
        .bind(company_id)
        .execute(&mut *self.connection)
        .await?;
        let _ = (command_key, actor_id, record, lead, input);
        Ok(())
    }

    /// Read-only qualification preparation. Qualification writes only through
    /// `apply_crm_qualification_retained`; this method validates the owner,
    /// state, version and selected scope without mutating an owner row.
    pub async fn prepare_crm_qualification(
        &mut self,
        actor: &LeadActor,
        lead_id: &str,
        input: &LeadChangeInput,
    ) -> Result<PreparedCrmQualification, Error> {
        let plan = crate::domain::leads_native::validate_lead_change(actor, input)?;
        if plan.action != "qualify" {
            return Err(BusinessError::invalid(
                "CRM_LEAD_INPUT_INVALID",
                "Use a qualification command.",
            )
            .into());
        }
        let record = self
            .lead(&actor.company_id, lead_id)
            .await?
            .ok_or_else(not_found)?;
        if record.version != plan.expected_version {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "CRM_LEAD_VERSION_CHANGED",
                "The lead changed. Reload and retry.",
            )
            .into());
        }
        let owner_is_actor = actor.user_id == record.owner_id;
        crate::domain::leads_native::require_lead_state_for_change(
            &plan.action,
            &record.status,
            record.stage.as_deref(),
            owner_is_actor,
            actor.can_manage,
        )?;
        Ok(PreparedCrmQualification { plan, record })
    }

    /// Read-only owner preparation for a native lead change. The row is locked
    /// FOR UPDATE in this retained unit so the later CAS update cannot race.
    pub async fn prepare_lead_change(
        &mut self,
        actor: &LeadActor,
        lead_id: &str,
        input: &LeadChangeInput,
    ) -> Result<PreparedLeadChange, Error> {
        if !native_lead_action(&input.action) {
            return Err(not_native(&input.action).into());
        }
        let plan = crate::domain::leads_native::validate_lead_change(actor, input)?;
        let record = self
            .lead(&actor.company_id, lead_id)
            .await?
            .ok_or_else(not_found)?;
        if record.version != plan.expected_version {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "CRM_LEAD_VERSION_CHANGED",
                "The lead changed. Reload and retry.",
            )
            .into());
        }
        let owner_is_actor =
            actor.user_id == record.owner_id;
        crate::domain::leads_native::require_lead_state_for_change(
            &plan.action,
            &record.status,
            record.stage.as_deref(),
            owner_is_actor,
            actor.can_manage,
        )?;
        Ok(PreparedLeadChange {
            snapshot: snapshot(&plan)?,
            plan,
            record,
            owner_is_actor,
        })
    }

    /// Apply the native lead change, write the immutable CRM case event and
    /// verify the exact after-image in the same retained unit.
    pub async fn apply_lead_change(
        &mut self,
        actor: &LeadActor,
        lead_id: &str,
        command_key: &str,
        prepared: PreparedLeadChange,
        now: i64,
    ) -> Result<RetainedCrmLeadChange, Error> {
        serializable(&mut *self.connection).await?;
        let PreparedLeadChange {
            plan,
            record,
            snapshot,
            ..
        } = prepared;
        if !native_lead_action(&plan.action) {
            return Err(not_native(&plan.action).into());
        }
        let next_version = plan.expected_version.checked_add(1).ok_or_else(|| {
            BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Lead version is too large.")
        })?;
        let updated = match plan.action.as_str() {
            "reassign" => sqlx::query(
                "UPDATE crm_leads SET owner_id=$1,due_date=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6 AND status<>'disqualified' AND NOT EXISTS(SELECT 1 FROM crm_opportunities o WHERE o.lead_id=crm_leads.id AND o.stage='won')",
            )
            .bind(plan.owner_id.as_deref())
            .bind(plan.due_date.as_deref())
            .bind(now)
            .bind(lead_id)
            .bind(&actor.company_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            "disqualify" => sqlx::query(
                "UPDATE crm_leads SET status='disqualified',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 AND status='open'",
            )
            .bind(now)
            .bind(lead_id)
            .bind(&actor.company_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            "reopen" => sqlx::query(
                "UPDATE crm_leads SET status='open',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 AND status='disqualified'",
            )
            .bind(now)
            .bind(lead_id)
            .bind(&actor.company_id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            _ => 0,
        };
        if updated != 1 {
            return Err(changed("CRM_LEAD_STATE_CHANGED"));
        }
        let event_id = lead_event_id(command_key);
        sqlx::query(
            "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&event_id)
        .bind(lead_id)
        .bind(next_version)
        .bind(&plan.action)
        .bind(&actor.user_id)
        .bind(&plan.reason)
        .bind(&snapshot)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let owner_id = match plan.action.as_str() {
            "reassign" => plan
                .owner_id
                .clone()
                .ok_or_else(|| BusinessError::invalid("CRM_LEAD_INPUT_INVALID", "Use an owner."))?,
            _ => record.owner_id.clone(),
        };
        let status = match plan.action.as_str() {
            "disqualify" => "disqualified",
            "reopen" | "reassign" => "open",
            _ => "",
        };
        let facts = ChangedCrmLeadFacts {
            company_id: actor.company_id.clone(),
            lead_id: lead_id.into(),
            owner_id,
            status: status.into(),
            version: next_version,
            updated_at: now,
            event_id: event_id.clone(),
            action: plan.action.clone(),
            command_key: command_key.into(),
            actor_id: actor.user_id.clone(),
        };
        let image = lead_image(&mut *self.connection, &facts.company_id, &facts.lead_id)
            .await?
            .ok_or_else(|| changed("CRM_LEAD_CHANGED"))?;
        let retained = RetainedCrmLeadChange {
            lead_image: image,
            facts,
            snapshot,
        };
        retained.require_current(&mut *self.connection).await?;
        Ok(retained)
    }

    /// Read-only owner preparation for a native opportunity change.
    pub async fn prepare_opportunity_change(
        &mut self,
        actor: &LeadActor,
        lead_id: &str,
        input: &OpportunityChangeInput,
    ) -> Result<PreparedOpportunityChange, Error> {
        if !native_opportunity_action(&input.action) {
            return Err(not_native(&input.action).into());
        }
        let plan = crate::domain::leads_native::validate_opportunity_change(actor, input)?;
        let lead = self
            .lead(&actor.company_id, lead_id)
            .await?
            .ok_or_else(not_found)?;
        if lead.version != plan.lead_version {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "CRM_LEAD_VERSION_CHANGED",
                "The lead changed. Reload and retry.",
            )
            .into());
        }
        if lead.status != "qualified" {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "CRM_STATE_CHANGED",
                "Qualify the lead before managing a deal.",
            )
            .into());
        }
        let opportunity = self
            .opportunity_for_lead(&actor.company_id, lead_id)
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
        if matches!(plan.action.as_str(), "negotiate" | "win")
            && (opportunity.quote_id.is_none() || opportunity.quote_version.is_none())
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "CRM_QUOTATION_REQUIRED",
                "Link an approved quotation first.",
            )
            .into());
        }
        let owner_is_actor =
            actor.user_id == lead.owner_id;
        crate::domain::leads_native::require_opportunity_state(
            &plan.action,
            &opportunity.stage,
            owner_is_actor,
            actor.can_manage,
        )?;
        Ok(PreparedOpportunityChange {
            snapshot: snapshot(&plan)?,
            plan,
            lead,
            opportunity,
            owner_is_actor,
            reference_minor: None,
        })
    }

    /// Apply the native opportunity change and verify both owner images and
    /// both immutable case events in the same retained unit.
    pub async fn apply_opportunity_change(
        &mut self,
        actor: &LeadActor,
        lead_id: &str,
        command_key: &str,
        prepared: PreparedOpportunityChange,
        now: i64,
    ) -> Result<RetainedCrmOpportunityChange, Error> {
        serializable(&mut *self.connection).await?;
        let PreparedOpportunityChange {
            plan,
            lead,
            opportunity,
            snapshot,
            reference_minor,
            ..
        } = prepared;
        if !native_opportunity_action(&plan.action) {
            return Err(not_native(&plan.action).into());
        }
        let next_opportunity_version =
            plan.expected_version.checked_add(1).ok_or_else(|| {
                BusinessError::invalid(
                    "CRM_OPPORTUNITY_INPUT_INVALID",
                    "Opportunity version is too large.",
                )
            })?;
        let next_lead_version = plan.lead_version.checked_add(1).ok_or_else(|| {
            BusinessError::invalid("CRM_OPPORTUNITY_INPUT_INVALID", "Lead version is too large.")
        })?;
        let updated = match plan.action.as_str() {
            "link-quote" => {
                let amount = reference_minor.ok_or_else(|| {
                    BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "CRM_SOURCE_CHANGED",
                        "The approved quotation changed.",
                    )
                })?;
                sqlx::query(
                    "UPDATE crm_opportunities SET quote_id=$1,quote_version=$2,expected_minor=$3,stage='proposal',version=version+1,updated_at=$4 WHERE id=$5 AND version=$6 AND stage IN ('discovery','proposal','negotiation')",
                )
                .bind(plan.quote_id.as_deref())
                .bind(plan.quote_version)
                .bind(amount)
                .bind(now)
                .bind(&opportunity.id)
                .bind(plan.expected_version)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            }
            "negotiate" => sqlx::query(
                "UPDATE crm_opportunities SET stage='negotiation',version=version+1,updated_at=$1 WHERE id=$2 AND version=$3 AND stage IN ('discovery','proposal','negotiation')",
            )
            .bind(now)
            .bind(&opportunity.id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            "win" => {
                let amount = reference_minor.ok_or_else(|| {
                    BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "CRM_SOURCE_CHANGED",
                        "The confirmed sale changed.",
                    )
                })?;
                sqlx::query(
                    "UPDATE crm_opportunities SET sales_id=$1,win_source='sales',won_at=$2,expected_minor=$3,stage='won',version=version+1,updated_at=$2 WHERE id=$4 AND version=$5 AND stage IN ('discovery','proposal','negotiation') AND quote_id IS NOT NULL",
                )
                .bind(plan.sales_id.as_deref())
                .bind(now)
                .bind(amount)
                .bind(&opportunity.id)
                .bind(plan.expected_version)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            }
            "lose" => sqlx::query(
                "UPDATE crm_opportunities SET stage='lost',version=version+1,updated_at=$1 WHERE id=$2 AND version=$3 AND stage IN ('discovery','proposal','negotiation')",
            )
            .bind(now)
            .bind(&opportunity.id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            "reopen" => sqlx::query(
                "UPDATE crm_opportunities SET stage='discovery',quote_id=NULL,quote_version=NULL,version=version+1,updated_at=$1 WHERE id=$2 AND version=$3 AND stage='lost'",
            )
            .bind(now)
            .bind(&opportunity.id)
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected(),
            _ => 0,
        };
        if updated != 1 {
            return Err(changed("CRM_OPPORTUNITY_STATE_CHANGED"));
        }
        let lead_updated = sqlx::query(
            "UPDATE crm_leads SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4",
        )
        .bind(now)
        .bind(lead_id)
        .bind(&actor.company_id)
        .bind(plan.lead_version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if lead_updated != 1 {
            return Err(changed("CRM_LEAD_STATE_CHANGED"));
        }
        let lead_action = format!("opportunity-{}", plan.action);
        let lead_event = lead_event_id(command_key);
        sqlx::query(
            "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES($1,$2,NULL,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&lead_event)
        .bind(lead_id)
        .bind(next_lead_version)
        .bind(&lead_action)
        .bind(&actor.user_id)
        .bind(&plan.reason)
        .bind(&snapshot)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let mut deal_snapshot = snapshot.clone();
        if let serde_json::Value::Object(object) = &mut deal_snapshot {
            object.insert("stage".into(), serde_json::Value::String(plan.stage.clone()));
        }
        let deal_event = deal_event_id(command_key);
        sqlx::query(
            "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(&deal_event)
        .bind(lead_id)
        .bind(&opportunity.id)
        .bind(next_opportunity_version)
        .bind(&plan.action)
        .bind(&actor.user_id)
        .bind(&plan.reason)
        .bind(&deal_snapshot)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        let facts = ChangedCrmOpportunityFacts {
            company_id: actor.company_id.clone(),
            lead_id: lead_id.into(),
            opportunity_id: opportunity.id.clone(),
            owner_id: lead.owner_id.clone(),
            lead_version: next_lead_version,
            opportunity_version: next_opportunity_version,
            stage: plan.stage.clone(),
            updated_at: now,
            lead_event_id: lead_event,
            lead_action,
            deal_event_id: deal_event,
            deal_action: plan.action.clone(),
            command_key: command_key.into(),
            actor_id: actor.user_id.clone(),
        };
        let lead_image = lead_image(&mut *self.connection, &facts.company_id, &facts.lead_id)
            .await?
            .ok_or_else(|| changed("CRM_LEAD_CHANGED"))?;
        let opportunity_image =
            opportunity_image(&mut *self.connection, &facts.company_id, &facts.opportunity_id)
                .await?
                .ok_or_else(|| changed("CRM_OPPORTUNITY_CHANGED"))?;
        let retained = RetainedCrmOpportunityChange {
            lead_image,
            opportunity_image,
            facts,
            lead_snapshot: snapshot,
            deal_snapshot,
        };
        retained.require_current(&mut *self.connection).await?;
        Ok(retained)
    }

    async fn opportunity_for_lead(
        &mut self,
        company_id: &str,
        lead_id: &str,
    ) -> Result<Option<OpportunityFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT o.id,o.lead_id,o.stage,o.version,o.customer_id,o.currency,o.quote_id,o.quote_version,o.sales_id FROM public.crm_opportunities o JOIN public.crm_leads l ON l.id=o.lead_id WHERE o.lead_id=$1 AND l.company_id=$2 FOR UPDATE OF o",
        )
        .bind(lead_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(OpportunityFacts {
                id: row.try_get("id")?,
                lead_id: row.try_get("lead_id")?,
                stage: row.try_get("stage")?,
                version: row.try_get("version")?,
                customer_id: row.try_get("customer_id")?,
                currency: row.try_get("currency")?,
                quote_id: row.try_get::<Option<String>, _>("quote_id").ok().flatten(),
                quote_version: row.try_get::<Option<i64>, _>("quote_version").ok().flatten(),
                sales_id: row.try_get::<Option<String>, _>("sales_id").ok().flatten(),
            })
        })
        .transpose()
    }
}

impl LeadRepository for PgCrmLeadCommands<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> LeadsFuture<'a, Option<LeadFacts>, Self::Error> {
        Box::pin(async move { PgCrmLeadCommands::lead(self, company_id, lead_id).await })
    }

    fn opportunity<'a>(
        &'a mut self,
        company_id: &'a str,
        opportunity_id: &'a str,
    ) -> LeadsFuture<'a, Option<OpportunityFacts>, Self::Error> {
        Box::pin(
            async move { PgCrmLeadCommands::opportunity(self, company_id, opportunity_id).await },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        created_by: &'a str,
        plan: &'a crate::domain::leads_native::LeadCreatePlan,
        _input: &'a LeadCreateInput,
        account_id: &'a str,
        contact_id: &'a str,
        department_id: Option<&'a str>,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::insert_lead(
                self,
                company_id,
                record_id,
                command_key,
                created_by,
                plan,
                _input,
                account_id,
                contact_id,
                department_id,
                now,
            )
            .await
        })
    }

    fn update_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a LeadFacts,
        input: &'a LeadChangeInput,
        plan: &'a crate::domain::leads_native::LeadChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::update_lead(
                self,
                company_id,
                record_id,
                command_key,
                actor_id,
                record,
                input,
                plan,
                now,
            )
            .await
        })
    }

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
        plan: &'a crate::domain::leads_native::OpportunityChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::update_opportunity(
                self,
                company_id,
                opportunity_id,
                lead_id,
                command_key,
                actor_id,
                record,
                lead,
                input,
                plan,
                now,
            )
            .await
        })
    }
}
