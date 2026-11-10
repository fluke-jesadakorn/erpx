//! Native Benefits command use cases.
//!
//! The owner adapter receives one serializable Holding transaction and the
//! current command authority from composition. This layer owns the retained
//! command order: current actor policy, exact idempotency replay, current
//! package entitlement, worker and reviewed-source facts, optimistic version,
//! independent review, persisted readback and the payroll handoff row.
use crate::application::workspace::{BenefitsEntitlementPort, BenefitsPeoplePort};
use crate::domain::command::{
    BenefitCommandResult, BenefitElectionInput, BenefitEvidenceRequest, BenefitLearningRequest,
    BenefitPlanInput, BenefitRecordKind, BenefitRecordState, BenefitTransitionChanges,
    BenefitTransitionInput, BenefitWindowInput, BenefitsCommandActor, ChangeBenefit,
    CreateDependent, CreateElection, CreatePlan, CreateWindow, StoredBenefitReceipt,
    benefit_evidence_binding_type, benefit_evidence_policy, command_incomplete, current_date,
    election_sources, idempotency_conflict, require_independent_review, require_submit_owner,
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

fn evidence_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFIT_EVIDENCE_PORT_REQUIRED",
        "Native Benefits evidence binding requires the current Files attachment port, which composition has not mounted.",
    )
}

fn learning_port_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED",
        "Native Benefits election requires the current Learning eligibility consumption port, which composition has not mounted.",
    )
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

    /// Required plan eligibility code for the election's current plan, read in
    /// the same owner unit. `None` means the plan is not eligibility-gated.
    fn learning_requirement<'a>(
        &'a mut self,
        id: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<String>, Self::Error>;
}

/// Current Records/Files business-evidence port bound to the same serializable
/// owner unit. Composition implements it from the Files owner adapter; the
/// prepared value stays opaque to Benefits and is re-read after the owner
/// effect. It never grants Benefits authority.
pub trait BenefitsEvidencePort: Send {
    type Prepared: Send;

    fn prepare<'a>(
        &'a mut self,
        request: BenefitEvidenceRequest,
    ) -> BenefitsCommandFuture<'a, Option<Self::Prepared>, BusinessError>;

    fn require_current<'a>(
        &'a mut self,
        prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError>;
}

/// Current Learning benefit-consumption port. The Learning owner writes and
/// re-reads the receipt inside the caller's election transaction; Benefits
/// only states the required code/person/effective-date selection.
pub trait BenefitsLearningPort: Send {
    type Prepared: Send;

    fn prepare<'a>(
        &'a mut self,
        request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError>;

    fn validate<'a>(
        &'a mut self,
        request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError>;

    fn require_current<'a>(
        &'a mut self,
        prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError>;
}

pub struct BenefitCommandPorts<'a, V, L> {
    pub evidence: &'a mut V,
    pub learning: &'a mut L,
}

/// Fail-closed owner default used by the retained mounted command entrypoint.
/// It binds no evidence and consumes no Learning receipt.
pub struct NoBenefitPorts;

impl BenefitsEvidencePort for NoBenefitPorts {
    type Prepared = ();

    fn prepare<'a>(
        &'a mut self,
        _request: BenefitEvidenceRequest,
    ) -> BenefitsCommandFuture<'a, Option<Self::Prepared>, BusinessError> {
        Box::pin(async { Err(evidence_port_required()) })
    }

    fn require_current<'a>(
        &'a mut self,
        _prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(evidence_port_required()) })
    }
}

impl BenefitsLearningPort for NoBenefitPorts {
    type Prepared = ();

    fn prepare<'a>(
        &'a mut self,
        _request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError> {
        Box::pin(async { Err(learning_port_required()) })
    }

    fn validate<'a>(
        &'a mut self,
        _request: BenefitLearningRequest,
    ) -> BenefitsCommandFuture<'a, Self::Prepared, BusinessError> {
        Box::pin(async { Err(learning_port_required()) })
    }

    fn require_current<'a>(
        &'a mut self,
        _prepared: &'a Self::Prepared,
    ) -> BenefitsCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(learning_port_required()) })
    }
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
    let mut evidence = NoBenefitPorts;
    let mut learning = NoBenefitPorts;
    let mut ports = BenefitCommandPorts {
        evidence: &mut evidence,
        learning: &mut learning,
    };
    create_election_with_ports(
        actor,
        raw,
        key,
        repository,
        people,
        entitlement,
        &mut ports,
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_election_with_ports<R, P, E, V, L>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    ports: &mut BenefitCommandPorts<'_, V, L>,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    V: BenefitsEvidencePort,
    L: BenefitsLearningPort,
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
    if !input.dependent_ids.is_empty() {
        let verified = repository.verified_dependents(&worker.person_id).await?;
        require_verified_dependents(&input, &verified)?;
    }
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_new_work()?;
    let learning = match plan.required_eligibility_code.as_ref() {
        Some(required_code) => Some(
            ports
                .learning
                .prepare(BenefitLearningRequest {
                    company_id: actor.company_id.as_str().to_owned(),
                    person_id: worker.person_id.clone(),
                    required_code: required_code.clone(),
                    effective_date: window.effective_on.clone(),
                    election_id: id.clone(),
                    command_id: key.as_str().to_owned(),
                    today: current_date(now),
                })
                .await?,
        ),
        None => None,
    };
    let result = repository
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
        .await?;
    if let Some(prepared) = &learning {
        ports.learning.require_current(prepared).await?;
    }
    Ok(result)
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
    let mut evidence = NoBenefitPorts;
    let mut learning = NoBenefitPorts;
    let mut ports = BenefitCommandPorts {
        evidence: &mut evidence,
        learning: &mut learning,
    };
    change_with_ports(
        actor,
        kind,
        id,
        raw,
        key,
        repository,
        people,
        entitlement,
        &mut ports,
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn change_with_ports<R, P, E, V, L>(
    actor: &BenefitsCommandActor,
    kind: BenefitRecordKind,
    id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    ports: &mut BenefitCommandPorts<'_, V, L>,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    V: BenefitsEvidencePort,
    L: BenefitsLearningPort,
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
    let entitlement = entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?;
    if matches!(input.action.as_str(), "publish" | "open") {
        entitlement.require_new_work()?;
    } else {
        entitlement.require_settle_existing()?;
    }
    let evidence = match benefit_evidence_policy(kind, &input.action) {
        Some(policy) if !input.attachment_set_ids.is_empty() => Some(
            ports
                .evidence
                .prepare(BenefitEvidenceRequest {
                    attachment_set_ids: input.attachment_set_ids.clone(),
                    target_type: policy.target_type.to_owned(),
                    purpose: policy.purpose.to_owned(),
                    component_ref: policy.component_ref.to_owned(),
                    target_id: id.to_owned(),
                    expected_target_version: input.expected_version,
                    command_id: key.as_str().to_owned(),
                    binding_type: benefit_evidence_binding_type(kind, &input.action),
                    now: now.get(),
                })
                .await?
                .ok_or_else(evidence_port_required)?,
        ),
        _ => None,
    };
    let learning = if kind == BenefitRecordKind::Election && input.action != "cancel" {
        match repository.learning_requirement(id).await? {
            Some(required_code) => {
                let person_id = state.person_id.clone().ok_or_else(learning_port_required)?;
                let effective_date = state
                    .effective_from
                    .clone()
                    .ok_or_else(learning_port_required)?;
                Some(
                    ports
                        .learning
                        .validate(BenefitLearningRequest {
                            company_id: actor.company_id.as_str().to_owned(),
                            person_id,
                            required_code,
                            effective_date,
                            election_id: id.to_owned(),
                            command_id: key.as_str().to_owned(),
                            today: current_date(now),
                        })
                        .await?,
                )
            }
            None => None,
        }
    } else {
        None
    };
    let changes: BenefitTransitionChanges = transition_changes(&input, &actor.user_id);
    let result = repository
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
        .await?;
    if let Some(prepared) = &evidence {
        ports.evidence.require_current(prepared).await?;
    }
    if let Some(prepared) = &learning {
        ports.learning.require_current(prepared).await?;
    }
    Ok(result)
}

/// Dispatch the retained Benefits command envelope exactly once. `entity` and
/// an optional `id` select the owner action; current Account/IAM resolution and
/// the serializable transaction remain composition responsibilities. This
/// retained entrypoint binds no evidence and consumes no Learning receipt.
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
    let mut evidence = NoBenefitPorts;
    let mut learning = NoBenefitPorts;
    let mut ports = BenefitCommandPorts {
        evidence: &mut evidence,
        learning: &mut learning,
    };
    execute_benefits_command_with_ports(
        actor,
        raw,
        key,
        repository,
        people,
        entitlement,
        &mut ports,
        now,
    )
    .await
}

/// Dispatch through the mounted Records/Files evidence and Learning
/// consumption ports. Unregistered entities and malformed envelopes fail
/// before any owner or port effect.
#[allow(clippy::too_many_arguments)]
pub async fn execute_benefits_command_with_ports<R, P, E, V, L>(
    actor: &BenefitsCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    ports: &mut BenefitCommandPorts<'_, V, L>,
    now: TimestampMillis,
) -> Result<BenefitCommandResult, R::Error>
where
    R: BenefitsCommandRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    V: BenefitsEvidencePort,
    L: BenefitsLearningPort,
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
            change_with_ports(
                actor,
                kind,
                id,
                raw,
                key,
                repository,
                people,
                entitlement,
                ports,
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
                create_election_with_ports(
                    actor,
                    raw,
                    key,
                    repository,
                    people,
                    entitlement,
                    ports,
                    now,
                )
                .await
            }
        },
        Some(_) => Err(malformed().into()),
    }
}
