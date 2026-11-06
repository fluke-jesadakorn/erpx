//! Native Operations policy and exception command use cases.
//!
//! Every command is validated against the current selected actor facts, then
//! checked against canonical Records/People facts supplied by owning public
//! ports. The repository owns the one serializable transaction, Workflow
//! receipt, audit/outbox and owner effects. A replay returns the retained
//! receipt only when the key, operation, record and request fingerprint agree.
use crate::domain::field_exceptions::{Exception, Transition, transition};
use crate::domain::field_policy::identifier;
use crate::domain::field_policy::{DraftPolicy, PolicyAction, QueuePolicy, review_policy};
use crate::domain::operations_commands::{
    ChangeExceptionCommand, CommandActor, CommandReceipt, CommandScope, DraftPolicyCommand,
    OpenExceptionCommand, ReviewPolicyCommand, StoredReceipt, exception_record_id,
    exception_reference, fingerprint_json, incomplete_receipt, invalid_receipt, missing_exception,
    missing_policy, queue_policy_record_id, transition_actor, validate_policy, validate_source,
};
use erpx_server_shared::public::record_source_contract::{SourceRecord, SourceReference};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::future::Future;
use std::pin::Pin;

pub type OperationsCommandFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct OpenPlan {
    pub key: CommandKey,
    pub company_id: String,
    pub actor_id: String,
    pub source: SourceRecord,
    pub source_reference_json: String,
    pub policy: QueuePolicy,
    pub policy_snapshot_json: String,
    pub input: OpenExceptionCommand,
    pub due_at: i64,
    pub now: i64,
    pub record_id: String,
    pub request_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct ChangePlan {
    pub key: CommandKey,
    pub company_id: String,
    pub actor_id: String,
    pub row: Exception,
    pub input: ChangeExceptionCommand,
    pub transition: Transition,
    pub now: i64,
    pub request_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct PolicyDraftPlan {
    pub key: CommandKey,
    pub company_id: String,
    pub actor_id: String,
    pub input: DraftPolicy,
    pub now: i64,
    pub record_id: String,
    pub request_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct PolicyReviewPlan {
    pub key: CommandKey,
    pub company_id: String,
    pub actor_id: String,
    pub row: QueuePolicy,
    pub action: PolicyAction,
    pub expected_version: i64,
    pub next_version: i64,
    pub same_creator: bool,
    pub reason: String,
    pub now: i64,
    pub record_id: String,
    pub request_fingerprint: String,
}

/// Owner persistence boundary. Implementations must use the caller's retained
/// serializable unit and write the command receipt, audit/outbox and owner rows
/// atomically. A key may never commit a second time.
pub trait OperationsCommandRepository: Send {
    type Error: From<BusinessError> + Send;
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OperationsCommandFuture<'a, Option<StoredReceipt>, Self::Error>;
    fn exception<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        exception_id: &'a str,
    ) -> OperationsCommandFuture<'a, Option<Exception>, Self::Error>;
    fn policy<'a>(
        &'a mut self,
        company_id: &'a str,
        policy_id: &'a str,
    ) -> OperationsCommandFuture<'a, Option<QueuePolicy>, Self::Error>;
    fn open<'a>(
        &'a mut self,
        plan: OpenPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error>;
    fn change<'a>(
        &'a mut self,
        plan: ChangePlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error>;
    fn draft_policy<'a>(
        &'a mut self,
        plan: PolicyDraftPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error>;
    fn review_policy<'a>(
        &'a mut self,
        plan: PolicyReviewPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error>;
}

/// Current Records source visibility and People/IAM recipient facts. These
/// implementations live with the owning ports in composition; no client source
/// label or HTTP read may replace them.
pub trait OperationsCommandSources: Send {
    type Error: From<BusinessError> + Send;
    fn source<'a>(
        &'a mut self,
        actor: &'a CommandActor,
        reference: &'a SourceReference,
    ) -> OperationsCommandFuture<'a, SourceRecord, Self::Error>;
    fn recipient<'a>(
        &'a mut self,
        actor: &'a CommandActor,
        scope: &'a CommandScope,
        user_id: &'a str,
        review: bool,
    ) -> OperationsCommandFuture<'a, (), Self::Error>;
    fn same_person<'a>(
        &'a mut self,
        actor: &'a CommandActor,
        user_id: &'a str,
    ) -> OperationsCommandFuture<'a, bool, Self::Error>;
}

fn replay(
    stored: Option<StoredReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
    actor: &CommandActor,
) -> Result<Option<CommandReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != operation
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
        || stored.company_id != actor.company_id
        || stored.branch_id != actor.branch_id
        || stored.department_id != actor.department_id
        || stored.actor_id != actor.user_id
    {
        return Err(crate::domain::operations_commands::idempotency_conflict());
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: CommandReceipt = serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id || receipt.version < 1 || receipt.status.is_empty() {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn source_reference_json(source: &SourceRecord) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({
        "kind": source.kind.as_str(),
        "id": source.id,
        "version": source.version,
        "companyId": source.company_id,
        "branchId": source.branch_id,
        "departmentId": source.department_id,
        "title": source.title,
        "status": source.status,
    }))
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_SOURCE_INVALID",
            "The source record could not be retained.",
        )
    })
}

fn policy_snapshot_json(policy: &QueuePolicy) -> Result<String, BusinessError> {
    Ok(OpenExceptionCommand::policy_snapshot(policy)?.to_string())
}

fn check_receipt(
    receipt: CommandReceipt,
    record_id: &str,
    status: &str,
    version: i64,
) -> Result<CommandReceipt, BusinessError> {
    if receipt.id != record_id || receipt.status != status || receipt.version != version {
        return Err(invalid_receipt());
    }
    Ok(receipt)
}

pub async fn open_operations_exception<R, S>(
    actor: &CommandActor,
    repository: &mut R,
    sources: &mut S,
    input: &OpenExceptionCommand,
    key: &str,
    now: i64,
) -> Result<CommandReceipt, R::Error>
where
    R: OperationsCommandRepository,
    S: OperationsCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    input.validate()?;
    let key = CommandKey::new(key)?;
    let record_id = exception_record_id(&key);
    let operation = "operations.exception.open";
    let fingerprint = fingerprint_json(operation, &record_id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
        actor,
    )? {
        return Ok(check_receipt(receipt, &record_id, "open", 1)?);
    }
    let reference = input.reference();
    let source = sources
        .source(actor, &reference)
        .await
        .map_err(R::Error::from)?;
    validate_source(actor, &source, &reference)?;
    let policy = repository
        .policy(&actor.company_id, &input.policy_id)
        .await?
        .ok_or_else(missing_policy)?;
    validate_policy(
        actor,
        &policy,
        &input.policy_id,
        input.policy_version,
        source.kind,
    )?;
    let due_at = input.due_at(&policy, now)?;
    let scope = CommandScope::from_source(&source);
    sources
        .recipient(actor, &scope, &input.owner_id, false)
        .await
        .map_err(R::Error::from)?;
    let receipt = repository
        .open(OpenPlan {
            key,
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            source: source.clone(),
            source_reference_json: source_reference_json(&source)?,
            policy: policy.clone(),
            policy_snapshot_json: policy_snapshot_json(&policy)?,
            input: input.clone(),
            due_at,
            now,
            record_id: record_id.clone(),
            request_fingerprint: fingerprint,
        })
        .await?;
    Ok(check_receipt(receipt, &record_id, "open", 1)?)
}

pub async fn change_operations_exception<R, S>(
    actor: &CommandActor,
    repository: &mut R,
    sources: &mut S,
    input: &ChangeExceptionCommand,
    key: &str,
    now: i64,
) -> Result<CommandReceipt, R::Error>
where
    R: OperationsCommandRepository,
    S: OperationsCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_write(now)?;
    let transition_input = input.transition_input()?;
    let action = transition_input.action;
    if action.manages() {
        actor.require_review(now)?;
    }
    let key = CommandKey::new(key)?;
    identifier(&input.id)?;
    let operation = format!("operations.exception.{}", action.as_str());
    let row = repository
        .exception(
            &actor.company_id,
            actor.branch_id.as_deref(),
            actor.department_id.as_deref(),
            &input.id,
        )
        .await?
        .ok_or_else(missing_exception)?;
    if row.company_id != actor.company_id {
        return Err(missing_exception().into());
    }
    let reference = exception_reference(&row)?;
    let source = sources
        .source(actor, &reference)
        .await
        .map_err(R::Error::from)?;
    validate_source(actor, &source, &reference)?;
    let same_person_owner = sources
        .same_person(actor, &row.owner_id)
        .await
        .map_err(R::Error::from)?;
    let same_person_resolver = match row.resolved_by.as_deref() {
        Some(user) => sources
            .same_person(actor, user)
            .await
            .map_err(R::Error::from)?,
        None => false,
    };
    let facts = transition_actor(actor, same_person_owner, same_person_resolver);
    let plan = transition(&facts, &row, &transition_input)?;
    let scope = CommandScope::from_exception(&row);
    sources
        .recipient(actor, &scope, &actor.user_id, action.manages())
        .await
        .map_err(R::Error::from)?;
    if action == crate::domain::field_exceptions::Action::Assign {
        sources
            .recipient(actor, &scope, plan.owner.as_str(), false)
            .await
            .map_err(R::Error::from)?;
    }
    if action == crate::domain::field_exceptions::Action::Escalate {
        let escalation_owner = plan.escalation_owner.as_deref().ok_or_else(|| {
            BusinessError::invalid(
                "OPERATIONS_INPUT_INVALID",
                "Choose a current escalation reviewer.",
            )
        })?;
        sources
            .recipient(actor, &scope, escalation_owner, true)
            .await
            .map_err(R::Error::from)?;
    }
    let fingerprint = fingerprint_json(&operation, &row.id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        &row.id,
        &fingerprint,
        actor,
    )? {
        return check_receipt(receipt, &row.id, &plan.status, plan.version).map_err(Into::into);
    }
    let receipt = repository
        .change(ChangePlan {
            key,
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            row: row.clone(),
            input: input.clone(),
            transition: plan.clone(),
            now,
            request_fingerprint: fingerprint,
        })
        .await?;
    check_receipt(receipt, &row.id, &plan.status, plan.version).map_err(Into::into)
}

pub async fn draft_operations_policy<R, S>(
    actor: &CommandActor,
    repository: &mut R,
    _sources: &mut S,
    input: &DraftPolicyCommand,
    key: &str,
    now: i64,
) -> Result<CommandReceipt, R::Error>
where
    R: OperationsCommandRepository,
    S: OperationsCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_review(now)?;
    actor.require_holding_scope()?;
    let validated = input.validate()?;
    let key = CommandKey::new(key)?;
    let record_id = queue_policy_record_id(&key);
    let operation = "operations.policy.draft";
    let fingerprint = fingerprint_json(operation, &record_id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        operation,
        &record_id,
        &fingerprint,
        actor,
    )? {
        return Ok(check_receipt(receipt, &record_id, "draft", 1)?);
    }
    let receipt = repository
        .draft_policy(PolicyDraftPlan {
            key,
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            input: validated,
            now,
            record_id: record_id.clone(),
            request_fingerprint: fingerprint,
        })
        .await?;
    check_receipt(receipt, &record_id, "draft", 1).map_err(Into::into)
}

pub async fn review_operations_policy<R, S>(
    actor: &CommandActor,
    repository: &mut R,
    sources: &mut S,
    input: &ReviewPolicyCommand,
    key: &str,
    now: i64,
) -> Result<CommandReceipt, R::Error>
where
    R: OperationsCommandRepository,
    S: OperationsCommandSources,
    R::Error: From<S::Error> + From<BusinessError>,
{
    actor.require_review(now)?;
    actor.require_holding_scope()?;
    let (action, expected_version, reason) = input.validate()?;
    let key = CommandKey::new(key)?;
    let row = repository
        .policy(&actor.company_id, &input.id)
        .await?
        .ok_or_else(missing_policy)?;
    if row.company_id != actor.company_id {
        return Err(missing_policy().into());
    }
    let same_creator = sources
        .same_person(actor, &row.created_by)
        .await
        .map_err(R::Error::from)?;
    let next_version = review_policy(&row, action, expected_version, same_creator, &reason)?;
    let operation = format!("operations.policy.{}", action.as_str());
    let fingerprint = fingerprint_json(&operation, &input.id, input)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        &input.id,
        &fingerprint,
        actor,
    )? {
        return check_receipt(receipt, &input.id, action.status(), next_version)
            .map_err(Into::into);
    }
    let receipt = repository
        .review_policy(PolicyReviewPlan {
            key,
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            row: row.clone(),
            action,
            expected_version,
            next_version,
            same_creator,
            reason,
            now,
            record_id: input.id.clone(),
            request_fingerprint: fingerprint,
        })
        .await?;
    check_receipt(receipt, &input.id, action.status(), next_version).map_err(Into::into)
}
