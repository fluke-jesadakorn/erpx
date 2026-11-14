//! Native Sales document read facts.
//!
//! These are owner read projections only. They do not create, revise or freeze
//! a quotation or sales order. Composition retains the normal Account/Holding
//! session, purchase admission and current authority; this slice enforces the
//! Company, selected branch/department, work-context expiry, `commerce.read`
//! and Sales module policy before the bounded source read.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PAGE_SIZE: usize = 30;
const SEARCH_LIMIT: usize = 200;
const CURSOR_LIMIT: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SalesDocumentKind {
    Quote,
    Sales,
}

impl SalesDocumentKind {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "quote" => Ok(Self::Quote),
            "sales" => Ok(Self::Sales),
            _ => Err(BusinessError::invalid(
                "SALES_DOCUMENT_KIND_INVALID",
                "Choose a quotation or sales order.",
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quote => "quote",
            Self::Sales => "sales",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SalesDocumentReadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl SalesDocumentReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }

    pub fn can_read_sales(&self) -> bool {
        self.has("commerce.read") && self.has_module("sales")
    }

    pub fn require_documents(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "SALES_PERMISSION_REQUIRED",
                "Your selected role does not grant Sales read access.",
            ));
        }
        if !self.has_module("sales") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "SALES_MODULE_FORBIDDEN",
                "Your current role cannot access Sales.",
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

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_DOCUMENTS_SOURCE_INVALID",
        "The current sales document source is invalid.",
    )
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_DOCUMENTS_REQUEST_INVALID",
        "Use one bounded quotation or sales-order list request.",
    )
}

pub(crate) fn cursor_invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_DOCUMENTS_CURSOR_INVALID",
        "Restart this sales document search to load results.",
    )
}

fn too_large() -> BusinessError {
    BusinessError::invalid(
        "SALES_DOCUMENTS_TOO_LARGE",
        "Narrow this sales document search before reading it.",
    )
}

/// Bounded transport limit for the document search query. This is a byte
/// length bound on the query string (a DoS/transport bound), not a Zod
/// `salesQuerySchema.q` is `z.string().max(200)` (Zod counts code points).
pub fn validate_search(value: &str) -> Result<(), BusinessError> {
    if value.chars().count() > SEARCH_LIMIT || value.chars().any(char::is_control) {
        return Err(request_invalid());
    }
    Ok(())
}

pub fn normalize_status(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if !matches!(
        value,
        "draft"
            | "submitted"
            | "confirmed"
            | "partially_fulfilled"
            | "fulfilled"
            | "converted"
            | "cancelled"
    ) {
        return Err(BusinessError::invalid(
            "SALES_DOCUMENT_STATUS_INVALID",
            "Choose an available sales document status.",
        ));
    }
    Ok(Some(value.to_owned()))
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

/// Byte-length source sanity bound for persisted document text (number and
/// party name). These are native row-validation bounds over the stored
/// Commerce/Finance source text, not Zod length schemas, so bytes are kept.
fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn valid_state(value: &str) -> bool {
    matches!(
        value,
        "draft"
            | "submitted"
            | "confirmed"
            | "partially_fulfilled"
            | "fulfilled"
            | "converted"
            | "cancelled"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct SalesDocumentRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub kind: String,
    pub number: String,
    pub party_id: String,
    pub party_name: String,
    pub state: String,
    pub order_date: String,
    pub due_date: String,
    pub currency: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub version: i64,
    pub created_at: i64,
}

impl SalesDocumentRow {
    pub fn validate(
        &self,
        actor: &SalesDocumentReadActor,
        kind: SalesDocumentKind,
    ) -> Result<(), BusinessError> {
        let total = self
            .subtotal_minor
            .checked_add(self.tax_minor)
            .ok_or_else(source_invalid)?;
        if self.company_id != actor.company_id.as_str()
            || self.kind != kind.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.party_id)
            || !valid_text(&self.number, 100)
            || !valid_text(&self.party_name, 300)
            || !valid_state(&self.state)
            || !valid_date(&self.order_date)
            || !valid_date(&self.due_date)
            || !valid_currency(&self.currency)
            || self.subtotal_minor < 0
            || self.tax_minor < 0
            || self.total_minor != total
            || self.version < 1
            || self.created_at < 0
            || self
                .branch_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || self
                .department_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
        {
            return Err(source_invalid());
        }
        if actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| self.branch_id.as_deref() != Some(branch.as_str()))
        {
            return Err(source_invalid());
        }
        if actor
            .department_id
            .as_ref()
            .is_some_and(|department| self.department_id.as_deref() != Some(department.as_str()))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesDocumentCursor {
    pub created_at: i64,
    pub id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    scope: String,
    created_at: i64,
    id: String,
}

pub(crate) fn cursor_scope(
    actor: &SalesDocumentReadActor,
    kind: SalesDocumentKind,
    search: &str,
    status: Option<&str>,
) -> Result<String, BusinessError> {
    serde_json::to_string(&(
        actor.company_id.as_str(),
        actor.branch_id.as_ref().map(RecordId::as_str),
        actor.department_id.as_ref().map(RecordId::as_str),
        kind.as_str(),
        search,
        status,
    ))
    .map_err(|_| cursor_invalid())
}

pub(crate) fn encode_cursor(
    scope: &str,
    cursor: &SalesDocumentCursor,
) -> Result<String, BusinessError> {
    let wire = CursorWire {
        scope: scope.to_owned(),
        created_at: cursor.created_at,
        id: cursor.id.clone(),
    };
    let value = serde_json::to_string(&wire).map_err(|_| cursor_invalid())?;
    if value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    Ok(value)
}

pub(crate) fn decode_cursor(
    value: &str,
    scope: &str,
) -> Result<SalesDocumentCursor, BusinessError> {
    if value.is_empty() || value.len() > CURSOR_LIMIT {
        return Err(cursor_invalid());
    }
    let wire: CursorWire = serde_json::from_str(value).map_err(|_| cursor_invalid())?;
    if wire.scope != scope || wire.created_at < 0 || !valid_identifier(&wire.id) {
        return Err(cursor_invalid());
    }
    Ok(SalesDocumentCursor {
        created_at: wire.created_at,
        id: wire.id,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct SalesDocumentsPage {
    pub items: Vec<SalesDocumentRow>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

pub(crate) fn validate_page(
    actor: &SalesDocumentReadActor,
    kind: SalesDocumentKind,
    cursor: Option<&SalesDocumentCursor>,
    source: Vec<SalesDocumentRow>,
    scope: &str,
) -> Result<SalesDocumentsPage, BusinessError> {
    if source.len() > PAGE_SIZE + 1 {
        return Err(too_large());
    }
    let mut seen = BTreeSet::new();
    let mut previous: Option<(i64, String)> = None;
    for row in &source {
        row.validate(actor, kind)?;
        if !seen.insert(row.id.clone()) {
            return Err(source_invalid());
        }
        let current = (row.created_at, row.id.clone());
        if previous
            .as_ref()
            .is_some_and(|previous| current >= *previous)
        {
            return Err(source_invalid());
        }
        if cursor.is_some_and(|cursor| (cursor.created_at, cursor.id.clone()) <= current) {
            return Err(source_invalid());
        }
        previous = Some(current);
    }
    let has_more = source.len() > PAGE_SIZE;
    let mut items = source;
    if has_more {
        items.truncate(PAGE_SIZE);
    }
    let next_cursor = if has_more {
        items
            .last()
            .map(|row| {
                encode_cursor(
                    scope,
                    &SalesDocumentCursor {
                        created_at: row.created_at,
                        id: row.id.clone(),
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(SalesDocumentsPage {
        items,
        next_cursor,
        has_more,
    })
}
