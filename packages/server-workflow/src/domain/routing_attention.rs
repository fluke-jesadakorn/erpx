//! Portable Routing-attention query contract and page facts.
//!
//! Exact port of `packages/server-workflow/src/domain/routing-attention.ts`
//! (the `routingQuerySchema` defaults/limits) and the item/page shapes returned
//! by `packages/server-workflow/src/application/routing-attention.ts`. It owns
//! no SQL, authority or flow resolution; the caller supplies owner rows and
//! resolved flows.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

/// The retained `flowKinds` order from `@erpx/contracts/command-flow`.
pub const FLOW_KINDS: [&str; 11] = [
    "expense",
    "leave",
    "staff_task",
    "request",
    "stock_count",
    "transfer",
    "order",
    "invoice",
    "journal",
    "payroll",
    "record",
];

/// One retained flow kind. Only these values are accepted by the query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowKind {
    Expense,
    Leave,
    StaffTask,
    Request,
    StockCount,
    Transfer,
    Order,
    Invoice,
    Journal,
    Payroll,
    Record,
}

impl FlowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Leave => "leave",
            Self::StaffTask => "staff_task",
            Self::Request => "request",
            Self::StockCount => "stock_count",
            Self::Transfer => "transfer",
            Self::Order => "order",
            Self::Invoice => "invoice",
            Self::Journal => "journal",
            Self::Payroll => "payroll",
            Self::Record => "record",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        FLOW_KINDS
            .iter()
            .position(|kind| *kind == value)
            .map(|index| {
                [
                    Self::Expense,
                    Self::Leave,
                    Self::StaffTask,
                    Self::Request,
                    Self::StockCount,
                    Self::Transfer,
                    Self::Order,
                    Self::Invoice,
                    Self::Journal,
                    Self::Payroll,
                    Self::Record,
                ][index]
            })
    }
}

pub const MAX_AFTER: usize = 180;
pub const DEFAULT_LIMIT: i64 = 25;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 50;

/// The validated `routingQuerySchema` output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingQuery {
    pub kind: FlowKind,
    pub after: String,
    pub limit: i64,
}

fn query_invalid() -> BusinessError {
    BusinessError::invalid(
        "ROUTING_QUERY_INVALID",
        "Check the routing scan query and try again.",
    )
}

/// Coerce one JSON value exactly like `z.coerce.number()` (`Number(value)`).
fn coerce_number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => parse_js_number(text),
        serde_json::Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        serde_json::Value::Null => Some(0.0),
        _ => None,
    }
}

/// JavaScript `Number(string)` for the query-string shapes that reach here.
fn parse_js_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
        if let Some(digits) = trimmed.strip_prefix(prefix) {
            return i64::from_str_radix(digits, radix)
                .ok()
                .map(|value| value as f64);
        }
    }
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if matches!(digits, "Infinity" | "infinity") {
        return Some(sign * f64::INFINITY);
    }
    if matches!(digits, "NaN") {
        return Some(f64::NAN);
    }
    trimmed.parse::<f64>().ok()
}

/// Count UTF-16 code units like JavaScript string length (`z.string().max`).
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

impl RoutingQuery {
    /// Parse the retained `routingQuerySchema` from an untrusted object.
    ///
    /// Missing fields take the retained defaults; explicit `null` is invalid
    /// (Zod defaults apply to `undefined`, not `null`); unknown fields are
    /// stripped like the retained non-strict object schema.
    pub fn parse(input: &serde_json::Value) -> Result<Self, BusinessError> {
        let object = input.as_object().ok_or_else(query_invalid)?;

        let kind = match object.get("kind") {
            None => FlowKind::Expense,
            Some(serde_json::Value::String(value)) => {
                FlowKind::parse(value).ok_or_else(query_invalid)?
            }
            Some(_) => return Err(query_invalid()),
        };

        let after = match object.get("after") {
            None => String::new(),
            Some(serde_json::Value::String(value)) => {
                if utf16_len(value) > MAX_AFTER {
                    return Err(query_invalid());
                }
                value.clone()
            }
            Some(_) => return Err(query_invalid()),
        };

        let limit = match object.get("limit") {
            None => DEFAULT_LIMIT,
            Some(value) => {
                let number = coerce_number(value).ok_or_else(query_invalid)?;
                if !number.is_finite() || number.fract() != 0.0 {
                    return Err(query_invalid());
                }
                // JavaScript/Serde numbers beyond i64 are outside the retained
                // integer range and therefore fail the schema limits anyway.
                let limit = number as i64;
                if !(MIN_LIMIT..=MAX_LIMIT).contains(&limit) {
                    return Err(query_invalid());
                }
                limit
            }
        };

        Ok(Self { kind, after, limit })
    }
}

/// One owner routing page row. Only the display filters and the scan cursor
/// are retained; every authority/flow fact comes from the owner ports.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingAttentionRow {
    pub id: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub invoice_number: Option<String>,
    #[serde(default)]
    pub journal_number: Option<String>,
    #[serde(default)]
    pub merchant: Option<String>,
    #[serde(default)]
    pub pay_date: Option<String>,
}

impl RoutingAttentionRow {
    /// Retained `row.title || row.number || ... || row.id` display fallback.
    pub fn display_title(&self) -> String {
        let candidates = [
            &self.title,
            &self.number,
            &self.invoice_number,
            &self.journal_number,
            &self.merchant,
            &self.pay_date,
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| self.id.clone())
    }
}

/// The subset of the resolved `FlowView` routing attention consumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingAttentionFlow {
    pub state: String,
    pub blocker: String,
    pub can_manage: bool,
    pub action: Option<String>,
    pub team: Option<String>,
    pub record_version: i64,
    pub route_version: i64,
}

/// One returned attention item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingAttentionItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub blocker: String,
    pub action: Option<String>,
    pub team: Option<String>,
    pub record_version: i64,
    pub route_version: i64,
}

/// The retained route payload (`coverage: 'named_flow'`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingAttentionPage {
    pub kind: String,
    pub items: Vec<RoutingAttentionItem>,
    pub next_after: Option<String>,
    pub checked_at: i64,
    pub coverage: String,
}

/// The retained owner page size; a short page ends the scan.
pub const ROUTING_PAGE_SIZE: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_the_retained_query_schema() {
        let query = RoutingQuery::parse(&json!({})).unwrap();
        assert_eq!(
            query,
            RoutingQuery {
                kind: FlowKind::Expense,
                after: String::new(),
                limit: 25,
            }
        );
        assert_eq!(query.kind.as_str(), "expense");
    }

    #[test]
    fn coerces_numeric_strings_and_rejects_limits_outside_one_to_fifty() {
        assert_eq!(
            RoutingQuery::parse(&json!({"kind": "invoice", "after": "abc", "limit": "1"}))
                .unwrap()
                .limit,
            1
        );
        assert_eq!(
            RoutingQuery::parse(&json!({"limit": 50})).unwrap().limit,
            50
        );
        // JavaScript `Number("0x10")` is 16; explicit empty/zero stay invalid.
        assert_eq!(
            RoutingQuery::parse(&json!({"limit": "0x10"})).unwrap().limit,
            16
        );
        for limit in [json!(0), json!(51), json!(1.5), json!("many"), json!(null), json!("")] {
            assert_eq!(
                RoutingQuery::parse(&json!({ "limit": limit }))
                    .unwrap_err()
                    .code,
                "ROUTING_QUERY_INVALID"
            );
        }
    }

    #[test]
    fn rejects_unknown_kinds_typed_after_and_non_object_input() {
        assert_eq!(
            RoutingQuery::parse(&json!({"kind": "unknown"}))
                .unwrap_err()
                .code,
            "ROUTING_QUERY_INVALID"
        );
        assert_eq!(
            RoutingQuery::parse(&json!({"after": 7}))
                .unwrap_err()
                .code,
            "ROUTING_QUERY_INVALID"
        );
        assert_eq!(
            RoutingQuery::parse(&json!({"after": "x".repeat(181)}))
                .unwrap_err()
                .code,
            "ROUTING_QUERY_INVALID"
        );
        assert_eq!(
            RoutingQuery::parse(&json!("expense")).unwrap_err().code,
            "ROUTING_QUERY_INVALID"
        );
        // Unknown fields are stripped like the retained non-strict object.
        assert_eq!(
            RoutingQuery::parse(&json!({"kind": "leave", "extra": true}))
                .unwrap()
                .kind,
            FlowKind::Leave
        );
    }

    #[test]
    fn display_title_uses_the_retained_truthy_fallback_order() {
        let mut row = RoutingAttentionRow {
            id: "id".into(),
            title: Some(String::new()),
            number: Some("EXP-1".into()),
            invoice_number: Some("INV-1".into()),
            ..Default::default()
        };
        assert_eq!(row.display_title(), "EXP-1");
        row.number = None;
        assert_eq!(row.display_title(), "INV-1");
        row.invoice_number = None;
        assert_eq!(row.display_title(), "id");
    }
}
