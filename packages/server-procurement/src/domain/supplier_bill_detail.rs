//! Pure domain for the native supplier-bill detail read.
//!
//! The retained Supply route returns `bill`, `lines`, `receipts`, `events`,
//! `accounts`, `canReview`, `canPrepare` and the composition-owned `purchase`.
//! Every row is a closed owner JSON projection: the adapter reads the same
//! columns with sqlx and the domain never executes I/O.

use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::{Map, Value};

pub type Row = Value;

pub struct SupplierBillDetailActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// User ids the current identity is allowed to stand for (the actor and
    /// any merged identities), supplied by composition's current IAM fact.
    pub same_person_ids: Vec<String>,
    pub expires_at: i64,
}

impl SupplierBillDetailActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn same_person(&self, id: Value) -> bool {
        id.as_str()
            .is_some_and(|id| self.same_person_ids.iter().any(|value| value == id))
    }

    pub fn can_write(&self) -> bool {
        self.has("finance.write")
    }

    pub fn can_approve(&self) -> bool {
        self.has("finance.approve")
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("finance.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant Finance read access.",
            ));
        }
        if !self.modules.iter().any(|value| value == "procurement") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Procurement.",
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

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPLIER_BILL_NOT_FOUND",
        "Supplier invoice review not found in this scope.",
    )
}

#[derive(Clone, Debug)]
pub struct SupplierBillDetail {
    pub bill: Row,
    pub lines: Vec<Row>,
    pub receipts: Vec<Row>,
    pub events: Vec<Row>,
    pub accounts: Vec<Row>,
    pub can_review: bool,
    pub can_prepare: bool,
    pub purchase: Option<Row>,
}

impl SupplierBillDetail {
    /// Exact retained route `data` payload. Row keys stay snake_case because
    /// the Supply page consumes database column names directly.
    pub fn into_value(self) -> Value {
        let mut value = Map::new();
        value.insert("bill".into(), self.bill);
        value.insert("lines".into(), Value::Array(self.lines));
        value.insert("receipts".into(), Value::Array(self.receipts));
        value.insert("events".into(), Value::Array(self.events));
        value.insert("accounts".into(), Value::Array(self.accounts));
        value.insert("canReview".into(), Value::Bool(self.can_review));
        value.insert("canPrepare".into(), Value::Bool(self.can_prepare));
        if let Some(purchase) = self.purchase {
            value.insert("purchase".into(), purchase);
        }
        Value::Object(value)
    }
}

/// The retained `get(id)` guard: a foreign, out-of-scope or missing match is
/// rejected before any detail row is exposed.
pub fn validate_bill(actor: &SupplierBillDetailActor, bill: &Row) -> Result<(), BusinessError> {
    if bill.get("company_id").and_then(Value::as_str) != Some(actor.company_id.as_str()) {
        return Err(not_found());
    }
    if let Some(branch) = &actor.branch_id {
        if bill.get("branch_id").and_then(Value::as_str) != Some(branch.as_str()) {
            return Err(not_found());
        }
    }
    if let Some(department) = &actor.department_id {
        if bill.get("department_id").and_then(Value::as_str) != Some(department.as_str()) {
            return Err(not_found());
        }
    }
    Ok(())
}

pub fn can_review(actor: &SupplierBillDetailActor, bill: &Row) -> bool {
    let status = bill.get("status").and_then(Value::as_str).unwrap_or_default();
    if actor.can_approve()
        && actor.can_write()
        && !actor.same_person(bill.get("created_by").cloned().unwrap_or(Value::Null))
    {
        if status != "invoiced" {
            return true;
        }
        return bill.get("invoice_status").and_then(Value::as_str) == Some("draft")
            && !actor.same_person(bill.get("invoice_created_by").cloned().unwrap_or(Value::Null));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> SupplierBillDetailActor {
        SupplierBillDetailActor {
            user_id: "reviewer".into(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            permissions: vec![
                "finance.read".into(),
                "finance.write".into(),
                "finance.approve".into(),
            ],
            modules: vec!["procurement".into()],
            same_person_ids: vec!["reviewer".into()],
            expires_at: i64::MAX,
        }
    }

    #[test]
    fn payload_keeps_every_retained_field() {
        let detail = SupplierBillDetail {
            bill: serde_json::json!({"id": "bill-1", "status": "review"}),
            lines: vec![serde_json::json!({"id": "line-1"})],
            receipts: vec![],
            events: vec![],
            accounts: vec![serde_json::json!({"id": "acct", "code": "6300"})],
            can_review: true,
            can_prepare: false,
            purchase: Some(serde_json::json!({"newWork": true})),
        };
        let value = detail.into_value();
        for key in [
            "bill",
            "lines",
            "receipts",
            "events",
            "accounts",
            "canReview",
            "canPrepare",
            "purchase",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert_eq!(value["canReview"], Value::Bool(true));
    }

    #[test]
    fn independent_reviewer_rule_is_preserved() {
        let mut a = actor();
        let own = serde_json::json!({"created_by": "reviewer", "status": "review"});
        assert!(!can_review(&a, &own));
        let other = serde_json::json!({"created_by": "maker", "status": "review"});
        assert!(can_review(&a, &other));
        let invoiced = serde_json::json!({"created_by": "maker", "status": "invoiced", "invoice_status": "posted"});
        assert!(!can_review(&a, &invoiced));
        let draft = serde_json::json!({"created_by": "maker", "status": "invoiced", "invoice_status": "draft", "invoice_created_by": "other"});
        assert!(can_review(&a, &draft));
        a.same_person_ids.push("other".into());
        assert!(!can_review(&a, &draft));
    }

    #[test]
    fn foreign_bill_is_not_found() {
        let a = actor();
        let foreign = serde_json::json!({"company_id": "elsewhere"});
        assert_eq!(
            validate_bill(&a, &foreign).unwrap_err().code,
            "SUPPLIER_BILL_NOT_FOUND"
        );
    }
}
