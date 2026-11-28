//! Native-owned Legal register/detail read contract.
//!
//! The list search clause, total/state counts, hold and link reads plus the
//! hold-preservable guard were the last TypeScript statement tokens in
//! legal/service.ts. The exact templates now live in this owner's JSON contract,
//! bound into Rust with `include_str!` and imported by the TypeScript adapter.
//! Callers supply only the owner-built visibility, hold-scope, link-scope and
//! purchase fragments plus the contracted hold-own-target fragment.
use serde::Deserialize;
use std::sync::OnceLock;

/// Native-owned service read contract.
pub const LEGAL_SERVICE_SOURCES_JSON: &str = include_str!("legal-service-sources.json");

#[derive(Clone, Debug, Deserialize)]
pub struct LegalServiceSources {
    #[serde(rename = "listClause")]
    pub list_clause: String,
    pub total: String,
    #[serde(rename = "stateCounts")]
    pub state_counts: String,
    pub holds: String,
    pub links: String,
    #[serde(rename = "holdPreservable")]
    pub hold_preservable: String,
}

static SOURCES: OnceLock<LegalServiceSources> = OnceLock::new();

pub fn legal_service_sources() -> &'static LegalServiceSources {
    SOURCES.get_or_init(|| {
        serde_json::from_str(LEGAL_SERVICE_SOURCES_JSON)
            .expect("legal service SQL contract must be valid JSON")
    })
}

/// Render the list search clause for the owner visibility predicate.
pub fn render_legal_list_clause(visibility: &str) -> String {
    legal_service_sources()
        .list_clause
        .replace("{visibility}", visibility)
}

/// Render the filtered total count.
pub fn render_legal_total(clause: &str, purchase: &str) -> String {
    legal_service_sources()
        .total
        .replace("{clause}", clause)
        .replace("{purchase}", purchase)
}

/// Render the state counts read.
pub fn render_legal_state_counts(visibility: &str, purchase: &str) -> String {
    legal_service_sources()
        .state_counts
        .replace("{visibility}", visibility)
        .replace("{purchase}", purchase)
}

/// Render the item holds read.
pub fn render_legal_holds(hold_scope: &str) -> String {
    legal_service_sources()
        .holds
        .replace("{holdScope}", hold_scope)
}

/// Render the item links read.
pub fn render_legal_links(link_scope: &str) -> String {
    legal_service_sources()
        .links
        .replace("{linkScope}", link_scope)
}

/// Render the hold-preservable guard with the contracted hold-own target.
pub fn render_legal_hold_preservable(hold_own_target: &str) -> String {
    legal_service_sources()
        .hold_preservable
        .replace("{holdOwnTarget}", hold_own_target)
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
            digest(LEGAL_SERVICE_SOURCES_JSON),
            "88908af744d8fe2de5d815f8e92e49e22fd2179cb30c82e5500346983b187405"
        );
        let clause = render_legal_list_clause("i.company_id=?");
        assert!(clause.starts_with("i.company_id=? AND (?::text IS NULL OR i.kind=?)"));
        assert!(clause.ends_with("(?='false' OR i.owner_id=?)"));
        assert_eq!(clause.matches('?').count(), 11);
        assert_eq!(
            render_legal_total("c=?", "p=?"),
            "SELECT count(*)::int n FROM legal_items i WHERE c=? AND p=?"
        );
        assert_eq!(
            render_legal_state_counts("v=?", "p=?"),
            "SELECT i.state,count(*)::int count FROM legal_items i WHERE v=? AND p=? GROUP BY i.state"
        );
        let holds = render_legal_holds("hs=?");
        assert_eq!(
            holds,
            "SELECT h.id,CASE WHEN hs=? THEN h.reason ELSE 'Protected by a related legal hold' END reason,h.state,(h.item_id=?) direct,(SELECT n.acknowledged_at FROM legal_hold_notices n WHERE n.hold_id=h.id AND n.item_id=t.item_id AND n.user_id=?) acknowledged_at FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id JOIN legal_items source ON source.id=h.item_id WHERE t.item_id=? AND h.state!='released'"
        );
        assert_eq!(holds.matches('?').count(), 4);
        assert_eq!(
            render_legal_links("ls=?"),
            "SELECT l.target_id,l.relation,t.title FROM legal_links l JOIN legal_items t ON t.id=l.target_id WHERE l.source_id=? AND ls=?"
        );
        assert_eq!(
            render_legal_hold_preservable("own=?"),
            "NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id JOIN legal_destructions d ON d.item_id=t.item_id WHERE h.state!='released' AND d.state IN ('executing','completed') AND (h.item_id=? OR own=?))"
        );
        for sql in [
            clause.as_str(),
            &render_legal_total("c", "p"),
            &render_legal_state_counts("v", "p"),
            holds.as_str(),
            &render_legal_links("ls"),
            &render_legal_hold_preservable("own"),
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
                assert!(!sql.contains(write), "{write}");
            }
        }
    }

    #[test]
    fn render_preserves_trusted_fragments_and_leaves_no_placeholders() {
        let clause = render_legal_list_clause("v.$&");
        assert!(clause.starts_with("v.$& AND"));
        assert!(!clause.contains("{visibility}"));
        let total = render_legal_total("c.$1", "p.$2");
        assert!(total.contains("c.$1"));
        assert!(total.contains("p.$2"));
        assert!(!total.contains("{clause}"));
        assert!(!total.contains("{purchase}"));
        let counts = render_legal_state_counts("v.$&", "p.$1");
        assert!(counts.contains("v.$&"));
        assert!(counts.contains("p.$1"));
        assert!(!counts.contains("{visibility}"));
        assert!(!counts.contains("{purchase}"));
        let holds = render_legal_holds("hs.$&");
        assert!(holds.contains("hs.$&"));
        assert!(!holds.contains("{holdScope}"));
        let links = render_legal_links("ls.$1");
        assert!(links.contains("ls.$1"));
        assert!(!links.contains("{linkScope}"));
        let guard = render_legal_hold_preservable("own.$2");
        assert!(guard.contains("own.$2"));
        assert!(!guard.contains("{holdOwnTarget}"));
    }
}
