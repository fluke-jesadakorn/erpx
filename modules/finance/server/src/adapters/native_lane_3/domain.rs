//! Pure response types and invariants for native lane 3's invoice-line read.
//!
//! This is the owner-local successor of the single SELECT in
//! `adapters/invoice-line-read.ts`. The axum router owns the query, the company
//! scope and the response shape; the retained TypeScript consumer only
//! transports bounded identities and validates the reply.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Byte-identical statement inherited from the retired TypeScript reader.
pub const INVOICE_LINES_SQL: &str =
    "SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal";

/// One persisted invoice line. Field names and nullability mirror
/// `PersistedInvoiceLine` in `domain/invoice-line-revision.ts`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct InvoiceLine {
    pub id: String,
    pub company_id: String,
    pub invoice_id: String,
    pub source_version: i64,
    pub ordinal: i32,
    pub description: String,
    pub quantity_millis: Option<i64>,
    pub unit_price_minor: Option<i64>,
    pub tax_bps: Option<i32>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub account_id: String,
    pub tax_account_id: Option<String>,
    pub source_snapshot_json: String,
    pub currency: String,
    pub origin_kind: String,
}

/// Bounded read request. `company_id` always arrives from the trusted context
/// header, never from the caller-supplied body.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct InvoiceLineRequest {
    pub company_id: String,
    pub invoice_id: String,
    pub revision: i64,
}

impl InvoiceLineRequest {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company_id) || !valid_id(&self.invoice_id) || self.revision <= 0 {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct InvoiceLinesData {
    pub lines: Vec<InvoiceLine>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct InvoiceLinesResponse {
    pub data: InvoiceLinesData,
}

impl InvoiceLinesResponse {
    pub fn new(lines: Vec<InvoiceLine>) -> Self {
        Self {
            data: InvoiceLinesData { lines },
        }
    }
}

pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE3_INVOICE_LINES_INPUT_INVALID",
        "Use one bounded company, invoice and revision.",
    )
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE3_INVOICE_LINES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LANE3_INVOICE_LINES_UNAVAILABLE",
        "The native Finance invoice-line service is unavailable.",
    )
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes.as_ref());
    format!("{:x}", hash.finalize())
}

/// Oracle for the exact SQL bytes executed by the owner adapter.
pub fn sql_oracle() -> String {
    hex(INVOICE_LINES_SQL)
}

/// Oracle for a canonical (sorted-key) result projection. `serde_json::Value`
/// uses a `BTreeMap`, so re-serializing a value pins the same canonical ordering
/// as the TypeScript contract oracle.
pub fn result_oracle(lines: &[InvoiceLine]) -> String {
    let value = serde_json::to_value(lines).expect("invoice lines serialize");
    hex(serde_json::to_string(&value).expect("value serializes"))
}

#[cfg(test)]
pub fn fixture() -> Vec<InvoiceLine> {
    let line = |ordinal: i32,
                description: &str,
                subtotal_minor: i64,
                tax_minor: i64,
                total_minor: i64| InvoiceLine {
        id: format!("line-{ordinal}"),
        company_id: "company-1".to_owned(),
        invoice_id: "inv-1".to_owned(),
        source_version: 3,
        ordinal,
        description: description.to_owned(),
        quantity_millis: Some(1000),
        unit_price_minor: Some(5000),
        tax_bps: Some(700),
        subtotal_minor,
        tax_minor,
        total_minor,
        account_id: "acc-1".to_owned(),
        tax_account_id: None,
        source_snapshot_json: "{}".to_owned(),
        currency: "THB".to_owned(),
        origin_kind: "native_line".to_owned(),
    };
    vec![line(1, "alpha", 5000, 350, 5350), line(2, "beta", 10000, 700, 10700)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_oracle_is_stable() {
        assert_eq!(
            sql_oracle(),
            "f49986445a3c579345afe3114abe22f4a5a06d715c043d5c94ead51a5ba00754"
        );
    }

    #[test]
    fn result_oracle_matches_typescript_fixture() {
        assert_eq!(
            result_oracle(&fixture()),
            "b4d46d527382ce7b6ce7cee1edaefd71d397aa755ca85d98e9820a070a14d675"
        );
    }

    #[test]
    fn validation_accepts_bounded_identities() {
        assert!(valid_id("inv-1"));
        assert!(valid_id("company_01"));
        assert!(InvoiceLineRequest {
            company_id: "company-1".to_owned(),
            invoice_id: "inv:1".to_owned(),
            revision: 3,
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn validation_rejects_unsafe_or_unbounded_input() {
        for id in ["", "a/b", "a b", "a?b", &"x".repeat(201)] {
            assert!(!valid_id(id), "accepted {id}");
        }
        for request in [
            InvoiceLineRequest { company_id: "c".into(), invoice_id: "i".into(), revision: 0 },
            InvoiceLineRequest { company_id: "c".into(), invoice_id: "i/../j".into(), revision: 1 },
            InvoiceLineRequest { company_id: "c/1".into(), invoice_id: "i".into(), revision: 1 },
        ] {
            assert_eq!(request.validate(), Err(invalid()));
        }
    }

    #[test]
    fn response_shape_is_nested_under_data() {
        let response = InvoiceLinesResponse::new(fixture());
        let value = serde_json::to_value(&response).expect("serializes");
        assert_eq!(value["data"]["lines"][0]["origin_kind"], "native_line");
        assert_eq!(value["data"]["lines"][1]["total_minor"], 10700);
        assert_eq!(value["data"]["lines"][0]["tax_account_id"], serde_json::Value::Null);
    }
}
