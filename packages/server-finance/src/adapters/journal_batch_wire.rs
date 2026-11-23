//! Canonical owner-side admission for `{journals:[{id,expectedVersion}],reason}`.
//! Receipt identity binds the normalized ECMAScript JSON, never caller text,
//! hashes, permissions or any other unknown field.
use crate::application::journal_batch::JournalBatchCommand;
use crate::domain::journal_batch::{BatchJournalRef, JournalBatchInput};
use erpx_server_shared::public::{
    BusinessError, CommandKey, RecordId, json_numbers::deserialize_safe_i64,
};
use erpx_server_workflow::public::commands::CommandJson;
use serde::Deserialize;

#[derive(Debug)]
pub struct JournalBatchWireFailure {
    pub status: u16,
    pub error: BusinessError,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct SafeInteger(#[serde(deserialize_with = "deserialize_safe_i64")] i64);

#[derive(Clone, Debug)]
pub struct JournalBatchWire {
    input: JournalBatchInput,
    normalized: CommandJson,
}

/// Constructible only through the bounded normalizer or canonical typed data.
/// No `Deserialize` or public fields: raw caller JSON cannot override its
/// receipt identity.
#[derive(Clone, Debug)]
pub struct NormalizedJournalBatchCommand {
    command: JournalBatchCommand,
    input: CommandJson,
}

impl NormalizedJournalBatchCommand {
    pub fn from_typed(command: JournalBatchCommand) -> Result<Self, BusinessError> {
        let input = CommandJson::from_serializable(&command.input)?;
        Ok(Self { command, input })
    }

    pub fn command(&self) -> &JournalBatchCommand {
        &self.command
    }

    pub fn input_json(&self) -> &CommandJson {
        &self.input
    }

    pub(crate) fn input_for(
        &self,
        command: &JournalBatchCommand,
    ) -> Result<CommandJson, BusinessError> {
        if command.key != self.command.key
            || command.batch_id != self.command.batch_id
            || command.input != self.command.input
        {
            return Err(invalid("JOURNAL_BATCH_NORMALIZED_MISMATCH"));
        }
        Ok(self.input.clone())
    }
}

impl JournalBatchWire {
    pub fn normalized_json(&self) -> &str {
        self.normalized.as_str()
    }

    pub fn into_command(
        self,
        key: CommandKey,
    ) -> Result<NormalizedJournalBatchCommand, BusinessError> {
        let batch_id = RecordId::new(format!("journal-batch-{}", key.as_str()))?;
        NormalizedJournalBatchCommand::from_typed(JournalBatchCommand {
            key,
            batch_id,
            input: self.input,
        })
    }
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Provide a bounded Finance journal batch body.")
}

fn invalid_json() -> JournalBatchWireFailure {
    JournalBatchWireFailure {
        status: 422,
        error: invalid("INVALID_JSON"),
    }
}

fn validation(message: &'static str) -> JournalBatchWireFailure {
    JournalBatchWireFailure {
        status: 422,
        error: BusinessError::invalid("VALIDATION_FAILED", message),
    }
}

fn document(bytes: &[u8]) -> Result<CommandJson, JournalBatchWireFailure> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    CommandJson::parse(text).map_err(|_| invalid_json())
}

fn string(value: &CommandJson) -> Result<String, JournalBatchWireFailure> {
    value
        .string_utf8_storage()
        .map_err(|_| validation("The batch body is malformed."))?
        .ok_or_else(|| validation("The batch body is malformed."))
}

pub fn normalize_journal_batch(bytes: &[u8]) -> Result<JournalBatchWire, JournalBatchWireFailure> {
    let body = document(bytes)?;
    let journals = body
        .property("journals")
        .map_err(|_| validation("journals is required."))?
        .ok_or_else(|| validation("journals is required."))?;
    let elements = journals
        .array_elements()
        .map_err(|_| validation("journals must be an array."))?
        .ok_or_else(|| validation("journals must be an array."))?;
    if elements.is_empty() || elements.len() > 100 {
        return Err(validation("Select between one and one hundred journals."));
    }
    let mut refs = Vec::with_capacity(elements.len());
    for element in elements {
        let id = element
            .property("id")
            .map_err(|_| validation("A batch journal id is required."))?
            .ok_or_else(|| validation("A batch journal id is required."))?;
        let id = string(&id)?;
        if id.chars().count() == 0 || id.encode_utf16().count() > 128 {
            return Err(validation("A batch journal id is malformed."));
        }
        let version = element
            .property("expectedVersion")
            .map_err(|_| validation("expectedVersion is required."))?
            .ok_or_else(|| validation("expectedVersion is required."))?;
        let SafeInteger(expected_version) = serde_json::from_str(version.as_str())
            .map_err(|_| validation("expectedVersion is malformed."))?;
        if expected_version <= 0 || expected_version >= 9_007_199_254_740_991 {
            return Err(validation("expectedVersion is malformed."));
        }
        refs.push(BatchJournalRef {
            id,
            expected_version,
        });
    }
    let reason = body
        .property("reason")
        .map_err(|_| validation("reason is required."))?
        .ok_or_else(|| validation("reason is required."))?;
    let reason = string(&reason)?;
    let reason = reason.trim().to_owned();
    let units = reason.encode_utf16().count();
    if units < 10 || units > 2000 {
        return Err(validation(
            "A batch reason of 10 to 2000 characters is required.",
        ));
    }
    let input = JournalBatchInput {
        journals: refs,
        reason,
    };
    let normalized = CommandJson::from_serializable(&input)
        .map_err(|_| validation("The batch body is malformed."))?;
    Ok(JournalBatchWire { input, normalized })
}
