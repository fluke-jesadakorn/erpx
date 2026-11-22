//! Native Manufacturing accounting-profile command use case (wave 3).
//!
//! The caller owns one serializable Holding transaction. This layer validates
//! the retained create/review input, decides the transition from the current
//! row, replays an exact command receipt and orders the owner write together
//! with the receipt, audit and outbox events in the same unit. It never opens a
//! transaction, touches SQL or writes another owner's rows.
use crate::domain::accounting_profile_wave3::{
    CreatePlan, PROFILE_CREATE_OPERATION, PROFILE_RECORD_TYPE, ProfileDraft, ProfileRow,
    ReviewAction, ReviewInput, ReviewPlan, invalid, plan_create, plan_review,
    require_profile_sources,
};
use erpx_server_runtime::public::http_support::serde_json::{self, Value};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde::Serialize;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type ProfileFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Caller-resolved authority facts. Composition supplies these; the owner never
/// mints a session, permission or Selected scope.
#[derive(Clone, Debug)]
pub struct ProfileActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub authority_json: String,
}

impl ProfileActor {
    pub fn requires(&self, permission: &str) -> Result<(), BusinessError> {
        if self.authority_json.contains(permission) {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "MANUFACTURING_ACCOUNTING_PERMISSION_REQUIRED",
                "Your selected role cannot manage the production accounting profile.",
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoredProfileReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct ProfileLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub actor: &'a ProfileActor,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ProfileInsert<'a> {
    pub id: &'a str,
    pub actor: &'a ProfileActor,
    pub draft: &'a ProfileDraft,
    pub version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct ProfileReview<'a> {
    pub id: &'a str,
    pub plan: &'a ReviewPlan,
    pub actor: &'a ProfileActor,
    pub now: i64,
}

/// Owner repository port. The adapter is the only place allowed to hold SQL.
pub trait ProfileRepository: Send {
    type Error: From<BusinessError> + Send;

    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> ProfileFuture<'a, (), Self::Error>;
    fn request_hash<'a>(&'a mut self, identity: &'a str) -> ProfileFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
        company_id: &'a str,
    ) -> ProfileFuture<'a, Option<StoredProfileReceipt>, Self::Error>;
    fn current<'a>(
        &'a mut self,
        id: &'a str,
        company_id: &'a str,
    ) -> ProfileFuture<'a, Option<ProfileRow>, Self::Error>;
    fn chart_currency<'a>(
        &'a mut self,
        chart_id: &'a str,
        company_id: &'a str,
    ) -> ProfileFuture<'a, Option<String>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a ProfileLedgerEntry<'a>,
    ) -> ProfileFuture<'a, (), Self::Error>;
    fn insert_profile<'a>(
        &'a mut self,
        command: &'a ProfileInsert<'a>,
    ) -> ProfileFuture<'a, (), Self::Error>;
    fn review_profile<'a>(
        &'a mut self,
        command: &'a ProfileReview<'a>,
    ) -> ProfileFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    operation: &'a str,
    company_id: &'a str,
    actor_id: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: ProfileRepository, I: Serialize>(
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    actor: &ProfileActor,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        operation,
        company_id: &actor.company_id,
        actor_id: &actor.user_id,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str(), &actor.company_id).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This Idempotency-Key was already used for a different profile command.",
            )
            .into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMAND_INCOMPLETE",
                "This command did not finish. Retry the same action.",
            )
            .into()),
        };
    }
    Ok((request_hash, None))
}

fn command_key(key: Option<&CommandKey>) -> Result<&CommandKey, BusinessError> {
    key.ok_or_else(|| {
        BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        )
    })
}

/// Create a draft accounting profile at version 1.
#[allow(clippy::too_many_arguments)]
pub async fn create_profile<R: ProfileRepository>(
    actor: &ProfileActor,
    id: String,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.requires("finance.write").map_err(R::Error::from)?;
    let key = command_key(key).map_err(R::Error::from)?;
    let draft = ProfileDraft::parse(raw).map_err(R::Error::from)?;
    let plan = plan_create(id, draft).map_err(R::Error::from)?;
    let operation = PROFILE_CREATE_OPERATION;
    let (request_hash, prior) = prepared(repository, key, operation, actor, &plan.id, raw).await?;
    if let Some(result) = prior {
        return serde_json::from_value(result).map_err(|_| R::Error::from(invalid()));
    }
    let currency = repository
        .chart_currency(&plan.draft.chart_id, &actor.company_id)
        .await?;
    require_profile_sources(&plan.draft, currency.as_deref()).map_err(R::Error::from)?;
    let result = serde_json::json!({
        "id": plan.id,
        "status": "draft",
        "version": plan.version,
    });
    repository
        .ledger(&ProfileLedgerEntry {
            key: key.as_str(),
            operation,
            record_id: &plan.id,
            version: plan.version,
            request_hash: &request_hash,
            result: &result,
            actor,
            now: now.get(),
        })
        .await?;
    repository
        .insert_profile(&ProfileInsert {
            id: &plan.id,
            actor,
            draft: &plan.draft,
            version: plan.version,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

/// Independently approve, reject or retire a profile with a version CAS.
#[allow(clippy::too_many_arguments)]
pub async fn review_profile<R: ProfileRepository>(
    actor: &ProfileActor,
    id: &str,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.requires("finance.approve").map_err(R::Error::from)?;
    let key = command_key(key).map_err(R::Error::from)?;
    let input = ReviewInput::parse(raw).map_err(R::Error::from)?;
    let current = repository.current(id, &actor.company_id).await?;
    let plan = plan_review(&input, current.as_ref(), &actor.user_id).map_err(R::Error::from)?;
    let operation = plan.action.operation();
    let (request_hash, prior) = prepared(repository, key, operation, actor, id, raw).await?;
    if let Some(result) = prior {
        return serde_json::from_value(result).map_err(|_| R::Error::from(invalid()));
    }
    let result = serde_json::json!({
        "id": id,
        "status": plan.action.status(),
        "version": plan.next_version,
    });
    repository
        .ledger(&ProfileLedgerEntry {
            key: key.as_str(),
            operation,
            record_id: id,
            version: plan.next_version,
            request_hash: &request_hash,
            result: &result,
            actor,
            now: now.get(),
        })
        .await?;
    repository
        .review_profile(&ProfileReview {
            id,
            plan: &plan,
            actor,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;

    fn actor(user: &str) -> ProfileActor {
        ProfileActor {
            user_id: user.into(),
            company_id: "company-1".into(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment-1".into(),
            authority_json: json!({"permissions":["finance.write","finance.approve"]}).to_string(),
        }
    }

    #[test]
    fn permission_check_reads_caller_authority() {
        assert!(actor("maker").requires("finance.write").is_ok());
        let mut denied = actor("maker");
        denied.authority_json = "{}".into();
        assert!(denied.requires("finance.write").is_err());
    }

    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("profile-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }
}
