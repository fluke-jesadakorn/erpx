//! Native-owned Legal read-tail r42 contract.
//!
//! The parameterized feature-search read head was the last TypeScript statement
//! token in the r42 family. The exact template now lives in this owner's JSON
//! contract, bound into Rust with `include_str!` and imported by the TypeScript
//! adapter. Callers supply only the fixed projection and trusted fact table; no
//! request text is interpolated.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r42 feature-search contract.
pub const LEGAL_READ_TAIL_R42_SOURCES_JSON: &str = include_str!("legal-read-tail-r42-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR42Sources {
    #[serde(rename = "featureSearch")]
    pub feature_search: String,
}

static SOURCES: OnceLock<LegalReadTailR42Sources> = OnceLock::new();

pub fn legal_read_tail_r42_sources() -> &'static LegalReadTailR42Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R42_SOURCES_JSON)
            .expect("legal read-tail r42 SQL contract must be valid JSON")
    })
}

/// Render the feature-search head for one fixed projection and fact table.
pub fn render_legal_feature_search(projection: &str, table: &str) -> String {
    legal_read_tail_r42_sources()
        .feature_search
        .replace("{projection}", projection)
        .replace("{table}", table)
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
            digest(LEGAL_READ_TAIL_R42_SOURCES_JSON),
            "d06959eeb15ff385058b086a2c284c22e64d1119c77664f019763c014773d500"
        );
        assert_eq!(
            render_legal_feature_search("count(*) n", "legal_obligations"),
            "SELECT count(*) n FROM legal_obligations WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)"
        );
        assert_eq!(
            render_legal_feature_search("*", "legal_case_events"),
            "SELECT * FROM legal_case_events WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)"
        );
        let sql = render_legal_feature_search("*", "legal_case_events");
        assert_eq!(sql.matches('?').count(), 3);
        for write in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
        ] {
            assert!(!sql.contains(write), "{write}");
        }
    }

    #[test]
    fn render_preserves_literal_dollar_text_and_leaves_no_placeholders() {
        assert_eq!(
            render_legal_feature_search("$&", "legal_$1"),
            "SELECT $& FROM legal_$1 WHERE item_id=? AND (?='' OR strpos(lower(title||' '||data::text),lower(?))>0)"
        );
        let sql = render_legal_feature_search("*", "legal_obligations");
        assert!(!sql.contains("{projection}"));
        assert!(!sql.contains("{table}"));
    }
}
