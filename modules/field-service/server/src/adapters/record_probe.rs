//! Native-owned fixed record probes for the retained generic Records partition.
//!
//! These statements replaced the TypeScript `record-purchases` SQL literals.
//! The exact text lives in this owner's JSON contract, which `include_str!`
//! binds into the Rust crate; the TypeScript Records adapter reads the same
//! contract through its native port and carries no SQL literal. A caller never
//! supplies SQL or a relation name: it selects a closed `kind`, and executes the
//! returned statement on its already-open serializable Holding unit so the
//! guard observes uncommitted effects in the same transaction.

/// Native-owned SQL contract for the Field record-source probes.
pub const FIELD_RECORD_PROBE_SOURCES_JSON: &str = include_str!("record-probe-sources.json");

#[derive(serde::Deserialize)]
struct FieldRecordProbeSources {
    #[serde(rename = "sourceReadScopeProbe")]
    source_read_scope_probe: String,
    #[serde(rename = "workspaceRecordProbe")]
    workspace_record_probe: String,
}

static FIELD_RECORD_PROBES: std::sync::OnceLock<FieldRecordProbeSources> = std::sync::OnceLock::new();

fn probes() -> &'static FieldRecordProbeSources {
    FIELD_RECORD_PROBES.get_or_init(|| {
        serde_json::from_str(FIELD_RECORD_PROBE_SOURCES_JSON)
            .expect("field record probe SQL contract must be valid JSON")
    })
}

/// Exact fixed SQL of one retired TypeScript Field record probe. The kind is a
/// closed discriminator, never caller-supplied SQL.
pub fn field_record_probe(kind: &str) -> &'static str {
    let source = probes();
    match kind {
        "sourceReadScopeProbe" => &source.source_read_scope_probe,
        "workspaceRecordProbe" => &source.workspace_record_probe,
        _ => panic!("unknown Field record probe kind"),
    }
}

#[cfg(test)]
mod record_probe_contract_tests {
    use super::*;

    const IDS: &str = "SELECT id FROM field_service_agreements UNION SELECT id FROM field_service_orders";

    #[test]
    fn retired_typescript_probes_are_native_owner_fixed_selects() {
        let scope = field_record_probe("sourceReadScopeProbe");
        let record = field_record_probe("workspaceRecordProbe");
        assert_eq!(scope, format!("SELECT 1 FROM ({IDS}) native_field_record WHERE native_field_record.id=workspace_records.id"));
        assert_eq!(record, format!("SELECT 1 FROM ({IDS}) native_field_record WHERE id=?"));
        for sql in [scope, record] {
            assert!(sql.starts_with("SELECT 1 FROM ("));
            assert!(sql.contains("native_field_record"));
            assert!(!sql.contains(';'));
            assert!(!sql.to_ascii_uppercase().contains("FOR UPDATE"));
            assert!(!sql.to_ascii_uppercase().contains("FOR SHARE"));
        }
        assert_eq!(record.matches('?').count(), 1, "single-record guard binds exactly one id");
    }

    #[test]
    fn probe_kinds_are_closed_and_never_accept_caller_sql() {
        assert!(std::panic::catch_unwind(|| field_record_probe("SELECT 1")).is_err());
        assert!(std::panic::catch_unwind(|| field_record_probe("workspace_records.id=?")).is_err());
    }
}
