//! Native eligibility handoff read use case.
//!
//! The caller keeps its serializable Holding transaction. This layer checks the
//! current Learning actor and selected scope, validates every bounded source
//! fact and never writes or consumes a certificate.
use crate::domain::eligibility_facts::{
    EligibilityCertificateRow, EligibilityQuery, EligibilityScope, eligibility_facts_json,
};
use crate::domain::learning::LearningActor;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type EligibilityFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait EligibilityReadRepository {
    type Error: From<BusinessError>;

    fn eligibility_certificates<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error>;
}

pub async fn learning_eligibility_facts<R: EligibilityReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    query: &EligibilityQuery,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let scope = EligibilityScope::from_learning_actor(actor);
    let query = query.validate()?;
    let rows = repository
        .eligibility_certificates(&scope, &query.person_id, &query.eligibility_code)
        .await?;
    eligibility_facts_json(&scope, &query, rows).map_err(Into::into)
}

/// Owner facts for a direct consumer such as Benefits inside its own
/// serializable transaction. The caller owns its business authority; this
/// function enforces only current Company/branch/department source scope.
pub async fn learning_eligibility_consumer_facts<R: EligibilityReadRepository>(
    repository: &mut R,
    scope: &EligibilityScope,
    query: &EligibilityQuery,
) -> Result<Value, R::Error> {
    let query = query.validate()?;
    let rows = repository
        .eligibility_certificates(scope, &query.person_id, &query.eligibility_code)
        .await?;
    eligibility_facts_json(scope, &query, rows).map_err(Into::into)
}

use crate::domain::eligibility_facts::{
    BENEFIT_CONSUME_OPERATION, BenefitConsumptionActor, BenefitConsumptionCommand,
    BenefitConsumptionPlan, BenefitConsumptionReceipt, BenefitConsumptionRequest,
    PreparedBenefitConsumption, benefit_consume_identity, benefit_consumption_plan,
    benefit_certificate_expired, benefit_eligibility_required, certificate_source_json,
    require_certificate_current, select_benefit_certificate,
};
use crate::domain::learning::{CertificateRow, ReceiptSnapshot, readback_invalid, utc_date};
use erpx_server_shared::public::CommandKey;

/// Caller-owned command envelope for the Learning-native consumption entrypoint.
/// The Benefits port leaves this `None` because the Benefits command already
/// retains its own receipt/audit/outbox for the election key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitCommandEnvelope {
    pub operation: &'static str,
    pub request_hash: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub record_id: String,
    pub now: i64,
}

pub trait BenefitConsumptionRepository {
    type Error: From<BusinessError>;

    fn benefit_certificate_rows<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error>;

    fn lock_benefit_certificate<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        certificate_id: &'a str,
    ) -> EligibilityFuture<'a, Option<CertificateRow>, Self::Error>;

    fn benefit_consumption<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        election_id: &'a str,
    ) -> EligibilityFuture<'a, Option<BenefitConsumptionReceipt>, Self::Error>;

    fn benefit_consumption_by_command<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        command_id: &'a str,
    ) -> EligibilityFuture<'a, Option<BenefitConsumptionReceipt>, Self::Error>;

    fn retained_election_certificate<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        election_id: &'a str,
    ) -> EligibilityFuture<'a, Option<String>, Self::Error>;

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> EligibilityFuture<'a, String, Self::Error>;

    fn insert_benefit_consumption<'a>(
        &'a mut self,
        plan: &'a BenefitConsumptionPlan,
        envelope: Option<&'a BenefitCommandEnvelope>,
    ) -> EligibilityFuture<'a, bool, Self::Error>;

    fn command_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EligibilityFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
}

fn request_from_plan(
    plan: &BenefitConsumptionPlan,
    today: &str,
    effective_date: &str,
) -> BenefitConsumptionRequest {
    BenefitConsumptionRequest {
        company_id: plan.company_id.clone(),
        person_id: plan.person_id.clone(),
        required_code: plan.eligibility_code.clone(),
        effective_date: effective_date.to_owned(),
        election_id: plan.election_id.clone(),
        command_id: plan.command_id.clone(),
        today: today.to_owned(),
        actor_id: plan.actor_id.clone(),
    }
}

/// Select and validate the current certificate for a Benefits election. No
/// receipt is written until the caller has inserted its election row.
pub async fn prepare_benefit_consumption<R: BenefitConsumptionRepository>(
    repository: &mut R,
    request: &BenefitConsumptionRequest,
    now: i64,
) -> Result<PreparedBenefitConsumption, R::Error> {
    let scope = EligibilityScope::new(&request.company_id, None, None)?;
    prepare_benefit_consumption_in_scope(repository, &scope, request, now).await
}

async fn prepare_benefit_consumption_in_scope<R: BenefitConsumptionRepository>(
    repository: &mut R,
    scope: &EligibilityScope,
    request: &BenefitConsumptionRequest,
    now: i64,
) -> Result<PreparedBenefitConsumption, R::Error> {
    let request = request.validate()?;
    if request.company_id != scope.company_id() {
        return Err(benefit_eligibility_required().into());
    }
    let rows = repository
        .benefit_certificate_rows(scope, &request.person_id, &request.required_code)
        .await?;
    let selected = select_benefit_certificate(scope, &request, rows)?;
    let Some(locked) = repository
        .lock_benefit_certificate(scope, &selected.certificate.id)
        .await?
    else {
        return Err(benefit_certificate_expired().into());
    };
    if locked != selected.certificate {
        return Err(benefit_certificate_expired().into());
    }
    let digest = repository
        .digest(&certificate_source_json(&locked))
        .await?;
    let plan = benefit_consumption_plan(&request, &locked, digest, now)?;
    Ok(PreparedBenefitConsumption {
        plan,
        existing: None,
        today: request.today,
        effective_date: request.effective_date,
    })
}

/// Validate the receipt retained by an existing election before a Benefits
/// transition. The receipt and current certificate are rechecked; nothing is
/// written.
pub async fn validate_benefit_consumption<R: BenefitConsumptionRepository>(
    repository: &mut R,
    request: &BenefitConsumptionRequest,
) -> Result<PreparedBenefitConsumption, R::Error> {
    let request = request.validate()?;
    let scope = EligibilityScope::new(&request.company_id, None, None)?;
    let receipt = repository
        .benefit_consumption(&scope, &request.election_id)
        .await?
        .ok_or_else(benefit_eligibility_required)?;
    receipt.validate_source()?;
    if receipt.company_id != request.company_id
        || receipt.person_id != request.person_id
        || receipt.eligibility_code != request.required_code
    {
        return Err(benefit_eligibility_required().into());
    }
    let retained = repository
        .retained_election_certificate(&scope, &request.election_id)
        .await?
        .ok_or_else(benefit_eligibility_required)?;
    if retained != receipt.certificate_id {
        return Err(benefit_eligibility_required().into());
    }
    let Some(certificate) = repository
        .lock_benefit_certificate(&scope, &receipt.certificate_id)
        .await?
    else {
        return Err(benefit_certificate_expired().into());
    };
    certificate.validate_source()?;
    if certificate.company_id != request.company_id
        || certificate.person_id != request.person_id
        || certificate.eligibility_code != request.required_code
        || certificate.source_assignment_version != receipt.source_assignment_version
    {
        return Err(benefit_certificate_expired().into());
    }
    let rows = repository
        .benefit_certificate_rows(&scope, &request.person_id, &request.required_code)
        .await?;
    require_certificate_current(&scope, &request, &receipt.certificate_id, rows)?;
    let plan = BenefitConsumptionPlan {
        id: receipt.id.clone(),
        handoff_id: receipt.handoff_id.clone(),
        certificate_id: receipt.certificate_id.clone(),
        election_id: receipt.election_id.clone(),
        company_id: receipt.company_id.clone(),
        person_id: receipt.person_id.clone(),
        source_assignment_version: receipt.source_assignment_version,
        eligibility_code: receipt.eligibility_code.clone(),
        source_digest: receipt.source_digest.clone(),
        purpose: receipt.purpose.clone(),
        actor_id: receipt.actor_id.clone(),
        command_id: receipt.command_id.clone(),
        created_at: receipt.created_at,
    };
    Ok(PreparedBenefitConsumption {
        plan,
        existing: Some(receipt),
        today: request.today,
        effective_date: request.effective_date,
    })
}

/// Write the consumption receipt if it does not exist yet, then re-read the
/// certificate and receipt in the caller's transaction. This is the post-effect
/// hook Benefits calls; the election row exists by then.
pub async fn finalize_benefit_consumption<R: BenefitConsumptionRepository>(
    repository: &mut R,
    prepared: &PreparedBenefitConsumption,
) -> Result<BenefitConsumptionReceipt, R::Error> {
    let plan = &prepared.plan;
    let scope = EligibilityScope::new(&plan.company_id, None, None)?;
    let request = request_from_plan(plan, &prepared.today, &prepared.effective_date);
    let rows = repository
        .benefit_certificate_rows(&scope, &request.person_id, &request.required_code)
        .await?;
    require_certificate_current(&scope, &request, &plan.certificate_id, rows)?;
    match repository
        .benefit_consumption(&scope, &plan.election_id)
        .await?
    {
        Some(existing) => {
            existing.validate_source()?;
            if !existing.matches_source(plan) {
                return Err(benefit_eligibility_required().into());
            }
        }
        None => {
            let inserted = repository
                .insert_benefit_consumption(plan, None)
                .await?;
            let existing = repository
                .benefit_consumption(&scope, &plan.election_id)
                .await?
                .ok_or_else(readback_invalid)?;
            existing.validate_source()?;
            if !inserted || !existing.matches_source(plan) {
                return Err(benefit_eligibility_required().into());
            }
        }
    }
    let rows = repository
        .benefit_certificate_rows(&scope, &request.person_id, &request.required_code)
        .await?;
    require_certificate_current(&scope, &request, &plan.certificate_id, rows)?;
    Ok(repository
        .benefit_consumption(&scope, &plan.election_id)
        .await?
        .ok_or_else(readback_invalid)?)
}

/// Learning-native consumption command entrypoint. It writes the owner receipt
/// plus the command receipt/audit/outbox for its own idempotency key; the
/// Benefits election key keeps its Benefits command envelope.
pub async fn consume_benefit<R: BenefitConsumptionRepository>(
    repository: &mut R,
    actor: &BenefitConsumptionActor,
    key: &CommandKey,
    input: &BenefitConsumptionCommand,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_consume(now)?;
    let scope = actor.scope()?;
    let request = BenefitConsumptionRequest {
        company_id: actor.company_id.as_str().to_owned(),
        person_id: input.person_id.clone(),
        required_code: input.required_code.clone(),
        effective_date: input.effective_date.clone(),
        election_id: input.election_id.clone(),
        command_id: key.as_str().to_owned(),
        today: utc_date(now),
        actor_id: actor.user_id.as_str().to_owned(),
    }
    .validate()?;
    let identity = benefit_consume_identity(actor, key.as_str(), &request);
    let request_hash = repository.digest(&identity).await?;
    if request_hash.len() != 64 || !request_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = repository.command_receipt(key.as_str()).await? {
        if prior.request_hash != request_hash
            || prior.operation != BENEFIT_CONSUME_OPERATION
            || prior.actor_id != actor.user_id.as_str()
            || prior.record_id != format!("learning-benefit-{}", key.as_str())
        {
            return Err(crate::domain::eligibility_facts::benefit_idempotency_conflict().into());
        }
        let receipt = repository
            .benefit_consumption_by_command(&scope, key.as_str())
            .await?
            .ok_or_else(readback_invalid)?;
        return Ok(receipt.to_json());
    }
    let prepared = prepare_benefit_consumption_in_scope(repository, &scope, &request, now).await?;
    if let Some(existing) = repository
        .benefit_consumption(&scope, &request.election_id)
        .await?
    {
        existing.validate_source()?;
        if !existing.matches_command(&prepared.plan) {
            return Err(benefit_eligibility_required().into());
        }
    } else {
        let envelope = BenefitCommandEnvelope {
            operation: BENEFIT_CONSUME_OPERATION,
            request_hash: request_hash.clone(),
            actor_id: actor.user_id.as_str().to_owned(),
            assignment_id: actor.assignment_id.as_str().to_owned(),
            authority_json: actor.authority_json(),
            record_id: prepared.plan.id.clone(),
            now,
        };
        if !repository
            .insert_benefit_consumption(&prepared.plan, Some(&envelope))
            .await?
        {
            return Err(benefit_eligibility_required().into());
        }
    }
    let receipt = repository
        .benefit_consumption_by_command(&scope, key.as_str())
        .await?
        .ok_or_else(readback_invalid)?;
    receipt.validate_source()?;
    if !receipt.matches_command(&prepared.plan) {
        return Err(readback_invalid().into());
    }
    let rows = repository
        .benefit_certificate_rows(&scope, &request.person_id, &request.required_code)
        .await?;
    require_certificate_current(&scope, &request, &prepared.plan.certificate_id, rows)?;
    Ok(receipt.to_json())
}
