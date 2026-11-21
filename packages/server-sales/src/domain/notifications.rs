//! Native Sales customer-notification command rules.
//!
//! Settings, queue and retry are normal owner commands over the immutable
//! notice/attempt model. Delivery itself remains the native worker; these
//! commands never send mail, never rewrite an attempt and never accept a
//! supplied provider receipt as truth.
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const SETTINGS_OPERATION: &str = "commerce.customer-notification-settings.update";
pub const QUEUE_OPERATION: &str = "commerce.customer-notification.queue";
pub const RETRY_OPERATION: &str = "commerce.customer-notification.retry";
pub const MAX_NOTE: usize = 500;

pub fn not_configured() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_EMAIL_NOT_CONFIGURED",
        "Configure the original customer-notice provider before retrying.",
    )
}

pub fn feature_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FEATURE_COMING_SOON",
        "Customer notices are not released for this Company.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "The customer notification changed. Refresh before retrying.",
    )
}

pub fn delivery_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "DELIVERY_NOT_FOUND",
        "The customer delivery is unavailable in this work scope.",
    )
}

pub fn notification_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_NOTIFICATION_NOT_FOUND",
        "The customer notification is unavailable in this work scope.",
    )
}

pub fn consent_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_NOTIFICATION_CONSENT_REQUIRED",
        "Record a verified customer notification email and granted consent before sending.",
    )
}

pub fn provider_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_NOTIFICATION_PROVIDER_REQUIRED",
        "Configure a sandbox or live customer email adapter before sending.",
    )
}

pub fn already_notified() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_ALREADY_NOTIFIED",
        "This delivery already has a customer notification.",
    )
}

pub fn not_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_NOT_FAILED",
        "Only a failed customer notification can be retried.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SALES_NOTIFICATION_SOURCE_CHANGED",
        "The captured customer notice source changed before commit.",
    )
}

pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FOLLOWUP_ACCESS_CHANGED",
        "Choose a current authorized Sales role.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_NOTIFICATION_REQUEST_INVALID",
        "Review the customer notification request.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to another customer notification command.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SALES_NOTIFICATION_READBACK_INVALID",
        "The original customer notification result is unavailable.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub role: String,
    pub session_id: Option<String>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
}

impl NotificationActor {
    pub fn can(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "userId": self.user_id,
            "assignmentId": self.assignment_id,
            "role": self.role,
            "companyId": self.company_id,
            "branchId": self.branch_id,
            "departmentId": self.department_id,
            "iam": {
                "sessionId": self.session_id,
                "queuedAssignmentVersion": self.assignment_version,
            },
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAdapter {
    Disabled,
    Sandbox,
    Resend,
}

impl SettingsAdapter {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "disabled" => Some(Self::Disabled),
            "sandbox" => Some(Self::Sandbox),
            "resend" => Some(Self::Resend),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Sandbox => "sandbox",
            Self::Resend => "resend",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateSettings {
    pub expected_version: i64,
    pub adapter: SettingsAdapter,
}

impl UpdateSettings {
    pub fn validate(self) -> Result<Self, BusinessError> {
        if self.expected_version < 0 {
            return Err(request_invalid());
        }
        Ok(self)
    }

    pub fn identity(&self, actor: &NotificationActor, key: &str) -> String {
        serde_json::json!({
            "operation": SETTINGS_OPERATION,
            "recordId": actor.company_id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": {
                "expectedVersion": self.expected_version,
                "adapter": self.adapter.as_str(),
            },
        })
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueNotification {
    pub expected_version: i64,
    pub note: String,
}

impl QueueNotification {
    pub fn validate(self) -> Result<Self, BusinessError> {
        let note = self.note.trim().to_owned();
        if self.expected_version < 1 || note.chars().count() < 3 || note.chars().count() > MAX_NOTE
        {
            return Err(request_invalid());
        }
        Ok(Self { note, ..self })
    }

    pub fn identity(&self, actor: &NotificationActor, fulfillment_id: &str, key: &str) -> String {
        serde_json::json!({
            "operation": QUEUE_OPERATION,
            "recordId": fulfillment_id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": {
                "expectedVersion": self.expected_version,
                "note": self.note,
            },
        })
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryNotification {
    pub expected_version: i64,
}

impl RetryNotification {
    pub fn validate(self) -> Result<Self, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        Ok(self)
    }

    pub fn identity(&self, actor: &NotificationActor, id: &str, key: &str) -> String {
        serde_json::json!({
            "operation": RETRY_OPERATION,
            "recordId": id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": {"expectedVersion": self.expected_version},
        })
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryFacts {
    pub fulfillment_id: String,
    pub fulfillment_version: i64,
    pub fulfillment_number: String,
    pub fulfillment_date: String,
    pub order_id: String,
    pub order_number: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub party_id: String,
    pub party_name: String,
    pub party_version: i64,
    pub notification_email: Option<String>,
    pub notification_consent: Option<String>,
    pub adapter: Option<String>,
    pub settings_version: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticeSource {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub fulfillment_id: String,
    pub order_id: String,
    pub party_id: String,
    pub recipient_email: String,
    pub subject: String,
    pub body_text: String,
    pub adapter: String,
    pub status: String,
    pub version: i64,
    pub attempts: i64,
    pub provider_message_id: Option<String>,
    pub created_by: String,
    pub order_state: Option<String>,
    pub customer_status: Option<String>,
    pub notification_email: Option<String>,
    pub notification_consent: Option<String>,
    pub current_adapter: Option<String>,
    pub source_company: Option<String>,
    pub customer_company: Option<String>,
    pub source_hash: String,
}

impl NoticeSource {
    pub fn in_scope(&self, actor: &NotificationActor) -> bool {
        self.company_id == actor.company_id
            && actor
                .branch_id
                .as_ref()
                .is_none_or(|branch| Some(branch) == self.branch_id.as_ref())
            && actor
                .department_id
                .as_ref()
                .is_none_or(|department| Some(department) == self.department_id.as_ref())
    }

    pub fn invalid_reason(&self) -> Option<&'static str> {
        if self.source_company.as_deref() != Some(self.company_id.as_str())
            || self.customer_company.as_deref() != Some(self.company_id.as_str())
            || !matches!(
                self.order_state.as_deref(),
                Some("confirmed" | "partially_fulfilled" | "fulfilled")
            )
        {
            return Some("CUSTOMER_NOTIFICATION_SOURCE_CHANGED");
        }
        if self.customer_status.as_deref() != Some("active")
            || self.notification_consent.as_deref() != Some("granted")
            || self
                .notification_email
                .as_deref()
                .map(str::to_lowercase)
                .as_deref()
                != Some(self.recipient_email.as_str())
        {
            return Some("CUSTOMER_CONSENT_OR_EMAIL_CHANGED");
        }
        if self.current_adapter.as_deref() != Some(self.adapter.as_str()) {
            return Some("CUSTOMER_NOTIFICATION_PROVIDER_CHANGED");
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationReceipt {
    pub id: String,
    pub order_id: String,
    pub status: String,
    pub version: i64,
}

impl NotificationReceipt {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "orderId": self.order_id,
            "status": self.status,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsReceipt {
    pub company_id: String,
    pub adapter: String,
    pub version: i64,
}

impl SettingsReceipt {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "companyId": self.company_id,
            "adapter": self.adapter,
            "version": self.version,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
}

pub fn replay_settings(snapshot: &NotificationReceiptSnapshot) -> Result<SettingsReceipt, BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(&snapshot.result_json).map_err(|_| readback_invalid())?;
    let object = value.as_object().ok_or_else(readback_invalid)?;
    let company = object.get("companyId").and_then(serde_json::Value::as_str);
    let adapter = object.get("adapter").and_then(serde_json::Value::as_str);
    let version = object.get("version").and_then(serde_json::Value::as_i64);
    match (company, adapter, version) {
        (Some(company), Some(adapter), Some(version))
            if !company.is_empty()
                && SettingsAdapter::parse(adapter).is_some()
                && version >= 1 =>
        {
            Ok(SettingsReceipt {
                company_id: company.to_owned(),
                adapter: adapter.to_owned(),
                version,
            })
        }
        _ => Err(readback_invalid()),
    }
}

pub fn replay_notice(snapshot: &NotificationReceiptSnapshot) -> Result<NotificationReceipt, BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(&snapshot.result_json).map_err(|_| readback_invalid())?;
    let object = value.as_object().ok_or_else(readback_invalid)?;
    let id = object.get("id").and_then(serde_json::Value::as_str);
    let order = object.get("orderId").and_then(serde_json::Value::as_str);
    let status = object.get("status").and_then(serde_json::Value::as_str);
    let version = object.get("version").and_then(serde_json::Value::as_i64);
    match (id, order, status, version) {
        (Some(id), Some(order), Some(status), Some(version))
            if !id.is_empty() && !order.is_empty() && status == "pending" && version >= 1 =>
        {
            Ok(NotificationReceipt {
                id: id.to_owned(),
                order_id: order.to_owned(),
                status: status.to_owned(),
                version,
            })
        }
        _ => Err(readback_invalid()),
    }
}

pub fn message(facts: &DeliveryFacts, lines: &[(String, i64, String)], note: &str) -> (String, String) {
    let subject = format!(
        "Delivery {} for order {}",
        facts.fulfillment_number, facts.order_number
    );
    let mut body = vec![
        format!("Hello {},", facts.party_name),
        format!(
            "Your delivery {} for order {} was recorded on {}.",
            facts.fulfillment_number, facts.order_number, facts.fulfillment_date
        ),
    ];
    for (sku, quantity_milli, unit) in lines {
        body.push(format!(
            "{sku}: {} {unit}",
            *quantity_milli as f64 / 1000.0
        ));
    }
    body.push(format!("Update from our team: {note}"));
    (subject, body.join("\n\n"))
}
