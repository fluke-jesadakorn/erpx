//! Pure domain for the native Commerce entrypoint identity read family.
//!
//! These facts gate module routing and native-eligibility decisions. The owned
//! response is deliberately small: an order/fulfillment/return kind, and the
//! mutable order state only for the `order_facts` capability. No Company,
//! permission or cross-owner projection is inferred here.

use erpx_platform_shared_server::public::{BusinessError, RecordId};
use serde_json::Value;

/// The five owner-local reads retired from `adapters/entrypoint-facts.ts`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EntrypointKind {
    OrderKind,
    OrderFacts,
    FulfillmentKind,
    InvoiceKind,
    ReturnKind,
}

impl EntrypointKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrderKind => "orderKind",
            Self::OrderFacts => "orderFacts",
            Self::FulfillmentKind => "fulfillmentKind",
            Self::InvoiceKind => "invoiceKind",
            Self::ReturnKind => "returnKind",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orderKind" => Some(Self::OrderKind),
            "orderFacts" => Some(Self::OrderFacts),
            "fulfillmentKind" => Some(Self::FulfillmentKind),
            "invoiceKind" => Some(Self::InvoiceKind),
            "returnKind" => Some(Self::ReturnKind),
            _ => None,
        }
    }

    pub const fn carries_state(self) -> bool {
        matches!(self, Self::OrderFacts)
    }
}

/// Bounded request carried over the wire. No SQL or caller-supplied predicate
/// is accepted; only the registered discriminator and one record identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntrypointQuery {
    pub kind: EntrypointKind,
    pub id: String,
}

/// Owner response. `state` is present only for `order_facts`, preserving the
/// exact retired call-site shapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntrypointFacts {
    pub kind: String,
    pub state: Option<String>,
}

impl EntrypointFacts {
    pub fn into_value(self) -> Value {
        let mut value = serde_json::json!({"kind": self.kind});
        if let Some(state) = self.state {
            value
                .as_object_mut()
                .expect("object literal")
                .insert("state".to_owned(), Value::String(state));
        }
        value
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_REQUEST_INVALID",
        "Use one bounded Commerce entrypoint read.",
    )
}

/// Record identities are bounded before they reach a bind parameter.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate(value: &Value, company: &str) -> Result<(EntrypointQuery, String), BusinessError> {
    let kind = value.get("kind").and_then(Value::as_str).ok_or_else(invalid)?;
    let id = value.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
    let kind = EntrypointKind::parse(kind).ok_or_else(invalid)?;
    if !valid_id(id) || !valid_id(company) {
        return Err(invalid());
    }
    Ok((EntrypointQuery { kind, id: id.to_owned() }, company.to_owned()))
}

// ---- Invoice/delivery read family ---------------------------------------
// Retired from `invoice-command-policy.ts` (`invoiceCommandFlowTarget`) and
// `finance-sales-sources.ts` (`commerceInvoiceSalesLines`). Both are plain
// reads of committed Commerce records, owner-local and company-scoped.

/// Closed discriminator for the invoice/delivery read family.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InvoiceReadKind {
    DeliveryTarget,
    InvoiceSalesLines,
}

impl InvoiceReadKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeliveryTarget => "deliveryTarget",
            Self::InvoiceSalesLines => "invoiceSalesLines",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "deliveryTarget" => Some(Self::DeliveryTarget),
            "invoiceSalesLines" => Some(Self::InvoiceSalesLines),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceReadQuery {
    pub kind: InvoiceReadKind,
    pub id: String,
}

/// Bounded owner projection of a delivery -> order command target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandTarget {
    pub kind: String,
    pub id: String,
    pub action: String,
}

impl CommandTarget {
    pub fn into_value(self) -> Value {
        serde_json::json!({"kind": self.kind, "id": self.id, "action": self.action})
    }
}

/// One delivered sales line; the owner keeps the original column names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesLine {
    pub id: String,
    pub fulfillment_id: String,
    pub description: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub item_type: String,
    pub product_id: String,
    pub completion_date: String,
}

impl SalesLine {
    pub fn into_value(self) -> Value {
        serde_json::json!({
            "id": self.id,
            "fulfillment_id": self.fulfillment_id,
            "description": self.description,
            "quantity_milli": self.quantity_milli,
            "price_minor": self.price_minor,
            "tax_bps": self.tax_bps,
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "item_type": self.item_type,
            "product_id": self.product_id,
            "completion_date": self.completion_date,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceFacts {
    Target(Option<CommandTarget>),
    Lines(Vec<SalesLine>),
}

impl InvoiceFacts {
    /// Exactly one envelope key is present, so a strict client cannot confuse
    /// a command target with a line list.
    pub fn into_value(self) -> Value {
        match self {
            Self::Target(target) => serde_json::json!({"target": target.map(CommandTarget::into_value)}),
            Self::Lines(lines) => {
                serde_json::json!({"lines": lines.into_iter().map(SalesLine::into_value).collect::<Vec<_>>()})
            }
        }
    }
}

pub fn invoice_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_INVOICE_READ_REQUEST_INVALID",
        "Use one bounded Commerce invoice read.",
    )
}

pub fn validate_invoice(value: &Value, company: &str) -> Result<(InvoiceReadQuery, String), BusinessError> {
    let kind = value.get("kind").and_then(Value::as_str).ok_or_else(invoice_invalid)?;
    let id = value.get("id").and_then(Value::as_str).ok_or_else(invoice_invalid)?;
    let kind = InvoiceReadKind::parse(kind).ok_or_else(invoice_invalid)?;
    if !valid_id(id) || !valid_id(company) {
        return Err(invoice_invalid());
    }
    Ok((InvoiceReadQuery { kind, id: id.to_owned() }, company.to_owned()))
}

// ---- Billing test finance entry reads -----------------------------------
// Retired from `billing-test-finance.ts`: the two plain reads of Commerce-owned
// `billing_test_finance_entries`. The `FOR UPDATE` document read and the
// lines-balance read stay in the retained TypeScript unit.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BillingTestReadKind {
    EntryBySourceKey,
    EntryTotalsByStripe,
}

impl BillingTestReadKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EntryBySourceKey => "entryBySourceKey",
            Self::EntryTotalsByStripe => "entryTotalsByStripe",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "entryBySourceKey" => Some(Self::EntryBySourceKey),
            "entryTotalsByStripe" => Some(Self::EntryTotalsByStripe),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingTestQuery {
    pub kind: BillingTestReadKind,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingEntry {
    pub entry_kind: String,
    pub amount_satang: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingTotals {
    pub issued: i64,
    pub received: i64,
    pub reversed: i64,
    pub credited: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BillingFacts {
    Entry(Option<BillingEntry>),
    Totals(BillingTotals),
}

impl BillingFacts {
    pub fn into_value(self) -> Value {
        match self {
            Self::Entry(entry) => serde_json::json!({
                "entry": entry.map(|entry| serde_json::json!({
                    "entry_kind": entry.entry_kind,
                    "amount_satang": entry.amount_satang,
                }))
            }),
            Self::Totals(totals) => serde_json::json!({
                "totals": {
                    "issued": totals.issued,
                    "received": totals.received,
                    "reversed": totals.reversed,
                    "credited": totals.credited,
                }
            }),
        }
    }
}

pub fn billing_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_BILLING_TEST_READ_REQUEST_INVALID",
        "Use one bounded Commerce billing test read.",
    )
}

pub fn validate_billing(value: &Value, company: &str) -> Result<(BillingTestQuery, String), BusinessError> {
    let kind = value.get("kind").and_then(Value::as_str).ok_or_else(billing_invalid)?;
    let id = value.get("id").and_then(Value::as_str).ok_or_else(billing_invalid)?;
    let kind = BillingTestReadKind::parse(kind).ok_or_else(billing_invalid)?;
    if !valid_id(id) || !valid_id(company) {
        return Err(billing_invalid());
    }
    Ok((BillingTestQuery { kind, id: id.to_owned() }, company.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_registry_is_closed() {
        for kind in [
            "orderKind",
            "orderFacts",
            "fulfillmentKind",
            "invoiceKind",
            "returnKind",
        ] {
            let parsed = EntrypointKind::parse(kind).expect("registered kind");
            assert_eq!(parsed.as_str(), kind);
        }
        assert!(EntrypointKind::parse("SELECT 1").is_none());
        assert!(EntrypointKind::parse("").is_none());
        assert!(EntrypointKind::OrderFacts.carries_state());
        assert!(!EntrypointKind::OrderKind.carries_state());
    }

    #[test]
    fn validation_rejects_unbounded_identities() {
        assert!(!valid_id(""));
        assert!(!valid_id("bad id!"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_id("order-1"));
        let request = serde_json::json!({"kind": "orderFacts", "id": "order-1"});
        let (query, company) = validate(&request, "co-1").expect("valid request");
        assert_eq!(query.kind, EntrypointKind::OrderFacts);
        assert_eq!(query.id, "order-1");
        assert_eq!(company, "co-1");
        assert!(validate(&serde_json::json!({"kind": "drop", "id": "x"}), "co-1").is_err());
        assert!(validate(&request, "bad company!").is_err());
    }

    #[test]
    fn response_shape_omits_state_for_kind_reads() {
        let kind = EntrypointFacts {
            kind: "sales".to_owned(),
            state: None,
        }
        .into_value();
        assert_eq!(kind, serde_json::json!({"kind": "sales"}));
        assert!(kind.get("state").is_none());
        let facts = EntrypointFacts {
            kind: "sales".to_owned(),
            state: Some("draft".to_owned()),
        }
        .into_value();
        assert_eq!(facts, serde_json::json!({"kind": "sales", "state": "draft"}));
    }

    #[test]
    fn invoice_registry_is_closed_and_validated() {
        assert_eq!(InvoiceReadKind::parse("deliveryTarget"), Some(InvoiceReadKind::DeliveryTarget));
        assert_eq!(InvoiceReadKind::parse("invoiceSalesLines"), Some(InvoiceReadKind::InvoiceSalesLines));
        assert!(InvoiceReadKind::parse("SELECT").is_none());
        let (query, company) = validate_invoice(
            &serde_json::json!({"kind": "deliveryTarget", "id": "d-1"}),
            "co-1",
        )
        .expect("valid");
        assert_eq!(query.kind, InvoiceReadKind::DeliveryTarget);
        assert_eq!(query.id, "d-1");
        assert_eq!(company, "co-1");
        assert!(validate_invoice(&serde_json::json!({"kind": "x", "id": "d"}), "co-1").is_err());
        assert!(validate_invoice(&serde_json::json!({"kind": "deliveryTarget", "id": "d"}), "bad company").is_err());
    }

    #[test]
    fn invoice_response_envelopes_are_disjoint() {
        let target = InvoiceFacts::Target(Some(CommandTarget {
            kind: "order".to_owned(),
            id: "o-1".to_owned(),
            action: "invoice".to_owned(),
        }))
        .into_value();
        assert_eq!(
            target,
            serde_json::json!({"target":{"kind":"order","id":"o-1","action":"invoice"}})
        );
        assert!(target.get("lines").is_none());
        let null = InvoiceFacts::Target(None).into_value();
        assert_eq!(null, serde_json::json!({"target":null}));
        let lines = InvoiceFacts::Lines(vec![SalesLine {
            id: "fl-1".to_owned(),
            fulfillment_id: "f-1".to_owned(),
            description: "service".to_owned(),
            quantity_milli: 1000,
            price_minor: 250,
            tax_bps: 700,
            subtotal_minor: 250,
            tax_minor: 18,
            item_type: "service".to_owned(),
            product_id: "p-1".to_owned(),
            completion_date: "2026-11-25".to_owned(),
        }])
        .into_value();
        assert!(lines.get("lines").is_some());
        assert!(lines.get("target").is_none());
        assert_eq!(lines["lines"][0]["quantity_milli"], 1000);
    }

    #[test]
    fn billing_registry_is_closed_and_validated() {
        assert_eq!(
            BillingTestReadKind::parse("entryBySourceKey"),
            Some(BillingTestReadKind::EntryBySourceKey)
        );
        assert_eq!(
            BillingTestReadKind::parse("entryTotalsByStripe"),
            Some(BillingTestReadKind::EntryTotalsByStripe)
        );
        assert!(BillingTestReadKind::parse("FOR UPDATE").is_none());
        let (query, company) = validate_billing(
            &serde_json::json!({"kind": "entryBySourceKey", "id": "inv:issued"}),
            "erpx_billing_sandbox",
        )
        .expect("valid");
        assert_eq!(query.kind, BillingTestReadKind::EntryBySourceKey);
        assert_eq!(query.id, "inv:issued");
        assert_eq!(company, "erpx_billing_sandbox");
        assert!(validate_billing(&serde_json::json!({"kind": "x", "id": "k"}), "co").is_err());
    }

    #[test]
    fn billing_envelopes_are_disjoint() {
        let entry = BillingFacts::Entry(Some(BillingEntry {
            entry_kind: "issued".to_owned(),
            amount_satang: 500,
        }))
        .into_value();
        assert_eq!(
            entry,
            serde_json::json!({"entry":{"entry_kind":"issued","amount_satang":500}})
        );
        assert!(entry.get("totals").is_none());
        let absent = BillingFacts::Entry(None).into_value();
        assert_eq!(absent, serde_json::json!({"entry":null}));
        let totals = BillingFacts::Totals(BillingTotals {
            issued: 500,
            received: 400,
            reversed: 0,
            credited: 100,
        })
        .into_value();
        assert_eq!(
            totals,
            serde_json::json!({"totals":{"issued":500,"received":400,"reversed":0,"credited":100}})
        );
        assert!(totals.get("entry").is_none());
    }
}
