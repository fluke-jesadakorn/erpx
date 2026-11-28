//! Native-owned universal projection-FROM head for the Assets read tail.
//!
//! The last TypeScript `SELECT` literal in the owner rendered every delegated
//! read: the projection head and its FROM fragment. The exact templates now
//! live in this owner's JSON contract, bound into Rust with `include_str!` and
//! read by the TypeScript adapter through its native contract port. No
//! statement token remains in TypeScript and no caller supplies SQL: the
//! renderer only substitutes owner-controlled projection/FROM/tail fragments
//! into the frozen templates.

/// Native-owned SQL head contract for delegated Assets projections.
pub const ASSET_PROJECTION_HEAD_SOURCES_JSON: &str =
    include_str!("asset-projection-head-sources.json");

#[derive(serde::Deserialize)]
struct ProjectionHeadSources {
    #[serde(rename = "selectHead")]
    select_head: String,
    #[serde(rename = "fromHead")]
    from_head: String,
}

static HEADS: std::sync::OnceLock<ProjectionHeadSources> = std::sync::OnceLock::new();

fn heads() -> &'static ProjectionHeadSources {
    HEADS.get_or_init(|| {
        serde_json::from_str(ASSET_PROJECTION_HEAD_SOURCES_JSON)
            .expect("assets projection head SQL contract must be valid JSON")
    })
}

/// Frozen universal head template. `{projection}` and `{tail}` are the only
/// substitution points; callers never pass a statement or relation name.
pub fn asset_projection_head() -> &'static str {
    &heads().select_head
}

/// Frozen FROM-fragment template used by the projection-FROM renderer.
pub fn asset_from_head() -> &'static str {
    &heads().from_head
}

/// Render the exact statement the retired TypeScript head produced, from the
/// owner contract. Used by Rust oracles; TS consumers substitute the same
/// closed fragments on their own already-open unit.
pub fn render_projection_from(projection: &str, from: &str, tail: &str) -> String {
    let from_tail = asset_from_head()
        .replace("{from}", from)
        .replace("{tail}", tail);
    asset_projection_head()
        .replace("{projection}", projection)
        .replace("{tail}", &from_tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_HEAD: &str = include_str!("asset-scope-tuple.ts");

    #[test]
    fn head_contract_is_byte_exact_to_the_retired_typescript_literal() {
        assert_eq!(asset_projection_head(), "SELECT {projection}{tail}");
        assert_eq!(asset_from_head(), " FROM{from}{tail}");
        assert_eq!(
            render_projection_from("id,version", " operational_assets", " WHERE id=?"),
            "SELECT id,version FROM operational_assets WHERE id=?"
        );
        assert_eq!(render_projection_from("*", " operational_assets", ""), "SELECT * FROM operational_assets");
        // The scope tuple renders from the head alone, without a FROM fragment.
        let tuple = asset_projection_head()
            .replace("{projection}", "?::text company_id,?::text branch_id,?::text department_id")
            .replace("{tail}", "");
        assert_eq!(tuple, "SELECT ?::text company_id,?::text branch_id,?::text department_id");
    }

    #[test]
    fn typescript_head_carries_no_statement_token() {
        assert!(!TS_HEAD.contains("SELECT"));
        assert!(!TS_HEAD.contains("DELETE"));
        assert!(!TS_HEAD.contains("INSERT"));
        assert!(!TS_HEAD.contains("UPDATE "));
        assert!(TS_HEAD.contains("heads.selectHead"));
        assert!(TS_HEAD.contains("heads.fromHead"));
        assert!(TS_HEAD.contains("export const assetSelectProjectionSql"));
        assert!(TS_HEAD.contains("export const assetProjectionFromSql"));
    }

    #[test]
    fn renderer_accepts_only_the_two_closed_substitution_points() {
        let rendered = render_projection_from("1", " operational_assets", " WHERE company_id=?");
        assert!(rendered.starts_with("SELECT 1 FROM "));
        assert!(rendered.contains(" WHERE company_id=?"));
        assert!(!rendered.contains('{'));
        assert!(!rendered.contains('}'));
        assert!(!rendered.contains(';'));
    }
}
