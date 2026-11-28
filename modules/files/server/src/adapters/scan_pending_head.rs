//! Native-owned scan-pending bind witness head for the Files read tail.
//!
//! The TypeScript `scanPendingSql` helper was the last owner-local projection of
//! the upload/scan bind primary result: a pending/running/error scan attempt
//! makes a bound link pending. That exact statement token now lives in this
//! owner's JSON contract, bound into Rust with `include_str!` and read by the
//! TypeScript adapter through its native contract port. No statement token
//! remains in TypeScript and no caller supplies SQL: the renderer only
//! substitutes the owner-controlled attachment reference into the frozen
//! template. Binds, locks and the caller transaction stay unchanged.

/// Native-owned SQL head contract for the scan-pending bind witness.
pub const SCAN_PENDING_HEAD_SOURCES_JSON: &str =
    include_str!("scan-pending-head-sources.json");

#[derive(serde::Deserialize)]
struct ScanPendingSources {
    #[serde(rename = "scanPending")]
    scan_pending: String,
}

static SOURCES: std::sync::OnceLock<ScanPendingSources> = std::sync::OnceLock::new();

fn sources() -> &'static ScanPendingSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(SCAN_PENDING_HEAD_SOURCES_JSON)
            .expect("files scan-pending head SQL contract must be valid JSON")
    })
}

/// Frozen scan-pending witness template. `{attachmentRef}` is the only
/// substitution point; callers never pass a statement or relation name.
pub fn scan_pending_head() -> &'static str {
    &sources().scan_pending
}

/// Render the exact statement the retired TypeScript helper produced, from the
/// owner contract. Used by Rust oracles; TS consumers substitute the same closed
/// attachment reference on their own already-open unit.
pub fn render_scan_pending(attachment_ref: &str) -> String {
    scan_pending_head().replace("{attachmentRef}", attachment_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_HELPER: &str = include_str!("scan-pending-dedupe-sql.ts");

    const RETIRED: &str = "EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id AND scan.state IN ('pending','running','error'))";

    #[test]
    fn head_contract_is_byte_exact_to_the_retired_typescript_literal() {
        assert_eq!(
            scan_pending_head(),
            "EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id={attachmentRef}.id AND scan.state IN ('pending','running','error'))"
        );
        assert_eq!(render_scan_pending("a"), RETIRED);
        assert_eq!(render_scan_pending("source"), RETIRED.replace("a.id", "source.id"));
        assert!(scan_pending_head().contains("'pending','running','error'"));
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        assert!(!TS_HELPER.contains("SELECT"));
        assert!(!TS_HELPER.contains("INSERT"));
        assert!(!TS_HELPER.contains("DELETE"));
        assert!(!TS_HELPER.contains("UPDATE "));
        assert!(TS_HELPER.contains("import contract from"));
        assert!(TS_HELPER.contains("export const scanPendingSql"));
        assert!(!TS_HELPER.contains("prepare("));
        assert!(!TS_HELPER.contains(".bind("));
    }

    #[test]
    fn renderer_accepts_only_the_closed_attachment_reference() {
        let rendered = render_scan_pending("attachment");
        assert!(rendered.starts_with("EXISTS(SELECT 1 FROM attachment_scan_attempts scan"));
        assert!(rendered.contains("attachment.id"));
        assert!(!rendered.contains('{'));
        assert!(!rendered.contains('}'));
        assert!(!rendered.contains(';'));
    }
}
