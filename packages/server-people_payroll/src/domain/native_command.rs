//! Pure domain for the native people-payroll command path.
//!
//! A command is an owner-allow-listed statement `kind`, a full-parity positional
//! payload and a caller clock reading. This module is transport- and storage-free:
//! it validates shape, bounds and bind arity and produces a replayable plan. The
//! SQL itself lives in the owner adapter, never in TypeScript and never here.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

/// Maximum positional values admitted on one command (matches the ingress).
pub const MAX_VALUES: usize = 64;
/// Maximum serialized size of a single bound value.
pub const MAX_VALUE_BYTES: usize = 16 * 1024;

/// A validated native write command. `kind` is checked against the owner
/// allow-list by the ingress before the application layer is reached.
#[derive(Clone, Debug)]
pub struct NativeCommand {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
}

impl NativeCommand {
    /// Strict parse of a retained write command envelope. Unknown fields are
    /// ignored here; the HTTP ingress additionally enforces a bounded body and
    /// a single Idempotency-Key before decoding. Write commands always bind at
    /// least one positional value.
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        Self::parse_inner(raw, false)
    }

    /// Strict parse of an allow-listed owner read envelope. Owner reads include
    /// schema probes that bind no positional values; bind arity is still
    /// checked by the read-plan ingress against the registered statement.
    pub fn parse_read(raw: &Value) -> Result<Self, BusinessError> {
        Self::parse_inner(raw, true)
    }

    fn parse_inner(raw: &Value, allow_empty: bool) -> Result<Self, BusinessError> {
        let Some(kind) = raw.get("kind").and_then(Value::as_str) else {
            return Err(invalid());
        };
        if kind.is_empty()
            || kind.len() > 120
            || !kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Err(invalid());
        }
        let Some(values) = raw.get("values").and_then(Value::as_array) else {
            return Err(invalid());
        };
        if (values.is_empty() && !allow_empty) || values.len() > MAX_VALUES {
            return Err(invalid());
        }
        for value in values {
            if serialized_len(value) > MAX_VALUE_BYTES {
                return Err(invalid());
            }
        }
        let now = match raw.get("now") {
            None | Some(Value::Null) => 0,
            Some(Value::Number(number)) => number
                .as_i64()
                .filter(|value| *value >= 0)
                .ok_or_else(invalid)?,
            Some(_) => return Err(invalid()),
        };
        Ok(Self {
            kind: kind.to_owned(),
            values: values.clone(),
            now,
        })
    }

    /// Plan the command against an admitted statement: bind arity must match
    /// exactly so a mis-shaped payload can never reach the database adapter.
    pub fn plan<'a>(
        &'a self,
        statement: &'a crate::adapters::native_command_sql::NativeStatement,
    ) -> Result<CommandPlan<'a>, BusinessError> {
        if statement.kind != self.kind || self.values.len() != statement.bind_count {
            return Err(invalid());
        }
        Ok(CommandPlan {
            kind: &statement.kind,
            sql: statement.sql,
            values: &self.values,
            now: self.now,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandPlan<'a> {
    pub kind: &'a str,
    pub sql: &'a str,
    pub values: &'a [Value],
    pub now: i64,
}

fn serialized_len(value: &Value) -> usize {
    // Cheap upper bound without allocating for the common scalar cases.
    match value {
        Value::String(text) => text.len() + 2,
        Value::Null | Value::Bool(_) | Value::Number(_) => 32,
        other => other.to_string().len(),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PEOPLE_PAYROLL_NATIVE_COMMAND_INVALID",
        "Use one bounded native people-payroll command.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_NATIVE_UNAVAILABLE",
        "The native people-payroll service is unavailable.",
    )
}

pub fn context_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PEOPLE_PAYROLL_CONTEXT_INVALID",
        "The current company context is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_bounded_command() {
        let cmd = NativeCommand::parse(
            &json!({"kind":"tech_provision_stmt_1","values":["a",2,null],"now":42}),
        )
        .unwrap();
        assert_eq!(cmd.kind, "tech_provision_stmt_1");
        assert_eq!(cmd.values.len(), 3);
        assert_eq!(cmd.now, 42);
    }

    #[test]
    fn rejects_empty_or_oversized_payload() {
        assert!(NativeCommand::parse(&json!({"kind":"x","values":[]})).is_err());
        let values: Vec<Value> = (0..super::MAX_VALUES + 1).map(|_| json!(1)).collect();
        assert!(NativeCommand::parse(&json!({"kind":"x","values":values})).is_err());
        assert!(NativeCommand::parse(&json!({"kind":"bad kind","values":[1]})).is_err());
        assert!(NativeCommand::parse(&json!({"kind":"x","values":[1],"now":-1})).is_err());
    }

    #[test]
    fn parse_read_admits_zero_bind_schema_probes_only() {
        let cmd = NativeCommand::parse_read(
            &json!({"kind":"people_payroll_rd_final_pay_cancellation_schema_available","values":[]}),
        )
        .unwrap();
        assert!(cmd.values.is_empty());
        assert!(NativeCommand::parse_read(&json!({"kind":"bad kind","values":[]})).is_err());
        let oversized: Vec<Value> = (0..super::MAX_VALUES + 1).map(|_| json!(1)).collect();
        assert!(NativeCommand::parse_read(&json!({"kind":"x","values":oversized})).is_err());
        assert!(NativeCommand::parse_read(&json!({"kind":"x","values":[1],"now":-1})).is_err());
    }
}
