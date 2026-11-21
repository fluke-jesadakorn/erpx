//! Native Customer 360 customer directory read facts.
//!
//! Customer 360 owns the bounded read projection only. The directory lists the
//! customer parties visible through the same Company/branch/department scope
//! and source gates as the federated timeline; it never writes or revises a
//! source record.
use crate::domain::timeline::CustomerTimelineActor;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

pub const MAX_DIRECTORY: usize = 201;
const CURSOR_LIMIT: usize = 1_000;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomerDirectoryQuery {
    /// Opaque `(name,id)` keyset cursor bound to the current Company and source
    /// gates. An empty cursor is rejected; omit it to read the first page.
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

impl CustomerDirectoryQuery {
    pub fn normalized_limit(&self) -> Result<usize, BusinessError> {
        match self.limit {
            None => Ok(MAX_DIRECTORY),
            Some(value) if (1..=MAX_DIRECTORY as u16).contains(&value) => Ok(value as usize),
            Some(_) => Err(BusinessError::invalid(
                "CUSTOMER_DIRECTORY_LIMIT_INVALID",
                "Choose a customer directory page size between 1 and 201.",
            )),
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        self.normalized_limit()?;
        if self.cursor.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > CURSOR_LIMIT || value.chars().any(char::is_control)
        }) {
            return Err(cursor_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerDirectoryPage {
    pub items: Vec<CustomerDirectoryEntry>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerDirectoryCursor {
    pub name: String,
    pub id: String,
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_DIRECTORY_REQUEST_INVALID",
        "Use one bounded customer directory request.",
    )
}

pub(crate) fn cursor_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_DIRECTORY_CURSOR_INVALID",
        "Restart the customer directory to load more customers.",
    )
}

fn directory_scope(
    actor: &CustomerTimelineActor,
    view: CustomerDirectoryView,
) -> Result<String, BusinessError> {
    serde_json::to_string(&(
        actor.company_id.as_str(),
        actor.branch_id.as_ref().map(RecordId::as_str),
        actor.department_id.as_ref().map(RecordId::as_str),
        view.field_service,
        view.finance,
        view.legal,
    ))
    .map_err(|_| cursor_invalid())
}

pub(crate) fn encode_directory_cursor(
    actor: &CustomerTimelineActor,
    view: CustomerDirectoryView,
    cursor: &CustomerDirectoryCursor,
) -> Result<String, BusinessError> {
    #[derive(Serialize)]
    struct Wire<'a> {
        scope: &'a str,
        name: &'a str,
        id: &'a str,
    }
    let scope = directory_scope(actor, view)?;
    let encoded = base64url_encode(
        &serde_json::to_vec(&Wire {
            scope: &scope,
            name: &cursor.name,
            id: &cursor.id,
        })
        .map_err(|_| cursor_invalid())?,
    );
    if encoded.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    Ok(encoded)
}

pub(crate) fn decode_directory_cursor(
    value: &str,
    actor: &CustomerTimelineActor,
    view: CustomerDirectoryView,
) -> Result<CustomerDirectoryCursor, BusinessError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        scope: String,
        name: String,
        id: String,
    }
    if value.is_empty() || value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    let bytes = base64url_decode(value).ok_or_else(cursor_invalid)?;
    let wire: Wire = serde_json::from_slice(&bytes).map_err(|_| cursor_invalid())?;
    if wire.scope != directory_scope(actor, view)?
        || wire.name.is_empty()
        || wire.name.len() > 300
        || RecordId::new(wire.id.clone()).is_err()
        || wire.name.chars().any(char::is_control)
    {
        return Err(cursor_invalid());
    }
    Ok(CustomerDirectoryCursor {
        name: wire.name,
        id: wire.id,
    })
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let combined = (first << 16) | (second << 8) | third;
        encoded.push(BASE64URL[((combined >> 18) & 63) as usize] as char);
        encoded.push(BASE64URL[((combined >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(BASE64URL[((combined >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(BASE64URL[(combined & 63) as usize] as char);
        }
    }
    encoded
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn base64url_decode(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || value.len() % 4 == 1 {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut index = 0;
    while index < bytes.len() {
        let remaining = bytes.len() - index;
        let first = base64url_value(bytes[index])? as u32;
        let second = base64url_value(*bytes.get(index + 1)?)? as u32;
        let third = if remaining > 2 {
            base64url_value(*bytes.get(index + 2)?)? as u32
        } else {
            0
        };
        let fourth = if remaining > 3 {
            base64url_value(*bytes.get(index + 3)?)? as u32
        } else {
            0
        };
        let combined = (first << 18) | (second << 12) | (third << 6) | fourth;
        decoded.push(((combined >> 16) & 255) as u8);
        if remaining > 2 {
            decoded.push(((combined >> 8) & 255) as u8);
        }
        if remaining > 3 {
            decoded.push((combined & 255) as u8);
        }
        index += 4;
    }
    Some(decoded)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomerDirectoryView {
    pub field_service: bool,
    pub finance: bool,
    pub legal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerDirectoryEntry {
    pub id: String,
    pub name: String,
    pub code: String,
}

impl CustomerDirectoryEntry {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.id.clone()).is_err()
            || self.name.is_empty()
            || self.name.len() > 300
            || self.code.is_empty()
            || self.code.len() > 200
            || self.name.chars().any(char::is_control)
            || self.code.chars().any(char::is_control)
        {
            return Err(BusinessError::invalid(
                "CUSTOMER_DIRECTORY_ENTRY_INVALID",
                "Customer directory entries must be complete and scoped.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn directory_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_DIRECTORY_SOURCE_INVALID",
        "The current customer directory source is invalid.",
    )
}

impl CustomerTimelineActor {
    pub fn require_directory_read(
        &self,
        now: erpx_server_shared::public::TimestampMillis,
    ) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CUSTOMER_DIRECTORY_PERMISSION_REQUIRED",
                "Your selected role does not grant customer read access.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub fn directory_view(actor: &CustomerTimelineActor) -> CustomerDirectoryView {
    CustomerDirectoryView {
        field_service: actor.field_service_visible(),
        finance: actor.finance_visible(),
        legal: actor.legal_visible(),
    }
}
