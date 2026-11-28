//! Pure domain for the native Procurement command path.
//! A command is an owner-allow-listed statement `kind`, a full-parity positional
//! payload and a caller clock reading. No SQL and no IO live here.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

pub const MAX_VALUES: usize = 64;
pub const MAX_VALUE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub struct NativeCommand {
    pub kind: String,
    pub values: Vec<Value>,
    pub now: i64,
}

impl NativeCommand {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
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
        if values.len() > MAX_VALUES {
            return Err(invalid());
        }
        for value in values {
            if serialized_len(value) > MAX_VALUE_BYTES {
                return Err(invalid());
            }
        }
        let now = match raw.get("now") {
            None | Some(Value::Null) => 0,
            Some(Value::Number(number)) => {
                number.as_i64().filter(|v| *v >= 0).ok_or_else(invalid)?
            }
            Some(_) => return Err(invalid()),
        };
        Ok(Self {
            kind: kind.to_owned(),
            values: values.clone(),
            now,
        })
    }

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
    match value {
        Value::String(text) => text.len() + 2,
        Value::Null | Value::Bool(_) | Value::Number(_) => 32,
        other => other.to_string().len(),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "PROCUREMENT_NATIVE_COMMAND_INVALID",
        "Use one bounded native Procurement command.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_NATIVE_COMMAND_UNAVAILABLE",
        "The native Procurement service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_bounded_command() {
        let c = NativeCommand::parse(&json!({"kind":"procurement_stmt_1","values":["a",2,null],"now":42})).unwrap();
        assert_eq!(c.values.len(), 3);
        assert_eq!(c.now, 42);
    }

    #[test]
    fn rejects_bad_envelope() {
        assert!(NativeCommand::parse(&json!({"kind":"x"})).is_err());
        assert!(NativeCommand::parse(&json!({"kind":"bad kind","values":[1]})).is_err());
    }
}
