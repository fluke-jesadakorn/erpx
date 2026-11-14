//! Native People workforce command use cases.
//!
//! The owner adapter receives one caller-owned serializable Holding transaction
//! and fresh Account/IAM authority from composition. This layer owns command
//! ordering: actor policy, exact idempotency replay, current owner facts,
//! optimistic person/engagement/compensation versions, independent review and
//! the Organization/Identity/Finance handoffs. It never opens another
//! transaction and never writes another owner's rows.
use crate::domain::workforce::{
    COMPENSATION_CREATE_OPERATION, COMPENSATION_TRANSITION_OPERATION, CompensationCreateInput,
    CompensationRow, CompensationTransitionInput, CompensationWriteResult,
    DIRECTORY_CREATE_OPERATION, DIRECTORY_STATUS_OPERATION, DIRECTORY_UPDATE_OPERATION,
    ENGAGEMENT_CREATE_OPERATION, ENGAGEMENT_TRANSITION_OPERATION, EngagementCreateInput,
    EngagementRow, EngagementTransitionInput, EngagementWriteResult, PERSON_CREATE_OPERATION,
    PersonCreateInput, PersonProfile, PersonRecordInput, PersonStatusFacts, PersonStatusInput,
    PersonStatusResult, PersonUpdateInput, PersonWriteResult, WorkforceCommandActor,
    WorkforceCompensationSummary, WorkforcePersonDetail, WorkforcePersonRow, command_incomplete,
    compensation_not_found, engagement_not_found, idempotency_conflict, not_found,
    plan_compensation_transition, plan_engagement_transition, plan_person_status, request_invalid,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type WorkforceCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredWorkforceReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event. The adapter writes them
/// in the same caller-owned transaction as the People row change.
#[derive(Clone, Debug)]
pub struct WorkforceLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub state: &'a str,
    pub now: i64,
}

/// Desired engagement placement resolved by the use case. Organization owns
/// level/position validation and its own history rows.
#[derive(Clone, Debug, Default)]
pub struct ResolvedPlacement {
    pub level_id: Option<String>,
    pub position_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PersonInsert<'a> {
    pub id: &'a str,
    pub employee_id: &'a str,
    pub engagement_id: &'a str,
    pub profile: &'a PersonProfile,
    pub placement: &'a ResolvedPlacement,
    pub linked_user_id: Option<&'a str>,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct PersonUpdate<'a> {
    pub id: &'a str,
    pub engagement_id: &'a str,
    pub profile: &'a PersonProfile,
    pub placement: &'a ResolvedPlacement,
    pub linked_user_id: Option<&'a str>,
    pub work_email: Option<&'a str>,
    pub work_phone: Option<&'a str>,
    pub expected_version: i64,
    pub expected_engagement_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct PersonRecordInsert<'a> {
    pub id: &'a str,
    pub input: &'a PersonRecordInput,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct PersonStatusWrite<'a> {
    pub id: &'a str,
    pub status: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct EngagementInsert<'a> {
    pub id: &'a str,
    pub input: &'a EngagementCreateInput,
    pub created_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct EngagementTransitionWrite<'a> {
    pub id: &'a str,
    pub status: &'a str,
    pub end_date: Option<&'a str>,
    pub expected_version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CompensationInsert<'a> {
    pub id: &'a str,
    pub input: &'a CompensationCreateInput,
    pub components_json: &'a str,
    pub created_by: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CompensationTransitionWrite<'a> {
    pub id: &'a str,
    pub status: &'a str,
    pub approved_by: &'a str,
    pub expected_version: i64,
    pub now: i64,
}

pub trait WorkforceCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> WorkforceCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> WorkforceCommandFuture<'a, Option<StoredWorkforceReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a WorkforceLedgerEntry<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn person<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
        engagement_id: Option<&'a str>,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> WorkforceCommandFuture<'a, Option<WorkforcePersonRow>, Self::Error>;
    fn person_exists<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> WorkforceCommandFuture<'a, bool, Self::Error>;
    fn person_status_facts<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
    ) -> WorkforceCommandFuture<'a, Option<PersonStatusFacts>, Self::Error>;
    fn email_available<'a>(
        &'a mut self,
        company: &'a CompanyId,
        email: &'a str,
        person_id: &'a str,
    ) -> WorkforceCommandFuture<'a, bool, Self::Error>;
    fn create_person<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a PersonInsert<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn create_person_record<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a PersonRecordInsert<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn update_person<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a PersonUpdate<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn set_person_status<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a PersonStatusWrite<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn engagement<'a>(
        &'a mut self,
        company: &'a CompanyId,
        engagement_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> WorkforceCommandFuture<'a, Option<EngagementRow>, Self::Error>;
    fn create_engagement<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a EngagementInsert<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn transition_engagement<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a EngagementTransitionWrite<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn compensation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        compensation_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> WorkforceCommandFuture<'a, Option<CompensationRow>, Self::Error>;
    fn compensation_evaluation_draft<'a>(
        &'a mut self,
        compensation_id: &'a str,
    ) -> WorkforceCommandFuture<'a, bool, Self::Error>;
    fn overlapping_compensation<'a>(
        &'a mut self,
        engagement_id: &'a str,
        compensation_id: &'a str,
        effective_from: &'a str,
        effective_to: Option<&'a str>,
    ) -> WorkforceCommandFuture<'a, bool, Self::Error>;
    fn create_compensation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a CompensationInsert<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn transition_compensation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a CompensationTransitionWrite<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error>;
    fn person_compensation<'a>(
        &'a mut self,
        engagement_id: &'a str,
    ) -> WorkforceCommandFuture<'a, Option<WorkforceCompensationSummary>, Self::Error>;

    /// Lends the caller's transaction to the composed Organization placement
    /// port. The default refuses, so no non-SQL adapter can skip the owner
    /// validation and history.
    fn apply_placement<'a, P: StaffPlacementPort>(
        &'a mut self,
        _placement: &'a mut P,
        _request: &'a StaffPlacementRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error> {
        Box::pin(async { Err(crate::domain::workforce::staff_placement_port_unavailable().into()) })
    }

    fn verify_account_link<'a, A: PersonAccountLinkPort>(
        &'a mut self,
        _account_link: &'a mut A,
        _request: &'a PersonAccountLinkRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error> {
        Box::pin(async { Err(crate::domain::workforce::account_link_port_unavailable().into()) })
    }

    fn revoke_company_access<'a, A: PersonAccountLinkPort>(
        &'a mut self,
        _account_link: &'a mut A,
        _company_id: &'a CompanyId,
        _user_id: &'a str,
        _person_id: &'a str,
        _now: i64,
    ) -> WorkforceCommandFuture<'a, (), Self::Error> {
        Box::pin(async { Err(crate::domain::workforce::account_link_port_unavailable().into()) })
    }

    fn snapshot_compensation_money<'a, M: CompensationMoneyPort>(
        &'a mut self,
        _money: &'a mut M,
        _request: &'a CompensationMoneyRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), Self::Error> {
        Box::pin(async {
            Err(crate::domain::workforce::compensation_money_port_unavailable().into())
        })
    }
}

/// Organization-owned placement validation and side effects: graph guard,
/// manager/branch/department checks, level history, position moves, roster
/// exits and staff defaults. People writes only its own engagement columns.
pub struct StaffPlacementRequest<'a> {
    pub company_id: &'a CompanyId,
    pub command_key: &'a str,
    pub person_id: &'a str,
    pub engagement_id: &'a str,
    pub branch_id: &'a str,
    pub department_id: Option<&'a str>,
    pub manager_person_id: Option<&'a str>,
    pub level_id: Option<&'a str>,
    pub position_id: Option<&'a str>,
    pub active_engagement: bool,
    pub start_date: &'a str,
    pub currency: &'a str,
    pub accept_recommended_pay: bool,
    pub accept_recommended_leave: bool,
    pub reason: &'a str,
    pub now: i64,
}

pub trait StaffPlacementPort: Send {
    fn apply<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a StaffPlacementRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError>;
}

/// Explicit fail-closed default. No in-memory adapter can place a person.
pub struct NoStaffPlacementPort;

impl StaffPlacementPort for NoStaffPlacementPort {
    fn apply<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _request: &'a StaffPlacementRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::workforce::staff_placement_port_unavailable()) })
    }
}

pub struct PersonAccountLinkRequest<'a> {
    pub company_id: &'a CompanyId,
    pub person_id: &'a str,
    pub user_id: &'a str,
    pub email: &'a str,
    pub current_user_id: Option<&'a str>,
    pub now: i64,
}

/// Identity-owned member/account link verification and company access
/// revocation. People never writes IAM rows itself.
pub trait PersonAccountLinkPort: Send {
    fn verify<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a PersonAccountLinkRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError>;
    fn revoke_company_access<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a CompanyId,
        user_id: &'a str,
        person_id: &'a str,
        now: i64,
    ) -> WorkforceCommandFuture<'a, (), BusinessError>;
}

pub struct NoPersonAccountLinkPort;

impl PersonAccountLinkPort for NoPersonAccountLinkPort {
    fn verify<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _request: &'a PersonAccountLinkRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::workforce::account_link_port_unavailable()) })
    }
    fn revoke_company_access<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _company_id: &'a CompanyId,
        _user_id: &'a str,
        _person_id: &'a str,
        _now: i64,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::workforce::account_link_port_unavailable()) })
    }
}

pub struct CompensationMoneyRequest<'a> {
    pub company_id: &'a CompanyId,
    pub revision_id: &'a str,
    pub currency: &'a str,
    pub effective_from: &'a str,
    pub amounts: &'a [(String, i64)],
    pub now: i64,
}

/// Finance-owned money transaction snapshot for a compensation revision.
pub trait CompensationMoneyPort: Send {
    fn snapshot<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a CompensationMoneyRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError>;
}

pub struct NoCompensationMoneyPort;

impl CompensationMoneyPort for NoCompensationMoneyPort {
    fn snapshot<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _request: &'a CompensationMoneyRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Err(crate::domain::workforce::compensation_money_port_unavailable()) })
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

async fn prepared<R: WorkforceCommandRepository, I: Serialize>(
    actor: &WorkforceCommandActor,
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        role: &actor.role,
        company_id: actor.company_id.as_str(),
        branch_id: actor.branch_id.as_ref().map(|id| id.as_str()),
        department_id: actor.department_id.as_ref().map(|id| id.as_str()),
        work_context: &actor.work_context,
        operation,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(request_invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
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

fn person_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PERSON_CHANGED",
        "Reload the person before saving changes.",
    )
}

fn engagement_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ENGAGEMENT_CHANGED",
        "Reload the engagement before saving changes.",
    )
}

fn level_required() -> BusinessError {
    BusinessError::invalid(
        "LEVEL_REQUIRED",
        "Assign a staff level before updating an active engagement.",
    )
}

fn self_manager() -> BusinessError {
    BusinessError::invalid("SELF_MANAGER", "A person cannot report to themselves.")
}

fn compensation_overlap() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMPENSATION_PERIOD_OVERLAP",
        "Another approved compensation revision covers this period.",
    )
}

fn currency_mismatch() -> BusinessError {
    BusinessError::invalid(
        "CURRENCY_MISMATCH",
        "Compensation must use the engagement currency.",
    )
}

/// Directory create/update. `person_id` is `None` for a new person.
pub async fn save_person<
    R: WorkforceCommandRepository,
    P: StaffPlacementPort,
    A: PersonAccountLinkPort,
>(
    actor: &WorkforceCommandActor,
    person_id: Option<&str>,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    placement: &mut P,
    account_link: &mut A,
    now: TimestampMillis,
) -> Result<PersonWriteResult, R::Error> {
    actor.require_manage(now)?;
    if let Some(person_id) = person_id {
        let input = PersonUpdateInput::parse(raw)?;
        update_person(
            actor,
            person_id,
            &input,
            key,
            repository,
            placement,
            account_link,
            now,
        )
        .await
    } else {
        let input = PersonCreateInput::parse(raw)?;
        create_person(actor, &input, key, repository, placement, account_link, now).await
    }
}

async fn create_person<
    R: WorkforceCommandRepository,
    P: StaffPlacementPort,
    A: PersonAccountLinkPort,
>(
    actor: &WorkforceCommandActor,
    input: &PersonCreateInput,
    key: &CommandKey,
    repository: &mut R,
    placement: &mut P,
    account_link: &mut A,
    now: TimestampMillis,
) -> Result<PersonWriteResult, R::Error> {
    input.profile.require_valid_placement(actor)?;
    let id = format!("person-{}", key.as_str());
    let engagement_id = format!("eng-{}", key.as_str());
    let employee_id = format!("employee-{}", key.as_str());
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        DIRECTORY_CREATE_OPERATION,
        &id,
        input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    if !repository
        .email_available(&actor.company_id, &input.profile.email, &id)
        .await?
    {
        return Err(email_taken().into());
    }
    if input
        .profile
        .manager_person_id
        .as_ref()
        .is_some_and(|manager| manager.as_str() == id)
    {
        return Err(self_manager().into());
    }
    let linked_user_id = link_account(
        repository,
        actor,
        None,
        &id,
        &input.profile,
        account_link,
        now,
    )
    .await?;
    let resolved = ResolvedPlacement {
        level_id: input
            .profile
            .level_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        position_id: input
            .profile
            .position_id
            .as_ref()
            .and_then(|value| value.as_ref())
            .map(|id| id.as_str().to_owned()),
    };
    apply_placement(
        repository,
        actor,
        &id,
        &engagement_id,
        &input.profile,
        &resolved,
        false,
        placement,
        key,
        now,
    )
    .await?;
    let result = serde_json::to_value(&PersonWriteResult {
        id: id.clone(),
        version: 1,
    })
    .map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: DIRECTORY_CREATE_OPERATION,
            record_type: crate::domain::workforce::PERSON_RECORD_TYPE,
            record_id: &id,
            version: 1,
            request_hash: &request_hash,
            result: &result,
            state: "active",
            now: now.get(),
        })
        .await?;
    repository
        .create_person(
            &actor.company_id,
            &PersonInsert {
                id: &id,
                employee_id: &employee_id,
                engagement_id: &engagement_id,
                profile: &input.profile,
                placement: &resolved,
                linked_user_id: linked_user_id.as_deref(),
                now: now.get(),
            },
        )
        .await?;
    Ok(PersonWriteResult { id, version: 1 })
}

async fn update_person<
    R: WorkforceCommandRepository,
    P: StaffPlacementPort,
    A: PersonAccountLinkPort,
>(
    actor: &WorkforceCommandActor,
    person_id: &str,
    input: &PersonUpdateInput,
    key: &CommandKey,
    repository: &mut R,
    placement: &mut P,
    account_link: &mut A,
    now: TimestampMillis,
) -> Result<PersonWriteResult, R::Error> {
    input.profile.require_valid_placement(actor)?;
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        DIRECTORY_UPDATE_OPERATION,
        person_id,
        input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let current = repository
        .person(
            &actor.company_id,
            person_id,
            input.engagement_id.as_ref().map(|id| id.as_str()),
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(not_found)?;
    if current.version != input.expected_version {
        return Err(person_changed().into());
    }
    let engagement_id = current.engagement_id.clone().ok_or_else(not_found)?;
    let current_engagement_version = current.engagement_version.ok_or_else(not_found)?;
    if input
        .expected_engagement_version
        .is_some_and(|expected| expected != current_engagement_version)
    {
        return Err(engagement_changed().into());
    }
    if input
        .profile
        .manager_person_id
        .as_ref()
        .is_some_and(|manager| manager.as_str() == person_id)
    {
        return Err(self_manager().into());
    }
    if !repository
        .email_available(&actor.company_id, &input.profile.email, person_id)
        .await?
    {
        return Err(email_taken().into());
    }
    let level_id = input
        .profile
        .level_id
        .as_ref()
        .map(|id| id.as_str().to_owned())
        .or_else(|| current.level_id.clone());
    if current.engagement_status.as_deref() == Some("active") && level_id.is_none() {
        return Err(level_required().into());
    }
    let position_id = match &input.profile.position_id {
        None => current.position_id.clone(),
        Some(value) => value.as_ref().map(|id| id.as_str().to_owned()),
    };
    let linked_user_id = link_account(
        repository,
        actor,
        current.user_id.as_deref(),
        person_id,
        &input.profile,
        account_link,
        now,
    )
    .await?;
    let resolved = ResolvedPlacement {
        level_id,
        position_id,
    };
    apply_placement(
        repository,
        actor,
        person_id,
        &engagement_id,
        &input.profile,
        &resolved,
        current.engagement_status.as_deref() == Some("active"),
        placement,
        key,
        now,
    )
    .await?;
    let work_email = match &input.profile.work_email {
        None => current.work_email.as_deref(),
        Some(value) => value.as_deref(),
    };
    let work_phone = match &input.profile.work_phone {
        None => current.work_phone.as_deref(),
        Some(value) => value.as_deref(),
    };
    let version = input.expected_version + 1;
    let result = serde_json::to_value(&PersonWriteResult {
        id: person_id.to_owned(),
        version,
    })
    .map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: DIRECTORY_UPDATE_OPERATION,
            record_type: crate::domain::workforce::PERSON_RECORD_TYPE,
            record_id: person_id,
            version,
            request_hash: &request_hash,
            result: &result,
            state: "active",
            now: now.get(),
        })
        .await?;
    repository
        .update_person(
            &actor.company_id,
            &PersonUpdate {
                id: person_id,
                engagement_id: &engagement_id,
                profile: &input.profile,
                placement: &resolved,
                linked_user_id: linked_user_id.as_deref().or(current.user_id.as_deref()),
                work_email,
                work_phone,
                expected_version: input.expected_version,
                expected_engagement_version: input
                    .expected_engagement_version
                    .unwrap_or(current_engagement_version),
                now: now.get(),
            },
        )
        .await?;
    Ok(PersonWriteResult {
        id: person_id.to_owned(),
        version,
    })
}

/// Standalone `people.person.create` with no placement or login.
pub async fn create_person_record<R: WorkforceCommandRepository>(
    actor: &WorkforceCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PersonRecordResult, R::Error> {
    actor.require_manage(now)?;
    let input = PersonRecordInput::parse(raw)?;
    let id = format!("person-{}", key.as_str());
    let (request_hash, prior) =
        prepared(actor, repository, key, PERSON_CREATE_OPERATION, &id, &input).await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let result = PersonRecordResult {
        id: id.clone(),
        legal_name: input.legal_name.clone(),
        status: "active".to_owned(),
        version: 1,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: PERSON_CREATE_OPERATION,
            record_type: crate::domain::workforce::PERSON_RECORD_TYPE,
            record_id: &id,
            version: 1,
            request_hash: &request_hash,
            result: &text,
            state: "active",
            now: now.get(),
        })
        .await?;
    repository
        .create_person_record(
            &actor.company_id,
            &PersonRecordInsert {
                id: &id,
                input: &input,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonRecordResult {
    pub id: String,
    pub legal_name: String,
    pub status: String,
    pub version: i64,
}

pub async fn set_person_status<
    R: WorkforceCommandRepository,
    P: StaffPlacementPort,
    A: PersonAccountLinkPort,
>(
    actor: &WorkforceCommandActor,
    person_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    placement: &mut P,
    account_link: &mut A,
    now: TimestampMillis,
) -> Result<PersonStatusResult, R::Error> {
    actor.require_manage(now)?;
    let input = PersonStatusInput::parse(raw)?;
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        DIRECTORY_STATUS_OPERATION,
        person_id,
        &input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let facts = repository
        .person_status_facts(&actor.company_id, person_id)
        .await?
        .ok_or_else(not_found)?;
    let plan = plan_person_status(actor, &input, &facts)?;
    if input.status == "active" && facts.engagement_status.as_deref() != Some("ended") {
        let current = repository
            .person(
                &actor.company_id,
                person_id,
                None,
                actor.branch_id.as_ref().map(|id| id.as_str()),
                actor.department_id.as_ref().map(|id| id.as_str()),
            )
            .await?
            .ok_or_else(not_found)?;
        let request = StaffPlacementRequest {
            company_id: &actor.company_id,
            command_key: key.as_str(),
            person_id,
            engagement_id: current.engagement_id.as_deref().unwrap_or_default(),
            branch_id: current.branch_id.as_deref().unwrap_or_default(),
            department_id: current.department_id.as_deref(),
            manager_person_id: current.manager_person_id.as_deref(),
            level_id: current.level_id.as_deref(),
            position_id: current.position_id.as_deref(),
            active_engagement: true,
            start_date: current.start_date.as_deref().unwrap_or_default(),
            currency: current.currency.as_deref().unwrap_or_default(),
            accept_recommended_pay: false,
            accept_recommended_leave: false,
            reason: "Person status change",
            now: now.get(),
        };
        repository.apply_placement(placement, &request).await?;
    }
    let result = PersonStatusResult {
        id: person_id.to_owned(),
        status: plan.status.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: DIRECTORY_STATUS_OPERATION,
            record_type: crate::domain::workforce::PERSON_RECORD_TYPE,
            record_id: person_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.status,
            now: now.get(),
        })
        .await?;
    // Revoke this Company's access before the status write; restoration always
    // requires a separate explicit assignment.
    if plan.status != "active"
        && let Some(user_id) = facts.user_id.as_deref()
    {
        repository
            .revoke_company_access(
                account_link,
                &actor.company_id,
                user_id,
                person_id,
                now.get(),
            )
            .await?;
    }
    repository
        .set_person_status(
            &actor.company_id,
            &PersonStatusWrite {
                id: person_id,
                status: &plan.status,
                expected_version: input.expected_version,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

pub async fn create_engagement<R: WorkforceCommandRepository>(
    actor: &WorkforceCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<EngagementWriteResult, R::Error> {
    actor.require_manage(now)?;
    let input = EngagementCreateInput::parse(raw)?;
    let id = format!("eng-{}", key.as_str());
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        ENGAGEMENT_CREATE_OPERATION,
        &id,
        &input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    if !repository
        .person_exists(&actor.company_id, input.person_id.as_str())
        .await?
    {
        return Err(not_found().into());
    }
    let result = EngagementWriteResult {
        id: id.clone(),
        person_id: input.person_id.as_str().to_owned(),
        status: "onboarding".to_owned(),
        version: 1,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: ENGAGEMENT_CREATE_OPERATION,
            record_type: crate::domain::workforce::ENGAGEMENT_RECORD_TYPE,
            record_id: &id,
            version: 1,
            request_hash: &request_hash,
            result: &text,
            state: "onboarding",
            now: now.get(),
        })
        .await?;
    repository
        .create_engagement(
            &actor.company_id,
            &EngagementInsert {
                id: &id,
                input: &input,
                created_by: &actor.user_id,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

pub async fn transition_engagement<R: WorkforceCommandRepository, P: StaffPlacementPort>(
    actor: &WorkforceCommandActor,
    engagement_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    placement: &mut P,
    now: TimestampMillis,
) -> Result<EngagementWriteResult, R::Error> {
    actor.require_manage(now)?;
    let input = EngagementTransitionInput::parse(raw)?;
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        ENGAGEMENT_TRANSITION_OPERATION,
        engagement_id,
        &input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .engagement(
            &actor.company_id,
            engagement_id,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(engagement_not_found)?;
    let plan = plan_engagement_transition(actor, &input, &row)?;
    if plan.status == "active" {
        let request = StaffPlacementRequest {
            company_id: &actor.company_id,
            command_key: key.as_str(),
            person_id: &row.person_id,
            engagement_id,
            branch_id: row.branch_id.as_deref().unwrap_or_default(),
            department_id: row.department_id.as_deref(),
            manager_person_id: row.manager_person_id.as_deref(),
            level_id: row.level_id.as_deref(),
            position_id: row.position_id.as_deref(),
            active_engagement: true,
            start_date: &row.start_date,
            currency: &row.currency,
            accept_recommended_pay: false,
            accept_recommended_leave: false,
            reason: "Engagement activated",
            now: now.get(),
        };
        repository.apply_placement(placement, &request).await?;
    }
    let result = EngagementWriteResult {
        id: engagement_id.to_owned(),
        person_id: row.person_id.clone(),
        status: plan.status.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: ENGAGEMENT_TRANSITION_OPERATION,
            record_type: crate::domain::workforce::ENGAGEMENT_RECORD_TYPE,
            record_id: engagement_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.status,
            now: now.get(),
        })
        .await?;
    repository
        .transition_engagement(
            &actor.company_id,
            &EngagementTransitionWrite {
                id: engagement_id,
                status: &plan.status,
                end_date: plan.end_date.as_deref(),
                expected_version: input.expected_version,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

pub async fn create_compensation<R: WorkforceCommandRepository, M: CompensationMoneyPort>(
    actor: &WorkforceCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    money: &mut M,
    now: TimestampMillis,
) -> Result<CompensationWriteResult, R::Error> {
    actor.require_manage(now)?;
    let input = CompensationCreateInput::parse(raw)?;
    let id = format!("comp-{}", key.as_str());
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        COMPENSATION_CREATE_OPERATION,
        &id,
        &input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let engagement = repository
        .engagement(
            &actor.company_id,
            input.engagement_id.as_str(),
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(engagement_not_found)?;
    if engagement.currency != input.currency {
        return Err(currency_mismatch().into());
    }
    let components_json = input.components_json()?;
    let amounts = input.money_amounts();
    repository
        .snapshot_compensation_money(
            money,
            &CompensationMoneyRequest {
                company_id: &actor.company_id,
                revision_id: &id,
                currency: &input.currency,
                effective_from: &input.effective_from,
                amounts: &amounts,
                now: now.get(),
            },
        )
        .await?;
    let result = CompensationWriteResult {
        id: id.clone(),
        status: "draft".to_owned(),
        version: 1,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: COMPENSATION_CREATE_OPERATION,
            record_type: crate::domain::workforce::COMPENSATION_RECORD_TYPE,
            record_id: &id,
            version: 1,
            request_hash: &request_hash,
            result: &text,
            state: "draft",
            now: now.get(),
        })
        .await?;
    repository
        .create_compensation(
            &actor.company_id,
            &CompensationInsert {
                id: &id,
                input: &input,
                components_json: &components_json,
                created_by: &actor.user_id,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

pub async fn transition_compensation<R: WorkforceCommandRepository>(
    actor: &WorkforceCommandActor,
    compensation_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CompensationWriteResult, R::Error> {
    actor.require_compensation_approve(now)?;
    let input = CompensationTransitionInput::parse(raw)?;
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        COMPENSATION_TRANSITION_OPERATION,
        compensation_id,
        &input,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let row = repository
        .compensation(
            &actor.company_id,
            compensation_id,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(compensation_not_found)?;
    let evaluation_draft = repository
        .compensation_evaluation_draft(compensation_id)
        .await?;
    let plan = plan_compensation_transition(actor, &input, &row, evaluation_draft)?;
    if plan.status == "approved"
        && repository
            .overlapping_compensation(
                &row.engagement_id,
                compensation_id,
                &row.effective_from,
                row.effective_to.as_deref(),
            )
            .await?
    {
        return Err(compensation_overlap().into());
    }
    let result = CompensationWriteResult {
        id: compensation_id.to_owned(),
        status: plan.status.clone(),
        version: plan.version,
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&WorkforceLedgerEntry {
            key: key.as_str(),
            operation: COMPENSATION_TRANSITION_OPERATION,
            record_type: crate::domain::workforce::COMPENSATION_RECORD_TYPE,
            record_id: compensation_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: &plan.status,
            now: now.get(),
        })
        .await?;
    repository
        .transition_compensation(
            &actor.company_id,
            &CompensationTransitionWrite {
                id: compensation_id,
                status: &plan.status,
                approved_by: &actor.user_id,
                expected_version: input.expected_version,
                now: now.get(),
            },
        )
        .await?;
    Ok(result)
}

pub async fn person_detail<R: WorkforceCommandRepository>(
    actor: &WorkforceCommandActor,
    person_id: &str,
    engagement_id: Option<&str>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<WorkforcePersonDetail, R::Error> {
    actor.require_read(now)?;
    let row = repository
        .person(
            &actor.company_id,
            person_id,
            engagement_id,
            actor.branch_id.as_ref().map(|id| id.as_str()),
            actor.department_id.as_ref().map(|id| id.as_str()),
        )
        .await?
        .ok_or_else(not_found)?;
    let private_access = actor.can_read_compensation(row.user_id.as_deref());
    if !actor.can_manage_people() && !private_access {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERSON_DETAILS_FORBIDDEN",
            "Open the Holding directory to view this person's work contact details.",
        )
        .into());
    }
    let compensation = if private_access {
        match row.engagement_id.as_deref() {
            Some(engagement_id) => repository.person_compensation(engagement_id).await?,
            None => None,
        }
    } else {
        None
    };
    Ok(row.detail(private_access, compensation))
}

fn email_taken() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "EMAIL_TAKEN",
        "Another active person already uses this email.",
    )
}

async fn link_account<R: WorkforceCommandRepository, A: PersonAccountLinkPort>(
    repository: &mut R,
    actor: &WorkforceCommandActor,
    current_user_id: Option<&str>,
    person_id: &str,
    profile: &PersonProfile,
    account_link: &mut A,
    now: TimestampMillis,
) -> Result<Option<String>, R::Error> {
    let Some(user_id) = profile.linked_member_user_id.as_deref() else {
        return Ok(None);
    };
    repository
        .verify_account_link(
            account_link,
            &PersonAccountLinkRequest {
                company_id: &actor.company_id,
                person_id,
                user_id,
                email: &profile.email,
                current_user_id,
                now: now.get(),
            },
        )
        .await?;
    Ok(Some(user_id.to_owned()))
}

#[allow(clippy::too_many_arguments)]
async fn apply_placement<R: WorkforceCommandRepository, P: StaffPlacementPort>(
    repository: &mut R,
    actor: &WorkforceCommandActor,
    person_id: &str,
    engagement_id: &str,
    profile: &PersonProfile,
    resolved: &ResolvedPlacement,
    active_engagement: bool,
    placement: &mut P,
    key: &CommandKey,
    now: TimestampMillis,
) -> Result<(), R::Error> {
    repository
        .apply_placement(
            placement,
            &StaffPlacementRequest {
                company_id: &actor.company_id,
                command_key: key.as_str(),
                person_id,
                engagement_id,
                branch_id: profile.branch_id.as_str(),
                department_id: profile.department_id.as_ref().map(|id| id.as_str()),
                manager_person_id: profile.manager_person_id.as_ref().map(|id| id.as_str()),
                level_id: resolved.level_id.as_deref(),
                position_id: resolved.position_id.as_deref(),
                active_engagement,
                start_date: &profile.start_date,
                currency: &profile.currency,
                accept_recommended_pay: profile.accept_recommended_pay,
                accept_recommended_leave: profile.accept_recommended_leave,
                reason: &profile.reason,
                now: now.get(),
            },
        )
        .await
}
