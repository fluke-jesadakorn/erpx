use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use erpx_cqrs_sqlx::WritePool;
use erpx_integration_contracts::organization::{
    OrganizationLineageKey, OrganizationLineageWitness, OrganizationNodeKind,
    OrganizationNodeStatus,
};
use erpx_kernel::AuthorityScope;
use erpx_organization::{
    OrganizationNodeId, OrganizationNodeKind as DomainNodeKind, OrganizationPlacement, PortError,
    ScopeContainmentPort,
};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use thiserror::Error;
use uuid::Uuid;

/// Maximum number of requested witnesses in one authorization lookup.
///
/// The bound applies before request-local deduplication so a duplicate-heavy
/// request cannot turn inexpensive cache hits into unbounded allocation work.
pub const MAX_ORGANIZATION_LINEAGE_BATCH_SIZE: usize = 256;

const LINEAGE_BATCH_QUERY: &str = "WITH requested(holding_id,node_id) AS (
         SELECT * FROM UNNEST($1::uuid[],$2::uuid[])
     ),
     targets AS (
         SELECT authority.holding_id,authority.node_id,authority.lineage_node_ids
         FROM erpx_organization_write.organization_node_authority authority
         JOIN requested
           ON requested.holding_id=authority.holding_id
          AND requested.node_id=authority.node_id
         WHERE authority.holding_id=erpx_cqrs_write.current_holding_id()
     ),
     required(holding_id,node_id) AS (
         SELECT holding_id,node_id FROM requested
         UNION
         SELECT target.holding_id,ancestor.node_id
         FROM targets target
         CROSS JOIN LATERAL UNNEST(target.lineage_node_ids) ancestor(node_id)
     )
     SELECT authority.holding_id,authority.node_id,authority.aggregate_version,
            authority.node_kind,authority.node_status,authority.parent_node_id,
            authority.lineage_node_ids,authority.company_id,authority.branch_id,
            authority.department_id
     FROM erpx_organization_write.organization_node_authority authority
     JOIN required
       ON required.holding_id=authority.holding_id
      AND required.node_id=authority.node_id
     WHERE authority.holding_id=erpx_cqrs_write.current_holding_id()
     ORDER BY authority.holding_id,authority.node_id
     FOR SHARE OF authority";

/// Fail-closed errors from authoritative Organization lineage resolution.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationLineageResolverError {
    #[error("organization lineage batch must not be empty")]
    Empty,
    #[error("organization lineage batch of {actual} exceeds the maximum of {maximum} requests")]
    BatchTooLarge { maximum: usize, actual: usize },
    #[error("organization lineage witness is missing for {0:?}")]
    Missing(OrganizationLineageKey),
    #[error("organization lineage authority is corrupt for {0:?}")]
    Corrupt(OrganizationLineageKey),
    #[error("organization lineage authority is unavailable")]
    Unavailable,
}

/// Write-side Organization hierarchy resolver.
///
/// Resolution uses a request-local map only. It deliberately keeps no
/// positive TTL cache: every call observes the authoritative write relation,
/// and command guards can use [`Self::resolve_batch_in_transaction`] to retain
/// the selected row locks until their atomic commit completes.
#[derive(Debug, Clone)]
pub struct OrganizationLineageResolver {
    pool: WritePool,
}

impl OrganizationLineageResolver {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    /// Resolves a bounded batch in one holding-bound transaction.
    ///
    /// Results align one-for-one with `requests`. Repeated keys are repeated
    /// in the result but fetched and validated once. Any missing or corrupt
    /// target/ancestor rejects the complete batch.
    pub async fn resolve_batch(
        &self,
        requests: &[OrganizationLineageKey],
    ) -> Result<Vec<OrganizationLineageWitness>, OrganizationLineageResolverError> {
        let prepared = PreparedBatch::try_new(requests)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| OrganizationLineageResolverError::Unavailable)?;
        let witnesses = resolve_prepared(&mut transaction, &prepared).await?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationLineageResolverError::Unavailable)?;
        Ok(witnesses)
    }

    /// Resolves and share-locks a bounded batch inside the caller's command
    /// transaction. The caller retains ownership of commit/rollback.
    pub async fn resolve_batch_in_transaction(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        requests: &[OrganizationLineageKey],
    ) -> Result<Vec<OrganizationLineageWitness>, OrganizationLineageResolverError> {
        let prepared = PreparedBatch::try_new(requests)?;
        resolve_prepared(transaction, &prepared).await
    }
}

#[async_trait]
impl ScopeContainmentPort for OrganizationLineageResolver {
    async fn contains_node(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError> {
        if authority_scope.validate().is_err() || placement.validate_for(node_id).is_err() {
            return Ok(false);
        }
        let key = OrganizationLineageKey::try_new(placement.holding_id(node_id), node_id.as_uuid())
            .map_err(|_| PortError::Denied)?;
        let Some(witness) = self.resolve_one(key).await? else {
            return Ok(false);
        };
        Ok(witness_matches_placement(&witness, node_id, placement)
            && structurally_contains(authority_scope, &witness))
    }

    async fn contains_parent(
        &self,
        authority_scope: &AuthorityScope,
        node_id: OrganizationNodeId,
        placement: &OrganizationPlacement,
    ) -> Result<bool, PortError> {
        if authority_scope.validate().is_err() || placement.validate_for(node_id).is_err() {
            return Ok(false);
        }
        let holding_id = placement.holding_id(node_id);
        let Some(parent_id) = placement.parent_id(node_id) else {
            // A Holding has no stored parent. Only Platform or the exact
            // Holding authority may bootstrap that canonical root.
            return Ok(match authority_scope {
                AuthorityScope::Platform => true,
                AuthorityScope::Holding {
                    holding_id: authority_holding,
                } => *authority_holding == holding_id && node_id.as_uuid() == holding_id,
                AuthorityScope::Company { .. }
                | AuthorityScope::Branch { .. }
                | AuthorityScope::Subtree { .. } => false,
            });
        };
        let key = OrganizationLineageKey::try_new(holding_id, parent_id.as_uuid())
            .map_err(|_| PortError::Denied)?;
        let Some(witness) = self.resolve_one(key).await? else {
            return Ok(false);
        };
        Ok(witness.status() == OrganizationNodeStatus::Active
            && witness_matches_parent(&witness, node_id, placement)
            && structurally_contains(authority_scope, &witness))
    }

    async fn contains_nodes(
        &self,
        authority_scope: &AuthorityScope,
        nodes: &[(OrganizationNodeId, OrganizationPlacement)],
    ) -> Result<Vec<bool>, PortError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }
        if authority_scope.validate().is_err()
            || nodes
                .iter()
                .any(|(node_id, placement)| placement.validate_for(*node_id).is_err())
        {
            return Ok(vec![false; nodes.len()]);
        }
        let keys = nodes
            .iter()
            .map(|(node_id, placement)| {
                OrganizationLineageKey::try_new(placement.holding_id(*node_id), node_id.as_uuid())
                    .map_err(|_| PortError::Denied)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let witnesses = match self.resolve_batch(&keys).await {
            Ok(witnesses) => witnesses,
            Err(OrganizationLineageResolverError::Unavailable) => {
                return Err(PortError::Unavailable);
            }
            Err(
                OrganizationLineageResolverError::Empty
                | OrganizationLineageResolverError::BatchTooLarge { .. }
                | OrganizationLineageResolverError::Missing(_)
                | OrganizationLineageResolverError::Corrupt(_),
            ) => return Ok(vec![false; nodes.len()]),
        };
        if witnesses.len() != nodes.len() {
            return Ok(vec![false; nodes.len()]);
        }
        Ok(nodes
            .iter()
            .zip(witnesses)
            .map(|((node_id, placement), witness)| {
                witness_matches_placement(&witness, *node_id, placement)
                    && structurally_contains(authority_scope, &witness)
            })
            .collect())
    }
}

impl OrganizationLineageResolver {
    async fn resolve_one(
        &self,
        key: OrganizationLineageKey,
    ) -> Result<Option<OrganizationLineageWitness>, PortError> {
        match self.resolve_batch(std::slice::from_ref(&key)).await {
            Ok(mut witnesses) => Ok(witnesses.pop()),
            Err(OrganizationLineageResolverError::Unavailable) => Err(PortError::Unavailable),
            Err(
                OrganizationLineageResolverError::Empty
                | OrganizationLineageResolverError::BatchTooLarge { .. }
                | OrganizationLineageResolverError::Missing(_)
                | OrganizationLineageResolverError::Corrupt(_),
            ) => Ok(None),
        }
    }
}

fn witness_matches_placement(
    witness: &OrganizationLineageWitness,
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> bool {
    witness.key().holding_id().as_uuid() == placement.holding_id(node_id)
        && witness.key().node_id().as_uuid() == node_id.as_uuid()
        && witness.kind() == integration_kind(placement.kind())
        && witness.parent_node_id().map(|id| id.as_uuid())
            == placement.parent_id(node_id).map(|id| id.as_uuid())
        && witness
            .lineage_node_ids()
            .iter()
            .map(|id| id.as_uuid())
            .eq(placement_lineage(node_id, placement))
}

fn witness_matches_parent(
    witness: &OrganizationLineageWitness,
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> bool {
    let lineage = placement_lineage(node_id, placement).collect::<Vec<_>>();
    let Some((parent_id, ancestors)) = lineage.split_last() else {
        return false;
    };
    let Some(expected_kinds) =
        expected_ancestor_kinds(integration_kind(placement.kind()), lineage.len())
    else {
        return false;
    };
    witness.key().holding_id().as_uuid() == placement.holding_id(node_id)
        && witness.key().node_id().as_uuid() == *parent_id
        && expected_kinds
            .last()
            .is_some_and(|kind| *kind == witness.kind())
        && witness
            .lineage_node_ids()
            .iter()
            .map(|id| id.as_uuid())
            .eq(ancestors.iter().copied())
}

fn placement_lineage(
    node_id: OrganizationNodeId,
    placement: &OrganizationPlacement,
) -> impl Iterator<Item = Uuid> {
    let mut lineage = match placement {
        OrganizationPlacement::Holding => Vec::new(),
        OrganizationPlacement::Company { holding_id } => vec![*holding_id],
        OrganizationPlacement::Headquarters {
            holding_id,
            company_id,
        }
        | OrganizationPlacement::Branch {
            holding_id,
            company_id,
        } => vec![*holding_id, *company_id],
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            values.extend(*branch_id);
            values
        }
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id,
            department_id,
        } => {
            let mut values = vec![*holding_id, *company_id];
            values.extend(*branch_id);
            values.push(*department_id);
            values
        }
    };
    lineage.retain(|id| *id != node_id.as_uuid());
    lineage.into_iter()
}

const fn integration_kind(kind: DomainNodeKind) -> OrganizationNodeKind {
    match kind {
        DomainNodeKind::Holding => OrganizationNodeKind::Holding,
        DomainNodeKind::Company => OrganizationNodeKind::Company,
        DomainNodeKind::Headquarters => OrganizationNodeKind::Headquarters,
        DomainNodeKind::Branch => OrganizationNodeKind::Branch,
        DomainNodeKind::Department => OrganizationNodeKind::Department,
        DomainNodeKind::Team => OrganizationNodeKind::Team,
    }
}

/// Structural containment deliberately ignores only the target's lifecycle
/// state. The resolver has already required every ancestor to be active. This
/// lets an authorized human reactivate a suspended node without trusting a
/// stale projection, while the domain still rejects archived mutations.
fn structurally_contains(
    authority_scope: &AuthorityScope,
    witness: &OrganizationLineageWitness,
) -> bool {
    if authority_scope.validate().is_err() || witness.validate().is_err() {
        return false;
    }
    let holding_id = witness.key().holding_id().as_uuid();
    match authority_scope {
        AuthorityScope::Platform => true,
        AuthorityScope::Holding {
            holding_id: authority_holding,
        } => *authority_holding == holding_id,
        AuthorityScope::Company {
            holding_id: authority_holding,
            company_id,
        } => {
            *authority_holding == holding_id
                && witness
                    .company_id()
                    .is_some_and(|id| id.as_uuid() == *company_id)
        }
        AuthorityScope::Branch {
            holding_id: authority_holding,
            company_id,
            branch_id,
        } => {
            *authority_holding == holding_id
                && witness
                    .company_id()
                    .is_some_and(|id| id.as_uuid() == *company_id)
                && witness
                    .branch_id()
                    .is_some_and(|id| id.as_uuid() == *branch_id)
        }
        AuthorityScope::Subtree {
            holding_id: authority_holding,
            organization_node_id,
        } => {
            *authority_holding == holding_id
                && matches!(
                    witness.kind(),
                    OrganizationNodeKind::Department | OrganizationNodeKind::Team
                )
                && (witness.key().node_id().as_uuid() == *organization_node_id
                    || (witness.kind() == OrganizationNodeKind::Team
                        && witness
                            .department_id()
                            .is_some_and(|id| id.as_uuid() == *organization_node_id)))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PreparedBatch {
    order: Vec<OrganizationLineageKey>,
    unique: Vec<OrganizationLineageKey>,
}

impl PreparedBatch {
    fn try_new(
        requests: &[OrganizationLineageKey],
    ) -> Result<Self, OrganizationLineageResolverError> {
        if requests.is_empty() {
            return Err(OrganizationLineageResolverError::Empty);
        }
        if requests.len() > MAX_ORGANIZATION_LINEAGE_BATCH_SIZE {
            return Err(OrganizationLineageResolverError::BatchTooLarge {
                maximum: MAX_ORGANIZATION_LINEAGE_BATCH_SIZE,
                actual: requests.len(),
            });
        }

        let mut seen = HashSet::with_capacity(requests.len());
        let mut unique = Vec::with_capacity(requests.len());
        for key in requests {
            if seen.insert(*key) {
                unique.push(*key);
            }
        }
        Ok(Self {
            order: requests.to_vec(),
            unique,
        })
    }
}

async fn resolve_prepared(
    transaction: &mut Transaction<'_, Postgres>,
    prepared: &PreparedBatch,
) -> Result<Vec<OrganizationLineageWitness>, OrganizationLineageResolverError> {
    let holding_ids = prepared
        .unique
        .iter()
        .map(|key| key.holding_id().as_uuid())
        .collect::<Vec<_>>();
    let node_ids = prepared
        .unique
        .iter()
        .map(|key| key.node_id().as_uuid())
        .collect::<Vec<_>>();
    let rows = sqlx::query(LINEAGE_BATCH_QUERY)
        .bind(&holding_ids)
        .bind(&node_ids)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|_| OrganizationLineageResolverError::Unavailable)?;

    let fallback_key = prepared.order[0];
    let mut cache = HashMap::with_capacity(rows.len());
    for row in rows {
        let authority = AuthorityRow::try_decode(&row, fallback_key)?;
        let key = authority.key;
        let witness = authority.try_into_witness()?;
        if cache.insert(key, witness).is_some() {
            return Err(OrganizationLineageResolverError::Corrupt(key));
        }
    }

    let mut resolved = Vec::with_capacity(prepared.order.len());
    for key in &prepared.order {
        let witness = cache
            .get(key)
            .ok_or(OrganizationLineageResolverError::Missing(*key))?;
        validate_ancestor_chain(witness, &cache)?;
        resolved.push(witness.clone());
    }
    Ok(resolved)
}

#[derive(Debug)]
struct AuthorityRow {
    key: OrganizationLineageKey,
    aggregate_version: i64,
    kind: String,
    status: String,
    parent_node_id: Option<Uuid>,
    lineage_node_ids: Vec<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    department_id: Option<Uuid>,
}

impl AuthorityRow {
    fn try_decode(
        row: &PgRow,
        fallback_key: OrganizationLineageKey,
    ) -> Result<Self, OrganizationLineageResolverError> {
        let holding_id = row
            .try_get("holding_id")
            .map_err(|_| OrganizationLineageResolverError::Corrupt(fallback_key))?;
        let node_id = row
            .try_get("node_id")
            .map_err(|_| OrganizationLineageResolverError::Corrupt(fallback_key))?;
        let key = OrganizationLineageKey::try_new(holding_id, node_id)
            .map_err(|_| OrganizationLineageResolverError::Corrupt(fallback_key))?;
        Ok(Self {
            key,
            aggregate_version: row
                .try_get("aggregate_version")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            kind: row
                .try_get("node_kind")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            status: row
                .try_get("node_status")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            parent_node_id: row
                .try_get("parent_node_id")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            lineage_node_ids: row
                .try_get("lineage_node_ids")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            company_id: row
                .try_get("company_id")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            branch_id: row
                .try_get("branch_id")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
            department_id: row
                .try_get("department_id")
                .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?,
        })
    }

    fn try_into_witness(
        self,
    ) -> Result<OrganizationLineageWitness, OrganizationLineageResolverError> {
        let key = self.key;
        let kind = parse_kind(&self.kind).ok_or(OrganizationLineageResolverError::Corrupt(key))?;
        let status =
            parse_status(&self.status).ok_or(OrganizationLineageResolverError::Corrupt(key))?;
        let aggregate_version = u64::try_from(self.aggregate_version)
            .ok()
            .filter(|version| *version > 0)
            .ok_or(OrganizationLineageResolverError::Corrupt(key))?;
        let witness = OrganizationLineageWitness::try_new(
            key,
            kind,
            status,
            self.parent_node_id,
            self.lineage_node_ids,
            aggregate_version,
        )
        .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?;
        if witness.company_id().map(|id| id.as_uuid()) != self.company_id
            || witness.branch_id().map(|id| id.as_uuid()) != self.branch_id
            || witness.department_id().map(|id| id.as_uuid()) != self.department_id
        {
            return Err(OrganizationLineageResolverError::Corrupt(key));
        }
        Ok(witness)
    }
}

const fn parse_kind(value: &str) -> Option<OrganizationNodeKind> {
    match value.as_bytes() {
        b"holding" => Some(OrganizationNodeKind::Holding),
        b"company" => Some(OrganizationNodeKind::Company),
        b"headquarters" => Some(OrganizationNodeKind::Headquarters),
        b"branch" => Some(OrganizationNodeKind::Branch),
        b"department" => Some(OrganizationNodeKind::Department),
        b"team" => Some(OrganizationNodeKind::Team),
        _ => None,
    }
}

const fn parse_status(value: &str) -> Option<OrganizationNodeStatus> {
    match value.as_bytes() {
        b"active" => Some(OrganizationNodeStatus::Active),
        b"suspended" => Some(OrganizationNodeStatus::Suspended),
        b"archived" => Some(OrganizationNodeStatus::Archived),
        _ => None,
    }
}

fn validate_ancestor_chain(
    witness: &OrganizationLineageWitness,
    cache: &HashMap<OrganizationLineageKey, OrganizationLineageWitness>,
) -> Result<(), OrganizationLineageResolverError> {
    let key = witness.key();
    let expected_kinds = expected_ancestor_kinds(witness.kind(), witness.lineage_node_ids().len())
        .ok_or(OrganizationLineageResolverError::Corrupt(key))?;
    if expected_kinds.len() != witness.lineage_node_ids().len() {
        return Err(OrganizationLineageResolverError::Corrupt(key));
    }

    let holding_id = key.holding_id().as_uuid();
    for (index, (ancestor_id, expected_kind)) in witness
        .lineage_node_ids()
        .iter()
        .zip(expected_kinds)
        .enumerate()
    {
        let ancestor_key = OrganizationLineageKey::try_new(holding_id, ancestor_id.as_uuid())
            .map_err(|_| OrganizationLineageResolverError::Corrupt(key))?;
        let ancestor = cache
            .get(&ancestor_key)
            .ok_or(OrganizationLineageResolverError::Missing(ancestor_key))?;
        if ancestor.kind() != *expected_kind
            || ancestor.lineage_node_ids() != &witness.lineage_node_ids()[..index]
            || ancestor.status() != OrganizationNodeStatus::Active
        {
            return Err(OrganizationLineageResolverError::Corrupt(key));
        }
    }
    Ok(())
}

fn expected_ancestor_kinds(
    kind: OrganizationNodeKind,
    lineage_length: usize,
) -> Option<&'static [OrganizationNodeKind]> {
    use OrganizationNodeKind::{Branch, Company, Department, Holding};

    match (kind, lineage_length) {
        (OrganizationNodeKind::Holding, 0) => Some(&[]),
        (OrganizationNodeKind::Company, 1) => Some(&[Holding]),
        (OrganizationNodeKind::Headquarters | OrganizationNodeKind::Branch, 2) => {
            Some(&[Holding, Company])
        }
        (OrganizationNodeKind::Department, 2) => Some(&[Holding, Company]),
        (OrganizationNodeKind::Department, 3) => Some(&[Holding, Company, Branch]),
        (OrganizationNodeKind::Team, 3) => Some(&[Holding, Company, Department]),
        (OrganizationNodeKind::Team, 4) => Some(&[Holding, Company, Branch, Department]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn key(node: u128) -> OrganizationLineageKey {
        OrganizationLineageKey::try_new(id(1), id(node)).unwrap()
    }

    fn witness(
        node: u128,
        kind: OrganizationNodeKind,
        parent: Option<u128>,
        lineage: &[u128],
    ) -> OrganizationLineageWitness {
        witness_with_status(node, kind, OrganizationNodeStatus::Active, parent, lineage)
    }

    fn witness_with_status(
        node: u128,
        kind: OrganizationNodeKind,
        status: OrganizationNodeStatus,
        parent: Option<u128>,
        lineage: &[u128],
    ) -> OrganizationLineageWitness {
        OrganizationLineageWitness::try_new(
            key(node),
            kind,
            status,
            parent.map(id),
            lineage.iter().copied().map(id).collect(),
            1,
        )
        .unwrap()
    }

    #[test]
    fn batch_bound_precedes_dedup_and_unique_keys_keep_first_seen_order() {
        assert_eq!(
            PreparedBatch::try_new(&[]),
            Err(OrganizationLineageResolverError::Empty)
        );

        let first = key(4);
        let second = key(5);
        let prepared = PreparedBatch::try_new(&[first, second, first]).unwrap();
        assert_eq!(prepared.order, vec![first, second, first]);
        assert_eq!(prepared.unique, vec![first, second]);

        let oversized = vec![first; MAX_ORGANIZATION_LINEAGE_BATCH_SIZE + 1];
        assert_eq!(
            PreparedBatch::try_new(&oversized),
            Err(OrganizationLineageResolverError::BatchTooLarge {
                maximum: MAX_ORGANIZATION_LINEAGE_BATCH_SIZE,
                actual: MAX_ORGANIZATION_LINEAGE_BATCH_SIZE + 1,
            })
        );
    }

    #[test]
    fn normalized_columns_cannot_reinterpret_a_department_node_as_a_company() {
        let department_key = key(4);
        let authority = AuthorityRow {
            key: department_key,
            aggregate_version: 3,
            kind: "department".into(),
            status: "active".into(),
            parent_node_id: Some(id(3)),
            lineage_node_ids: vec![id(1), id(2), id(3)],
            company_id: Some(id(2)),
            branch_id: Some(id(3)),
            department_id: Some(id(4)),
        };
        let resolved = authority.try_into_witness().unwrap();
        assert_eq!(resolved.company_id().map(|id| id.as_uuid()), Some(id(2)));
        assert_eq!(resolved.department_id().map(|id| id.as_uuid()), Some(id(4)));

        let corrupt = AuthorityRow {
            key: department_key,
            aggregate_version: 3,
            kind: "department".into(),
            status: "active".into(),
            parent_node_id: Some(id(3)),
            lineage_node_ids: vec![id(1), id(2), id(3)],
            company_id: Some(id(4)),
            branch_id: Some(id(3)),
            department_id: Some(id(4)),
        };
        assert_eq!(
            corrupt.try_into_witness(),
            Err(OrganizationLineageResolverError::Corrupt(department_key))
        );
    }

    #[test]
    fn complete_ancestor_chain_is_required_and_kind_checked() {
        let holding = witness(1, OrganizationNodeKind::Holding, None, &[]);
        let company = witness(2, OrganizationNodeKind::Company, Some(1), &[1]);
        let branch = witness(3, OrganizationNodeKind::Branch, Some(2), &[1, 2]);
        let department = witness(4, OrganizationNodeKind::Department, Some(3), &[1, 2, 3]);
        let cache = [holding, company, branch, department.clone()]
            .into_iter()
            .map(|value| (value.key(), value))
            .collect();
        assert_eq!(validate_ancestor_chain(&department, &cache), Ok(()));

        let mut missing = cache.clone();
        missing.remove(&key(3));
        assert_eq!(
            validate_ancestor_chain(&department, &missing),
            Err(OrganizationLineageResolverError::Missing(key(3)))
        );

        let headquarters = witness(3, OrganizationNodeKind::Headquarters, Some(2), &[1, 2]);
        let mut wrong_kind = cache;
        wrong_kind.insert(headquarters.key(), headquarters);
        assert_eq!(
            validate_ancestor_chain(&department, &wrong_kind),
            Err(OrganizationLineageResolverError::Corrupt(key(4)))
        );

        let inactive_branch = witness_with_status(
            3,
            OrganizationNodeKind::Branch,
            OrganizationNodeStatus::Suspended,
            Some(2),
            &[1, 2],
        );
        let mut inactive_ancestor = wrong_kind;
        inactive_ancestor.insert(inactive_branch.key(), inactive_branch);
        assert_eq!(
            validate_ancestor_chain(&department, &inactive_ancestor),
            Err(OrganizationLineageResolverError::Corrupt(key(4)))
        );
    }

    #[test]
    fn suspended_target_still_requires_an_active_complete_ancestor_chain() {
        let holding = witness(1, OrganizationNodeKind::Holding, None, &[]);
        let company = witness(2, OrganizationNodeKind::Company, Some(1), &[1]);
        let branch = witness_with_status(
            3,
            OrganizationNodeKind::Branch,
            OrganizationNodeStatus::Suspended,
            Some(2),
            &[1, 2],
        );
        let cache = [holding.clone(), company.clone(), branch.clone()]
            .into_iter()
            .map(|value| (value.key(), value))
            .collect();
        assert_eq!(validate_ancestor_chain(&branch, &cache), Ok(()));

        let suspended_holding = witness_with_status(
            1,
            OrganizationNodeKind::Holding,
            OrganizationNodeStatus::Suspended,
            None,
            &[],
        );
        let inactive_ancestor = [suspended_holding, company, branch.clone()]
            .into_iter()
            .map(|value| (value.key(), value))
            .collect();
        assert_eq!(
            validate_ancestor_chain(&branch, &inactive_ancestor),
            Err(OrganizationLineageResolverError::Corrupt(key(3)))
        );
    }

    #[test]
    fn authoritative_containment_binds_current_and_proposed_placements() {
        let team = witness_with_status(
            5,
            OrganizationNodeKind::Team,
            OrganizationNodeStatus::Suspended,
            Some(4),
            &[1, 2, 3, 4],
        );
        let placement = OrganizationPlacement::Team {
            holding_id: id(1),
            company_id: id(2),
            branch_id: Some(id(3)),
            department_id: id(4),
        };
        assert!(witness_matches_placement(
            &team,
            OrganizationNodeId::try_from_uuid(id(5)).unwrap(),
            &placement
        ));
        assert!(structurally_contains(
            &AuthorityScope::Branch {
                holding_id: id(1),
                company_id: id(2),
                branch_id: id(3),
            },
            &team,
        ));
        assert!(structurally_contains(
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(4),
            },
            &team,
        ));

        let stale = OrganizationPlacement::Team {
            holding_id: id(1),
            company_id: id(2),
            branch_id: Some(id(3)),
            department_id: id(9),
        };
        assert!(!witness_matches_placement(
            &team,
            OrganizationNodeId::try_from_uuid(id(5)).unwrap(),
            &stale,
        ));

        let department = witness(4, OrganizationNodeKind::Department, Some(3), &[1, 2, 3]);
        assert!(witness_matches_parent(
            &department,
            OrganizationNodeId::try_from_uuid(id(5)).unwrap(),
            &placement,
        ));
        assert!(!witness_matches_parent(
            &department,
            OrganizationNodeId::try_from_uuid(id(5)).unwrap(),
            &stale,
        ));
    }

    #[test]
    fn resolver_query_is_one_bounded_batch_and_locks_the_witness_rows() {
        assert!(LINEAGE_BATCH_QUERY.contains("UNNEST($1::uuid[],$2::uuid[])"));
        assert!(LINEAGE_BATCH_QUERY.contains("UNNEST(target.lineage_node_ids)"));
        assert!(LINEAGE_BATCH_QUERY.contains("FOR SHARE OF authority"));
        assert!(!LINEAGE_BATCH_QUERY.contains("company_id=ANY"));
    }
}
