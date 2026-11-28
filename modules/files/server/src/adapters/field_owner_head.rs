//! Native-owned Field owner marker heads for the Files read tail.
//!
//! The three TypeScript heads were the shared read-only witnesses for the Field
//! upload ancestor, the Field upload-intent set marker and the commerce
//! attachment-id guard. Their exact bytes now live in this owner's JSON
//! contract, bound into Rust with `include_str!` and read by the TypeScript
//! adapter through its native contract port. Each call site keeps its own prefix,
//! owner/WHERE tail, binds and transaction; no statement token remains in TS.

/// Native-owned SQL contract for the three Field owner marker heads.
pub const FIELD_OWNER_HEAD_SOURCES_JSON: &str =
    include_str!("field-owner-head-sources.json");

#[derive(serde::Deserialize)]
struct FieldOwnerSources {
    #[serde(rename = "fieldUploadAncestor")]
    field_upload_ancestor: String,
    #[serde(rename = "fieldSetUpload")]
    field_set_upload: String,
    #[serde(rename = "attachmentIdAnd")]
    attachment_id_and: String,
}

static SOURCES: std::sync::OnceLock<FieldOwnerSources> = std::sync::OnceLock::new();

fn sources() -> &'static FieldOwnerSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(FIELD_OWNER_HEAD_SOURCES_JSON)
            .expect("files Field owner head SQL contract must be valid JSON")
    })
}

/// Frozen Field upload-ancestor owner witness.
pub fn field_upload_ancestor() -> &'static str {
    &sources().field_upload_ancestor
}

/// Frozen Field upload-intent set-marker head.
pub fn field_set_upload() -> &'static str {
    &sources().field_set_upload
}

/// Frozen attachment-id / AND guard head.
pub fn attachment_id_and() -> &'static str {
    &sources().attachment_id_and
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, hash, len, expected, selects) in [
            (
                field_upload_ancestor(),
                "0ae8d0da4cbc376c4c67dcc009a0630bcea15d9c12531039c2b10d69b6cbe81f",
                102,
                "EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id))",
                1,
            ),
            (
                field_set_upload(),
                "b6fb6dc590d9a2654782fc3f69f76848a491da0f314ce27e29aa2e9d608a534e",
                76,
                "OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND ",
                1,
            ),
            (
                attachment_id_and(),
                "3d18048b4a0975e642531d845a0301c5a531574430a9231de600435e8bc3f220",
                36,
                "FROM attachments a WHERE a.id=? AND ",
                0,
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql, expected);
            assert_eq!(sql.matches("SELECT ").count(), selects);
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
        }
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        let helper = include_str!("read-tail-r43-source-sql.ts");
        for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
            assert!(!helper.contains(token));
        }
        assert!(helper.contains("field-owner-head-sources.json"));
        assert!(!helper.contains("prepare("));
        assert!(!helper.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_the_three_frozen_heads() {
        let port = include_str!("../public/field_owner_head.rs");
        assert!(port.contains("pub use crate::adapters::field_owner_head::"));
        for symbol in ["field_upload_ancestor", "field_set_upload", "attachment_id_and"] {
            assert!(port.contains(symbol));
        }
    }
}
