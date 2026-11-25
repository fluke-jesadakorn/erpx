//! Pure response types, validation and invariants for the lane-24 Organization
//! evaluation-reference read family. No SQL and no I/O live here.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};

pub const EVAL_POSITION_BY_LEVEL: &str = "eval_position_by_level";
pub const EVAL_LEVEL_ACTIVE: &str = "eval_level_active";
pub const EVAL_WORKSPACE_POSITIONS: &str = "eval_workspace_positions";
pub const EVAL_WORKSPACE_LEVELS: &str = "eval_workspace_levels";
pub const EVAL_MARKET_LEVELS: &str = "eval_market_levels";
pub const ORG_ACCESS_NODES: &str = "org_access_nodes";
pub const ORG_BRANCH_OPTIONS_ALL: &str = "org_branch_options_all";
pub const ORG_BRANCH_OPTIONS_SCOPED: &str = "org_branch_options_scoped";
pub const ORG_BRANCH_OPTIONS_POS: &str = "org_branch_options_pos";

/// Owner allow-list. A caller never supplies SQL text; it selects one kind.
pub const KINDS: [&str; 9] = [
    EVAL_POSITION_BY_LEVEL,
    EVAL_LEVEL_ACTIVE,
    EVAL_WORKSPACE_POSITIONS,
    EVAL_WORKSPACE_LEVELS,
    EVAL_MARKET_LEVELS,
    ORG_ACCESS_NODES,
    ORG_BRANCH_OPTIONS_ALL,
    ORG_BRANCH_OPTIONS_SCOPED,
    ORG_BRANCH_OPTIONS_POS,
];

/// Bounded positional arity for each kind. Company scope is taken only from the
/// verified request header and injected by the owner, never from these values.
pub fn arity(kind: &str) -> Option<usize> {
    match kind {
        EVAL_POSITION_BY_LEVEL => Some(2), // position id, level id
        EVAL_LEVEL_ACTIVE => Some(1),      // level id
        EVAL_WORKSPACE_POSITIONS | EVAL_WORKSPACE_LEVELS | EVAL_MARKET_LEVELS
        | ORG_ACCESS_NODES | ORG_BRANCH_OPTIONS_ALL => Some(0),
        ORG_BRANCH_OPTIONS_SCOPED => Some(1),
        ORG_BRANCH_OPTIONS_POS => Some(2),
        _ => None,
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PositionRef {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LevelRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspacePosition {
    pub id: String,
    pub title: String,
    #[serde(rename = "level_id")]
    pub level_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketLevel {
    pub id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AccessNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub display_name: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BranchOption {
    pub id: String,
    pub name: String,
}

/// Typed projection of one owner read. The variant determines the response key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadResult {
    Position(Option<PositionRef>),
    Exists(bool),
    WorkspacePositions(Vec<WorkspacePosition>),
    Levels(Vec<LevelRef>),
    MarketLevels(Vec<MarketLevel>),
    AccessNodes(Vec<AccessNode>),
    BranchOptions(Vec<BranchOption>),
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FactsEnvelope {
    pub data: Value,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_LANE24_READ_UNAVAILABLE",
        "The native Organization evaluation read is unavailable.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_LANE24_READ_INVALID",
        "Use one bounded Organization evaluation read.",
    )
}

/// Company ids are bounded owner-scope identifiers (same shape as the other
/// Organization native routes); a missing or hostile value fails closed.
pub fn valid_company(value: &str) -> bool {
    valid_id(value)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Validate the transport facts before any SQL is planned. The kind must be an
/// owner allow-list entry, the Company must be well formed, and every remaining
/// value must be a bounded owner id. No caller input reaches SQL as text.
pub fn validate(company: &str, kind: &str, values: &[Option<String>]) -> Result<(), BusinessError> {
    let expected = arity(kind).ok_or_else(invalid)?;
    if !valid_company(company)
        || values.len() != expected
        || values.iter().any(|value| value.as_deref().is_some_and(|value| !valid_id(value)))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Assemble the strictly shaped response. Only the owner-selected variant
/// contributes a data key, so a TS client can reject an unexpected shape.
pub fn envelope(kind: &str, result: ReadResult) -> Result<FactsEnvelope, BusinessError> {
    let data = match (kind, result) {
        (EVAL_POSITION_BY_LEVEL, ReadResult::Position(row)) => {
            json!({"kind": kind, "row": row})
        }
        (EVAL_LEVEL_ACTIVE, ReadResult::Exists(exists)) => {
            json!({"kind": kind, "exists": exists})
        }
        (EVAL_WORKSPACE_POSITIONS, ReadResult::WorkspacePositions(rows)) => {
            json!({"kind": kind, "rows": rows})
        }
        (EVAL_WORKSPACE_LEVELS, ReadResult::Levels(rows)) => {
            json!({"kind": kind, "rows": rows})
        }
        (EVAL_MARKET_LEVELS, ReadResult::MarketLevels(rows)) => {
            json!({"kind": kind, "rows": rows})
        }
        (ORG_ACCESS_NODES, ReadResult::AccessNodes(rows)) => {
            json!({"kind": kind, "rows": rows})
        }
        (
            ORG_BRANCH_OPTIONS_ALL | ORG_BRANCH_OPTIONS_SCOPED | ORG_BRANCH_OPTIONS_POS,
            ReadResult::BranchOptions(rows),
        ) => json!({"kind": kind, "rows": rows}),
        _ => return Err(unavailable()),
    };
    Ok(FactsEnvelope { data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_and_arity_are_closed() {
        assert_eq!(KINDS.len(), 9);
        for kind in KINDS {
            assert!(arity(kind).is_some());
            assert!(validate("acme-co", kind, &vec![Some("x".into()); arity(kind).unwrap()]).is_ok());
        }
        assert!(validate("acme-co", "not_a_kind", &[]).is_err());
        assert!(validate("", EVAL_MARKET_LEVELS, &[]).is_err());
        assert!(validate("acme-co", EVAL_MARKET_LEVELS, &[Some("extra".into())]).is_err());
        assert!(validate("acme-co", EVAL_LEVEL_ACTIVE, &[Some("../bad".into())]).is_err());
    }

    #[test]
    fn envelope_carries_only_the_matching_variant() {
        let value = serde_json::to_value(envelope(
            EVAL_POSITION_BY_LEVEL,
            ReadResult::Position(None),
        )
        .unwrap())
        .unwrap();
        assert_eq!(value["data"]["kind"], EVAL_POSITION_BY_LEVEL);
        assert!(value["data"]["row"].is_null());
        assert!(value["data"].get("rows").is_none());
        assert!(value["data"].get("exists").is_none());
    }

    #[test]
    fn envelope_rejects_a_mismatched_variant() {
        assert!(envelope(EVAL_LEVEL_ACTIVE, ReadResult::MarketLevels(vec![])).is_err());
    }
}
