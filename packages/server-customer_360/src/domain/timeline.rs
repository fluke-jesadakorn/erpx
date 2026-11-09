//! Native Customer 360 federated timeline read facts.
//!
//! Customer 360 owns the bounded read projection only. It never creates,
//! revises or deletes a Commerce, Sales, CRM, Support, Field Service, Finance
//! or Legal source record. The composition root supplies the current Account
//! session, selected Company/branch/department scope, Support history purchase
//! state and the serializable owner transaction. This domain enforces the
//! `commerce.read` permission, selected-context expiry, the per-source module
//! and permission gates, a bounded cursor/limit, exact event shapes and a
//! deterministic descending `(at, id)` page before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const DEFAULT_LIMIT: usize = 30;
pub const MAX_LIMIT: usize = 100;
pub const MAX_COVERAGE: usize = 12;
const CURSOR_LIMIT: usize = 1_000;
const ID_LIMIT: usize = 300;
const SOURCE_ID_LIMIT: usize = 200;
const NAME_LIMIT: usize = 300;
const TITLE_LIMIT: usize = 500;
const STATUS_LIMIT: usize = 200;
const HREF_LIMIT: usize = 500;

pub const LIMITATIONS: &str = "Internal staff projection. Source records remain authoritative. Order/return entries show their latest state, not a complete transition log. Field Service, Finance and Legal entries show scoped source state; Portal and customer-authority records are not included.";

#[derive(Clone, Debug)]
pub struct CustomerTimelineActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub role: String,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub assignment_id: String,
    pub expires_at: i64,
}

impl CustomerTimelineActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CUSTOMER_TIMELINE_PERMISSION_REQUIRED",
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

    pub fn support_visible(&self, purchased: bool) -> bool {
        self.has("support.read") && purchased
    }

    pub fn field_service_visible(&self) -> bool {
        self.has_module("field_service")
    }

    pub fn finance_visible(&self) -> bool {
        self.has("finance.read")
    }

    pub fn legal_visible(&self) -> bool {
        self.has("legal.read")
    }
}

/// Source gates derived from the current actor and the current Support history
/// purchase. Disabled sources are neither read nor advertised in coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomerTimelineView {
    pub support: bool,
    pub field_service: bool,
    pub finance: bool,
    pub legal: bool,
}

impl CustomerTimelineView {
    pub fn coverage(self) -> Vec<&'static str> {
        let mut coverage = vec![
            "Sales quotations/orders",
            "Fulfillments and customer returns",
            "Internal contact history",
            "Customer follow-up history",
            "Qualified CRM lead/opportunity history",
            "CRM follow-up activities",
        ];
        if self.support {
            coverage.push("Support case history");
        }
        if self.field_service {
            coverage.push("Field Service agreements and visits");
        }
        if self.finance {
            coverage.push("Finance invoice status");
        }
        if self.legal {
            coverage.push("Legal records linked to orders");
        }
        coverage
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_TIMELINE_SOURCE_INVALID",
        "The current customer timeline source is invalid.",
    )
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_REQUEST_INVALID",
        "Use one bounded customer timeline request.",
    )
}

pub(crate) fn cursor_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_TIMELINE_CURSOR_INVALID",
        "Restart this customer timeline to load history.",
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomerTimelineQuery {
    /// Empty cursors are rejected; omit the value to read the first page.
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

impl CustomerTimelineQuery {
    pub fn normalized_limit(&self) -> Result<usize, BusinessError> {
        match self.limit {
            None => Ok(DEFAULT_LIMIT),
            Some(value) if (1..=MAX_LIMIT as u16).contains(&value) => Ok(value as usize),
            Some(_) => Err(BusinessError::invalid(
                "CUSTOMER_TIMELINE_LIMIT_INVALID",
                "Choose a customer timeline page size between 1 and 100.",
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

fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn valid_href(value: &str) -> bool {
    valid_text(value, HREF_LIMIT) && value.starts_with('/') && !value.starts_with("//")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerAccount {
    pub id: String,
    pub name: String,
    pub code: String,
}

impl CustomerAccount {
    pub fn validate(&self, customer_id: &str) -> Result<(), BusinessError> {
        if self.id != customer_id
            || RecordId::new(self.id.clone()).is_err()
            || !valid_text(&self.name, NAME_LIMIT)
            || !valid_text(&self.code, SOURCE_ID_LIMIT)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerTimelineItem {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub status: String,
    pub version: i64,
    pub at: i64,
    pub href: String,
}

fn valid_source_pair(source_type: &str, id_prefix: &str) -> bool {
    match source_type {
        "order" => id_prefix == "order",
        "fulfillment" => id_prefix == "fulfillment",
        "return" => id_prefix == "return",
        "contact" => id_prefix == "contact",
        "followup" => id_prefix == "followup",
        "support" => id_prefix == "support",
        "field_service" => matches!(id_prefix, "service-agreement" | "service-order"),
        "legal" => id_prefix == "legal",
        "invoice" => id_prefix == "invoice",
        "crm" => id_prefix == "crm",
        "crm_activity" => id_prefix == "crm-activity",
        _ => false,
    }
}

impl CustomerTimelineItem {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let Some((prefix, suffix)) = self.id.split_once(':') else {
            return Err(source_invalid());
        };
        if !valid_source_pair(&self.source_type, prefix)
            || RecordId::new(suffix).is_err()
            || self.id.len() > ID_LIMIT
            || RecordId::new(self.source_id.clone()).is_err()
            || !valid_text(&self.title, TITLE_LIMIT)
            || !valid_text(&self.status, STATUS_LIMIT)
            || !valid_href(&self.href)
            || self.version < 1
            || self.at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutiveCustomerActivityRow {
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub status: String,
    pub version: i64,
    pub at: i64,
    pub href: String,
    pub customer_id: String,
    pub customer_name: String,
}

impl ExecutiveCustomerActivityRow {
    pub fn validate(&self) -> Result<(), BusinessError> {
        CustomerTimelineItem {
            id: self.id.clone(),
            source_id: self.source_id.clone(),
            source_type: self.source_type.clone(),
            title: self.title.clone(),
            status: self.status.clone(),
            version: self.version,
            at: self.at,
            href: self.href.clone(),
        }
        .validate()?;
        if RecordId::new(self.customer_id.clone()).is_err()
            || !valid_text(&self.customer_name, NAME_LIMIT)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Bounded Executive customer-activity search over the same federated sources
/// as the per-customer timeline. `record_id` narrows one evidence row.
#[derive(Clone, Debug)]
pub struct ExecutiveCustomerActivityQuery {
    pub from: i64,
    pub to: i64,
    pub query: Option<String>,
    pub cursor: Option<CustomerTimelineCursor>,
    pub limit: usize,
    pub record_id: Option<String>,
}

impl ExecutiveCustomerActivityQuery {
    pub fn normalized_limit(&self) -> Result<usize, BusinessError> {
        if (1..=MAX_LIMIT).contains(&self.limit) {
            Ok(self.limit)
        } else {
            Err(BusinessError::invalid(
                "CUSTOMER_ACTIVITY_LIMIT_INVALID",
                "Choose a customer activity page size between 1 and 100.",
            ))
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        self.normalized_limit()?;
        const MAX_RANGE: i64 = 366 * 24 * 60 * 60 * 1000;
        if self.from < 0
            || self.to <= self.from
            || self.to - self.from > MAX_RANGE
        {
            return Err(BusinessError::invalid(
                "CUSTOMER_ACTIVITY_RANGE_INVALID",
                "Choose a customer activity range within one year.",
            ));
        }
        if self.query.as_deref().is_some_and(|value| {
            value.len() > 200 || value.chars().any(char::is_control)
        }) {
            return Err(BusinessError::invalid(
                "CUSTOMER_ACTIVITY_QUERY_INVALID",
                "Use a bounded customer activity search.",
            ));
        }
        if self
            .record_id
            .as_deref()
            .is_some_and(|value| !valid_text(value, ID_LIMIT))
        {
            return Err(source_invalid());
        }
        if let Some(cursor) = &self.cursor {
            if cursor.at < 0 || !valid_text(&cursor.id, ID_LIMIT) {
                return Err(cursor_invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerTimelineCursor {
    pub at: i64,
    pub id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    scope: String,
    at: i64,
    id: String,
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

pub(crate) fn cursor_scope(
    actor: &CustomerTimelineActor,
    customer_id: &str,
) -> Result<String, BusinessError> {
    serde_json::to_string(&(
        actor.company_id.as_str(),
        actor.branch_id.as_ref().map(RecordId::as_str),
        actor.department_id.as_ref().map(RecordId::as_str),
        customer_id,
    ))
    .map_err(|_| cursor_invalid())
}

pub(crate) fn encode_cursor(
    scope: &str,
    cursor: &CustomerTimelineCursor,
) -> Result<String, BusinessError> {
    let wire = CursorWire {
        scope: scope.to_owned(),
        at: cursor.at,
        id: cursor.id.clone(),
    };
    let value = serde_json::to_vec(&wire).map_err(|_| cursor_invalid())?;
    let encoded = base64url_encode(&value);
    if encoded.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    Ok(encoded)
}

pub(crate) fn decode_cursor(
    value: &str,
    scope: &str,
) -> Result<CustomerTimelineCursor, BusinessError> {
    if value.is_empty() || value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    let bytes = base64url_decode(value).ok_or_else(cursor_invalid)?;
    let wire: CursorWire = serde_json::from_slice(&bytes).map_err(|_| cursor_invalid())?;
    if wire.scope != scope || wire.at < 0 || RecordId::new(wire.id.clone()).is_err() {
        return Err(cursor_invalid());
    }
    Ok(CustomerTimelineCursor {
        at: wire.at,
        id: wire.id,
    })
}

#[derive(Clone, Debug)]
pub struct CustomerTimelinePageData {
    pub items: Vec<CustomerTimelineItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Validate and bound rows ordered and cursor-checked by the source repository.
/// Text ordering must use the source database collation, not Rust byte ordering.
pub(crate) fn validate_page(
    cursor: Option<&CustomerTimelineCursor>,
    mut source: Vec<CustomerTimelineItem>,
    limit: usize,
    scope: &str,
    view: CustomerTimelineView,
) -> Result<CustomerTimelinePageData, BusinessError> {
    for row in &source {
        row.validate()?;
        let enabled = match row.source_type.as_str() {
            "support" => view.support,
            "field_service" => view.field_service,
            "invoice" => view.finance,
            "legal" => view.legal,
            "order" | "fulfillment" | "return" | "contact" | "followup" | "crm"
            | "crm_activity" => true,
            _ => false,
        };
        if !enabled {
            return Err(source_invalid());
        }
        if cursor.is_some_and(|cursor| row.at > cursor.at) {
            return Err(source_invalid());
        }
    }
    if source.windows(2).any(|pair| pair[0].at < pair[1].at) {
        return Err(source_invalid());
    }
    let mut seen = BTreeSet::new();
    for row in &source {
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
    }
    let has_more = source.len() > limit;
    if has_more {
        source.truncate(limit);
    }
    let next_cursor = if has_more {
        source
            .last()
            .map(|row| {
                encode_cursor(
                    scope,
                    &CustomerTimelineCursor {
                        at: row.at,
                        id: row.id.clone(),
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(CustomerTimelinePageData {
        items: source,
        next_cursor,
        has_more,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct CustomerTimelinePage {
    pub customer: CustomerAccount,
    pub items: Vec<CustomerTimelineItem>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    pub coverage: Vec<&'static str>,
    pub limitations: &'static str,
}
