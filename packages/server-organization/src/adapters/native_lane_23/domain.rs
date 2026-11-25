//! Pure response types and invariants for the native Organization directory
//! hierarchy read. This module owns no SQL and no transport: it only describes
//! the shape the native service is allowed to return and validates it before a
//! reply is released.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

/// A native reply is holding-wide; cap every collection so one response can
/// never become an unbounded payload.
pub const MAX_ROWS: usize = 20_000;

pub const NODE_KINDS: [&str; 5] = ["company", "headquarters", "branch", "department", "team"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirectoryNode {
    pub id: String,
    pub company_id: String,
    pub kind: String,
    pub slug: String,
    pub display_name: String,
    pub parent_id: Option<String>,
    pub branch_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirectoryLevel {
    pub id: String,
    pub company_id: String,
    pub code: String,
    pub name: String,
    pub level_number: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirectoryPosition {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub title: String,
    pub level_id: String,
    pub reports_to_position_id: Option<String>,
    /// INTEGER(0|1) column in `organization_positions`; kept numeric to match
    /// the TypeScript directory projection contract.
    pub is_ceo: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DirectoryHierarchy {
    pub nodes: Vec<DirectoryNode>,
    pub levels: Vec<DirectoryLevel>,
    pub positions: Vec<DirectoryPosition>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_DIRECTORY_HIERARCHY_INVALID",
        "The native Organization directory hierarchy was invalid.",
    )
}

fn bounded(rows: usize) -> Result<(), BusinessError> {
    if rows > MAX_ROWS {
        Err(invalid())
    } else {
        Ok(())
    }
}

fn unique<'a>(values: impl Iterator<Item = &'a str>) -> Result<(), BusinessError> {
    let mut seen = std::collections::HashSet::with_capacity(16);
    for value in values {
        if value.is_empty() || !seen.insert(value) {
            return Err(invalid());
        }
    }
    Ok(())
}

impl DirectoryHierarchy {
    /// Fail-closed shape check applied by the application layer to every native
    /// result before the router serializes it.
    pub fn validate(&self) -> Result<(), BusinessError> {
        bounded(self.nodes.len())?;
        bounded(self.levels.len())?;
        bounded(self.positions.len())?;
        unique(self.nodes.iter().map(|node| node.id.as_str()))?;
        unique(self.levels.iter().map(|level| level.id.as_str()))?;
        unique(self.positions.iter().map(|position| position.id.as_str()))?;
        for node in &self.nodes {
            if node.company_id.is_empty()
                || node.slug.is_empty()
                || node.display_name.is_empty()
                || !NODE_KINDS.contains(&node.kind.as_str())
            {
                return Err(invalid());
            }
        }
        for level in &self.levels {
            if level.company_id.is_empty()
                || level.code.is_empty()
                || level.name.is_empty()
                || level.level_number.is_some_and(|number| !(1..=6).contains(&number))
            {
                return Err(invalid());
            }
        }
        for position in &self.positions {
            if position.company_id.is_empty()
                || position.title.is_empty()
                || position.level_id.is_empty()
                || !(0..=1).contains(&position.is_ceo)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DirectoryHierarchy {
        DirectoryHierarchy {
            nodes: vec![DirectoryNode {
                id: "n1".into(),
                company_id: "c1".into(),
                kind: "branch".into(),
                slug: "b1".into(),
                display_name: "Branch".into(),
                parent_id: None,
                branch_id: None,
            }],
            levels: vec![DirectoryLevel {
                id: "l1".into(),
                company_id: "c1".into(),
                code: "L1".into(),
                name: "Level 1".into(),
                level_number: Some(1),
            }],
            positions: vec![DirectoryPosition {
                id: "p1".into(),
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                title: "CEO".into(),
                level_id: "l1".into(),
                reports_to_position_id: None,
                is_ceo: 1,
            }],
        }
    }

    #[test]
    fn sample_hierarchy_is_valid() {
        assert!(sample().validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_ids_and_unknown_kinds() {
        let mut facts = sample();
        facts.nodes.push(facts.nodes[0].clone());
        assert_eq!(facts.validate(), Err(invalid()));
        let mut facts = sample();
        facts.nodes[0].kind = "unknown".into();
        assert_eq!(facts.validate(), Err(invalid()));
    }

    #[test]
    fn rejects_out_of_range_numbers_and_empty_keys() {
        let mut facts = sample();
        facts.levels[0].level_number = Some(0);
        assert_eq!(facts.validate(), Err(invalid()));
        let mut facts = sample();
        facts.positions[0].is_ceo = 2;
        assert_eq!(facts.validate(), Err(invalid()));
        let mut facts = sample();
        facts.nodes[0].company_id.clear();
        assert_eq!(facts.validate(), Err(invalid()));
    }

    #[test]
    fn rejects_unbounded_collections() {
        let mut facts = sample();
        let template = facts.nodes[0].clone();
        facts.nodes = (0..MAX_ROWS + 1)
            .map(|index| DirectoryNode {
                id: format!("n{index}"),
                ..template.clone()
            })
            .collect();
        assert_eq!(facts.validate(), Err(invalid()));
    }
}
