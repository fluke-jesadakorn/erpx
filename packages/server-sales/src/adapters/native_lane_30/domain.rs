//! Pure domain types for the native Sales customer contact-note read.
//!
//! This is the owner-local successor of the four SELECT executions in
//! `adapters/notes.ts`. The Rust owner owns the SQL, the company/branch/department
//! scope, the row projection and the exact response shape. TypeScript is a
//! transport-only client; no business rule or SQL remains in the caller.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde_json::{Map, Value, json};

pub const NOTE_SOURCE_ORDER: &str = "order";
pub const NOTE_SOURCE_CUSTOMER: &str = "customer";

/// Byte-identical rendered statements retired from `notes.ts`. The `?`
/// placeholders are the TypeScript unit's positional binds; `postgres.rs`
/// starts from these statements and rewrites only the placeholder numbering and
/// the projection for sqlx. These constants are the SQL result oracle.
pub const ORACLE_ORDER_NOTES_SQL: &str = "SELECT n.* FROM customer_contact_notes n JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) ORDER BY n.created_at DESC,n.id DESC";
pub const ORACLE_CUSTOMER_NOTES_SQL: &str = "SELECT n.* FROM customer_contact_notes n LEFT JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY n.created_at DESC,n.id DESC";
pub const ORACLE_ORDER_HISTORY_SQL: &str = "SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) ORDER BY h.created_at DESC,h.version DESC";
pub const ORACLE_CUSTOMER_HISTORY_SQL: &str = "SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id LEFT JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY h.created_at DESC,h.version DESC";

/// Exact note projection. The TypeScript `ContactNote` shape is preserved
/// field-for-field; the scope-witness columns never leave the owner.
#[derive(Clone, Debug)]
pub struct NoteRecord {
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
}

impl NoteRecord {
    pub fn into_value(self) -> Value {
        let mut object = Map::new();
        object.insert("id".into(), json!(self.id));
        object.insert("company_id".into(), json!(self.company_id));
        object.insert("customer_id".into(), json!(self.customer_id));
        object.insert("order_id".into(), json!(self.order_id));
        object.insert("channel".into(), json!(self.channel));
        object.insert("occurred_at".into(), json!(self.occurred_at));
        object.insert("body".into(), json!(self.body));
        object.insert("status".into(), json!(self.status));
        object.insert("version".into(), json!(self.version));
        object.insert("created_by".into(), json!(self.created_by));
        object.insert("created_at".into(), json!(self.created_at));
        object.insert("updated_at".into(), json!(self.updated_at));
        Value::Object(object)
    }
}

/// Immutable history projection; `customer_contact_note_history` has no
/// company column, so the company witness comes from the parent note.
#[derive(Clone, Debug)]
pub struct HistoryRecord {
    pub id: String,
    pub note_id: String,
    pub version: i64,
    pub action: String,
    pub body: String,
    pub reason: String,
    pub actor_id: String,
    pub created_at: i64,
}

impl HistoryRecord {
    pub fn into_value(self) -> Value {
        let mut object = Map::new();
        object.insert("id".into(), json!(self.id));
        object.insert("note_id".into(), json!(self.note_id));
        object.insert("version".into(), json!(self.version));
        object.insert("action".into(), json!(self.action));
        object.insert("body".into(), json!(self.body));
        object.insert("reason".into(), json!(self.reason));
        object.insert("actor_id".into(), json!(self.actor_id));
        object.insert("created_at".into(), json!(self.created_at));
        Value::Object(object)
    }
}

/// The exact response shape returned to the transport client.
#[derive(Clone, Debug, Default)]
pub struct NotesResponse {
    pub notes: Vec<NoteRecord>,
    pub history: Vec<HistoryRecord>,
}

impl NotesResponse {
    pub fn into_value(self) -> Value {
        json!({
            "notes": self.notes.into_iter().map(NoteRecord::into_value).collect::<Vec<_>>(),
            "history": self.history.into_iter().map(HistoryRecord::into_value).collect::<Vec<_>>(),
        })
    }
}

/// Bounded, owned read request. All values cross the wire as bounded headers;
/// none is concatenated into SQL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteQuery {
    pub company: String,
    pub branch: Option<String>,
    pub department: Option<String>,
    pub source: String,
    pub id: String,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_CUSTOMER_NOTES_REQUEST_INVALID",
        "Use one bounded customer-note read.",
    )
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

impl NoteQuery {
    /// Validate every field before a reader is invoked. Branch and department
    /// are optional null-scope witnesses; a supplied value must be a valid id.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company)
            || !valid_id(&self.id)
            || !matches!(self.source.as_str(), NOTE_SOURCE_ORDER | NOTE_SOURCE_CUSTOMER)
            || self.branch.as_deref().is_some_and(|value| !valid_id(value))
            || self.department.as_deref().is_some_and(|value| !valid_id(value))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> NoteQuery {
        NoteQuery {
            company: "company-1".into(),
            branch: None,
            department: None,
            source: NOTE_SOURCE_ORDER.into(),
            id: "order-1".into(),
        }
    }

    #[test]
    fn valid_query_passes() {
        assert!(query().validate().is_ok());
        let mut scoped = query();
        scoped.branch = Some("branch-1".into());
        scoped.department = Some("dept-1".into());
        assert!(scoped.validate().is_ok());
    }

    #[test]
    fn invalid_ids_sources_and_scopes_fail_closed() {
        let mut q = query();
        q.company = String::new();
        assert!(q.validate().is_err());
        let mut q = query();
        q.id = "bad id".into();
        assert!(q.validate().is_err());
        let mut q = query();
        q.source = "other".into();
        assert!(q.validate().is_err());
        let mut q = query();
        q.branch = Some("../../orders".into());
        assert!(q.validate().is_err());
    }

    #[test]
    fn response_shape_is_exact() {
        let response = NotesResponse {
            notes: vec![NoteRecord {
                id: "note-1".into(),
                company_id: "company-1".into(),
                customer_id: "customer-1".into(),
                order_id: Some("order-1".into()),
                channel: "phone".into(),
                occurred_at: 1,
                body: "Called the customer.".into(),
                status: "active".into(),
                version: 2,
                created_by: "user-1".into(),
                created_at: 3,
                updated_at: 4,
            }],
            history: vec![HistoryRecord {
                id: "history-1".into(),
                note_id: "note-1".into(),
                version: 1,
                action: "create".into(),
                body: "Called the customer.".into(),
                reason: "initial".into(),
                actor_id: "user-1".into(),
                created_at: 3,
            }],
        };
        let value = response.into_value();
        let object = value.as_object().expect("object");
        assert_eq!(object.len(), 2);
        assert_eq!(object["notes"][0].as_object().expect("note").len(), 12);
        assert_eq!(object["history"][0].as_object().expect("history").len(), 8);
    }

    #[test]
    fn oracle_sql_is_byte_identical_read_only_and_company_scoped() {
        // Byte counts pin the rendered TypeScript statements (git HEAD) before
        // the projection was moved into Rust. SHA-256 digests are in the report.
        for (sql, bytes, binds) in [
            (ORACLE_ORDER_NOTES_SQL, 655, 6),
            (ORACLE_CUSTOMER_NOTES_SQL, 754, 8),
            (ORACLE_ORDER_HISTORY_SQL, 715, 6),
            (ORACLE_CUSTOMER_HISTORY_SQL, 814, 8),
        ] {
            assert_eq!(sql.len(), bytes);
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            let words: Vec<&str> = upper.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            assert!(!words.contains(&"INSERT"));
            assert!(!words.contains(&"UPDATE"));
            assert!(!words.contains(&"DELETE"));
            assert!(upper.contains("COMPANY_ID"));
            assert_eq!(sql.matches('?').count(), binds);
        }
    }
}
