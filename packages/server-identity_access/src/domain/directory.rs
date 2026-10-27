//! B0 Directory observations are facts, never an Actor or grant.
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::{BTreeMap, BTreeSet};

pub const DIRECTORY_FRESH_MS: i64 = 300_000;
pub const MAX_DIRECTORY_OBJECTS: usize = 10_000;
pub const MAX_DIRECTORY_CONTAINERS: usize = 20_000;
pub const MAX_DIRECTORY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DIRECTORY_EDGES: usize = 200_000;

pub fn rejected(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DIRECTORY_INCOMPLETE",
        "A complete bounded Directory observation is required.",
    )
}
#[derive(Clone, Copy, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryProvider {
    ActiveDirectory,
    EntraId,
    Univention,
}
#[derive(Clone, Copy, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DirectoryMode {
    Ldaps,
    Oidc,
}

/// Exact issuer/connection/profile version is retained around provider work.
/// Its string fields must be loaded from the actual trusted route/connection;
/// possession of this copied descriptor alone authorizes no account.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryConnectionConfig {
    pub mode: DirectoryMode,
    pub secret_profile: String,
    pub authority: String,
    pub ldap_url: Option<String>,
    pub base_dn: Option<String>,
    pub group_base_dn: Option<String>,
    pub user_filter: Option<String>,
    pub issuer: Option<String>,
    pub tenant_id: Option<String>,
    pub client_id: Option<String>,
}
#[derive(Clone)]
pub struct DirectoryConnectionDescriptor {
    pub connection_id: String,
    pub holding_id: Option<String>,
    pub company_id: String,
    pub provider: DirectoryProvider,
    pub mode: DirectoryMode,
    pub version: i64,
    pub tested_version: Option<i64>,
    pub config_fingerprint: String,
    pub profile_fingerprint: String,
    pub config: DirectoryConnectionConfig,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedDirectoryUser {
    pub external_id: String,
    pub username: String,
    pub name: String,
    pub email: Option<String>,
    pub enabled: bool,
    pub group_ids: Vec<String>,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedDirectoryGroup {
    pub external_id: String,
    pub name: String,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedContainer {
    pub external_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub kind: String,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPlacement {
    pub kind: String,
    pub external_id: String,
    pub container_id: Option<String>,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryGroupEdge {
    pub parent_id: String,
    pub child_id: String,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryHierarchy {
    pub containers: Vec<ObservedContainer>,
    pub placements: Vec<DirectoryPlacement>,
    pub group_edges: Vec<DirectoryGroupEdge>,
}

/// Counts before each retained observation, including transitive expansion.
pub struct ObservationBudget {
    bytes: usize,
    edges: usize,
}
impl ObservationBudget {
    pub fn new() -> Self {
        Self { bytes: 0, edges: 0 }
    }
    pub fn text(&mut self, text: &str) -> Result<(), BusinessError> {
        if text.len() > 16_384 {
            return Err(incomplete());
        }
        self.bytes = self.bytes.checked_add(text.len()).ok_or_else(incomplete)?;
        if self.bytes > MAX_DIRECTORY_BYTES {
            return Err(incomplete());
        }
        Ok(())
    }
    pub fn edge(&mut self) -> Result<(), BusinessError> {
        self.edges = self.edges.checked_add(1).ok_or_else(incomplete)?;
        if self.edges > MAX_DIRECTORY_EDGES {
            return Err(incomplete());
        }
        Ok(())
    }
}

/// Only the provider adapter finalizes this value after every required LDAP
/// search/page/range has a successful terminal response. Partial input never
/// reaches synchronization or missing-identity reconciliation.
pub struct CompleteDirectorySnapshot {
    users: Vec<ObservedDirectoryUser>,
    groups: Vec<ObservedDirectoryGroup>,
    hierarchy: DirectoryHierarchy,
    observed_at: i64,
}
impl CompleteDirectorySnapshot {
    pub fn checked(
        users: Vec<ObservedDirectoryUser>,
        groups: Vec<ObservedDirectoryGroup>,
        hierarchy: DirectoryHierarchy,
        observed_at: i64,
    ) -> Result<Self, BusinessError> {
        if users.len() > MAX_DIRECTORY_OBJECTS
            || groups.len() > MAX_DIRECTORY_OBJECTS
            || hierarchy.containers.len() > MAX_DIRECTORY_CONTAINERS
            || observed_at < 0
        {
            return Err(incomplete());
        }
        let mut budget = ObservationBudget::new();
        let mut user_ids = BTreeSet::new();
        let mut group_ids = BTreeSet::new();
        for group in &groups {
            checked_identifier(&group.external_id)?;
            budget.text(&group.external_id)?;
            budget.text(&group.name)?;
            if !group_ids.insert(group.external_id.as_str()) {
                return Err(rejected(
                    "DIRECTORY_DUPLICATE_IDENTITY",
                    "The Directory returned duplicate immutable identifiers.",
                ));
            }
        }
        for user in &users {
            checked_identifier(&user.external_id)?;
            budget.text(&user.external_id)?;
            budget.text(&user.username)?;
            budget.text(&user.name)?;
            if let Some(email) = &user.email {
                budget.text(email)?;
            }
            if !user_ids.insert(user.external_id.as_str()) {
                return Err(rejected(
                    "DIRECTORY_DUPLICATE_IDENTITY",
                    "The Directory returned duplicate immutable identifiers.",
                ));
            }
            for group in &user.group_ids {
                budget.edge()?;
                budget.text(group)?;
                if !group_ids.contains(group.as_str()) {
                    return Err(incomplete());
                }
            }
        }
        validate_hierarchy(&hierarchy, &user_ids, &group_ids, &groups, &mut budget)?;
        Ok(Self {
            users,
            groups,
            hierarchy,
            observed_at,
        })
    }
    pub fn users(&self) -> &[ObservedDirectoryUser] {
        &self.users
    }
    pub fn groups(&self) -> &[ObservedDirectoryGroup] {
        &self.groups
    }
    pub fn hierarchy(&self) -> &DirectoryHierarchy {
        &self.hierarchy
    }
    pub fn observed_at(&self) -> i64 {
        self.observed_at
    }
    pub fn require_fresh(&self, now: i64) -> Result<(), BusinessError> {
        if self.observed_at > now || self.observed_at <= now - DIRECTORY_FRESH_MS {
            return Err(rejected(
                "DIRECTORY_ACCESS_UNAVAILABLE",
                "Your Directory access needs a successful current refresh.",
            ));
        }
        Ok(())
    }
}
fn checked_identifier(id: &str) -> Result<(), BusinessError> {
    if id.is_empty() || id.len() > 4096 {
        return Err(incomplete());
    }
    Ok(())
}
fn validate_hierarchy(
    hierarchy: &DirectoryHierarchy,
    user_ids: &BTreeSet<&str>,
    group_ids: &BTreeSet<&str>,
    groups: &[ObservedDirectoryGroup],
    budget: &mut ObservationBudget,
) -> Result<(), BusinessError> {
    let mut containers = BTreeMap::new();
    for row in &hierarchy.containers {
        checked_identifier(&row.external_id)?;
        budget.text(&row.external_id)?;
        budget.text(&row.name)?;
        budget.text(&row.kind)?;
        if !matches!(row.kind.as_str(), "ou" | "container") {
            return Err(incomplete());
        }
        if containers
            .insert(row.external_id.as_str(), row.parent_id.as_deref())
            .is_some()
        {
            return Err(incomplete());
        }
    }
    for parent in containers.values().flatten() {
        if !containers.contains_key(parent) {
            return Err(incomplete());
        }
    }
    let container_edges = hierarchy
        .containers
        .iter()
        .filter_map(|row| {
            row.parent_id
                .as_ref()
                .map(|parent| (parent.as_str(), row.external_id.as_str()))
        })
        .collect::<Vec<_>>();
    acyclic(
        hierarchy
            .containers
            .iter()
            .map(|row| row.external_id.as_str()),
        container_edges,
    )?;
    let mut placed = BTreeSet::new();
    for placement in &hierarchy.placements {
        budget.edge()?;
        budget.text(&placement.external_id)?;
        if let Some(container) = &placement.container_id {
            budget.text(container)?;
        }
        if placement
            .container_id
            .as_ref()
            .is_some_and(|id| !containers.contains_key(id.as_str()))
            || !placed.insert((placement.kind.as_str(), placement.external_id.as_str()))
            || match placement.kind.as_str() {
                "identity" => !user_ids.contains(placement.external_id.as_str()),
                "group" => !group_ids.contains(placement.external_id.as_str()),
                _ => true,
            }
        {
            return Err(incomplete());
        }
    }
    let mut edges = BTreeSet::new();
    for edge in &hierarchy.group_edges {
        budget.edge()?;
        budget.text(&edge.parent_id)?;
        budget.text(&edge.child_id)?;
        if !group_ids.contains(edge.parent_id.as_str())
            || !group_ids.contains(edge.child_id.as_str())
            || edge.parent_id == edge.child_id
            || !edges.insert((edge.parent_id.as_str(), edge.child_id.as_str()))
        {
            return Err(incomplete());
        }
    }
    acyclic(
        groups.iter().map(|group| group.external_id.as_str()),
        hierarchy
            .group_edges
            .iter()
            .map(|row| (row.parent_id.as_str(), row.child_id.as_str())),
    )?;
    Ok(())
}

fn acyclic<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    edges: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), BusinessError> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (parent, child) in edges {
        children.entry(parent).or_default().push(child);
    }
    let mut done = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for root in ids {
        let mut stack = vec![(root, 0usize, false)];
        while let Some((node, depth, exit)) = stack.pop() {
            if exit {
                visiting.remove(node);
                done.insert(node);
                continue;
            }
            if visiting.contains(node) || depth > 64 {
                return Err(incomplete());
            }
            if done.contains(node) {
                continue;
            }
            visiting.insert(node);
            stack.push((node, depth, true));
            if let Some(rows) = children.get(node) {
                for child in rows.iter().rev() {
                    stack.push((child, depth + 1, false));
                }
            }
        }
    }
    Ok(())
}

pub struct ApprovedDirectoryIdentity {
    pub user_id: String,
    pub principal_id: String,
    pub person_id: Option<String>,
    pub identity_id: String,
    pub member_version: i64,
    pub observed_at: i64,
    pub connection_version: i64,
    pub identity_enabled: bool,
    pub member_active: bool,
    pub member_directory: bool,
    pub identity_user_id: String,
    pub identity_company_id: String,
    pub member_company_id: String,
    pub identity_connection_id: String,
}
impl ApprovedDirectoryIdentity {
    pub fn require_fresh(
        &self,
        connection: &DirectoryConnectionDescriptor,
        now: i64,
    ) -> Result<(), BusinessError> {
        if self.user_id.is_empty()
            || self.identity_id.is_empty()
            || self.user_id != self.identity_user_id
            || self.identity_company_id != connection.company_id
            || self.member_company_id != connection.company_id
            || self.identity_connection_id != connection.connection_id
            || !self.identity_enabled
            || !self.member_active
            || !self.member_directory
            || self.member_version <= 0
            || self.connection_version != connection.version
            || self.observed_at > now
            || self.observed_at <= now - DIRECTORY_FRESH_MS
        {
            return Err(rejected(
                "DIRECTORY_ACCESS_UNAVAILABLE",
                "Your approved Directory identity needs current verification.",
            ));
        }
        Ok(())
    }
}
