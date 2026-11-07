//! Canonical Finance body normalization. The sealed command binds normalized
//! ECMAScript JSON for receipt identity separately from UTF-8 SQL storage text.
use crate::application::journal_review::{JournalReviewCommand, JournalReviewInput};
use erpx_server_shared::public::{
    BusinessError, CommandKey, RecordId, json_numbers::deserialize_safe_i64,
};
use erpx_server_workflow::public::commands::CommandJson;
use serde::Deserialize;
#[derive(Debug)]
pub struct JournalWireFailure {
    pub status: u16,
    pub error: BusinessError,
}
#[derive(Deserialize)]
#[serde(transparent)]
struct SafeInteger(#[serde(deserialize_with = "deserialize_safe_i64")] i64);
#[derive(Clone, Debug)]
pub struct JournalPostWire {
    input: JournalReviewInput,
    normalized: CommandJson,
}
#[derive(Clone, Debug)]
pub struct JournalReverseWire {
    input: JournalReviewInput,
    normalized: CommandJson,
}
/// Constructible only through canonical typed data or the bounded normalizers.
/// No Deserialize or public fields; raw caller JSON can never override a hash.
#[derive(Clone, Debug)]
pub struct NormalizedJournalReviewCommand {
    command: JournalReviewCommand,
    input: CommandJson,
}
impl NormalizedJournalReviewCommand {
    pub fn from_typed(command: JournalReviewCommand) -> Result<Self, BusinessError> {
        let input = CommandJson::from_serializable(&command.input)?;
        Ok(Self { command, input })
    }
    pub fn command(&self) -> &JournalReviewCommand {
        &self.command
    }
    pub(crate) fn input_for(
        &self,
        command: &JournalReviewCommand,
    ) -> Result<CommandJson, BusinessError> {
        if command.key != self.command.key
            || command.journal_id != self.command.journal_id
            || command.input != self.command.input
        {
            return Err(BusinessError::invalid(
                "JOURNAL_NORMALIZED_INPUT_MISMATCH",
                "Use the exact normalized Finance command.",
            ));
        }
        Ok(self.input.clone())
    }
}
macro_rules! wire {
    ($kind:ident) => {
        impl $kind {
            pub fn normalized_json(&self) -> &str {
                self.normalized.as_str()
            }
            pub fn into_command(
                self,
                key: CommandKey,
                journal_id: RecordId,
            ) -> NormalizedJournalReviewCommand {
                NormalizedJournalReviewCommand {
                    command: JournalReviewCommand {
                        key,
                        journal_id,
                        input: self.input,
                    },
                    input: self.normalized,
                }
            }
        }
    };
}
wire!(JournalPostWire);
wire!(JournalReverseWire);
fn invalid_json() -> JournalWireFailure {
    JournalWireFailure {
        status: 422,
        error: BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."),
    }
}
fn invalid(post: bool) -> JournalWireFailure {
    JournalWireFailure {
        status: 422,
        error: BusinessError::invalid(
            "VALIDATION_FAILED",
            if post {
                "expectedVersion is required."
            } else {
                "A reversal period and date are required."
            },
        ),
    }
}
fn document(bytes: &[u8]) -> Result<CommandJson, JournalWireFailure> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    CommandJson::parse(text).map_err(|_| invalid_json())
}
fn string(value: &CommandJson) -> Result<String, JournalWireFailure> {
    value
        .string_utf8_storage()
        .map_err(|_| invalid(false))?
        .ok_or_else(|| invalid(false))
}
pub fn normalize_journal_post(bytes: &[u8]) -> Result<JournalPostWire, JournalWireFailure> {
    let value = document(bytes)?
        .property("expectedVersion")
        .map_err(|_| invalid(true))?
        .ok_or_else(|| invalid(true))?;
    let SafeInteger(expected_version) =
        serde_json::from_str(value.as_str()).map_err(|_| invalid(true))?;
    if expected_version <= 0 {
        return Err(invalid(true));
    }
    let input = JournalReviewInput::Post { expected_version };
    let normalized = CommandJson::from_serializable(&input).map_err(|_| invalid(true))?;
    Ok(JournalPostWire { input, normalized })
}
pub fn normalize_journal_reverse(bytes: &[u8]) -> Result<JournalReverseWire, JournalWireFailure> {
    let body = document(bytes)?;
    let property = |name| body.property(name).map_err(|_| invalid(false));
    let period = property("periodId")?.ok_or_else(|| invalid(false))?;
    let period_id = string(&period)?;
    let date = property("entryDate")?.ok_or_else(|| invalid(false))?;
    let entry_date = string(&date)?;
    let format = time::macros::format_description!("[year]-[month]-[day]");
    if entry_date.len() != 10
        || !entry_date.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
        || time::Date::parse(&entry_date, format).is_err()
    {
        return Err(invalid(false));
    }
    let original_memo = property("memo")?;
    let memo = original_memo.as_ref().map(string).transpose()?;
    // Known fields only, exact original schema order, absent memo omitted. The
    // source values already passed the canonical bounded ECMAScript JSON codec.
    let normalized = CommandJson::parse(&format!(
        "{{\"periodId\":{},\"entryDate\":{}{}}}",
        period.as_str(),
        date.as_str(),
        original_memo
            .as_ref()
            .map(|m| format!(",\"memo\":{}", m.as_str()))
            .unwrap_or_default()
    ))
    .map_err(|_| invalid(false))?;
    Ok(JournalReverseWire {
        input: JournalReviewInput::Reverse {
            period_id,
            entry_date,
            memo,
        },
        normalized,
    })
}
