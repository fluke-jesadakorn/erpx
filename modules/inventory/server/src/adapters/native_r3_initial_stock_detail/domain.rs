//! Bounded POS Initial Physical Stock count-detail read: pure query and row
//! shape. No SQL, no authority grant and no HTTP transport lives here.

/// Native mount requested for the R3 POS Initial Physical Stock detail read.
/// The lead owns the composition mount; this crate never edits it.
pub const READ_PATH: &str =
    "/api/v1/holdings/{holding_slug}/pos/initial-physical-stock/{count_id}";
pub const SOURCE_KIND: &str = "pos-initial-physical-stock-count";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailQuery {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub count_id: String,
    pub actor_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountRow {
    pub id: String,
    pub number: String,
    pub status: String,
    pub version: i64,
    pub count_date: String,
    pub reason: String,
    pub warehouse_id: String,
    pub warehouse_name: String,
    pub created_by: String,
    pub prepared_by: String,
    pub journal_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineRow {
    pub product_id: String,
    pub sku: String,
    pub description: String,
    pub unit: String,
    pub tracking_mode: String,
    pub counted_quantity_milli: i64,
    pub counted_value_minor: i64,
    pub unit_cost_minor: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LotRow {
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub counted_quantity_milli: i64,
    pub unit_cost_minor: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionRow {
    pub action: String,
    pub reason: Option<String>,
    pub actor_name: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailRow {
    pub count: CountRow,
    pub line: Option<LineRow>,
    pub lots: Vec<LotRow>,
    pub decisions: Vec<DecisionRow>,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_values_only() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_id("count-123"));
        assert!(!valid_id("count 123"));
    }
}
