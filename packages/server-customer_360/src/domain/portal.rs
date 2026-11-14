//! Native customer-portal projection and acknowledgement facts.
//!
//! The portal is external-customer scoped. Composition resolves the registered
//! portal host/slug, verifies the opaque session and the durable enrollment
//! (`erpx_server_customer_authority::public::portal_session`) and constructs
//! [`PortalCustomerActor`] from that proof. This owner never verifies a token,
//! trusts a caller-supplied email, or re-implements the sales.portal purchase
//! authority; it only reads the existing customer portal tables and records one
//! idempotent acknowledgement.
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};

pub const PROJECTION_VERSION: &str = "customer-order-v1";
pub const MAX_BINDING_ID: usize = 200;
pub const MAX_ATTACHMENT_SET: usize = 20;
pub const MAX_ATTACHMENT_ID: usize = 180;
pub const MAX_NOTE: usize = 500;
pub const MIN_KEY: usize = 8;
pub const MAX_KEY: usize = 180;
pub const MAX_ITEMS: usize = 201;
pub const MAX_IDENTIFIER: usize = 180;
pub const MAX_EVENT_IDENTIFIER: usize = 200;
const MAX_EMAIL: usize = 254;
const MAX_TEXT: usize = 300;
const MAX_TIMESTAMP: i64 = 9_007_199_254_740_991;
const ACTIONABLE_STATES: [&str; 4] = ["confirmed", "converted", "partially_fulfilled", "fulfilled"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalHolding {
    pub id: String,
    pub slug: String,
    pub name: String,
}

/// Verified external customer identity. `holding` is required only when the
/// command carries attachment evidence; it is the registered Holding resolved
/// by composition, never a browser-supplied value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalCustomerActor {
    pub user_id: String,
    pub email: String,
    pub holding: Option<PortalHolding>,
}

impl PortalCustomerActor {
    /// Email is normalized exactly once. Construction must come from the
    /// committed portal-session proof, not from a request header.
    pub fn new(
        user_id: impl Into<String>,
        email: impl Into<String>,
        holding: Option<PortalHolding>,
    ) -> Result<Self, BusinessError> {
        let user_id = user_id.into();
        let email = email.into();
        let normalized = email.trim().to_lowercase();
        let mut parts = normalized.split('@');
        let local = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        if user_id.is_empty()
            || user_id.len() > MAX_BINDING_ID
            || user_id.chars().any(char::is_control)
            || !valid_email(local, domain, parts.next())
        {
            return Err(session_invalid());
        }
        if let Some(holding) = &holding
            && (!bounded(&holding.id, MAX_BINDING_ID)
                || !bounded(&holding.slug, 128)
                || !bounded(&holding.name, MAX_TEXT)
                || !holding.id.chars().all(|value| {
                    value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.' | ':')
                })
                || !holding.slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                }))
        {
            return Err(session_invalid());
        }
        Ok(Self {
            user_id,
            email: normalized,
            holding,
        })
    }

    pub fn require_attachment_context(&self) -> Result<&PortalHolding, BusinessError> {
        self.holding.as_ref().ok_or_else(attachment_context)
    }
}

fn valid_email(local: &str, domain: &str, extra: Option<&str>) -> bool {
    !local.is_empty()
        && !domain.is_empty()
        && extra.is_none()
        && domain.contains('.')
        && local.len() + domain.len() < MAX_EMAIL
        && !local
            .bytes()
            .chain(domain.bytes())
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn bounded(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalPurchaseEnforcement {
    /// Composition reports Commercial package enforcement disabled; every
    /// approved binding visible to the verified identity is readable.
    Disabled,
    /// Only bindings in these companies have an active `sales.portal` history
    /// purchase. Composition derives the list from the Commercial owner.
    Enforced,
}

/// Purchase visibility supplied by the Commercial authority. This owner never
/// evaluates package capability SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalPurchaseScope {
    pub enforcement: PortalPurchaseEnforcement,
    pub company_ids: Vec<String>,
}

impl PortalPurchaseScope {
    pub fn disabled() -> Self {
        Self {
            enforcement: PortalPurchaseEnforcement::Disabled,
            company_ids: Vec::new(),
        }
    }

    pub fn enforced(company_ids: Vec<String>) -> Result<Self, BusinessError> {
        let mut unique = std::collections::BTreeSet::new();
        for company in &company_ids {
            erpx_server_shared::public::CompanyId::new(company.clone())
                .map_err(|_| projection_invalid())?;
            if !unique.insert(company.clone()) {
                return Err(projection_invalid());
            }
        }
        Ok(Self {
            enforcement: PortalPurchaseEnforcement::Enforced,
            company_ids,
        })
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        match self.enforcement {
            PortalPurchaseEnforcement::Disabled => {
                if !self.company_ids.is_empty() {
                    return Err(projection_invalid());
                }
            }
            PortalPurchaseEnforcement::Enforced => {
                let mut seen = std::collections::BTreeSet::new();
                for company in &self.company_ids {
                    erpx_server_shared::public::CompanyId::new(company.clone())
                        .map_err(|_| projection_invalid())?;
                    if !seen.insert(company) {
                        return Err(projection_invalid());
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalBinding {
    pub binding_id: String,
    pub company_id: String,
    pub order_id: String,
    pub customer_id: String,
    pub email: String,
    pub status: String,
    pub expires_at: i64,
    pub binding_version: i64,
    pub order_version: i64,
    pub number: String,
    pub state: String,
}

impl PortalBinding {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.binding_id, MAX_BINDING_ID)
            || erpx_server_shared::public::CompanyId::new(self.company_id.clone()).is_err()
            || RecordId::new(self.order_id.clone()).is_err()
            || RecordId::new(self.customer_id.clone()).is_err()
            || !bounded(&self.number, MAX_TEXT)
            || !bounded(&self.state, 64)
            || self.binding_version < 1
            || self.order_version < 1
            || self.expires_at < 0
            || self.expires_at > MAX_TIMESTAMP
        {
            return Err(source_invalid());
        }
        let mut parts = self.email.split('@');
        let local = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        if !valid_email(local, domain, parts.next()) {
            return Err(source_invalid());
        }
        Ok(())
    }

    pub fn is_current(&self, now: i64) -> bool {
        self.status == "approved" && self.expires_at > now
    }

    pub fn is_actionable(&self) -> bool {
        ACTIONABLE_STATES.contains(&self.state.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalAction {
    AcknowledgeDelivery,
    RequestSupport,
}

impl PortalAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AcknowledgeDelivery => "acknowledge_delivery",
            Self::RequestSupport => "request_support",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "acknowledge_delivery" => Some(Self::AcknowledgeDelivery),
            "request_support" => Some(Self::RequestSupport),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortalCommandInput {
    pub binding_id: String,
    pub expected_binding_version: i64,
    pub action: PortalAction,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

impl PortalCommandInput {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.binding_id, MAX_BINDING_ID) {
            return Err(request_invalid());
        }
        if self.expected_binding_version < 1 || self.expected_binding_version > MAX_TIMESTAMP {
            return Err(request_invalid());
        }
        if self.note.len() > MAX_NOTE || self.note.chars().any(char::is_control) {
            return Err(BusinessError::invalid(
                "CUSTOMER_PORTAL_NOTE_INVALID",
                "Record a note of at most 500 characters.",
            ));
        }
        if self.attachment_set_ids.len() > MAX_ATTACHMENT_SET {
            return Err(attachment_invalid());
        }
        let mut unique = std::collections::BTreeSet::new();
        for id in &self.attachment_set_ids {
            if !bounded(id, MAX_ATTACHMENT_ID) || !unique.insert(id) {
                return Err(attachment_invalid());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalActionRow {
    pub id: String,
    pub binding_id: String,
    pub order_id: String,
    pub action: String,
    pub status: String,
    pub created_at: i64,
    pub request_hash: String,
    pub user_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalCommandPurpose {
    SettleExisting,
    History,
}

/// Fully validated command passed to owner ports. Composition supplies
/// `request_hash` and a unique `id`; `key` remains the idempotency key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalCommandFacts {
    pub input: PortalCommandInput,
    pub key: String,
    pub id: String,
    pub now: i64,
    pub request_hash: String,
    pub actor: PortalCustomerActor,
    pub binding: PortalBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalCommandOutcome {
    pub id: String,
    pub binding_id: String,
    pub order_id: String,
    pub action: String,
    pub status: String,
    pub created_at: i64,
}

impl PortalCommandOutcome {
    pub fn new(
        id: String,
        binding_id: String,
        order_id: String,
        action: impl Into<String>,
        status: String,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            binding_id,
            order_id,
            action: action.into(),
            status,
            created_at,
        }
    }

    pub fn from_row(row: &PortalActionRow) -> Self {
        Self::new(
            row.id.clone(),
            row.binding_id.clone(),
            row.order_id.clone(),
            row.action.clone(),
            row.status.clone(),
            row.created_at,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalFulfillment {
    pub number: String,
    pub created_at: i64,
    pub recorded: bool,
}

impl PortalFulfillment {
    fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.number, MAX_TEXT) || self.created_at < 0 {
            return Err(projection_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalActionFact {
    pub action: String,
    pub created_at: i64,
}

impl PortalActionFact {
    fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.action, 64) || self.created_at < 0 {
            return Err(projection_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerSharedFile {
    pub link_id: String,
    pub label: String,
    pub preview_available: bool,
}

impl CustomerSharedFile {
    fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.link_id, MAX_BINDING_ID)
            || !bounded(&self.label, MAX_TEXT)
            || !self.preview_available
        {
            return Err(projection_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerPortalItem {
    pub binding_id: String,
    pub binding_version: i64,
    pub order_id: String,
    pub number: String,
    pub kind: String,
    pub state: String,
    pub due_date: String,
    pub currency: String,
    pub updated_at: i64,
    pub fulfillments: Vec<PortalFulfillment>,
    pub actions: Vec<PortalActionFact>,
    #[serde(rename = "sharedFiles", default)]
    pub shared_files: Vec<CustomerSharedFile>,
    /// Commercial purchase controls merged by composition from the existing
    /// `sales.portal` authority. Values are not re-authored here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase: Option<serde_json::Value>,
    /// Support thread projection merged by composition from Customer
    /// Authority/Support. Values are not re-authored here.
    #[serde(
        rename = "supportThreads",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub support_threads: Option<serde_json::Value>,
}

impl CustomerPortalItem {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !bounded(&self.binding_id, MAX_BINDING_ID)
            || RecordId::new(self.order_id.clone()).is_err()
            || !bounded(&self.number, MAX_TEXT)
            || !bounded(&self.kind, 64)
            || !bounded(&self.state, 64)
            || !bounded(&self.due_date, 32)
            || !bounded(&self.currency, 8)
            || self.binding_version < 1
            || self.updated_at < 0
            || self.updated_at > MAX_TIMESTAMP
            || self.fulfillments.len() > 100
            || self.actions.len() > 100
            || self.shared_files.len() > MAX_ATTACHMENT_SET
        {
            return Err(projection_invalid());
        }
        for fulfillment in &self.fulfillments {
            fulfillment.validate()?;
        }
        for action in &self.actions {
            action.validate()?;
        }
        for file in &self.shared_files {
            file.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPortalProjection {
    pub items: Vec<CustomerPortalItem>,
    pub projection_version: &'static str,
}

impl CustomerPortalProjection {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.projection_version != PROJECTION_VERSION || self.items.len() > MAX_ITEMS {
            return Err(projection_invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            item.validate()?;
            if !seen.insert(&item.binding_id) {
                return Err(projection_invalid());
            }
        }
        Ok(())
    }
}

pub fn session_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "CUSTOMER_PORTAL_SESSION_INVALID",
        "Sign in again to open customer documents.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_REQUEST_INVALID",
        "Use one bounded customer portal request.",
    )
}

pub fn command_key_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

pub fn binding_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_PORTAL_BINDING_UNAVAILABLE",
        "This customer document is not available to your account.",
    )
}

pub fn binding_expired() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_PORTAL_BINDING_EXPIRED",
        "This customer access has expired or was revoked.",
    )
}

pub fn binding_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_PORTAL_BINDING_CHANGED",
        "The customer document access changed. Refresh before submitting.",
    )
}

pub fn source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_PORTAL_SOURCE_UNAVAILABLE",
        "This customer document is no longer available for portal actions.",
    )
}

pub fn identity_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_PORTAL_IDENTITY_REQUIRED",
        "Open the approved document once before submitting a customer action.",
    )
}

pub fn action_already_recorded() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_PORTAL_ACTION_ALREADY_RECORDED",
        "This customer action has already been recorded.",
    )
}

pub fn attachment_action() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_ATTACHMENT_ACTION",
        "Files may be attached only to a support request.",
    )
}

pub fn attachment_context() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_PORTAL_ATTACHMENT_CONTEXT",
        "Reload the customer portal before attaching files.",
    )
}

pub fn attachment_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_ATTACHMENT_INVALID",
        "Choose at most 20 available attachments.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

pub fn projection_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_PORTAL_PROJECTION_INVALID",
        "The customer portal projection is invalid.",
    )
}

pub fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_PORTAL_SOURCE_INVALID",
        "The current customer portal source is invalid.",
    )
}

/// `Idempotency-Key` grammar shared by the HTTP ingress and the use case.
pub fn valid_key(value: &str) -> bool {
    (MIN_KEY..=MAX_KEY).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

/// Identifiers are supplied by composition so this owner needs no randomness.
pub fn valid_identifier(value: &str) -> bool {
    bounded(value, MAX_IDENTIFIER)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

pub fn valid_event_identifier(value: &str) -> bool {
    bounded(value, MAX_EVENT_IDENTIFIER)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

pub fn valid_read_id(value: &str) -> bool {
    valid_identifier(value)
}

pub fn valid_request_hash(value: &str) -> bool {
    (16..=128).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
