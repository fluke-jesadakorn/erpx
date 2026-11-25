//! Native-owned Legal read-tail r44 contract.
//!
//! The two parameterized read heads (`legalExternalUploadShareJoinHead` and
//! `legalFilePreservationHoldTargetHead`) were the last TypeScript statement
//! tokens in the external-upload / preservation family. The exact templates now
//! live in this owner's JSON contract, bound into Rust with `include_str!` and
//! imported by the TypeScript adapter. Callers supply only trusted projections,
//! constant aliases and the fixed state inequality token; no table, alias or
//! predicate comes from a request.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r44 read-head contract.
pub const LEGAL_READ_TAIL_R44_SOURCES_JSON: &str =
    include_str!("legal-read-tail-r44-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR44Sources {
    #[serde(rename = "externalUploadShare")]
    pub external_upload_share: String,
    #[serde(rename = "filePreservationHoldTarget")]
    pub file_preservation_hold_target: String,
}

static SOURCES: OnceLock<LegalReadTailR44Sources> = OnceLock::new();

pub fn legal_read_tail_r44_sources() -> &'static LegalReadTailR44Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R44_SOURCES_JSON)
            .expect("legal read-tail r44 SQL contract must be valid JSON")
    })
}

/// Render the external-upload/share join head. The trailing space is part of
/// the contract because every caller concatenates its own trusted tail.
pub fn render_external_upload_share(
    projection: &str,
    upload_alias: &str,
    share_alias: &str,
) -> String {
    legal_read_tail_r44_sources()
        .external_upload_share
        .replace("{projection}", projection)
        .replace("{uploadAlias}", upload_alias)
        .replace("{shareAlias}", share_alias)
}

/// Render the preservation/hold-target read head with the caller's fixed state
/// inequality token.
pub fn render_file_preservation_hold_target(
    projection: &str,
    target_alias: &str,
    hold_alias: &str,
    inequality: &str,
) -> String {
    legal_read_tail_r44_sources()
        .file_preservation_hold_target
        .replace("{projection}", projection)
        .replace("{targetAlias}", target_alias)
        .replace("{holdAlias}", hold_alias)
        .replace("{inequality}", inequality)
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
            digest(LEGAL_READ_TAIL_R44_SOURCES_JSON),
            "755bd9304bff30bffd2aade73bbb5ce8abc0e393b21b2af100adf2be357da75b"
        );
        assert_eq!(
            render_external_upload_share("u.*,s.item_id", "u", "s"),
            "SELECT u.*,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id "
        );
        assert_eq!(
            render_file_preservation_hold_target("1", "target", "hold", "<>"),
            "SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets target ON target.item_id=p.item_id JOIN legal_holds hold ON hold.id=target.hold_id WHERE hold.state<>'released'"
        );
        let sources = legal_read_tail_r44_sources();
        for template in [
            &sources.external_upload_share,
            &sources.file_preservation_hold_target,
        ] {
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
    fn render_keeps_both_inequality_tokens_and_literal_dollar_text() {
        assert_eq!(
            render_file_preservation_hold_target("1", "t", "h", "!="),
            "SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE h.state!='released'"
        );
        assert_eq!(
            render_external_upload_share("u.$&,s.item_id", "u", "s"),
            "SELECT u.$&,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id "
        );
        let ext = render_external_upload_share("u.*", "u", "s");
        assert!(!ext.contains("{projection}"));
        assert!(!ext.contains("{uploadAlias}"));
        assert!(!ext.contains("{shareAlias}"));
        let pres = render_file_preservation_hold_target("1", "t", "h", "<>");
        assert!(!pres.contains("{projection}"));
        assert!(!pres.contains("{targetAlias}"));
        assert!(!pres.contains("{holdAlias}"));
        assert!(!pres.contains("{inequality}"));
    }
}
