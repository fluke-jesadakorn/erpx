//! Native-owned statement heads for the Maintenance read tail.
//!
//! The six reusable TypeScript renderers (`projection`, `exists`, `subquery`,
//! `alias rows`, `settlement ref` and `record image`) carried the last owner
//! statement keyword at their head. Their exact templates now live in this
//! owner's JSON contract, bound into Rust with `include_str!` and read by the
//! TypeScript renderers through the same contract file. Callers keep their own
//! projection/FROM/WHERE tails, binds, locks and transactions; no statement
//! token remains in the TypeScript read tail and no caller supplies SQL.

/// Native-owned SQL contract for the six frozen read-tail heads.
pub const READ_HEAD_SOURCES_JSON: &str = include_str!("read-head-sources.json");

#[derive(serde::Deserialize)]
struct ReadHeadSources {
    #[serde(rename = "projectionHead")]
    projection_head: String,
    #[serde(rename = "existsGuard")]
    exists_guard: String,
    #[serde(rename = "subqueryHead")]
    subquery_head: String,
    #[serde(rename = "aliasRows")]
    alias_rows: String,
    #[serde(rename = "settlementRef")]
    settlement_ref: String,
    #[serde(rename = "recordImage")]
    record_image: String,
}

static SOURCES: std::sync::OnceLock<ReadHeadSources> = std::sync::OnceLock::new();

fn sources() -> &'static ReadHeadSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(READ_HEAD_SOURCES_JSON)
            .expect("maintenance read-head SQL contract must be valid JSON")
    })
}

/// Frozen `SELECT <fields> FROM (<source>) <alias> <tail>` row head.
pub fn projection_head() -> &'static str {
    &sources().projection_head
}

/// Frozen `EXISTS(SELECT 1 FROM <source> WHERE ` witness head.
pub fn exists_guard() -> &'static str {
    &sources().exists_guard
}

/// Frozen parenthesized `(SELECT <body>)` subquery head.
pub fn subquery_head() -> &'static str {
    &sources().subquery_head
}

/// Frozen alias-qualified `SELECT <alias>.*<tail>` row head.
pub fn alias_rows() -> &'static str {
    &sources().alias_rows
}

/// Frozen `SELECT <columns> FROM <source><tail>` reference head.
pub fn settlement_ref() -> &'static str {
    &sources().settlement_ref
}

/// Frozen `SELECT jsonb_build_object(<fields>)::text image<source>` image head.
pub fn record_image() -> &'static str {
    &sources().record_image
}

fn render(template: &str, substitutions: &[(&str, &str)]) -> String {
    substitutions
        .iter()
        .fold(template.to_string(), |sql, (key, value)| {
            sql.replace(&format!("{{{key}}}"), value)
        })
}

/// Render the retired row head from the owner contract.
pub fn render_projection(fields: &str, source: &str, alias: &str, tail: &str) -> String {
    render(
        projection_head(),
        &[("fields", fields), ("source", source), ("alias", alias), ("tail", tail)],
    )
}

/// Render the retired EXISTS witness head from the owner contract.
pub fn render_exists_guard(source: &str) -> String {
    render(exists_guard(), &[("source", source)])
}

/// Render the retired subquery head from the owner contract.
pub fn render_subquery(body: &str) -> String {
    render(subquery_head(), &[("body", body)])
}

/// Render the retired alias-qualified row head from the owner contract.
pub fn render_alias_rows(alias: &str, tail: &str) -> String {
    render(alias_rows(), &[("alias", alias), ("tail", tail)])
}

/// Render the retired settlement reference head from the owner contract.
pub fn render_settlement_ref(columns: &str, source: &str, tail: &str) -> String {
    render(
        settlement_ref(),
        &[("columns", columns), ("source", source), ("tail", tail)],
    )
}

/// Render the retired record-image head from the owner contract.
pub fn render_record_image(fields: &str, source: &str) -> String {
    render(record_image(), &[("fields", fields), ("source", source)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_is_byte_exact_to_the_retired_typescript_literals() {
        for (sql, expected, len) in [
            (
                projection_head(),
                "SELECT {fields} FROM ({source}) {alias} {tail}",
                46,
            ),
            (
                exists_guard(),
                "EXISTS(SELECT 1 FROM {source} WHERE ",
                36,
            ),
            (subquery_head(), "(SELECT {body})", 15),
            (alias_rows(), "SELECT {alias}.*{tail}", 22),
            (
                settlement_ref(),
                "SELECT {columns} FROM {source}{tail}",
                36,
            ),
            (
                record_image(),
                "SELECT jsonb_build_object({fields})::text image{source}",
                55,
            ),
        ] {
            assert_eq!(sql, expected);
            assert_eq!(sql.len(), len);
            assert_eq!(sql.matches("SELECT").count(), 1);
            assert!(!sql.contains("INSERT"));
            assert!(!sql.contains("DELETE"));
            assert!(!sql.contains("UPDATE "));
            assert!(!sql.contains("FOR UPDATE"));
        }
        assert_eq!(READ_HEAD_SOURCES_JSON.matches("SELECT").count(), 6);
    }

    #[test]
    fn renderers_reproduce_the_retired_compositions() {
        assert_eq!(
            render_projection("f", "src", "a", "WHERE x=?"),
            "SELECT f FROM (src) a WHERE x=?"
        );
        assert_eq!(
            render_projection("a.id,a.code", "<src>", "a", "WHERE a.company_id=? ORDER BY a.code"),
            "SELECT a.id,a.code FROM (<src>) a WHERE a.company_id=? ORDER BY a.code"
        );
        assert_eq!(
            render_exists_guard("maintenance_work_orders w"),
            "EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE "
        );
        assert_eq!(
            render_subquery("jsonb_array_elements_text(?::jsonb)"),
            "(SELECT jsonb_array_elements_text(?::jsonb))"
        );
        assert_eq!(render_alias_rows("s", ",owner_name"), "SELECT s.*,owner_name");
        assert_eq!(
            render_settlement_ref("*", "maintenance_work_events", " WHERE work_order_id=? ORDER BY version"),
            "SELECT * FROM maintenance_work_events WHERE work_order_id=? ORDER BY version"
        );
        assert_eq!(
            render_record_image("'work',to_jsonb(work)", " FROM maintenance_work_orders work"),
            "SELECT jsonb_build_object('work',to_jsonb(work))::text image FROM maintenance_work_orders work"
        );
    }

    #[test]
    fn typescript_renderers_carry_no_statement_token() {
        for helper in [
            include_str!("projection-lookup.ts"),
            include_str!("read-tail-r30-alias-rows.ts"),
            include_str!("asset-settlement-refs.ts"),
            include_str!("record-image-sources.ts"),
            include_str!("read-head-sources.ts"),
        ] {
            for token in ["SELECT", "INSERT", "DELETE", "UPDATE "] {
                assert!(!helper.contains(token));
            }
            assert!(helper.contains("read-head-sources"));
        }
        for helper in [
            include_str!("projection-lookup.ts"),
            include_str!("read-tail-r30-alias-rows.ts"),
            include_str!("record-image-sources.ts"),
        ] {
            assert!(!helper.contains("prepare("));
            assert!(!helper.contains(".bind("));
        }
        // This helper is not a pure renderer: Assets calls its one documented
        // SHARE lock before reading the same owner relations.
        let settlement = include_str!("asset-settlement-refs.ts");
        assert_eq!(settlement.matches("prepare(").count(), 1);
        assert!(settlement.contains("LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE"));
        assert!(!settlement.contains(".bind("));
    }

    #[test]
    fn native_port_reexports_the_six_frozen_heads() {
        let port = include_str!("../public/read_head.rs");
        assert!(port.contains("pub use crate::adapters::read_head::"));
        for symbol in [
            "projection_head",
            "exists_guard",
            "subquery_head",
            "alias_rows",
            "settlement_ref",
            "record_image",
        ] {
            assert!(port.contains(symbol));
        }
    }
}
