//! Pure response types, validation and invariants for the native lane 33
//! holding-binding read family. No SQL and no transport live here.
//!
//! The TypeScript authority was the three `SELECT ... FROM holding_bindings`
//! statements in `adapters/runtime.ts`. This module owns the bounded response
//! shape and the resource-slot scope invariant; `postgres.rs` owns the query.

use serde::{Deserialize, Serialize};

/// The closed set of read shapes the native router accepts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingQuery {
    BySlug,
    Default,
    Active,
}

impl BindingQuery {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "by-slug" => Some(Self::BySlug),
            "default" => Some(Self::Default),
            "active" => Some(Self::Active),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BySlug => "by-slug",
            Self::Default => "default",
            Self::Active => "active",
        }
    }
}

/// The exact projection the retired TS statements returned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HoldingBinding {
    pub holding_id: String,
    pub slug: String,
    pub display_name: String,
}

/// Maximum number of holding ids a caller may pass as scope. The retired TS
/// reader filtered by `resourceSlots(env)`, which is a small fixed registry.
pub const MAX_SCOPE: usize = 64;
pub const MAX_SLUG: usize = 128;
pub const MAX_ID: usize = 24;
pub const MIN_ID: usize = 3;

pub fn valid_holding_id(value: &str) -> bool {
    value.len() >= MIN_ID
        && value.len() <= MAX_ID
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SLUG
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

/// Parse the comma-separated, validated resource-slot scope. A blank or
/// malformed scope is rejected; duplicates are collapsed. The returned set is
/// sorted so the rendered scope and its oracle are deterministic.
pub fn parse_scope(raw: &str) -> Option<Vec<String>> {
    if raw.is_empty() || raw.len() > MAX_SCOPE * (MAX_ID + 1) {
        return None;
    }
    let mut ids: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect();
    if ids.is_empty() || ids.len() > MAX_SCOPE || !ids.iter().all(|id| valid_holding_id(id)) {
        return None;
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return None;
    }
    Some(ids)
}

/// Rust-side projection/filter of a fetched row set. SQL stays byte-identical
/// to the retired TS statement; the registry scope is applied here.
pub fn in_scope(binding: &HoldingBinding, scope: &[String]) -> bool {
    scope.iter().any(|id| id == &binding.holding_id)
}

/// Mirrors `ORDER BY holding_id LIMIT 1`: the smallest in-scope id wins even
/// when the fetched rows arrive in an arbitrary order.
pub fn select_one(bindings: Vec<HoldingBinding>, scope: &[String]) -> Option<HoldingBinding> {
    bindings
        .into_iter()
        .filter(|binding| in_scope(binding, scope))
        .min_by(|a, b| a.holding_id.cmp(&b.holding_id).then(a.slug.cmp(&b.slug)))
}

pub fn select_all(bindings: Vec<HoldingBinding>, scope: &[String]) -> Vec<HoldingBinding> {
    let mut rows: Vec<HoldingBinding> = bindings
        .into_iter()
        .filter(|binding| in_scope(binding, scope))
        .collect();
    rows.sort_by(|a, b| a.holding_id.cmp(&b.holding_id).then(a.slug.cmp(&b.slug)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(id: &str) -> HoldingBinding {
        HoldingBinding {
            holding_id: id.to_owned(),
            slug: format!("slug-{id}"),
            display_name: format!("Display {id}"),
        }
    }

    #[test]
    fn query_kinds_are_closed() {
        assert_eq!(BindingQuery::parse("by-slug"), Some(BindingQuery::BySlug));
        assert_eq!(BindingQuery::parse("default"), Some(BindingQuery::Default));
        assert_eq!(BindingQuery::parse("active"), Some(BindingQuery::Active));
        assert_eq!(BindingQuery::parse("BY-SLUG"), None);
        assert_eq!(BindingQuery::parse(""), None);
        assert_eq!(BindingQuery::parse("slug"), None);
        assert_eq!(BindingQuery::Default.as_str(), "default");
    }

    #[test]
    fn scope_is_bounded_unique_and_sorted() {
        assert_eq!(parse_scope("h002,h001,h002"), Some(vec!["h001".into(), "h002".into()]));
        assert_eq!(parse_scope(""), None);
        assert_eq!(parse_scope("h001,"), None);
        assert_eq!(parse_scope("H001"), None);
        assert_eq!(parse_scope("h001;DROP"), None);
        let many = (0..MAX_SCOPE + 1)
            .map(|index| format!("h{index:03}"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(parse_scope(&many), None);
    }

    #[test]
    fn projection_filters_and_orders_by_registry_scope() {
        let scope = parse_scope("h002,h001").unwrap();
        let rows = vec![binding("h003"), binding("h002"), binding("h001")];
        assert_eq!(select_one(rows.clone(), &scope), Some(binding("h001")));
        let all = select_all(rows, &scope);
        assert_eq!(all.iter().map(|row| row.holding_id.as_str()).collect::<Vec<_>>(), ["h001", "h002"]);
    }
}
