//! Native-owned Legal canonical attachment read contract.
//!
//! The canonical source image and the current-assignment probe were the last
//! TypeScript statement tokens in canonical-attachment-read.ts. The exact
//! templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapter. Callers supply only
//! the owner-built jsonb-agg, upload-share and assignment-scope fragments.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned canonical attachment contract.
pub const LEGAL_CANONICAL_ATTACHMENT_SOURCES_JSON: &str =
    include_str!("legal-canonical-attachment-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalCanonicalAttachmentSources {
    pub image: String,
    #[serde(rename = "currentScope")]
    pub current_scope: String,
}

static SOURCES: OnceLock<LegalCanonicalAttachmentSources> = OnceLock::new();

pub fn legal_canonical_attachment_sources() -> &'static LegalCanonicalAttachmentSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_CANONICAL_ATTACHMENT_SOURCES_JSON)
            .expect("legal canonical attachment SQL contract must be valid JSON")
    })
}

/// Render the canonical source image with the owner-built fragments.
pub fn render_legal_canonical_image(holds: &str, destructions: &str, uploads: &str) -> String {
    legal_canonical_attachment_sources()
        .image
        .replace("{holds}", holds)
        .replace("{destructions}", destructions)
        .replace("{uploads}", uploads)
}

/// Render the current-assignment probe.
pub fn render_legal_canonical_current_scope(actor_scope: &str, where_clause: &str) -> String {
    legal_canonical_attachment_sources()
        .current_scope
        .replace("{actorScope}", actor_scope)
        .replace("{where}", where_clause)
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
            digest(LEGAL_CANONICAL_ATTACHMENT_SOURCES_JSON),
            "0663245d9f91c41f44fb8d68a9c13480a3e8ad535713785428ee848ebf467e3c"
        );
        let image = render_legal_canonical_image(
            "holds_fragment",
            "destructions_fragment",
            "uploads_fragment",
        );
        assert_eq!(
            image,
            "jsonb_build_object('owner',?::jsonb,'settings',(SELECT to_jsonb(s) FROM legal_settings s WHERE s.company_id=i.company_id),'holds',holds_fragment,'disposals',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.document_id) FROM legal_disposals d JOIN legal_documents doc ON doc.id=d.document_id WHERE doc.item_id=i.id),'destructions',destructions_fragment,'uploads',(uploads_fragmentWHERE share.item_id=i.id))::text image"
        );
        assert_eq!(image.matches('?').count(), 1);
        assert_eq!(
            render_legal_canonical_current_scope("actor_scope", "scope=?"),
            "SELECT 1 FROM(actor_scope)workforce_actor_scope WHERE scope=?"
        );
        for sql in [image, render_legal_canonical_current_scope("a", "w")] {
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
    fn render_preserves_trusted_fragments_and_leaves_no_placeholders() {
        let image = render_legal_canonical_image("h.$&", "d.$1", "u.$2");
        assert!(image.contains("'holds',h.$&"));
        assert!(image.contains("'destructions',d.$1"));
        assert!(image.contains("(u.$2WHERE share.item_id=i.id)"));
        for token in ["{holds}", "{destructions}", "{uploads}"] {
            assert!(!image.contains(token), "{token}");
        }
        let scope = render_legal_canonical_current_scope("a.$&", "w.$1");
        assert!(scope.contains("FROM(a.$&)workforce_actor_scope"));
        assert!(scope.contains("w.$1"));
        assert!(!scope.contains("{actorScope}"));
        assert!(!scope.contains("{where}"));
    }
}
