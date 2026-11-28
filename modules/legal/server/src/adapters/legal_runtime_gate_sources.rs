//! Native-owned Legal runtime-gate contract.
//!
//! The settings predicate, capability probe and runtime-health read were the
//! last TypeScript statement tokens in legal/runtime-gate.ts. The exact templates
//! now live in this owner's JSON contract, bound into Rust with `include_str!`
//! and imported by the TypeScript adapter. Callers supply only trusted code-owned
//! company expressions and the scheduled-feature predicate.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned runtime-gate contract.
pub const LEGAL_RUNTIME_GATE_SOURCES_JSON: &str = include_str!("legal-runtime-gate-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalRuntimeGateSources {
    #[serde(rename = "settingsPredicate")]
    pub settings_predicate: String,
    #[serde(rename = "capabilityProbe")]
    pub capability_probe: String,
    pub health: String,
}

static SOURCES: OnceLock<LegalRuntimeGateSources> = OnceLock::new();

pub fn legal_runtime_gate_sources() -> &'static LegalRuntimeGateSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_RUNTIME_GATE_SOURCES_JSON)
            .expect("legal runtime-gate SQL contract must be valid JSON")
    })
}

/// Render the settings predicate for one trusted company expression.
pub fn render_legal_runtime_predicate(
    company_expression: &str,
    ai_enabled: bool,
    scheduled_feature_predicate: &str,
) -> String {
    legal_runtime_gate_sources()
        .settings_predicate
        .replace("{companyExpression}", company_expression)
        .replace(
            "{aiEnabled}",
            if ai_enabled {
                " AND runtime_legal_settings.ai_enabled"
            } else {
                ""
            },
        )
        .replace("{scheduledFeaturePredicate}", scheduled_feature_predicate)
}

/// Render the capability probe around one already-rendered predicate.
pub fn render_legal_capability_probe(predicate: &str) -> String {
    legal_runtime_gate_sources()
        .capability_probe
        .replace("{predicate}", predicate)
}

pub fn legal_runtime_health_sql() -> &'static str {
    &legal_runtime_gate_sources().health
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
            digest(LEGAL_RUNTIME_GATE_SOURCES_JSON),
            "5364f62235b3dd24ee136f4f62c903101acb29845924cf3ac74ad301fcb51c9e"
        );
        let operations = render_legal_runtime_predicate("?", false, "scheduled_feature('legal')");
        assert_eq!(
            operations,
            "EXISTS(SELECT 1 FROM legal_settings runtime_legal_settings WHERE runtime_legal_settings.company_id=? AND runtime_legal_settings.enabled AND scheduled_feature('legal'))"
        );
        let ai = render_legal_runtime_predicate(
            "runtime_legal_settings.company_id",
            true,
            "scheduled_feature('legal')",
        );
        assert_eq!(
            ai,
            "EXISTS(SELECT 1 FROM legal_settings runtime_legal_settings WHERE runtime_legal_settings.company_id=runtime_legal_settings.company_id AND runtime_legal_settings.enabled AND runtime_legal_settings.ai_enabled AND scheduled_feature('legal'))"
        );
        assert_eq!(
            render_legal_capability_probe("EXISTS(SELECT 1)"),
            "SELECT 1 WHERE EXISTS(SELECT 1)"
        );
        assert_eq!(
            legal_runtime_health_sql(),
            "SELECT task,heartbeat_at,last_success_at,failure_count,last_error FROM legal_runtime_health ORDER BY task"
        );
        for sql in [operations.as_str(), ai.as_str(), legal_runtime_health_sql()] {
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
    }

    #[test]
    fn render_preserves_literal_dollar_text_and_leaves_no_placeholders() {
        let predicate = render_legal_runtime_predicate("$&", false, "feature.$1");
        assert!(predicate.contains("company_id=$&"));
        assert!(predicate.contains("feature.$1"));
        assert!(!predicate.contains("{companyExpression}"));
        assert!(!predicate.contains("{aiEnabled}"));
        assert!(!predicate.contains("{scheduledFeaturePredicate}"));
        let probe = render_legal_capability_probe("p.$2");
        assert!(probe.contains("p.$2"));
        assert!(!probe.contains("{predicate}"));
    }
}
