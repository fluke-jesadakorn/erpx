use serde::{Deserialize, Serialize};

use super::error::BusinessError;

fn validate_identifier(value: &str) -> Result<(), BusinessError> {
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(BusinessError::invalid(
            "INVALID_IDENTIFIER",
            "Use a nonempty identifier of at most 200 bytes without control characters.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordId(String);
impl RecordId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RecordId {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<RecordId> for String {
    fn from(value: RecordId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct HoldingId(String);
impl HoldingId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for HoldingId {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<HoldingId> for String {
    fn from(value: HoldingId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct CompanyId(String);
impl CompanyId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for CompanyId {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<CompanyId> for String {
    fn from(value: CompanyId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(String);
impl UserId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for UserId {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<UserId> for String {
    fn from(value: UserId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct AssignmentId(String);
impl AssignmentId {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for AssignmentId {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<AssignmentId> for String {
    fn from(value: AssignmentId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommandKey(String);

impl CommandKey {
    pub fn new(value: impl Into<String>) -> Result<Self, BusinessError> {
        let value = value.into();
        if !(8..=180).contains(&value.len())
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b":_.-".contains(&byte))
        {
            return Err(BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CommandKey {
    type Error = BusinessError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CommandKey> for String {
    fn from(value: CommandKey) -> Self {
        value.0
    }
}

/// Milliseconds since the Unix epoch, independent of a wall-clock implementation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TimestampMillis(i64);

impl TimestampMillis {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> i64 {
        self.0
    }
}
