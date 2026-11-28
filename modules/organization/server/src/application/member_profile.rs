//! Native member-profile save use case.
//!
//! The owner adapter receives one caller-owned serializable transaction and
//! fresh Account/IAM authority from composition. This layer owns command
//! ordering: authority, exact idempotency replay, the optimistic-version plan,
//! the member_profiles row write and the standard receipt/audit/outbox ledger
//! in the same caller transaction. It never opens another transaction and never
//! writes another owner's rows.
use crate::domain::member_profile::{
    MEMBER_PROFILE_RECORD_TYPE, MEMBER_PROFILE_SAVE_OPERATION, MemberProfileActor,
    MemberProfileSaveInput, MemberProfileSavePlan, command_incomplete, idempotency_conflict,
    not_found, plan_save, request_invalid, require_current,
};
use erpx_platform_shared_server::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, TimestampMillis,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type MemberProfileCommandFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredMemberProfileReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event written in the same
/// caller-owned transaction as the member_profiles row change.
#[derive(Clone, Debug)]
pub struct MemberProfileLedgerEntry<'a> {
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

#[derive(Clone, Debug)]
pub struct MemberProfileWrite<'a> {
    pub user_id: &'a str,
    pub company_id: &'a str,
    pub bio: &'a str,
    pub links_json: &'a str,
    pub version: i64,
    pub expected_version: i64,
    pub insert: bool,
    pub key: &'a str,
    pub now: i64,
}

/// Owner repository port. Composition supplies the Postgres adapter; a local
/// fake may return `None` for the transaction handle.
pub trait MemberProfileCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> MemberProfileCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> MemberProfileCommandFuture<'a, Option<StoredMemberProfileReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a MemberProfileLedgerEntry<'a>,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error>;
    fn current_version<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
    ) -> MemberProfileCommandFuture<'a, Option<i64>, Self::Error>;
    fn save_profile<'a>(
        &'a mut self,
        command: &'a MemberProfileWrite<'a>,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    user_id: &'a str,
    company_id: &'a str,
    assignment_id: &'a str,
    role: &'a str,
    work_context: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: MemberProfileCommandRepository, I: Serialize>(
    actor: &MemberProfileActor,
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        company_id: actor.company_id.as_str(),
        assignment_id: &actor.assignment_id,
        role: &actor.role,
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MemberProfileWriteResult {
    pub version: i64,
    pub bio: String,
    pub links: Vec<crate::domain::member_profile::MemberProfileLink>,
}

/// Save the caller's own member profile. Identity membership is resolved by
/// composition before this use case runs; the planner only checks the retained
/// version rule on the supplied owner facts.
pub async fn save_member_profile<R: MemberProfileCommandRepository>(
    actor: &MemberProfileActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<MemberProfileWriteResult, R::Error>
where
    R::Error: From<BusinessError>,
{
    require_current(actor, now)?;
    let input = MemberProfileSaveInput::parse(raw)?;
    let record_id = actor.user_id.as_str();
    let (request_hash, prior) = prepared(
        actor,
        repository,
        key,
        MEMBER_PROFILE_SAVE_OPERATION,
        record_id,
        raw,
    )
    .await?;
    if let Some(result) = prior {
        return Ok(serde_json::from_value(result).map_err(|_| R::Error::from(request_invalid()))?);
    }
    let current = repository
        .current_version(&actor.company_id, record_id)
        .await?;
    let plan: MemberProfileSavePlan = plan_save(input.expected_version, current)?;
    let links_json = input.links_json().map_err(R::Error::from)?;
    let result = MemberProfileWriteResult {
        version: plan.version,
        bio: input.bio.clone(),
        links: input.links.clone(),
    };
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(request_invalid()))?;
    repository
        .ledger(&MemberProfileLedgerEntry {
            key: key.as_str(),
            operation: MEMBER_PROFILE_SAVE_OPERATION,
            record_type: MEMBER_PROFILE_RECORD_TYPE,
            record_id,
            version: plan.version,
            request_hash: &request_hash,
            result: &text,
            state: "saved",
            now: now.get(),
        })
        .await?;
    repository
        .save_profile(&MemberProfileWrite {
            user_id: record_id,
            company_id: actor.company_id.as_str(),
            bio: &input.bio,
            links_json: &links_json,
            version: plan.version,
            expected_version: input.expected_version,
            insert: plan.insert,
            key: key.as_str(),
            now: now.get(),
        })
        .await?;
    Ok(result)
}

/// Fail-closed errors kept here so the HTTP ingress and the application agree
/// on codes without either importing transport types.
pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_MEMBER_STORE_UNAVAILABLE",
        "The member profile store is unavailable.",
    )
}

pub fn profile_not_found() -> BusinessError {
    not_found()
}
