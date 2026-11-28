//! Native-owned Learning ownership-source contract.
//!
//! The Learning source union and its existence probe were the largest static
//! read family in `adapters/learning-read-sql.ts`. The exact bytes now live in
//! this owner's JSON contract, bound into Rust with `include_str!` and imported
//! by the TypeScript adapter. No caller supplies SQL: the probe substitutes
//! only the frozen union and the owner-controlled alias.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned Learning source-union contract.
pub const LEARNING_NATIVE_SOURCES_JSON: &str = include_str!("learning-native-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LearningNativeSources {
    #[serde(rename = "nativeSources")]
    pub native_sources: String,
    #[serde(rename = "nativeSourceProbe")]
    pub native_source_probe: String,
}

static SOURCES: OnceLock<LearningNativeSources> = OnceLock::new();

pub fn learning_native_sources() -> &'static LearningNativeSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEARNING_NATIVE_SOURCES_JSON)
            .expect("learning native sources SQL contract must be valid JSON")
    })
}

/// The frozen ownership-source union. It carries no bind and no caller value.
pub fn learning_native_source_union_sql() -> &'static str {
    &learning_native_sources().native_sources
}

/// Render the existence probe with the frozen union and one alias.
pub fn render_learning_native_source_probe(alias: &str) -> String {
    learning_native_sources()
        .native_source_probe
        .replace("{nativeSources}", learning_native_source_union_sql())
        .replace("{alias}", alias)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("{:x}", Sha256::digest(value.as_bytes()))
    }

    /// Byte oracle migrated from the retired TypeScript union in
    /// `adapters/learning-read-sql.ts`. The native owner is now the single
    /// source of truth for these bytes; no TypeScript statement remains.
    #[test]
    fn native_source_union_oracle_migrated_from_typescript() {
        const MIGRATED_ORACLE: &str = concat!(
            "SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'learn.courses' capability FROM learning_courses",
            "\n UNION ALL SELECT e.id,c.company_id,NULL::text,NULL::text,'learn.courses' FROM learning_course_events e JOIN learning_courses c ON c.id=e.course_id",
            "\n UNION ALL SELECT id,company_id,branch_id,department_id,'learn.assignments' FROM learning_assignments",
            "\n UNION ALL SELECT e.id,a.company_id,a.branch_id,a.department_id,CASE WHEN e.action IN ('complete','waive') THEN 'learn.results' ELSE 'learn.assignments' END FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id",
            "\n UNION ALL SELECT c.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificates c JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id",
            "\n UNION ALL SELECT r.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificate_revocations r JOIN learning_certificates c ON c.id=r.certificate_id AND c.company_id=r.company_id JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id",
            "\n UNION ALL SELECT h.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_eligibility_handoffs h JOIN learning_assignments a ON a.id=h.assignment_id AND a.company_id=h.company_id",
        );
        let union = learning_native_source_union_sql();
        assert_eq!(union, MIGRATED_ORACLE);
        assert_eq!(union.len(), 1299);
        assert_eq!(union.matches("SELECT ").count(), 7);
        assert_eq!(union.matches("UNION ALL SELECT ").count(), 6);
        assert_eq!(union.matches('?').count(), 0);
    }

    #[test]
    fn contract_bytes_are_pinned() {
        assert_eq!(
            digest(LEARNING_NATIVE_SOURCES_JSON),
            "cdf39beb66d1b850542ae2d07a0987542a8346a97bf94a2669da5d2d1e21105b"
        );
    }

    #[test]
    fn probe_renders_only_the_frozen_union_and_alias() {
        let union = learning_native_source_union_sql();
        for alias in ["learning_source", "actual_learning", "native"] {
            let probe = render_learning_native_source_probe(alias);
            assert_eq!(probe, format!("SELECT 1 FROM ({union}) {alias}"));
            assert_eq!(probe.matches("SELECT ").count(), 8);
            assert!(!probe.contains('{'));
            assert!(!probe.contains('}'));
            assert!(!probe.contains(';'));
        }
        assert!(!render_learning_native_source_probe("feature.$&").contains("{alias}"));
    }

    #[test]
    fn contract_is_read_only() {
        let mut statements = vec![learning_native_source_union_sql().to_owned()];
        statements.push(render_learning_native_source_probe("learning_source"));
        for sql in statements {
            assert!(sql.starts_with("SELECT "));
            for write in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
            ] {
                assert!(!sql.to_uppercase().contains(write), "{write}");
            }
        }
    }
}
