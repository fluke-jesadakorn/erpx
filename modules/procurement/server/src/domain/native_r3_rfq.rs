//! Pure domain for the native R3 RFQ workspace/detail owner slice.
//!
//! The Account session, current Company scope, purchased-package admission and
//! the serializable caller-owned Holding unit remain composition facts. This
//! module only states the owner permission gates and the pure row projections
//! that the adapter applies after its SQLx reads. No SQL, no command receipt and
//! no authority is created here.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::Value;

/// One row from the retained owner reads. The adapter maps every selected
/// column with `to_jsonb`; the domain never invents or reshapes a field.
pub type Row = Value;

/// Account-derived facts for the selected work context. Composition builds
/// this from the current assignment; nothing is accepted from the request.
#[derive(Clone, Debug)]
pub struct RfqActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl RfqActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn require(&self, permission: &str, message: &'static str, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has(permission) {
            return Err(BusinessError::new(FailureKind::Forbidden, "PERMISSION_REQUIRED", message));
        }
        if self.expires_at <= now.get() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "WORK_CONTEXT_EXPIRED",
                "The selected work context expired. Refresh the page and retry.",
            ));
        }
        Ok(())
    }

    /// Retained `commerce.read` gate for the workspace/detail reads.
    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require("commerce.read", "A current commerce.read assignment is required to list quotation requests.", now)
    }

    /// Retained `commerce.purchase` gate for RFQ commands.
    pub fn require_purchase(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require(
            "commerce.purchase",
            "A current commerce.purchase assignment is required to change quotation requests.",
            now,
        )
    }
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "RFQ_NOT_FOUND",
        "Quotation request not found in this scope.",
    )
}

pub fn data_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_RFQ_WORKSPACE_UNAVAILABLE",
        "The Procurement quotation workspace is unavailable.",
    )
}

fn arrays(data: &Value, keys: &[&str]) -> bool {
    keys.iter().all(|key| data.get(*key).is_some_and(Value::is_array))
}

/// Shape guard for the owner-owned workspace facts. The `canPrepare` and
/// `purchase` overlay is a composition fact and is deliberately not checked
/// here.
pub fn validate_workspace(data: &Value) -> Result<(), BusinessError> {
    if data.is_object()
        && arrays(data, &["rfqs", "products", "warehouses", "suppliers"])
        && data.get("currency").is_some_and(Value::is_string)
    {
        Ok(())
    } else {
        Err(data_invalid())
    }
}

/// Shape guard for the owner-owned detail facts. `currentRequestVersion`,
/// `canPrepare`, `canApprove`, `canCancel` and `purchase` are supplied by
/// composition after the owner read.
pub fn validate_detail(data: &Value) -> Result<(), BusinessError> {
    if data.is_object()
        && data.get("rfq").is_some_and(Value::is_object)
        && arrays(data, &["lines", "offers", "offerLines", "events", "current"])
        && data.get("sourceBlocked").is_some_and(Value::is_boolean)
    {
        Ok(())
    } else {
        Err(data_invalid())
    }
}

fn number_text(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::String(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Retained `sourceBlocked` comparison on the Records raw source facts.
pub fn source_blocked(rfq: &Value, source: Option<&Value>) -> bool {
    let Some(request_id) = rfq.get("request_id").and_then(Value::as_str).filter(|id| !id.is_empty()) else {
        return false;
    };
    let _ = request_id;
    if rfq.get("status").and_then(Value::as_str) == Some("ordered") {
        return false;
    }
    let Some(source) = source.filter(|source| source.is_object()) else {
        return true;
    };
    if source.get("state").and_then(Value::as_str) != Some("in_progress") {
        return true;
    }
    if number_text(source.get("evidence_version")) != number_text(rfq.get("request_evidence_version")) {
        return true;
    }
    if source.get("items_json").and_then(Value::as_str) != rfq.get("request_items_json").and_then(Value::as_str) {
        return true;
    }
    let approved = source
        .get("approval_json")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|json| json.get("decision").and_then(Value::as_str).map(|decision| decision == "approved"))
        .unwrap_or(false);
    !approved
}

/// Retained `currentRfqOffers`: one offer per supplier, keeping the first after
/// the adapter's `rfq_version DESC` ordering.
pub fn current_offers(offers: &[Value]) -> Vec<Value> {
    let mut seen = std::collections::BTreeSet::new();
    offers
        .iter()
        .filter(|offer| match offer.get("supplier_id").and_then(Value::as_str) {
            Some(supplier) => seen.insert(supplier.to_owned()),
            None => true,
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn actor(permissions: &[&str]) -> RfqActor {
        RfqActor {
            user_id: "owner".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    #[test]
    fn read_requires_commerce_read() {
        assert!(actor(&[]).require_read(NOW).is_err());
        assert!(actor(&["commerce.read"]).require_read(NOW).is_ok());
    }

    #[test]
    fn purchase_requires_commerce_purchase() {
        assert!(actor(&["commerce.read"]).require_purchase(NOW).is_err());
        assert!(actor(&["commerce.purchase"]).require_purchase(NOW).is_ok());
    }

    #[test]
    fn expired_context_is_rejected() {
        let mut value = actor(&["commerce.read"]);
        value.expires_at = NOW.get();
        assert!(value.require_read(NOW).is_err());
    }

    #[test]
    fn source_without_request_is_not_blocked() {
        assert!(!source_blocked(&json!({"status": "collecting"}), None));
    }

    #[test]
    fn stale_source_blocks() {
        let rfq = json!({"request_id": "r1", "status": "review", "request_evidence_version": 2, "request_items_json": "[]"});
        let source = json!({"state": "in_progress", "evidence_version": 1, "items_json": "[]", "approval_json": "{\"decision\":\"approved\"}"});
        assert!(source_blocked(&rfq, Some(&source)));
        let current = json!({"state": "in_progress", "evidence_version": 2, "items_json": "[]", "approval_json": "{\"decision\":\"approved\"}"});
        assert!(!source_blocked(&rfq, Some(&current)));
    }

    #[test]
    fn current_offers_keep_first_per_supplier() {
        let offers = vec![
            json!({"id": "o2", "supplier_id": "s1"}),
            json!({"id": "o1", "supplier_id": "s1"}),
            json!({"id": "o3", "supplier_id": "s2"}),
        ];
        let current = current_offers(&offers);
        assert_eq!(current.len(), 2);
        assert_eq!(current[0]["id"], "o2");
        assert_eq!(current[1]["id"], "o3");
    }

    #[test]
    fn projections_must_have_expected_shape() {
        let workspace = json!({"rfqs": [], "products": [], "warehouses": [], "suppliers": [], "currency": "THB"});
        assert!(validate_workspace(&workspace).is_ok());
        assert!(validate_workspace(&json!({"currency": "THB"})).is_err());
        let detail = json!({"rfq": {"id": "rfq-1"}, "lines": [], "offers": [], "offerLines": [], "events": [], "current": [], "sourceBlocked": false});
        assert!(validate_detail(&detail).is_ok());
        assert!(validate_detail(&json!({"rfq": "wrong"})).is_err());
    }
}
