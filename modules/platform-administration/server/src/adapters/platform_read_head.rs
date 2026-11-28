//! Native-owned static read/probe head contract for Platform Administration.
//!
//! The one-row probe, the three parameterized release row renderers, the
//! records usage projection head, the workforce scope probe head and the
//! database identity read head were the remaining static TypeScript statement
//! tokens outside the release locks and the pg_catalog engineering fence. Their
//! exact templates now live in this owner's JSON contract, bound into Rust with
//! `include_str!` and imported by the TypeScript adapters.
//!
//! Every TypeScript call site still prepares and executes its own statement in
//! its own transaction with its own alias, column/WHERE tail and binds. Row
//! locks, advisory locks, `LOCK TABLE`, the engineering identity fence and the
//! native routes are unchanged.

/// Native-owned static read/probe head contract.
pub const PLATFORM_READ_HEAD_SOURCES_JSON: &str =
    include_str!("platform-read-head-sources.json");

#[derive(serde::Deserialize)]
pub struct PlatformReadHeads {
    #[serde(rename = "selectOne")]
    pub select_one: String,
    #[serde(rename = "selectOneFromWhere")]
    pub select_one_from_where: String,
    #[serde(rename = "selectProjectionFrom")]
    pub select_projection_from: String,
    #[serde(rename = "selectRowByPredicate")]
    pub select_row_by_predicate: String,
    #[serde(rename = "recordsUsageProjection")]
    pub records_usage_projection: String,
    #[serde(rename = "workforceScopeProbe")]
    pub workforce_scope_probe: String,
    #[serde(rename = "databaseIdentityRead")]
    pub database_identity_read: String,
}

static HEADS: std::sync::OnceLock<PlatformReadHeads> = std::sync::OnceLock::new();

/// The frozen static head contract, parsed once.
pub fn platform_read_heads() -> &'static PlatformReadHeads {
    HEADS.get_or_init(|| {
        serde_json::from_str(PLATFORM_READ_HEAD_SOURCES_JSON)
            .expect("platform administration read head SQL contract must be valid JSON")
    })
}

/// Frozen one-row probe base rendered by the native owner-write no-op.
pub fn select_one() -> &'static str {
    &platform_read_heads().select_one
}

/// Frozen `SELECT 1 FROM … WHERE …` head template.
pub fn select_one_from_where() -> &'static str {
    &platform_read_heads().select_one_from_where
}

/// Frozen parameterized projection-FROM head template.
pub fn projection_from_head() -> &'static str {
    &platform_read_heads().select_projection_from
}

/// Frozen parameterized row head template.
pub fn row_by_predicate() -> &'static str {
    &platform_read_heads().select_row_by_predicate
}

/// Frozen records usage projection head.
pub fn records_usage_projection() -> &'static str {
    &platform_read_heads().records_usage_projection
}

/// Frozen workforce current-scope probe head.
pub fn workforce_scope_probe() -> &'static str {
    &platform_read_heads().workforce_scope_probe
}

/// Frozen database identity read head.
pub fn database_identity_read() -> &'static str {
    &platform_read_heads().database_identity_read
}

/// Render the exact `SELECT 1 FROM … WHERE …` statement the retired TypeScript
/// head produced, from the owner contract.
pub fn render_select_one_from_where(from: &str, predicate: &str, gap: &str) -> String {
    select_one_from_where()
        .replace("{from}", from)
        .replace("{gap}", gap)
        .replace("{where}", predicate)
}

/// Render the exact projection-FROM statement the retired TypeScript head
/// produced, from the owner contract.
pub fn render_projection_from_head(
    columns: &str,
    projection: &str,
    alias: &str,
    predicate: &str,
) -> String {
    projection_from_head()
        .replace("{columns}", columns)
        .replace("{projection}", projection)
        .replace("{alias}", alias)
        .replace("{predicate}", predicate)
}

/// Render the exact row statement the retired TypeScript head produced, from
/// the owner contract.
pub fn render_row_head(columns: &str, predicate: &str, suffix: &str, table: &str) -> String {
    row_by_predicate()
        .replace("{columns}", columns)
        .replace("{predicate}", predicate)
        .replace("{suffix}", suffix)
        .replace("{table}", table)
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
        assert_eq!(
            sha(PLATFORM_READ_HEAD_SOURCES_JSON),
            "f87d53d53108140606ff23ed019c3b6944a1213d89a35a5956422f03ba7010aa"
        );
        for (sql, hash, len, selects) in [
            (
                select_one(),
                "e004ebd5b5532a4b85984a62f8ad48a81aa3460c1ca07701f386135d72cdecf5",
                8,
                1,
            ),
            (
                select_one_from_where(),
                "372253572886ee1d8f4c98e307586c6f0ff1b656478d0e0136ca66a8b6633dda",
                38,
                1,
            ),
            (
                projection_from_head(),
                "935be03b591ee41a5b55d7b4d155cca29c5a1a5c7e3fc4cca3ea186146b4920c",
                62,
                1,
            ),
            (
                row_by_predicate(),
                "fc7e3d52194e3323a96424f91b4f515f25e8f1259ee05e1c82a919ad23e4233f",
                55,
                1,
            ),
            (
                records_usage_projection(),
                "460cf931cc51787bfe4780401f61da23ff511ef5edd7572c15f21c07a595ac0c",
                103,
                1,
            ),
            (
                workforce_scope_probe(),
                "5ae735fab867f20e27d643e5768ee82ffc9511a7c70a17c3fd66b15974c69091",
                108,
                2,
            ),
            (
                database_identity_read(),
                "7b1797198a8e5b542023ea24884e1eaa211ccf96153c72d663151d9a42e53cde",
                50,
                1,
            ),
        ] {
            assert_eq!(sha(sql), hash);
            assert_eq!(sql.len(), len);
            assert_eq!(sql.matches("SELECT ").count(), selects);
            assert!(!sql.contains("INSERT INTO"));
            assert!(!sql.contains("DELETE FROM"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
            assert!(!sql.contains("FOR SHARE"));
            assert!(!sql.contains("LOCK TABLE"));
            assert!(!sql.contains("pg_advisory"));
        }
        assert_eq!(
            render_select_one_from_where("t", "x=1", " "),
            "SELECT 1 FROM t WHERE x=1"
        );
        assert_eq!(
            render_select_one_from_where("t", "x=1", "\n  "),
            "SELECT 1 FROM t\n  WHERE x=1"
        );
        assert_eq!(
            render_projection_from_head("columns", "SRC", "c", "c.x=1"),
            "SELECT columns FROM (SRC) c WHERE c.x=1"
        );
        assert_eq!(
            render_row_head("a,b", "id=?", " /*probe*/", "t"),
            "SELECT a,b FROM t WHERE id=? /*probe*/"
        );
    }

    #[test]
    fn render_replaces_every_placeholder_and_keeps_dollar_fragments() {
        let one = render_select_one_from_where("t.$&", "x.$1", " $2");
        assert_eq!(one, "SELECT 1 FROM t.$& $2WHERE x.$1");
        assert!(!one.contains('{'));
        let projection = render_projection_from_head("c.$&", "S.$1", "a.$2", "p.$3");
        assert_eq!(projection, "SELECT c.$& FROM (S.$1) a.$2 WHERE p.$3");
        assert!(!projection.contains('{'));
        let row = render_row_head("c.$&", "p.$1", " s.$2", "t.$3");
        assert_eq!(row, "SELECT c.$& FROM t.$3 WHERE p.$1 s.$2");
        assert!(!row.contains('{'));
        assert!(!select_one().contains('{'));
        assert!(select_one_from_where().contains("{from}"));
        assert!(projection_from_head().contains("{projection}"));
        assert!(row_by_predicate().contains("{predicate}"));
        assert!(!records_usage_projection().contains('{'));
        assert!(!workforce_scope_probe().contains('{'));
        assert!(!database_identity_read().contains('{'));
    }

    #[test]
    fn typescript_call_sites_carry_no_moved_statement_token() {
        let helper = include_str!("platform-read-heads.ts");
        assert!(!helper.contains("SELECT "));
        assert!(!helper.contains("INSERT "));
        assert!(!helper.contains("DELETE "));
        assert!(!helper.contains("UPDATE "));

        let owner_write = include_str!("native-owner-write.ts");
        assert!(owner_write.contains("platformReadHeads.selectOne"));
        assert!(!owner_write.contains("SELECT 1"));

        let records = include_str!("records-ai-usage-projection.ts");
        assert!(records.contains("platformReadHeads.recordsUsageProjection"));
        assert!(!records.contains("SELECT company_id"));

        let attachment = include_str!("company-release-attachment-read.ts");
        assert!(attachment.contains("platformReadHeads.workforceScopeProbe"));
        assert!(!attachment.contains("SELECT 1 FROM(SELECT ?::text"));

        let engineering = include_str!("engineering-verification.ts");
        assert!(engineering.contains("platformReadHeads.databaseIdentityRead"));
        assert!(!engineering.contains("SELECT current_database() name"));
        assert!(engineering.contains("/*erpx-native-engineering-fence*/"));
        assert!(engineering.contains("pg_catalog.pg_database"));

        let predicate = include_str!("release-predicate.ts");
        assert!(predicate.contains("platformReadHeads.selectOneFromWhere"));
        assert!(predicate.contains("platformReadHeads.selectProjectionFrom"));
        assert!(predicate.contains("platformReadHeads.selectRowByPredicate"));
        assert!(!predicate.contains("`SELECT 1 FROM ${from}${gap}WHERE ${where}`"));
        assert!(!predicate.contains("`SELECT ${columns} FROM (${projection}) ${alias} WHERE ${predicate}`"));
        assert!(!predicate.contains("`SELECT ${columns} FROM ${table} WHERE ${predicate}${suffix}`"));
        assert_eq!(predicate.matches("SELECT ").count(), 4);
    }

    #[test]
    fn native_port_reexports_the_frozen_heads() {
        let port = include_str!("../public/platform_read_head.rs");
        assert!(port.contains("pub use crate::adapters::platform_read_head::"));
        for symbol in [
            "PLATFORM_READ_HEAD_SOURCES_JSON",
            "PlatformReadHeads",
            "platform_read_heads",
            "select_one",
            "select_one_from_where",
            "projection_from_head",
            "row_by_predicate",
            "records_usage_projection",
            "workforce_scope_probe",
            "database_identity_read",
            "render_select_one_from_where",
            "render_projection_from_head",
            "render_row_head",
        ] {
            assert!(port.contains(symbol), "{symbol}");
        }
    }
}
