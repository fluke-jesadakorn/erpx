//! Native Executive evidence-access command use case.
//!
//! The caller owns one serializable Holding transaction. This layer validates
//! the retained input, replays an exact idempotency receipt, and orders the
//! owner audit write together with the command receipt, audit event and outbox
//! event in the same unit. It never opens a transaction and never touches
//! another owner's rows.
use crate::domain::executive_evidence::{
    EVIDENCE_ACCESS_OPERATION, EVIDENCE_ACCESS_RECORD_TYPE, EvidenceAccessInput, EvidenceRecord,
    invalid,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type EvidenceFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredEvidenceReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// Caller-owned actor. Composition resolves it from the private session/context;
/// this layer only binds it to the aggregate and the ledger rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceActor {
    pub user_id: String,
    pub company_id: String,
    pub assignment_id: String,
}

/// One command receipt, audit event and outbox event committed inside the
/// caller-owned transaction as the audit rows are appended (CQRS command side).
#[derive(Clone, Debug)]
pub struct EvidenceLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub now: i64,
    pub actor: &'a EvidenceActor,
}

#[derive(Clone, Debug)]
pub struct EvidenceInsert<'a> {
    pub actor: &'a EvidenceActor,
    pub provider: &'a str,
    pub metric: &'a str,
    pub action: &'a str,
    pub field_ids: &'a str,
    pub query_hash: &'a str,
    pub records: &'a [EvidenceRecord],
    pub now: i64,
}

/// Owner repository port. The adapter is the only place allowed to hold SQL.
pub trait EvidenceAccessRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Real adapters return the caller-owned transaction so composed ports
    /// resolve in the same unit; local fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> EvidenceFuture<'a, (), Self::Error>;
    fn request_hash<'a>(&'a mut self, identity: &'a str)
    -> EvidenceFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EvidenceFuture<'a, Option<StoredEvidenceReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a EvidenceLedgerEntry<'a>,
    ) -> EvidenceFuture<'a, (), Self::Error>;
    fn insert_events<'a>(
        &'a mut self,
        command: &'a EvidenceInsert<'a>,
    ) -> EvidenceFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    operation: &'a str,
    actor: &'a str,
    company: &'a str,
    input: &'a I,
}

async fn prepared<R: EvidenceAccessRepository, I: Serialize>(
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    actor: &EvidenceActor,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        operation,
        actor: &actor.user_id,
        company: &actor.company_id,
        input,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different evidence access.",
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

/// Validate and append one evidence-access audit command. `actor` is the
/// caller-owned identity and scope resolved by composition.
pub async fn record_evidence_access<R: EvidenceAccessRepository>(
    actor: &EvidenceActor,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    let key = command_key(key).map_err(R::Error::from)?;
    let input = EvidenceAccessInput::parse(raw).map_err(R::Error::from)?;
    let operation = EVIDENCE_ACCESS_OPERATION;
    let (request_hash, prior) = prepared(repository, key, operation, actor, raw).await?;
    if let Some(result) = prior {
        return serde_json::from_value(result).map_err(|_| R::Error::from(invalid()));
    }
    let result = serde_json::json!({
        "companyId": actor.company_id,
        "actorId": actor.user_id,
        "assignmentId": actor.assignment_id,
        "provider": input.provider,
        "metric": input.metric,
        "action": input.action,
        "count": input.records.len(),
    });
    repository
        .ledger(&EvidenceLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: EVIDENCE_ACCESS_RECORD_TYPE,
            record_id: &actor.user_id,
            version: 1,
            request_hash: &request_hash,
            result: &result,
            now: now.get(),
            actor,
        })
        .await?;
    repository
        .insert_events(&EvidenceInsert {
            actor,
            provider: &input.provider,
            metric: &input.metric,
            action: &input.action,
            field_ids: &input.field_ids,
            query_hash: &input.query_hash,
            records: &input.records,
            now: now.get(),
        })
        .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("evidence-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }

    #[test]
    fn identity_is_stable_for_the_same_input() {
        let actor = EvidenceActor {
            user_id: "user-1".into(),
            company_id: "company-1".into(),
            assignment_id: "assignment-1".into(),
        };
        let first = serde_json::to_string(&CommandIdentity {
            operation: EVIDENCE_ACCESS_OPERATION,
            actor: &actor.user_id,
            company: &actor.company_id,
            input: &json!({"action": "open"}),
        })
        .unwrap();
        let second = serde_json::to_string(&CommandIdentity {
            operation: EVIDENCE_ACCESS_OPERATION,
            actor: &actor.user_id,
            company: &actor.company_id,
            input: &json!({"action": "open"}),
        })
        .unwrap();
        assert_eq!(first, second);
    }
}
