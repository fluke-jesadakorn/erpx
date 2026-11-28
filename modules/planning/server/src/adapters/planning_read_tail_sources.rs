//! Native-owned Planning attachment-lineage read-tail contract.
//!
//! The parameterized source-row, ordered JSONB aggregate and single JSON row
//! heads were the r32 TypeScript statement tokens of the attachment lineage
//! read family. The exact templates now live in this owner's JSON contract,
//! bound into Rust with `include_str!` and imported by the TypeScript adapters.
//! Callers supply only fixed owner-controlled projections and predicates; no
//! bind value or request text is interpolated.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned r32 attachment-lineage contract.
pub const PLANNING_READ_TAIL_SOURCES_JSON: &str =
    include_str!("planning-read-tail-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct PlanningReadTailSources {
    #[serde(rename = "sourceRows")]
    pub source_rows: String,
    #[serde(rename = "jsonbAggOrdered")]
    pub jsonb_agg_ordered: String,
    #[serde(rename = "jsonbRowByKey")]
    pub jsonb_row_by_key: String,
}

static SOURCES: OnceLock<PlanningReadTailSources> = OnceLock::new();

pub fn planning_read_tail_sources() -> &'static PlanningReadTailSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(PLANNING_READ_TAIL_SOURCES_JSON)
            .expect("planning read-tail sources contract must be valid JSON")
    })
}

fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut rendered = template.to_owned();
    for (name, value) in values {
        let token = format!("{{{name}}}");
        assert!(
            rendered.contains(&token),
            "planning read-tail contract lost {token}"
        );
        rendered = rendered.replace(&token, value);
    }
    rendered
}

/// Render the source-row head for one fixed projection, alias and FROM frame.
pub fn render_source_rows(
    columns: &str,
    lead: &str,
    fragment: &str,
    alias: &str,
    tail: &str,
    from: &str,
) -> String {
    fill(
        &planning_read_tail_sources().source_rows,
        &[
            ("columns", columns),
            ("lead", lead),
            ("fragment", fragment),
            ("alias", alias),
            ("tail", tail),
            ("from", from),
        ],
    )
}

/// Render the ordered JSONB aggregate subquery for one fixed projection.
pub fn render_jsonb_agg_ordered(
    projection: &str,
    order: &str,
    from: &str,
    predicate: &str,
) -> String {
    fill(
        &planning_read_tail_sources().jsonb_agg_ordered,
        &[
            ("projection", projection),
            ("order", order),
            ("from", from),
            ("predicate", predicate),
        ],
    )
}

/// Render the single JSON row subquery for one fixed table, alias and predicate.
pub fn render_jsonb_row_by_key(table: &str, alias: &str, predicate: &str) -> String {
    fill(
        &planning_read_tail_sources().jsonb_row_by_key,
        &[("table", table), ("alias", alias), ("predicate", predicate)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_is_byte_exact_and_read_only() {
        assert_eq!(PLANNING_READ_TAIL_SOURCES_JSON.len(), 261);
        assert_eq!(
            planning_read_tail_sources().source_rows,
            "SELECT {columns}{lead}{fragment} {alias}{tail}{from}"
        );
        assert_eq!(
            planning_read_tail_sources().jsonb_agg_ordered,
            "(SELECT jsonb_agg({projection} ORDER BY {order}){from} WHERE {predicate})"
        );
        assert_eq!(
            planning_read_tail_sources().jsonb_row_by_key,
            "(SELECT to_jsonb({alias}) FROM {table} {alias} WHERE {predicate})"
        );
        for template in [
            &planning_read_tail_sources().source_rows,
            &planning_read_tail_sources().jsonb_agg_ordered,
            &planning_read_tail_sources().jsonb_row_by_key,
        ] {
            for write in [
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE ",
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
            ] {
                assert!(!template.contains(write), "{write}");
            }
        }
    }

    #[test]
    fn renders_the_retired_heads_byte_for_byte() {
        assert_eq!(
            render_source_rows("a,b", ",\n ", "img", "image", "\n FROM ", "src"),
            "SELECT a,b,\n img image\n FROM src"
        );
        assert_eq!(
            render_jsonb_row_by_key(
                "planning_recommendations",
                "recommendation",
                "recommendation.scenario_id=scenario.id"
            ),
            "(SELECT to_jsonb(recommendation) FROM planning_recommendations recommendation WHERE recommendation.scenario_id=scenario.id)"
        );
        assert_eq!(
            render_jsonb_agg_ordered(
                "to_jsonb(calendar)",
                "calendar.work_date",
                " FROM production_resource_calendar calendar",
                "calendar.resource_id=resource.id"
            ),
            "(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)"
        );
        assert_eq!(
            render_source_rows("id", ",", "f", "witness", " FROM ", "production_resources"),
            "SELECT id,f witness FROM production_resources"
        );
        assert_eq!(
            render_source_rows("id", ",", "f", "witness", " FROM ", "production_resources")
                .matches('?')
                .count(),
            0
        );
    }

    #[test]
    fn renders_leave_no_placeholder_tokens() {
        let rows = render_source_rows("c", ",", "f", "a", " FROM ", "t");
        let agg = render_jsonb_agg_ordered("p", "o", " FROM t", "w");
        let row = render_jsonb_row_by_key("t", "a", "w");
        for sql in [rows, agg, row] {
            assert!(!sql.contains('{'), "{sql}");
            assert!(!sql.contains('}'), "{sql}");
        }
    }
}
