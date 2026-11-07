//! Native Benefits command use cases.
//!
//! The owner adapter receives one serializable Holding transaction and the
//! current command authority from composition. This layer owns the retained
//! command order: current actor policy, exact idempotency replay, current
//! package entitlement, worker and reviewed-source facts, optimistic version,
//! independent review, persisted readback and the payroll handoff row.
use crate::application::workspace::{BenefitsEntitlementPort, BenefitsPeoplePort};
use crate::domain::command::{
    BenefitCommandResult, BenefitElectionInput, BenefitPlanInput, BenefitRecordKind,
    BenefitRecordState, BenefitTransitionChanges, BenefitTransitionInput, BenefitWindowInput,
    BenefitsCommandActor, ChangeBenefit, CreateDependent, CreateElection, CreatePlan, CreateWindow,
    StoredBenefitReceipt, command_incomplete, current_date, election_sources, idempotency_conflict,
    require_independent_review, require_learning_port, require_submit_owner,
    require_verified_dependents, transition_changes, transition_state, validate_hash,
};
use crate::domain::workspace::{BenefitPlanRow, BenefitWindowRow};
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type BenefitsCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "BENEFIT_RECORD_NOT_FOUND",
        "Benefits record not found.",
    )
}

fn malformed() -> BusinessError {
    BusinessError::invalid("BENEFITS_VALIDATION_FAILED", "Review the Benefits request.")
}

pub trait BenefitsCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> BenefitsCommandFuture<'a, (), Self::Error>;

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> BenefitsCommandFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<StoredBenefitReceipt>, Self::Error>;

    fn record_state<'a>(
        &'a mut self,
        kind: BenefitRecordKind,
        id: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<BenefitRecordState>, Self::Error>;

    fn plan_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitPlanRow>, Self::Error>;

    fn window_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitWindowRow>, Self::Error>;

    fn verified_dependents<'a>(
        &'a mut self,
        person_id: &'a str,
    ) -> BenefitsCommandFuture<'a, Vec<String>, Self::Error>;

    fn create_plan<'a>(
        &'a mut self,
        command: &'a CreatePlan<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error>;

    fn create_window<'a>(
        &'a mut self,
        command: &'a CreateWindow<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error>;

    fn create_dependent<'a>(
        &'a mut self,
        command: &'a CreateDependent<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error>;

    fn create_election<'a>(
        &'a mut self,
        command: &'a CreateElection<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error>;

    fn change<'a>(
        &'a mut self,
        command: &'a ChangeBenefit<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    user_id: &'a str,
    assignment_id: &'a str,
    role: &'a str,
    company_id: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    work_context: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: BenefitsCommandRepository, I: Serialize>(
    actor: &BenefitsCommandActor,
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<BenefitCommandResult>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        role: &actor.role,
        company_id: actor.company_id.as_str(),
        branch_id: actor.branch_id.as_ref().map(RecordId::as_str),
        department_id: actor.department_id.as_ref().map(RecordId::as_str),
        work_context: &actor.work_context,
        operation,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(malformed()))?;
    let request_hash = repository.request_hash(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(command_incomplete().into()),
        };
    }
    Ok((request_hash, None))
}

pub async fn create_plan<R, E>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    E: BenefitsEntitlementPort,
    R::Error: From<E::Error>,
{
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = BenefitPlanInput::parse(raw)?;
    let id = format!("benefit-plan-{}", key.as_str());
    let operation = "benefits.plan.create";
    let (request_hash, prior) = prepared(actor, repository, key, operation, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_new_work()?;
    repository
        .create_plan(&CreatePlan {
            id: &id,
            key: key.as_str(),
            operation,
            request_hash: &request_hash,
            input: &input,
            now: now.get(),
        })
        .await
}

pub async fn create_window<R, E>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    E: BenefitsEntitlementPort,
    R::Error: From<E::Error>,
{
    actor.require_write(now)?;
    actor.require_review(now)?;
    let input = BenefitWindowInput::parse(raw)?;
    let id = format!("benefit-window-{}", key.as_str());
    let operation = "benefits.window.create";
    let (request_hash, prior) = prepared(actor, repository, key, operation, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_new_work()?;
    repository
        .create_window(&CreateWindow {
            id: &id,
            key: key.as_str(),
            operation,
            request_hash: &request_hash,
            input: &input,
            now: now.get(),
        })
        .await
}

pub async fn create_dependent<R, P, E>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    actor.require_write(now)?;
    let input = crate::domain::command::BenefitDependentInput::parse(raw)?;
    let worker = people
        .worker(&actor.company_id, &actor.user_id)
        .await
        .map_err(R::Error::from)?
        .ok_or_else(|| {
            BusinessError::invalid(
                "BENEFIT_WORKER_REQUIRED",
                "An active worker record is required.",
            )
        })?;
    worker.validate()?;
    let id = format!("benefit-dependent-{}", key.as_str());
    let operation = "benefits.dependent.create";
    let (request_hash, prior) = prepared(actor, repository, key, operation, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_new_work()?;
    repository
        .create_dependent(&CreateDependent {
            id: &id,
            key: key.as_str(),
            operation,
            request_hash: &request_hash,
            input: &input,
            person_id: &worker.person_id,
            now: now.get(),
        })
        .await
}

pub async fn create_election<R, P, E>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    actor.require_write(now)?;
    let input = BenefitElectionInput::parse(raw)?;
    let id = format!("benefit-election-{}", key.as_str());
    let operation = "benefits.election.create";
    let (request_hash, prior) = prepared(actor, repository, key, operation, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    let worker = people
        .worker(&actor.company_id, &actor.user_id)
        .await
        .map_err(R::Error::from)?
        .ok_or_else(|| {
            BusinessError::invalid(
                "BENEFIT_WORKER_REQUIRED",
                "An active worker record is required.",
            )
        })?;
    worker.validate()?;
    let plan = repository
        .plan_source(&input.plan_id, input.plan_version)
        .await?;
    let window = repository
        .window_source(&input.window_id, input.window_version)
        .await?;
    let (plan, window) =
        election_sources(plan.as_ref(), window.as_ref(), &current_date(now), &input)?;
    require_learning_port(plan)?;
    if !input.dependent_ids.is_empty() {
        let verified = repository.verified_dependents(&worker.person_id).await?;
        require_verified_dependents(&input, &verified)?;
    }
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_new_work()?;
    repository
        .create_election(&CreateElection {
            id: &id,
            key: key.as_str(),
            operation,
            request_hash: &request_hash,
            input: &input,
            worker: &worker,
            plan,
            window,
            now: now.get(),
        })
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn change<R, P, E>(
    actor: &BenefitsCommandActor,
    kind: BenefitRecordKind,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    actor.require_write(now)?;
    let input = BenefitTransitionInput::parse(raw)?;
    let operation = format!("benefits.{}.{}", kind.as_str(), input.action);
    let review = kind != BenefitRecordKind::Election || input.action != "submit";
    if review {
        actor.require_review(now)?;
    }
    let (request_hash, prior) = prepared(actor, repository, key, &operation, id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    if review {
        actor.require_review(now)?;
    }
    let state = repository
        .record_state(kind, id)
        .await?
        .ok_or_else(not_found)?;
    let (from, to) = transition_state(kind, &state, &input)?;
    if !review {
        let worker = people
            .worker(&actor.company_id, &actor.user_id)
            .await
            .map_err(R::Error::from)?;
        if let Some(worker) = &worker {
            worker.validate()?;
        }
        require_submit_owner(worker.as_ref(), &state)?;
    }
    require_independent_review(&state, &input, &actor.user_id)?;
    if !input.attachment_set_ids.is_empty()
        && ((kind == BenefitRecordKind::Dependent && input.action == "verify")
            || (kind == BenefitRecordKind::Election
                && matches!(input.action.as_str(), "submit" | "acknowledge")))
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "BENEFIT_EVIDENCE_PORT_REQUIRED",
            "Native Benefits evidence binding requires the current Files attachment port, which composition has not mounted.",
        )
        .into());
    }
    let entitlement = entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?;
    if matches!(input.action.as_str(), "publish" | "open") {
        entitlement.require_new_work()?;
    } else {
        entitlement.require_settle_existing()?;
    }
    let changes: BenefitTransitionChanges = transition_changes(&input, &actor.user_id);
    repository
        .change(&ChangeBenefit {
            kind,
            id,
            key: key.as_str(),
            operation: &operation,
            request_hash: &request_hash,
            input: &input,
            from,
            to,
            changes: &changes,
            release_payroll: kind == BenefitRecordKind::Election && input.action == "release",
            now: now.get(),
        })
        .await
}

/// Dispatch the retained Benefits command envelope exactly once. `entity` and
/// an optional `id` select the owner action; current Account/IAM resolution and
/// the serializable transaction remain composition responsibilities.
pub async fn execute_benefits_command<R, P, E>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    let object = raw.as_object().ok_or_else(malformed)?;
    let entity = object
        .get("entity")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?;
    let kind = BenefitRecordKind::parse(entity).ok_or_else(|| {
        BusinessError::invalid(
            "BENEFIT_ENTITY_INVALID",
            "Choose a registered Benefits record type.",
        )
    })?;
    match object.get("id") {
        Some(Value::String(id)) => {
            change(
                actor,
                kind,
                id,
                raw,
                key,
                repository,
                people,
                entitlement,
                now,
            )
            .await
        }
        Some(Value::Null) | None => match kind {
            BenefitRecordKind::Plan => {
                create_plan(actor, raw, key, repository, entitlement, now).await
            }
            BenefitRecordKind::Window => {
                create_window(actor, raw, key, repository, entitlement, now).await
            }
            BenefitRecordKind::Dependent => {
                create_dependent(actor, raw, key, repository, people, entitlement, now).await
            }
            BenefitRecordKind::Election => {
                create_election(actor, raw, key, repository, people, entitlement, now).await
            }
        },
        Some(_) => Err(malformed().into()),
    }
}
