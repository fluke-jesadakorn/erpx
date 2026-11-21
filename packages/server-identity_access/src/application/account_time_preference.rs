//! Native Account time-preference command use case.
//!
//! The caller owns one serializable Control transaction. This layer validates
//! the retained input, decides create vs change from the current row, replays
//! an exact idempotency receipt, and orders the owner write together with the
//! command receipt, audit event and outbox event in the same unit. It never
//! opens a transaction and never touches another owner's rows.
use crate::domain::account_time_preference::{
    TIME_PREFERENCE_CHANGE_OPERATION, TIME_PREFERENCE_RECORD_TYPE, TimePreference,
    TimePreferenceInput, TimePreferenceWriteKind, changed, invalid, plan_change,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type TimePreferenceFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredTimePreferenceReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event committed inside the
/// caller-owned transaction as the preference row change (CQRS command side).
#[derive(Clone, Debug)]
pub struct TimePreferenceLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct TimePreferenceInsert<'a> {
    pub user_id: &'a str,
    pub mode: &'a str,
    pub time_zone: Option<&'a str>,
    pub hour_cycle: &'a str,
    pub version: i64,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct TimePreferenceUpdate<'a> {
    pub user_id: &'a str,
    pub mode: &'a str,
    pub time_zone: Option<&'a str>,
    pub hour_cycle: &'a str,
    pub expected_version: i64,
    pub version: i64,
    pub now: i64,
}

/// Owner repository port. The adapter is the only place allowed to hold SQL.
pub trait TimePreferenceRepository: Send {
    type Error: From<BusinessError> + Send;

    /// Real adapters return the caller-owned transaction so composed ports
    /// resolve in the same unit; local fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> TimePreferenceFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> TimePreferenceFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> TimePreferenceFuture<'a, Option<StoredTimePreferenceReceipt>, Self::Error>;
    fn current<'a>(
        &'a mut self,
        user_id: &'a str,
    ) -> TimePreferenceFuture<'a, Option<TimePreference>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a TimePreferenceLedgerEntry<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error>;
    fn insert_preference<'a>(
        &'a mut self,
        command: &'a TimePreferenceInsert<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error>;
    fn update_preference<'a>(
        &'a mut self,
        command: &'a TimePreferenceUpdate<'a>,
    ) -> TimePreferenceFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    operation: &'a str,
    user_id: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: TimePreferenceRepository, I: Serialize>(
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        operation,
        user_id: record_id,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different preference.",
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

/// Validate and apply one time-preference change. `user_id` is the caller-owned
/// identity resolved by composition; it is the aggregate key for this owner.
pub async fn change_time_preference<R: TimePreferenceRepository>(
    user_id: &str,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<TimePreference, R::Error> {
    let key = command_key(key).map_err(R::Error::from)?;
    let input = TimePreferenceInput::parse(raw).map_err(R::Error::from)?;
    let operation = TIME_PREFERENCE_CHANGE_OPERATION;
    let (request_hash, prior) = prepared(repository, key, operation, user_id, raw).await?;
    if let Some(result) = prior {
        return serde_json::from_value(result).map_err(|_| R::Error::from(invalid()));
    }
    let current = repository.current(user_id).await?;
    let (kind, version) = plan_change(&input, current.as_ref()).map_err(R::Error::from)?;
    let result = input.clone().into_preference(version);
    let text = serde_json::to_value(&result).map_err(|_| R::Error::from(invalid()))?;
    repository
        .ledger(&TimePreferenceLedgerEntry {
            key: key.as_str(),
            operation,
            record_type: TIME_PREFERENCE_RECORD_TYPE,
            record_id: user_id,
            version,
            request_hash: &request_hash,
            result: &text,
            now: now.get(),
        })
        .await?;
    match kind {
        TimePreferenceWriteKind::Create => {
            repository
                .insert_preference(&TimePreferenceInsert {
                    user_id,
                    mode: &input.mode,
                    time_zone: input.time_zone.as_deref(),
                    hour_cycle: &input.hour_cycle,
                    version,
                    now: now.get(),
                })
                .await?;
        }
        TimePreferenceWriteKind::Change { expected_version } => {
            repository
                .update_preference(&TimePreferenceUpdate {
                    user_id,
                    mode: &input.mode,
                    time_zone: input.time_zone.as_deref(),
                    hour_cycle: &input.hour_cycle,
                    expected_version,
                    version,
                    now: now.get(),
                })
                .await?;
        }
    }
    Ok(result)
}

/// Fail-closed helper for adapters that must reject an optimistic version.
pub fn changed_error() -> BusinessError {
    changed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(version: i64) -> Value {
        json!({"mode":"system","timeZone":null,"hourCycle":"h23","version":version})
    }

    #[test]
    fn parse_rejects_malformed_input() {
        assert!(
            TimePreferenceInput::parse(
                &json!({"mode":"bad","timeZone":null,"hourCycle":"h23","version":0})
            )
            .is_err()
        );
        assert!(TimePreferenceInput::parse(&input(0)).is_ok());
    }

    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("time-preference-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }
}
