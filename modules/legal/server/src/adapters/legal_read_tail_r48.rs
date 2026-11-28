//! Native-owned Legal read-tail r48 contract.
//!
//! The three shared read heads (`legalCorpusSourceCleanHead`,
//! `legalMembersWhereHead`, `legalSettingsWhereHead`) were the last TypeScript
//! statement tokens in the compound corpus / members / settings witness family.
//! The exact templates now live in this owner's JSON contract, bound into Rust
//! with `include_str!` and imported by the TypeScript adapter. Only the trusted
//! trailing predicate is substituted; every table, alias, join and fixed bind
//! stays owner-controlled.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r48 read-head contract.
pub const LEGAL_READ_TAIL_R48_SOURCES_JSON: &str = include_str!("legal-read-tail-r48-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalReadTailR48Sources {
    #[serde(rename = "corpusSourceClean")]
    pub corpus_source_clean: String,
    #[serde(rename = "membersWhere")]
    pub members_where: String,
    #[serde(rename = "settingsWhere")]
    pub settings_where: String,
}

static SOURCES: OnceLock<LegalReadTailR48Sources> = OnceLock::new();

pub fn legal_read_tail_r48_sources() -> &'static LegalReadTailR48Sources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_READ_TAIL_R48_SOURCES_JSON)
            .expect("legal read-tail r48 SQL contract must be valid JSON")
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalReadTailR48Head {
    CorpusSourceClean,
    MembersWhere,
    SettingsWhere,
}

/// Render one owner head with the caller's trusted trailing predicate.
pub fn render_legal_read_tail_r48(head: LegalReadTailR48Head, predicate: &str) -> String {
    let sources = legal_read_tail_r48_sources();
    let template = match head {
        LegalReadTailR48Head::CorpusSourceClean => &sources.corpus_source_clean,
        LegalReadTailR48Head::MembersWhere => &sources.members_where,
        LegalReadTailR48Head::SettingsWhere => &sources.settings_where,
    };
    template.replace("{predicate}", predicate)
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
            digest(LEGAL_READ_TAIL_R48_SOURCES_JSON),
            "4a59be2dbe6939b392a22a0ffbefb75c464c0a4a2bdc723c1e5c7c8ee38b4303"
        );
        assert_eq!(
            render_legal_read_tail_r48(LegalReadTailR48Head::CorpusSourceClean, "X"),
            "SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND X"
        );
        assert_eq!(
            render_legal_read_tail_r48(
                LegalReadTailR48Head::MembersWhere,
                "item_id=? AND user_id=?"
            ),
            "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?"
        );
        assert_eq!(
            render_legal_read_tail_r48(
                LegalReadTailR48Head::SettingsWhere,
                "company_id=? AND enabled"
            ),
            "SELECT 1 FROM legal_settings WHERE company_id=? AND enabled"
        );
        let sources = legal_read_tail_r48_sources();
        for template in [
            &sources.corpus_source_clean,
            &sources.members_where,
            &sources.settings_where,
        ] {
            assert_eq!(template.matches("{predicate}").count(), 1);
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
    fn render_preserves_predicate_binds_and_does_not_add_owner_binds() {
        let members = render_legal_read_tail_r48(
            LegalReadTailR48Head::MembersWhere,
            "item_id=? AND user_id=? AND access='edit'",
        );
        assert_eq!(members.matches('?').count(), 2);
        assert_eq!(
            members,
            "SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? AND access='edit'"
        );
        assert_eq!(
            render_legal_read_tail_r48(
                LegalReadTailR48Head::MembersWhere,
                "item_id=$& AND user_id=?"
            ),
            "SELECT 1 FROM legal_members WHERE item_id=$& AND user_id=?"
        );
        let corpus = render_legal_read_tail_r48(
            LegalReadTailR48Head::CorpusSourceClean,
            "d.item_id=? AND i.company_id=? AND d.checksum=?",
        );
        assert_eq!(corpus.matches('?').count(), 4);
        assert!(
            corpus.contains("ON i.id=d.item_id WHERE d.id=? AND d.item_id=? AND i.company_id=?")
        );
    }
}
