//! Customer contact-note input and presentation facts.
//!
//! The browser supplies bounded presentation values only. Current Account
//! authority, source visibility, same-person policy and the command receipt
//! remain composition responsibilities behind the HTTP backend.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const NOTE_CHANNELS: [&str; 4] = ["phone", "email", "meeting", "other"];
pub const NOTE_BODY_MIN: usize = 3;
pub const NOTE_BODY_MAX: usize = 4000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn bounded_text(value: &str, minimum: usize, maximum: usize) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= minimum && bytes.len() <= maximum && !value.chars().any(char::is_control)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCustomerNoteInput {
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    pub expected_version: i64,
    pub channel: String,
    pub occurred_at: i64,
    pub body: String,
}

impl CreateCustomerNoteInput {
    pub fn validate(&self, now: i64) -> Result<(), BusinessError> {
        if self.order_id.is_some() == self.customer_id.is_some() {
            return Err(invalid(
                "SOURCE_REQUIRED",
                "Choose exactly one order or customer source.",
            ));
        }
        if let Some(id) = self.order_id.as_deref().or(self.customer_id.as_deref()) {
            if id.is_empty() || id.len() > 200 || id.chars().any(char::is_control) {
                return Err(invalid("SOURCE_INVALID", "Choose an available source."));
            }
        }
        if self.expected_version < 1 || self.expected_version > 9_007_199_254_740_991 {
            return Err(invalid(
                "CUSTOMER_SOURCE_CHANGED",
                "Refresh the Customer 360 record before recording a note.",
            ));
        }
        if !NOTE_CHANNELS.contains(&self.channel.as_str()) {
            return Err(invalid(
                "CONTACT_CHANNEL_INVALID",
                "Choose an available contact channel.",
            ));
        }
        if self.occurred_at < 0
            || self.occurred_at > 9_007_199_254_740_991
            || self.occurred_at > now.saturating_add(60_000)
        {
            return Err(invalid(
                "CONTACT_TIME_INVALID",
                "Record a contact that already happened.",
            ));
        }
        if !bounded_text(&self.body, NOTE_BODY_MIN, NOTE_BODY_MAX) {
            return Err(invalid(
                "CONTACT_BODY_INVALID",
                "Record a contact summary between 3 and 4000 characters.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeCustomerNoteInput {
    pub id: String,
    pub expected_version: i64,
    pub action: String,
    pub body: String,
    pub reason: String,
}

impl ChangeCustomerNoteInput {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.id.is_empty() || self.id.len() > 200 || self.id.chars().any(char::is_control) {
            return Err(invalid(
                "NOTE_ID_INVALID",
                "Choose an available contact note.",
            ));
        }
        if self.expected_version < 1 || self.expected_version > 9_007_199_254_740_991 {
            return Err(invalid(
                "NOTE_VERSION_REQUIRED",
                "Refresh the contact note before changing it.",
            ));
        }
        if !matches!(self.action.as_str(), "correct" | "archive") {
            return Err(invalid(
                "CONTACT_ACTION_INVALID",
                "Choose correct or archive.",
            ));
        }
        if !bounded_text(&self.body, NOTE_BODY_MIN, NOTE_BODY_MAX)
            || !bounded_text(&self.reason, NOTE_BODY_MIN, NOTE_BODY_MAX)
        {
            return Err(invalid(
                "CONTACT_CHANGE_INVALID",
                "Record a summary and reason between 3 and 4000 characters.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerNoteSource {
    pub id: String,
    pub name: String,
    pub code: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerNoteOrder {
    pub id: String,
    pub party_id: String,
    pub party_name: String,
    pub number: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerNoteRow {
    pub id: String,
    pub company_id: String,
    pub customer_id: String,
    pub order_id: Option<String>,
    pub channel: String,
    pub occurred_at: i64,
    pub body: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(rename = "canChange")]
    pub can_change: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerNoteHistoryRow {
    pub id: String,
    pub note_id: String,
    pub version: i64,
    pub action: String,
    pub body: String,
    pub reason: String,
    pub actor_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerNotesView {
    pub customer: CustomerNoteSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<CustomerNoteOrder>,
    pub notes: Vec<CustomerNoteRow>,
    pub history: Vec<CustomerNoteHistoryRow>,
    #[serde(rename = "canCreate")]
    pub can_create: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CustomerNoteCommandResult {
    pub id: String,
    pub version: i64,
    pub status: String,
}

pub fn note_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_NOTE_NOT_FOUND",
        "Contact note not found in this scope.",
    )
}

pub fn customer_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_NOT_FOUND",
        "Customer is not available in this workspace.",
    )
}

pub fn note_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_SOURCE_CHANGED",
        "Refresh the Customer 360 record before recording a note.",
    )
}

pub fn note_owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_NOTE_OWNER_REQUIRED",
        "The author or a sales manager must correct this contact record.",
    )
}
