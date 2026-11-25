//! Native-owned Legal read-tail r38 contract.
//!
//! The fact-table data read and the metric-count projection prefix were the
//! last TypeScript statement tokens in the r38 fragment family. The exact
//! templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. Callers supply only
//! trusted table names, aliases and fixed item references; no request text is
//! interpolated.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r38 read-head contract.
pub const LEGAL_READ_TAIL_R38_SOURCES_JSON: &str = include_str!("legal-read-tail-r38-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR38Sources {
    #[serde(rename = "factData")]
    pub fact_data: String,
    #[serde(rename = "metricCountPrefix")]
    pub metric_count_prefix: String,
}

static SOURCES: OnceLock<LegalReadTailR38Sources> = OnceLock::new();

pub fn legal_read_tail_r38_sources() -> &'static LegalReadTailR38Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R38_SOURCES_JSON)
            .expect("legal read-tail r38 SQL contract must be valid JSON")
    })
}

/// Render the fact-table data read for one trusted table and item reference.
pub fn render_legal_fact_data(table: &str, reference: &str) -> String {
    legal_read_tail_r38_sources()
        .fact_data
        .replace("{table}", table)
        .replace("{ref}", reference)
}

/// Render the metric-count projection prefix for one trusted label and id.
pub fn render_legal_metric_count_prefix(label: &str, id: &str) -> String {
    legal_read_tail_r38_sources()
        .metric_count_prefix
        .replace("{label}", label)
        .replace("{id}", id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_and_read_only() {
        assert_eq!(
            digest(LEGAL_READ_TAIL_R38_SOURCES_JSON),
            "538df0300bdb6b8e92b9db02530fe0effa6a5ce97f1951468862f55489199b1e"
        );
        assert_eq!(
            render_legal_fact_data("legal_requests", "?"),
            "SELECT data FROM legal_requests WHERE item_id=?"
        );
        assert_eq!(
            render_legal_fact_data("legal_matters", "i.id"),
            "SELECT data FROM legal_matters WHERE item_id=i.id"
        );
        assert_eq!(
            render_legal_metric_count_prefix("s.state", "s.id"),
            "SELECT s.state label,count(DISTINCT s.id)::text value"
        );
        let sources = legal_read_tail_r38_sources();
        for template in [&sources.fact_data, &sources.metric_count_prefix] {
            assert!(!template.contains("INSERT INTO"));
            assert!(!template.contains("DELETE FROM"));
            assert!(!template.contains("UPDATE "));
            assert!(!template.contains("FOR UPDATE"));
            assert!(!template.contains("FOR SHARE"));
            assert!(!template.contains("LOCK TABLE"));
            assert!(!template.contains("pg_advisory"));
        }
    }

    #[test]
    fn render_preserves_fixed_references_and_literal_dollar_text() {
        assert_eq!(
            render_legal_fact_data("legal_contracts", "$&"),
            "SELECT data FROM legal_contracts WHERE item_id=$&"
        );
        assert_eq!(
            render_legal_metric_count_prefix("$&", "$1"),
            "SELECT $& label,count(DISTINCT $1)::text value"
        );
        assert!(!render_legal_fact_data("legal_requests", "?").contains("{table}"));
        assert!(!render_legal_fact_data("legal_requests", "?").contains("{ref}"));
        assert!(!render_legal_metric_count_prefix("s.state", "s.id").contains("{label}"));
        assert!(!render_legal_metric_count_prefix("s.state", "s.id").contains("{id}"));
    }
}
