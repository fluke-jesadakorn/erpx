//! Native Files wave-1b full-command use case.
//!
//! A typed kind is validated against the owner allow-list, then executed by the
//! owner Postgres adapter inside the caller-owned serializable unit. The
//! adapter also writes command receipt, audit and outbox rows in that same unit.
use crate::domain::files_wave1b::{invalid, sql, supported};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type FilesCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilesCommandInput {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
    pub company_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub rows: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_affected: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<Value>,
}

/// Owner adapter port. Composition supplies the caller-owned serializable unit.
pub trait FilesCommandPort: Send {
    fn execute<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company: &'a CompanyId,
        key: &'a str,
        operation: &'a str,
        kind: &'a str,
        values: &'a [Value],
        now: i64,
    ) -> FilesCommandFuture<'a, CommandResult>;
}

pub struct NoFilesCommandPort;

impl FilesCommandPort for NoFilesCommandPort {
    fn execute<'a>(
        &'a mut self,
        _unit: Option<&'a mut PgConnection>,
        _company: &'a CompanyId,
        _key: &'a str,
        _operation: &'a str,
        _kind: &'a str,
        _values: &'a [Value],
        _now: i64,
    ) -> FilesCommandFuture<'a, CommandResult> {
        Box::pin(async {
            Err(BusinessError::new(
                erpx_platform_shared_server::public::FailureKind::Unavailable,
                "FILES_COMMAND_UNAVAILABLE",
                "The native Files command port is unavailable.",
            ))
        })
    }
}

/// Validate the typed kind and bind shape before any SQL touches the unit.
pub fn validate(input: &FilesCommandInput) -> Result<&'static str, BusinessError> {
    if input.now <= 0 || input.values.len() > 64 || !supported(&input.kind) {
        return Err(invalid());
    }
    let statement = sql(&input.kind).ok_or_else(invalid)?;
    // Count distinct placeholders: owner statements may repeat a parameter
    // (for example one `now` reused in the expiry CTE and its event insert).
    let mut seen = [false; 65];
    for fragment in statement.split('$').skip(1) {
        let digits: String = fragment.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(index) = digits.parse::<usize>() {
            if (1..seen.len()).contains(&index) {
                seen[index] = true;
            }
        }
    }
    let placeholders = seen.iter().filter(|used| **used).count();
    if placeholders != input.values.len() {
        return Err(invalid());
    }
    Ok(statement)
}

/// Execute one validated owner command on the supplied unit.
pub async fn execute<P: FilesCommandPort>(
    unit: Option<&mut PgConnection>,
    company: &CompanyId,
    key: &str,
    input: &FilesCommandInput,
    port: &mut P,
) -> Result<CommandResult, BusinessError> {
    validate(input)?;
    port.execute(
        unit,
        company,
        key,
        &format!("files.command.{}", input.kind),
        &input.kind,
        &input.values,
        input.now,
    )
    .await
}

/// Map a Postgres unit error to a BusinessError at the composition boundary.
pub fn unit_error(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => {
            // Surface the sanitized code/SQLSTATE so operators can identify the
            // failing owner statement without leaking SQL or parameters.
            eprintln!("FILES_COMMAND_UNIT_ERROR {error:?}");
            BusinessError::new(
            erpx_platform_shared_server::public::FailureKind::Unavailable,
                "FILES_COMMAND_PERSISTENCE",
                "The native Files command unit is unavailable.",
            )
        }
    }
}
