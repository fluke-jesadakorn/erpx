//! Native-owned head for the Brand private-attachment current-scope probe.
//!
//! The last TypeScript `SELECT` literal in the owner wrapped the Identity
//! Access-owned current-assignment predicate in this fixed `VALUES` scope
//! envelope. The exact head now lives in this owner's JSON contract, bound into
//! Rust with `include_str!` and exposed by `public::attachment_scope_head`.
//! Identity Access keeps owning the predicate and binds; the renderer only
//! appends that already-authorized `where` clause and accepts no SQL.

pub const ATTACHMENT_SCOPE_HEAD_SOURCES_JSON: &str =
    include_str!("attachment-scope-head-sources.json");

#[derive(serde::Deserialize)]
struct ScopeHeadSources {
    #[serde(rename = "scopeHead")]
    scope_head: String,
}

static HEADS: std::sync::OnceLock<ScopeHeadSources> = std::sync::OnceLock::new();

fn heads() -> &'static ScopeHeadSources {
    HEADS.get_or_init(|| {
        serde_json::from_str(ATTACHMENT_SCOPE_HEAD_SOURCES_JSON)
            .expect("brand attachment scope head SQL contract must be valid JSON")
    })
}

/// Frozen scope-probe head. `where` is the only substitution point and stays the
/// Identity Access-owned current-assignment predicate, never request SQL.
pub fn attachment_scope_head() -> &'static str {
    &heads().scope_head
}

/// Render the exact statement the retired TypeScript literal bound.
pub fn render_attachment_scope_probe(where_clause: &str) -> String {
    format!("{}{where_clause}", attachment_scope_head())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_HELPER: &str = include_str!("attachment-scope-head.ts");
    const TS_CALLER: &str = include_str!("attachment-read.ts");

    #[test]
    fn head_is_byte_exact_to_the_retired_typescript_prefix() {
        assert_eq!(
            attachment_scope_head(),
            "SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE "
        );
    }

    #[test]
    fn renderer_appends_only_the_identity_owned_predicate() {
        assert_eq!(
            render_attachment_scope_probe("scope.active"),
            "SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE scope.active"
        );
        let rendered = render_attachment_scope_probe("EXISTS(SELECT 1 FROM assignments g WHERE g.id=?)");
        assert!(rendered.starts_with(attachment_scope_head()));
        assert!(!rendered.contains('{'));
        assert!(!rendered.contains('}'));
        assert!(!rendered.contains(';'));
    }

    #[test]
    fn typescript_helper_carries_no_statement_token() {
        assert!(!TS_HELPER.contains("SELECT"));
        assert!(!TS_HELPER.contains("INSERT"));
        assert!(!TS_HELPER.contains("UPDATE "));
        assert!(!TS_HELPER.contains("DELETE"));
        assert!(TS_HELPER.contains("heads.scopeHead"));
        assert!(TS_HELPER.contains("export const brandAttachmentScopeProbeSql"));
    }

    #[test]
    fn caller_renders_the_probe_and_keeps_the_binds() {
        assert!(!TS_CALLER.contains("SELECT "));
        assert!(TS_CALLER.contains("brandAttachmentScopeProbeSql(currentScope.where)"));
        assert!(TS_CALLER.contains(
            ".bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)"
        ));
    }
}
