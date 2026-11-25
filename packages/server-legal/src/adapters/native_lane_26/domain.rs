//! Pure response types and invariants for the native Legal executed-copy read.
//!
//! The owner owns every query and every projection here. No SQL and no I/O
//! belong in this module; the Postgres adapter binds the fixed statements and
//! the HTTP adapter transports the resulting value.

use erpx_server_shared::public::RecordId;
use serde::{Deserialize, Serialize};

/// Maximum number of executed copies returned for one item. This mirrors the
/// original owner `LIMIT 100` and stays a Rust-owned invariant.
pub const COPY_LIMIT: i64 = 100;

/// A registered, exact Holding target. Composition injects these; duplicate or
/// malformed targets are rejected before any route is installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReviewRow {
    pub id: String,
    pub executed_copy_id: String,
    pub item_version: i64,
    pub decision: String,
    pub checks: serde_json::Value,
    pub reason: String,
    pub reviewer_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutedCopyRow {
    pub id: String,
    pub item_id: String,
    pub document_id: String,
    pub approved_cycle: i64,
    pub approved_document_id: String,
    pub approved_hash: String,
    pub executed_hash: String,
    pub metadata: serde_json::Value,
    pub state: String,
    pub imported_by: String,
    pub imported_at: i64,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub version: i64,
    pub document_name: String,
    pub document_version: i64,
    pub document_author: String,
    pub approved_author: String,
    pub reviews: Vec<ReviewRow>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalCycle {
    pub cycle: i64,
}

/// Complete owner-local read result for one item. Every field is authoritative;
/// the TypeScript transport accepts this shape verbatim and adds no projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutedCopyRead {
    pub item_id: String,
    pub member: bool,
    pub approval: Option<ApprovalCycle>,
    pub copies: Vec<ExecutedCopyRow>,
}

/// Registered Holding slugs are exact lowercase tokens, never free request text.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Item, user and Company identities are bounded opaque record ids.
pub fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Owner projection: bind the per-copy authors onto the list rows and attach the
/// ordered review history. Pure data work; it cannot fail or reach the database.
pub fn project(
    item_id: &str,
    member: bool,
    approval: Option<i64>,
    heads: Vec<CopyHead>,
    authors: Vec<CopyAuthors>,
    reviews: Vec<(String, ReviewRow)>,
) -> ExecutedCopyRead {
    let authors = authors
        .into_iter()
        .map(|a| {
            let id = a.id.clone();
            (id, a)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut by_copy = std::collections::BTreeMap::<String, Vec<ReviewRow>>::new();
    for (copy_id, review) in reviews {
        by_copy.entry(copy_id).or_default().push(review);
    }
    let copies = heads
        .into_iter()
        // Defensive: a concurrent delete cannot make the HTTP layer emit a
        // half-populated copy. The owner query is the authority for the set.
        .filter(|head| authors.contains_key(&head.id))
        .map(|head| {
            let author = &authors[&head.id];
            ExecutedCopyRow {
                id: head.id,
                item_id: head.item_id,
                document_id: head.document_id,
                approved_cycle: head.approved_cycle,
                approved_document_id: head.approved_document_id,
                approved_hash: head.approved_hash,
                executed_hash: head.executed_hash,
                metadata: head.metadata,
                state: head.state,
                imported_by: head.imported_by,
                imported_at: head.imported_at,
                reviewed_by: head.reviewed_by,
                reviewed_at: head.reviewed_at,
                version: head.version,
                document_name: head.document_name,
                document_version: head.document_version,
                document_author: author.document_author.clone(),
                approved_author: author.approved_author.clone(),
                reviews: by_copy.remove(&author.id).unwrap_or_default(),
            }
        })
        .collect();
    ExecutedCopyRead {
        item_id: item_id.to_owned(),
        member,
        approval: approval.map(|cycle| ApprovalCycle { cycle }),
        copies,
    }
}

/// Copy list row before per-copy authors are bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopyHead {
    pub id: String,
    pub item_id: String,
    pub document_id: String,
    pub approved_cycle: i64,
    pub approved_document_id: String,
    pub approved_hash: String,
    pub executed_hash: String,
    pub metadata: serde_json::Value,
    pub state: String,
    pub imported_by: String,
    pub imported_at: i64,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<i64>,
    pub version: i64,
    pub document_name: String,
    pub document_version: i64,
}

/// Per-copy document authors from the detail witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopyAuthors {
    pub id: String,
    pub document_author: String,
    pub approved_author: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_or_freeform_targets() {
        assert!(valid_slug("acme"));
        assert!(valid_slug("north-star_1"));
        assert!(!valid_slug(""));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
        assert!(valid_id("item-1"));
        assert!(valid_id("doc.7"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id(&"x".repeat(201)));
    }

    #[test]
    fn projection_binds_authors_reviews_and_approval() {
        let head = CopyHead {
            id: "copy-1".into(),
            item_id: "item-1".into(),
            document_id: "doc-1".into(),
            approved_cycle: 3,
            approved_document_id: "doc-0".into(),
            approved_hash: "aa".into(),
            executed_hash: "bb".into(),
            metadata: serde_json::json!({"pages": 2}),
            state: "pending_review".into(),
            imported_by: "importer".into(),
            imported_at: 10,
            reviewed_by: None,
            reviewed_at: None,
            version: 1,
            document_name: "agreement.pdf".into(),
            document_version: 2,
        };
        let authors = vec![CopyAuthors {
            id: "copy-1".into(),
            document_author: "author".into(),
            approved_author: "approver".into(),
        }];
        let reviews = vec![(
            "copy-1".to_owned(),
            ReviewRow {
                id: "review-1".into(),
                executed_copy_id: "copy-1".into(),
                item_version: 1,
                decision: "confirm".into(),
                checks: serde_json::json!({"contentMatches": true}),
                reason: "ok".into(),
                reviewer_id: "reviewer".into(),
                created_at: 11,
            },
        )];
        let read = project("item-1", true, Some(3), vec![head], authors, reviews);
        assert!(read.member);
        assert_eq!(read.approval, Some(ApprovalCycle { cycle: 3 }));
        assert_eq!(read.copies.len(), 1);
        assert_eq!(read.copies[0].document_author, "author");
        assert_eq!(read.copies[0].reviews.len(), 1);
        assert_eq!(read.copies[0].reviews[0].id, "review-1");
    }
}
