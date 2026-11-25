//! Pure domain types, validation and response invariants for the native
//! Manufacturing lane-28 read families: initial-physical-stock history and
//! existing-posting economic references. Rust owns the statements, aggregation
//! and response shapes; TypeScript only transports the facts.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::Serialize;

/// Byte-identical successor of the retired initial-stock history guard.
pub const STOCK_HISTORY_SQL: &str = "SELECT EXISTS(SELECT 1 FROM manufacturing_material_reservations r JOIN manufacturing_orders o ON o.id=r.order_id WHERE o.company_id=$1 AND r.warehouse_id=$2 AND r.product_id=$3) OR EXISTS(SELECT 1 FROM manufacturing_inventory_moves m JOIN manufacturing_orders o ON o.id=m.order_id WHERE o.company_id=$1 AND m.warehouse_id=$2 AND m.product_id=$3) AS history";
pub const STOCK_HISTORY_SQL_SHA256: &str = "f0c4bf5922861bd1152a965183c89f9175125d30af2eab2c3c600ebd02c289cf";

/// Byte-identical successor of the retired existing-posting statement. The
/// only differences are the Postgres bind marker (`$1` instead of `?`) and
/// the explicit `proven` result column, already the TypeScript alias.
pub const POSTING_REFERENCES_SQL: &str = r#"SELECT posting.id,posting.order_id,posting.journal_id,posting.amount_minor,to_jsonb(posting)::text image,
  EXISTS(SELECT 1 FROM (SELECT posting.id posting_id,posting.company_id,posting.journal_id id FROM manufacturing_order_postings posting
 JOIN manufacturing_orders production ON production.id=posting.order_id AND production.company_id=posting.company_id AND production.version>=posting.source_order_version
 JOIN planning_recommendations recommendation ON recommendation.id=production.recommendation_id
 JOIN planning_scenarios scenario ON scenario.id=recommendation.scenario_id AND scenario.company_id=production.company_id AND scenario.branch_id=production.branch_id AND scenario.warehouse_id=production.warehouse_id AND scenario.specification_id=production.specification_id AND scenario.specification_version=production.specification_version AND scenario.status='released'
 JOIN manufacturing_specifications specification ON specification.id=production.specification_id AND specification.company_id=production.company_id
 JOIN manufacturing_order_events event ON event.order_id=production.id AND event.version=posting.source_order_version AND event.actor_id=posting.created_by AND event.id='mfg-event-'||substring(posting.id FROM length('mfg-posting-')+1)
 JOIN command_receipts receipt ON receipt.command_id=substring(posting.id FROM length('mfg-posting-')+1) AND receipt.record_id=production.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=production.company_id AND receipt.operation='manufacturing.order.'||event.action
 JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id AND profile.version>=posting.profile_version
 JOIN finance_journals journal ON journal.id=posting.journal_id AND journal.company_id=posting.company_id AND journal.chart_id=profile.chart_id AND journal.currency=posting.currency AND journal.currency=profile.currency AND journal.source_type='manufacturing_'||posting.phase AND journal.source_id=production.id AND journal.status='posted'
 WHERE posting.amount_minor>0 AND event.action=CASE posting.phase WHEN 'issue' THEN 'issue' WHEN 'report' THEN 'report' WHEN 'output' THEN 'pass-qc' ELSE 'scrap' END
 AND (event.details_json::jsonb->>'expectedVersion')::bigint=posting.source_order_version-1
 AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=posting.amount_minor AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=posting.amount_minor
 AND (SELECT sum(amount_minor) FROM manufacturing_cost_movements movement WHERE movement.company_id=posting.company_id AND movement.order_id=production.id AND movement.source_type='production_event' AND movement.source_id=event.id AND movement.journal_id=journal.id)=posting.amount_minor) actual WHERE actual.posting_id=posting.id AND actual.company_id=posting.company_id AND actual.id=posting.journal_id) proven
  FROM manufacturing_order_postings posting WHERE posting.company_id=$1 ORDER BY posting.id"#;
pub const POSTING_REFERENCES_SQL_SHA256: &str = "df762c4105ba57bad6682939c76a1402170ff660b44a0f30e1c85f8bb59caafd";

pub const MAX_IDENTIFIER_BYTES: usize = 200;

/// Deterministic 64-bit FNV-1a shared with the TypeScript contract tests.
pub fn sql_oracle(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockHistoryFacts {
    pub history: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PostingRow {
    pub id: String,
    pub order_id: String,
    pub journal_id: String,
    pub amount_minor: i64,
    pub image: String,
    pub proven: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostingReferencesFacts {
    pub journal_ids: Vec<String>,
    pub order_ids: Vec<String>,
    pub invalid: bool,
    pub image: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_LANE28_STOCK_HISTORY_INVALID",
        "Use bounded Manufacturing stock-history identities.",
    )
}

pub fn validate_identifier(value: &str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !RecordId::new(value).is_ok()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
    {
        return Err(invalid());
    }
    Ok(())
}

/// Aggregation moved out of SQL: unique ids keep first-seen row order and the
/// invalid witness matches the retired TypeScript predicate exactly.
pub fn project(rows: Vec<PostingRow>) -> PostingReferencesFacts {
    let mut journal_ids: Vec<String> = Vec::new();
    let mut order_ids: Vec<String> = Vec::new();
    let mut invalid = false;
    for row in &rows {
        if !journal_ids.contains(&row.journal_id) {
            journal_ids.push(row.journal_id.clone());
        }
        if !order_ids.contains(&row.order_id) {
            order_ids.push(row.order_id.clone());
        }
        if row.amount_minor > 0 && !row.proven {
            invalid = true;
        }
    }
    PostingReferencesFacts {
        journal_ids,
        order_ids,
        invalid,
        image: serde_json::to_string(&rows).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_bounded() {
        assert!(validate_identifier("company").is_ok());
        assert!(validate_identifier("").is_err());
        assert!(validate_identifier("bad id!").is_err());
        assert!(validate_identifier(&"x".repeat(MAX_IDENTIFIER_BYTES + 1)).is_err());
    }

    #[test]
    fn rendered_sql_matches_byte_oracles() {
        assert_eq!(sql_oracle(STOCK_HISTORY_SQL), 0x374c62bc1d3d2304u64);
        assert_eq!(sql_oracle(POSTING_REFERENCES_SQL), 0xdee51aa0db9ca4fcu64);
        assert_eq!(STOCK_HISTORY_SQL.matches("SELECT ").count(), 3);
        assert_eq!(POSTING_REFERENCES_SQL.matches("SELECT ").count(), 6);
    }

    #[test]
    fn aggregation_preserves_order_and_witness() {
        let rows = vec![
            PostingRow { id: "p1".into(), order_id: "o1".into(), journal_id: "j1".into(), amount_minor: 100, image: "{}".into(), proven: true },
            PostingRow { id: "p2".into(), order_id: "o1".into(), journal_id: "j1".into(), amount_minor: 50, image: "{}".into(), proven: false },
        ];
        let facts = project(rows);
        assert_eq!(facts.journal_ids, vec!["j1"]);
        assert_eq!(facts.order_ids, vec!["o1"]);
        assert!(facts.invalid);
        assert_eq!(facts.image, r#"[{"id":"p1","order_id":"o1","journal_id":"j1","amount_minor":100,"image":"{}","proven":true},{"id":"p2","order_id":"o1","journal_id":"j1","amount_minor":50,"image":"{}","proven":false}]"#);
    }

    #[test]
    fn response_shapes_are_camel_case() {
        assert_eq!(
            serde_json::to_value(StockHistoryFacts { history: true }).unwrap(),
            serde_json::json!({"history": true}),
        );
        let facts = PostingReferencesFacts { journal_ids: vec![], order_ids: vec![], invalid: false, image: "[]".into() };
        let value = serde_json::to_value(&facts).unwrap();
        assert_eq!(value["journalIds"], serde_json::json!([]));
        assert_eq!(value["orderIds"], serde_json::json!([]));
        assert_eq!(value["invalid"], false);
    }
}
