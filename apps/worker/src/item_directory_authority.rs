//! Holding-bound authority evidence projection and finite grant reconciliation.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use erpx_collaboration::Audience;
use erpx_collaboration_sqlx::ItemDirectoryProjectionAdapter;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionPool, WritePool};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityEvidenceFact,
    AuthorityPrincipalKind, AuthorityRealm, AuthoritySharingDirection, AuthoritySharingStatus,
    EffectiveGrantRevoked, HoldingItemDirectoryFact, ItemDescriptor, ItemDirectoryFact,
    RecordPolicyPrincipalKind, RecordPolicyRealm, RecordVersionRef, StableRecordRef,
    organization::{
        self, ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE, ORGANIZATION_LINEAGE_CHANGED_VERSION,
        OrganizationLineageKey, OrganizationLineageWitness, OrganizationNodeKind,
        OrganizationNodeStatus, contains_scope,
    },
    record_policy,
};
use erpx_kernel::{AuthorityScope, DataClassification, HoldingId, PrincipalId, RecordId};
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    IamAuthorityEvidence, MembershipEvidence, OrganizationScopeEvidence,
    OrganizationScopeEvidenceKind, RecordGrantDecisionProducer, RecordGrantEvaluation,
};

pub const MAX_AUTHORITY_RECONCILIATION_ROWS: i64 = 10_000;

#[derive(Debug, Clone)]
pub struct HoldingAuthorityEvidenceProjection {
    holding_id: HoldingId,
    pool: ProjectionPool,
    authority_write: WritePool,
}

impl HoldingAuthorityEvidenceProjection {
    pub const fn new(
        holding_id: HoldingId,
        pool: ProjectionPool,
        authority_write: WritePool,
    ) -> Self {
        Self {
            holding_id,
            pool,
            authority_write,
        }
    }

    pub async fn apply(&self, event: &ProjectionEvent) -> Result<(), AuthorityProjectionError> {
        let fact = decode_authority_evidence(event)?;
        let mut transaction = self.pool.begin().await.map_err(database)?;
        match fact {
            AuthorityEvidenceFact::Principal(value) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_read.principals
                        (holding_id,principal_id,principal_kind,realm,principal_scope,active,
                         principal_version,source_event_position,changed_at,projected_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now())
                     ON CONFLICT (holding_id,principal_id) DO UPDATE SET
                        principal_kind=EXCLUDED.principal_kind,realm=EXCLUDED.realm,
                        principal_scope=EXCLUDED.principal_scope,active=EXCLUDED.active,
                        principal_version=EXCLUDED.principal_version,
                        source_event_position=EXCLUDED.source_event_position,
                        changed_at=EXCLUDED.changed_at,projected_at=now()
                     WHERE erpx_authority_read.principals.holding_id=$1
                       AND erpx_authority_read.principals.source_event_position
                           < EXCLUDED.source_event_position",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.principal_id.as_uuid())
                .bind(principal_kind_name(value.principal_kind))
                .bind(realm_name(value.realm))
                .bind(scope_json(&value.scope)?)
                .bind(value.active)
                .bind(to_i64(value.principal_version)?)
                .bind(to_i64(event.global_sequence)?)
                .bind(value.changed_at)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;

                let assignment_ids = value
                    .assignments
                    .iter()
                    .map(|assignment| assignment.assignment_id)
                    .collect::<Vec<_>>();
                sqlx::query(
                    "DELETE FROM erpx_authority_read.assignments
                     WHERE holding_id=$1 AND principal_id=$2
                       AND NOT (assignment_id=ANY($3))
                       AND source_event_position < $4",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.principal_id.as_uuid())
                .bind(&assignment_ids)
                .bind(to_i64(event.global_sequence)?)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
                for assignment in value.assignments {
                    sqlx::query(
                        "INSERT INTO erpx_authority_read.assignments
                            (holding_id,assignment_id,principal_id,role_id,assignment_scope,
                             assignment_version,valid_from,valid_until,revoked_at,
                             source_event_position,changed_at,projected_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now())
                         ON CONFLICT (holding_id,assignment_id) DO UPDATE SET
                            role_id=EXCLUDED.role_id,assignment_scope=EXCLUDED.assignment_scope,
                            assignment_version=EXCLUDED.assignment_version,
                            valid_from=EXCLUDED.valid_from,valid_until=EXCLUDED.valid_until,
                            revoked_at=EXCLUDED.revoked_at,
                            source_event_position=EXCLUDED.source_event_position,
                            changed_at=EXCLUDED.changed_at,projected_at=now()
                         WHERE erpx_authority_read.assignments.holding_id=$1
                           AND erpx_authority_read.assignments.principal_id=EXCLUDED.principal_id
                           AND erpx_authority_read.assignments.source_event_position
                               < EXCLUDED.source_event_position",
                    )
                    .bind(self.holding_id.as_uuid())
                    .bind(assignment.assignment_id)
                    .bind(value.principal_id.as_uuid())
                    .bind(assignment.role_id)
                    .bind(scope_json(&assignment.scope)?)
                    .bind(to_i64(assignment.assignment_version)?)
                    .bind(assignment.valid_from)
                    .bind(assignment.valid_until)
                    .bind(assignment.revoked_at)
                    .bind(to_i64(event.global_sequence)?)
                    .bind(value.changed_at)
                    .execute(&mut *transaction)
                    .await
                    .map_err(database)?;
                }

                let member_ids = value
                    .group_memberships
                    .iter()
                    .map(|membership| membership.member_id.as_uuid())
                    .collect::<Vec<_>>();
                sqlx::query(
                    "DELETE FROM erpx_authority_read.group_memberships
                     WHERE holding_id=$1 AND group_id=$2
                       AND NOT (member_principal_id=ANY($3))
                       AND source_event_position < $4",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.principal_id.as_uuid())
                .bind(&member_ids)
                .bind(to_i64(event.global_sequence)?)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
                for membership in value.group_memberships {
                    sqlx::query(
                        "INSERT INTO erpx_authority_read.group_memberships
                            (holding_id,group_id,member_principal_id,group_kind,group_version,
                             membership_version,membership_scope,valid_from,valid_until,revoked_at,
                             source_event_position,changed_at,projected_at)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())
                         ON CONFLICT (holding_id,group_id,member_principal_id) DO UPDATE SET
                            group_kind=EXCLUDED.group_kind,
                            group_version=EXCLUDED.group_version,
                            membership_version=EXCLUDED.membership_version,
                            membership_scope=EXCLUDED.membership_scope,
                            valid_from=EXCLUDED.valid_from,valid_until=EXCLUDED.valid_until,
                            revoked_at=EXCLUDED.revoked_at,
                            source_event_position=EXCLUDED.source_event_position,
                            changed_at=EXCLUDED.changed_at,projected_at=now()
                         WHERE erpx_authority_read.group_memberships.holding_id=$1
                           AND erpx_authority_read.group_memberships.group_id=EXCLUDED.group_id
                           AND erpx_authority_read.group_memberships.source_event_position
                               < EXCLUDED.source_event_position",
                    )
                    .bind(self.holding_id.as_uuid())
                    .bind(membership.group_id.as_uuid())
                    .bind(membership.member_id.as_uuid())
                    .bind(principal_kind_name(membership.group_kind))
                    .bind(to_i64(membership.group_version)?)
                    .bind(to_i64(membership.membership_version)?)
                    .bind(scope_json(&membership.scope)?)
                    .bind(membership.valid_from)
                    .bind(membership.valid_until)
                    .bind(membership.revoked_at)
                    .bind(to_i64(event.global_sequence)?)
                    .bind(membership.changed_at)
                    .execute(&mut *transaction)
                    .await
                    .map_err(database)?;
                }
            }
            AuthorityEvidenceFact::Role(value) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_read.roles
                        (holding_id,role_id,role_scope,role_version,permissions,
                         classification_ceiling,active,source_event_position,changed_at,projected_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now())
                     ON CONFLICT (holding_id,role_id) DO UPDATE SET
                        role_scope=EXCLUDED.role_scope,role_version=EXCLUDED.role_version,
                        permissions=EXCLUDED.permissions,
                        classification_ceiling=EXCLUDED.classification_ceiling,
                        active=EXCLUDED.active,
                        source_event_position=EXCLUDED.source_event_position,
                        changed_at=EXCLUDED.changed_at,projected_at=now()
                     WHERE erpx_authority_read.roles.holding_id=$1
                       AND erpx_authority_read.roles.source_event_position
                           < EXCLUDED.source_event_position",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.role_id)
                .bind(scope_json(&value.scope)?)
                .bind(to_i64(value.role_version)?)
                .bind(value.permissions)
                .bind(classification_name(value.classification_ceiling))
                .bind(value.active)
                .bind(to_i64(event.global_sequence)?)
                .bind(value.changed_at)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
            }
            AuthorityEvidenceFact::GroupMembership(value) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_read.group_memberships
                        (holding_id,group_id,member_principal_id,group_kind,group_version,
                         membership_version,membership_scope,valid_from,valid_until,revoked_at,
                         source_event_position,changed_at,projected_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())
                     ON CONFLICT (holding_id,group_id,member_principal_id) DO UPDATE SET
                        group_kind=EXCLUDED.group_kind,group_version=EXCLUDED.group_version,
                        membership_version=EXCLUDED.membership_version,
                        membership_scope=EXCLUDED.membership_scope,
                        valid_from=EXCLUDED.valid_from,valid_until=EXCLUDED.valid_until,
                        revoked_at=EXCLUDED.revoked_at,
                        source_event_position=EXCLUDED.source_event_position,
                        changed_at=EXCLUDED.changed_at,projected_at=now()
                     WHERE erpx_authority_read.group_memberships.holding_id=$1
                       AND erpx_authority_read.group_memberships.source_event_position
                           < EXCLUDED.source_event_position",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.group_id.as_uuid())
                .bind(value.member_id.as_uuid())
                .bind(principal_kind_name(value.group_kind))
                .bind(to_i64(value.group_version)?)
                .bind(to_i64(value.membership_version)?)
                .bind(scope_json(&value.scope)?)
                .bind(value.valid_from)
                .bind(value.valid_until)
                .bind(value.revoked_at)
                .bind(to_i64(event.global_sequence)?)
                .bind(value.changed_at)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
            }
            AuthorityEvidenceFact::OrganizationSharing(value) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_read.organization_sharing_agreements
                        (holding_id,sharing_agreement_id,agreement_version,source_scope,
                         target_scope,direction,permits_open_record,source_approved,
                         target_approved,valid_from,valid_until,revoked_at,sharing_status,
                         source_event_position,changed_at,projected_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now())
                     ON CONFLICT (holding_id,sharing_agreement_id) DO UPDATE SET
                        agreement_version=EXCLUDED.agreement_version,
                        source_scope=EXCLUDED.source_scope,target_scope=EXCLUDED.target_scope,
                        direction=EXCLUDED.direction,
                        permits_open_record=EXCLUDED.permits_open_record,
                        source_approved=EXCLUDED.source_approved,
                        target_approved=EXCLUDED.target_approved,
                        valid_from=EXCLUDED.valid_from,valid_until=EXCLUDED.valid_until,
                        revoked_at=EXCLUDED.revoked_at,sharing_status=EXCLUDED.sharing_status,
                        source_event_position=EXCLUDED.source_event_position,
                        changed_at=EXCLUDED.changed_at,projected_at=now()
                     WHERE erpx_authority_read.organization_sharing_agreements.holding_id=$1
                       AND erpx_authority_read.organization_sharing_agreements.source_event_position
                           < EXCLUDED.source_event_position",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.sharing_agreement_id)
                .bind(to_i64(value.agreement_version)?)
                .bind(scope_json(&value.source_scope)?)
                .bind(scope_json(&value.target_scope)?)
                .bind(sharing_direction_name(value.direction))
                .bind(value.permits_open_record)
                .bind(value.source_approved)
                .bind(value.target_approved)
                .bind(value.valid_from)
                .bind(value.valid_until)
                .bind(value.revoked_at)
                .bind(sharing_status_name(value.status))
                .bind(to_i64(event.global_sequence)?)
                .bind(value.changed_at)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
            }
            AuthorityEvidenceFact::CustomerPortalBinding(value) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_read.customer_portal_principal_bindings
                        (holding_id,binding_id,binding_version,customer_record_id,
                         customer_record_version,portal_principal_id,principal_kind,portal_realm,
                         binding_scope,verified_identity_digest,verified_identity_version,
                         permitted_use_cases,valid_from,valid_until,proposed_by,approved_by,
                         ended_at,binding_status,source_event_position,changed_at,projected_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,'customer_portal',$8,$9,$10,$11,$12,$13,
                             $14,$15,$16,$17,$18,$19,now())
                     ON CONFLICT (holding_id,binding_id) DO UPDATE SET
                        binding_version=EXCLUDED.binding_version,
                        customer_record_id=EXCLUDED.customer_record_id,
                        customer_record_version=EXCLUDED.customer_record_version,
                        portal_principal_id=EXCLUDED.portal_principal_id,
                        principal_kind=EXCLUDED.principal_kind,
                        binding_scope=EXCLUDED.binding_scope,
                        verified_identity_digest=EXCLUDED.verified_identity_digest,
                        verified_identity_version=EXCLUDED.verified_identity_version,
                        permitted_use_cases=EXCLUDED.permitted_use_cases,
                        valid_from=EXCLUDED.valid_from,valid_until=EXCLUDED.valid_until,
                        approved_by=EXCLUDED.approved_by,ended_at=EXCLUDED.ended_at,
                        binding_status=EXCLUDED.binding_status,
                        source_event_position=EXCLUDED.source_event_position,
                        changed_at=EXCLUDED.changed_at,projected_at=now()
                     WHERE erpx_authority_read.customer_portal_principal_bindings.holding_id=$1
                       AND erpx_authority_read.customer_portal_principal_bindings.source_event_position
                           < EXCLUDED.source_event_position",
                )
                .bind(self.holding_id.as_uuid())
                .bind(value.binding_id)
                .bind(to_i64(value.binding_version)?)
                .bind(value.customer.record().id().as_uuid())
                .bind(to_i64(value.customer.version())?)
                .bind(value.portal_principal_id.as_uuid())
                .bind(principal_kind_name(value.principal_kind))
                .bind(scope_json(&value.scope)?)
                .bind(value.verified_identity_digest)
                .bind(to_i64(value.verified_identity_version)?)
                .bind(
                    value
                        .permitted_use_cases
                        .into_iter()
                        .map(portal_use_case_name)
                        .collect::<Vec<_>>(),
                )
                .bind(value.valid_from)
                .bind(value.valid_until)
                .bind(value.proposed_by.as_uuid())
                .bind(value.approved_by.map(PrincipalId::as_uuid))
                .bind(value.ended_at)
                .bind(portal_binding_status_name(value.status))
                .bind(to_i64(event.global_sequence)?)
                .bind(value.changed_at)
                .execute(&mut *transaction)
                .await
                .map_err(database)?;
            }
        }
        transaction.commit().await.map_err(database)
    }

    /// Projects only the Organization-owned, version-pinned lineage witness.
    /// The cache is disposable and never becomes Organization command
    /// authority; monotonic aggregate and outbox positions prevent an older
    /// placement from replacing the current witness during replay.
    pub async fn apply_organization_lineage(
        &self,
        event: &ProjectionEvent,
    ) -> Result<(), AuthorityProjectionError> {
        let witness = decode_organization_lineage(event)?;
        self.apply_organization_lineage_witness(event, witness)
            .await
    }

    /// Rebuild compatibility for Organization nodes committed before the
    /// dedicated lineage fact existed. The owner adapter validates/upcasts the
    /// snapshot first; this projector persists only the neutral witness.
    pub async fn apply_organization_snapshot_lineage(
        &self,
        event: &ProjectionEvent,
    ) -> Result<(), AuthorityProjectionError> {
        let witness = erpx_organization_sqlx::lineage_witness_from_snapshot_event(event)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
        self.apply_organization_lineage_witness(event, witness)
            .await
    }

    async fn apply_organization_lineage_witness(
        &self,
        event: &ProjectionEvent,
        witness: OrganizationLineageWitness,
    ) -> Result<(), AuthorityProjectionError> {
        if witness.key().holding_id() != self.holding_id {
            return Err(AuthorityProjectionError::EnvelopeMismatch);
        }
        let key = witness.key();
        let lineage = witness
            .lineage_node_ids()
            .iter()
            .map(|node| node.as_uuid())
            .collect::<Vec<_>>();
        let mut transaction = self.pool.begin().await.map_err(database)?;
        sqlx::query(
            "INSERT INTO erpx_authority_read.organization_nodes
                (holding_id,node_id,node_kind,node_status,parent_node_id,lineage_node_ids,
                 aggregate_version,source_event_position,changed_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now())
             ON CONFLICT (holding_id,node_id) DO UPDATE SET
                node_kind=EXCLUDED.node_kind,node_status=EXCLUDED.node_status,
                parent_node_id=EXCLUDED.parent_node_id,
                lineage_node_ids=EXCLUDED.lineage_node_ids,
                aggregate_version=EXCLUDED.aggregate_version,
                source_event_position=EXCLUDED.source_event_position,
                changed_at=EXCLUDED.changed_at,projected_at=now()
             WHERE erpx_authority_read.organization_nodes.holding_id=$1
               AND erpx_authority_read.organization_nodes.aggregate_version
                   < EXCLUDED.aggregate_version
               AND erpx_authority_read.organization_nodes.source_event_position
                   < EXCLUDED.source_event_position",
        )
        .bind(self.holding_id.as_uuid())
        .bind(key.node_id().as_uuid())
        .bind(organization_node_kind_name(witness.kind()))
        .bind(organization_node_status_name(witness.status()))
        .bind(witness.parent_node_id().map(|node| node.as_uuid()))
        .bind(lineage)
        .bind(to_i64(witness.aggregate_version())?)
        .bind(to_i64(event.global_sequence)?)
        .bind(event.occurred_at)
        .execute(&mut *transaction)
        .await
        .map_err(database)?;
        transaction.commit().await.map_err(database)
    }

    pub async fn reconcile_grants(
        &self,
        directory: &ItemDirectoryProjectionAdapter,
        source_event_position: u64,
        now: DateTime<Utc>,
    ) -> Result<AuthorityGrantReconciliationReport, AuthorityProjectionError> {
        if source_event_position == 0 {
            return Ok(AuthorityGrantReconciliationReport::default());
        }
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let descriptor_rows = sqlx::query(
            "SELECT descriptor FROM erpx_collaboration_read.item_directory_descriptors
             WHERE holding_id=$1 AND tombstoned_at IS NULL
             ORDER BY record_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let direct_rows = sqlx::query(
            "SELECT p.principal_id,p.principal_kind,p.realm,p.principal_version,
                    a.assignment_scope,a.assignment_version,a.valid_from,a.valid_until,
                    a.revoked_at,r.role_version,r.permissions,r.classification_ceiling
             FROM erpx_authority_read.principals p
             JOIN erpx_authority_read.assignments a
               ON a.holding_id=p.holding_id AND a.principal_id=p.principal_id
             JOIN erpx_authority_read.roles r
               ON r.holding_id=a.holding_id AND r.role_id=a.role_id
             WHERE p.holding_id=$1 AND p.active AND r.active
             ORDER BY p.principal_id,a.assignment_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let group_rows = sqlx::query(
            "SELECT member.principal_id,member.principal_kind,member.realm,
                    member.principal_version,a.assignment_scope,a.assignment_version,
                    a.valid_from AS assignment_valid_from,
                    a.valid_until AS assignment_valid_until,a.revoked_at AS assignment_revoked_at,
                    r.role_version,r.permissions,r.classification_ceiling,
                    gm.group_kind,gm.membership_version,gm.valid_until AS membership_valid_until,
                    gm.revoked_at AS membership_revoked_at
             FROM erpx_authority_read.group_memberships gm
             JOIN erpx_authority_read.principals group_principal
               ON group_principal.holding_id=gm.holding_id
              AND group_principal.principal_id=gm.group_id
             JOIN erpx_authority_read.principals member
               ON member.holding_id=gm.holding_id
              AND member.principal_id=gm.member_principal_id
             JOIN erpx_authority_read.assignments a
               ON a.holding_id=gm.holding_id AND a.principal_id=gm.group_id
             JOIN erpx_authority_read.roles r
               ON r.holding_id=a.holding_id AND r.role_id=a.role_id
             WHERE gm.holding_id=$1 AND group_principal.active AND member.active AND r.active
               AND gm.group_kind='human_group'
             ORDER BY member.principal_id,gm.group_id,a.assignment_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let sharing_rows = sqlx::query(
            "SELECT sharing_agreement_id,agreement_version,source_scope,target_scope,direction,
                    permits_open_record,source_approved,target_approved,valid_from,valid_until,
                    revoked_at,sharing_status
             FROM erpx_authority_read.organization_sharing_agreements
             WHERE holding_id=$1 ORDER BY sharing_agreement_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let organization_node_rows = sqlx::query(
            "SELECT node_id,node_kind,node_status,parent_node_id,lineage_node_ids,
                    aggregate_version,source_event_position
             FROM erpx_authority_read.organization_nodes
             WHERE holding_id=$1 ORDER BY node_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let grant_rows = sqlx::query(
            "SELECT record_id,principal_id,authorization_policy_id,policy_version
             FROM erpx_collaboration_read.item_directory_effective_grants
             WHERE holding_id=$1 ORDER BY record_id,principal_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        let portal_binding_rows = sqlx::query(
            "SELECT binding_id,binding_version,customer_record_id,customer_record_version,
                    portal_principal_id,principal_kind,binding_scope,verified_identity_digest,
                    verified_identity_version,permitted_use_cases,valid_from,valid_until,
                    proposed_by,approved_by,ended_at,binding_status
             FROM erpx_authority_read.customer_portal_principal_bindings
             WHERE holding_id=$1 ORDER BY binding_id LIMIT $2",
        )
        .bind(self.holding_id.as_uuid())
        .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database)?;
        transaction.commit().await.map_err(database)?;

        let (collaboration_rows, external_owner_rows, field_service_rows) = {
            let mut transaction = self.authority_write.begin().await.map_err(database)?;
            let collaboration_rows = sqlx::query(
                "SELECT t.thread_id::text AS record_id,t.anchor_record_id,
                        a.audience_document,'thread'::text AS record_kind
                 FROM erpx_collaboration.threads t
                 JOIN erpx_collaboration.thread_audiences a
                   ON a.holding_id=t.holding_id AND a.thread_id=t.thread_id
                 WHERE t.holding_id=$1
                 UNION ALL
                 SELECT e.entity_id AS record_id,t.anchor_record_id,
                        a.audience_document,e.entity_kind AS record_kind
                 FROM erpx_collaboration.thread_entities e
                 JOIN erpx_collaboration.threads t
                   ON t.holding_id=e.holding_id AND t.thread_id=e.thread_id
                 JOIN erpx_collaboration.thread_audiences a
                   ON a.holding_id=e.holding_id AND a.thread_id=e.thread_id
                 WHERE e.holding_id=$1
                   AND e.entity_kind IN ('moderation_case','promotion')
                 ORDER BY record_id LIMIT $2",
            )
            .bind(self.holding_id.as_uuid())
            .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
            .fetch_all(&mut *transaction)
            .await
            .map_err(database)?;
            let external_owner_rows = sqlx::query(
                "SELECT cart_id AS record_id,'commerce.cart'::text AS record_type,
                        owner_principal_id,owner_realm
                 FROM erpx_ecommerce_write.carts WHERE holding_id=$1
                 UNION ALL
                 SELECT checkout_id,'commerce.order',owner_principal_id,owner_realm
                 FROM erpx_ecommerce_write.checkouts WHERE holding_id=$1
                 UNION ALL
                 SELECT return_id,'commerce.return',owner_principal_id,owner_realm
                 FROM erpx_ecommerce_write.returns WHERE holding_id=$1
                 UNION ALL
                 SELECT candidate_id,'talent.candidate',candidate_principal_id,'candidate'
                 FROM erpx_talent_write.candidates WHERE holding_id=$1
                 UNION ALL
                 SELECT application_id,'talent.application',candidate_principal_id,'candidate'
                 FROM erpx_talent_write.applications WHERE holding_id=$1
                 UNION ALL
                 SELECT offer_id,'talent.offer',candidate_principal_id,'candidate'
                 FROM erpx_talent_write.offers WHERE holding_id=$1
                 UNION ALL
                 SELECT record_id,'talent.interview',candidate_principal_id,'candidate'
                 FROM erpx_talent_write.governed_records
                 WHERE holding_id=$1 AND record_kind='talent_interview'
                 ORDER BY record_id LIMIT $2",
            )
            .bind(self.holding_id.as_uuid())
            .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
            .fetch_all(&mut *transaction)
            .await
            .map_err(database)?;
            let field_service_rows = sqlx::query(
                "SELECT agreement_id AS record_id,'field-service.agreement'::text AS record_type,
                        customer_ref,NULL::jsonb AS subject_ref
                 FROM erpx_field_service_write.agreements WHERE holding_id=$1
                 UNION ALL
                 SELECT record_id,
                        CASE record_kind
                            WHEN 'request' THEN 'field-service.request'
                            WHEN 'visit' THEN 'field-service.visit'
                        END,
                        NULL::jsonb,subject_ref
                 FROM erpx_field_service_write.governed_records
                 WHERE holding_id=$1 AND record_kind IN ('request','visit')
                 ORDER BY record_id LIMIT $2",
            )
            .bind(self.holding_id.as_uuid())
            .bind(MAX_AUTHORITY_RECONCILIATION_ROWS + 1)
            .fetch_all(&mut *transaction)
            .await
            .map_err(database)?;
            transaction.commit().await.map_err(database)?;
            (collaboration_rows, external_owner_rows, field_service_rows)
        };
        for len in [
            descriptor_rows.len(),
            direct_rows.len(),
            group_rows.len(),
            sharing_rows.len(),
            organization_node_rows.len(),
            grant_rows.len(),
            portal_binding_rows.len(),
            collaboration_rows.len(),
            external_owner_rows.len(),
            field_service_rows.len(),
        ] {
            if len > usize::try_from(MAX_AUTHORITY_RECONCILIATION_ROWS).unwrap_or(usize::MAX) {
                return Err(AuthorityProjectionError::FanoutLimit);
            }
        }

        let descriptors = descriptor_rows
            .into_iter()
            .map(|row| {
                serde_json::from_value(row.try_get("descriptor").map_err(database)?)
                    .map_err(|_| AuthorityProjectionError::MalformedEvidence)
            })
            .collect::<Result<Vec<ItemDescriptor>, _>>()?;
        let mut candidates = direct_rows
            .into_iter()
            .map(|row| candidate_from_row(&row, None))
            .collect::<Result<Vec<_>, _>>()?;
        candidates.extend(
            group_rows
                .into_iter()
                .map(|row| {
                    let membership = MembershipEvidence {
                        subject_kind: parse_principal_kind(
                            row.try_get("group_kind").map_err(database)?,
                        )?,
                        version: to_u64(row.try_get("membership_version").map_err(database)?)?,
                        valid_until: row.try_get("membership_valid_until").map_err(database)?,
                        active: row
                            .try_get::<Option<DateTime<Utc>>, _>("membership_revoked_at")
                            .map_err(database)?
                            .is_none(),
                    };
                    candidate_from_row_with_names(
                        &row,
                        "assignment_valid_from",
                        "assignment_valid_until",
                        "assignment_revoked_at",
                        Some(membership),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let sharing = sharing_rows
            .into_iter()
            .map(sharing_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let organization_nodes = organization_node_rows
            .into_iter()
            .map(|row| organization_lineage_from_row(row, self.holding_id, source_event_position))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let collaboration = collaboration_rows
            .into_iter()
            .map(collaboration_authority_from_row)
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let external_owners = external_owner_rows
            .into_iter()
            .map(external_owner_from_row)
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let portal_bindings = portal_binding_rows
            .into_iter()
            .map(portal_binding_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let field_service_customers = field_service_rows
            .into_iter()
            .map(field_service_customer_from_row)
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let producer = RecordGrantDecisionProducer;
        let mut desired = BTreeSet::new();
        let mut changed = Vec::new();
        for descriptor in &descriptors {
            let descriptor_target_lineage =
                target_lineage_for_scope(descriptor.scope(), self.holding_id, &organization_nodes)?;
            let Some(definition) = record_policy(descriptor.record().record_type().as_str()) else {
                continue;
            };
            if descriptor.record().owner_context()
                == erpx_integration_contracts::catalog::ContextId::Collaboration
            {
                let Some(evidence) = collaboration.get(&descriptor.record().id()) else {
                    continue;
                };
                let Some(anchor) = descriptors
                    .iter()
                    .find(|value| value.record().id() == evidence.anchor_record_id)
                else {
                    continue;
                };
                let Some(anchor_definition) = record_policy(anchor.record().record_type().as_str())
                else {
                    continue;
                };
                let anchor_target_lineage =
                    target_lineage_for_scope(anchor.scope(), self.holding_id, &organization_nodes)?;
                let mut principals = BTreeSet::new();
                for candidate in &candidates {
                    if principals.contains(&candidate.evidence.principal_id)
                        || !collaboration_audience_allows(
                            evidence,
                            candidate.evidence.principal_id,
                            candidate.evidence.realm,
                        )
                        || !descriptor
                            .classification()
                            .is_permitted_by(definition.classification_ceiling)
                        || !descriptor
                            .classification()
                            .is_permitted_by(candidate.evidence.classification_ceiling)
                    {
                        continue;
                    }
                    let Some(permission) = candidate
                        .permissions
                        .iter()
                        .find(|permission| {
                            anchor_definition.permits_access(
                                permission,
                                candidate.evidence.principal_kind,
                                candidate.evidence.realm,
                            )
                        })
                        .cloned()
                    else {
                        continue;
                    };
                    let mut iam = candidate.evidence.clone();
                    iam.permission = permission;
                    let evaluation = RecordGrantEvaluation {
                        descriptor: anchor.clone(),
                        target_lineage: anchor_target_lineage.clone(),
                        organization: organization_evidence(
                            anchor,
                            &iam.assignment_scope,
                            anchor_definition.published_to_descendants,
                            &sharing,
                            now,
                        ),
                        iam,
                        evaluated_at: now,
                        decision_source_position: source_event_position,
                    };
                    let ItemDirectoryFact::EffectiveGrantChanged(anchor_grant) = producer
                        .evaluate(&evaluation)
                        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?
                    else {
                        continue;
                    };
                    let grant = erpx_integration_contracts::EffectiveGrantChanged {
                        record: descriptor.record().clone(),
                        principal_id: candidate.evidence.principal_id,
                        authorization_policy: definition.policy_id(),
                        policy_version: definition.version,
                        effective_at: now,
                        expires_at: anchor_grant.expires_at,
                        source_event_position,
                    };
                    ItemDirectoryFact::EffectiveGrantChanged(grant.clone())
                        .validate()
                        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
                    principals.insert(candidate.evidence.principal_id);
                    desired.insert((grant.record.id(), grant.principal_id));
                    changed.push(ItemDirectoryFact::EffectiveGrantChanged(grant));
                }
                continue;
            }
            let mut principals = BTreeSet::new();
            for candidate in candidates.iter().filter(|candidate| {
                candidate
                    .permissions
                    .iter()
                    .any(|permission| definition.accepts_permission(permission))
            }) {
                if principals.contains(&candidate.evidence.principal_id) {
                    continue;
                }
                let organization = organization_evidence(
                    descriptor,
                    &candidate.evidence.assignment_scope,
                    definition.published_to_descendants,
                    &sharing,
                    now,
                );
                let mut iam = candidate.evidence.clone();
                let Some(permission) = candidate
                    .permissions
                    .iter()
                    .find(|permission| {
                        definition.permits_access(
                            permission,
                            candidate.evidence.principal_kind,
                            candidate.evidence.realm,
                        )
                    })
                    .cloned()
                else {
                    continue;
                };
                let portal_expiry = if candidate.evidence.realm == RecordPolicyRealm::CustomerPortal
                    && descriptor.record().owner_context()
                        == erpx_integration_contracts::catalog::ContextId::FieldService
                {
                    let Some(expires_at) = field_service_portal_expiry(
                        descriptor,
                        candidate,
                        &descriptors,
                        &portal_bindings,
                        &field_service_customers,
                        now,
                    ) else {
                        continue;
                    };
                    Some(expires_at)
                } else {
                    if !future_external_owner_allows(
                        descriptor.record().record_type().as_str(),
                        descriptor.record().id(),
                        candidate.evidence.principal_id,
                        candidate.evidence.realm,
                        &permission,
                        &external_owners,
                    ) {
                        continue;
                    }
                    None
                };
                if let Some(expires_at) = portal_expiry {
                    iam.valid_until = iam.valid_until.min(expires_at);
                }
                iam.permission = permission;
                let evaluation = RecordGrantEvaluation {
                    descriptor: descriptor.clone(),
                    iam,
                    target_lineage: descriptor_target_lineage.clone(),
                    organization,
                    evaluated_at: now,
                    decision_source_position: source_event_position,
                };
                if let ItemDirectoryFact::EffectiveGrantChanged(grant) = producer
                    .evaluate(&evaluation)
                    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?
                {
                    principals.insert(candidate.evidence.principal_id);
                    desired.insert((grant.record.id(), grant.principal_id));
                    changed.push(ItemDirectoryFact::EffectiveGrantChanged(grant));
                }
            }
        }
        let mut revoked = Vec::new();
        for row in grant_rows {
            let record_id =
                erpx_kernel::RecordId::try_from_uuid(row.try_get("record_id").map_err(database)?)
                    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
            let principal_id =
                PrincipalId::try_from_uuid(row.try_get("principal_id").map_err(database)?)
                    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
            if !desired.contains(&(record_id, principal_id)) {
                revoked.push(ItemDirectoryFact::EffectiveGrantRevoked(
                    EffectiveGrantRevoked {
                        record_id,
                        principal_id,
                        authorization_policy: erpx_kernel::PolicyId::try_from_uuid(
                            row.try_get("authorization_policy_id").map_err(database)?,
                        )
                        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
                        policy_version: to_u64(row.try_get("policy_version").map_err(database)?)?,
                        source_event_position,
                    },
                ));
            }
        }
        let changed_count = changed.len();
        let revoked_count = revoked.len();
        for fact in changed.into_iter().chain(revoked) {
            let publication = HoldingItemDirectoryFact::try_new(self.holding_id, fact)
                .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
            directory
                .apply_fact(&publication, now)
                .await
                .map_err(|_| AuthorityProjectionError::Unavailable)?;
        }
        Ok(AuthorityGrantReconciliationReport {
            descriptors: descriptors.len(),
            candidates: candidates.len(),
            changed: changed_count,
            revoked: revoked_count,
        })
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    evidence: IamAuthorityEvidence,
    permissions: Vec<String>,
}

#[derive(Debug, Clone)]
struct SharingEvidence {
    id: Uuid,
    version: u64,
    source_scope: AuthorityScope,
    target_scope: AuthorityScope,
    direction: String,
    permits_open_record: bool,
    source_approved: bool,
    target_approved: bool,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    status: String,
}

#[derive(Debug, Clone)]
struct CollaborationAuthorityEvidence {
    anchor_record_id: RecordId,
    audience: Audience,
    internal_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalOwnerRealm {
    Guest,
    Customer,
    Candidate,
}

impl ExternalOwnerRealm {
    fn expected_permission(self, record_type: &str) -> Option<&'static str> {
        match (record_type, self) {
            ("commerce.cart", Self::Guest) => Some("commerce.guest.cart.read"),
            ("commerce.cart", Self::Customer) => Some("commerce.customer.cart.read"),
            ("commerce.order", Self::Guest) => Some("commerce.guest.order.read"),
            ("commerce.order", Self::Customer) => Some("commerce.customer.order.read"),
            ("commerce.return", Self::Guest) => Some("commerce.guest.return.read"),
            ("commerce.return", Self::Customer) => Some("commerce.customer.return.read"),
            (
                "talent.candidate" | "talent.application" | "talent.interview" | "talent.offer",
                Self::Candidate,
            ) => Some("talent.candidate.self.read"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalOwnerEvidence {
    record_type: String,
    principal_id: PrincipalId,
    realm: ExternalOwnerRealm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PortalBindingEvidence {
    binding_id: Uuid,
    version: u64,
    customer: RecordVersionRef,
    principal_id: PrincipalId,
    principal_kind: RecordPolicyPrincipalKind,
    scope: AuthorityScope,
    verified_identity_digest: String,
    verified_identity_version: u64,
    permits_field_service: bool,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    proposed_by: PrincipalId,
    approved_by: Option<PrincipalId>,
    ended_at: Option<DateTime<Utc>>,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldServiceCustomerEvidence {
    record_type: String,
    link: FieldServiceCustomerLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FieldServiceCustomerLink {
    PinnedCustomer(RecordVersionRef),
    Subject(StableRecordRef),
}

fn portal_binding_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PortalBindingEvidence, AuthorityProjectionError> {
    let customer_record = StableRecordRef::try_new(
        RecordId::try_from_uuid(row.try_get("customer_record_id").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        erpx_integration_contracts::catalog::ContextId::CustomerAuthority,
        erpx_integration_contracts::RecordTypeId::try_new("customer.account")
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
    )
    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let customer = RecordVersionRef::try_new(
        customer_record,
        to_u64(row.try_get("customer_record_version").map_err(database)?)?,
    )
    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let use_cases: Vec<String> = row.try_get("permitted_use_cases").map_err(database)?;
    let principal_kind = parse_principal_kind(row.try_get("principal_kind").map_err(database)?)?;
    if !matches!(
        principal_kind,
        RecordPolicyPrincipalKind::Human | RecordPolicyPrincipalKind::Service
    ) {
        return Err(AuthorityProjectionError::MalformedEvidence);
    }
    Ok(PortalBindingEvidence {
        binding_id: row.try_get("binding_id").map_err(database)?,
        version: to_u64(row.try_get("binding_version").map_err(database)?)?,
        customer,
        principal_id: PrincipalId::try_from_uuid(
            row.try_get("portal_principal_id").map_err(database)?,
        )
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        principal_kind,
        scope: serde_json::from_value(row.try_get("binding_scope").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        verified_identity_digest: row.try_get("verified_identity_digest").map_err(database)?,
        verified_identity_version: to_u64(
            row.try_get("verified_identity_version").map_err(database)?,
        )?,
        permits_field_service: use_cases
            .iter()
            .any(|value| value == "field_service_record_access"),
        valid_from: row.try_get("valid_from").map_err(database)?,
        valid_until: row.try_get("valid_until").map_err(database)?,
        proposed_by: PrincipalId::try_from_uuid(row.try_get("proposed_by").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        approved_by: row
            .try_get::<Option<Uuid>, _>("approved_by")
            .map_err(database)?
            .map(PrincipalId::try_from_uuid)
            .transpose()
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        ended_at: row.try_get("ended_at").map_err(database)?,
        status: row.try_get("binding_status").map_err(database)?,
    })
}

fn field_service_customer_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<(RecordId, FieldServiceCustomerEvidence), AuthorityProjectionError> {
    let record_id = RecordId::try_from_uuid(row.try_get("record_id").map_err(database)?)
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let record_type = row.try_get("record_type").map_err(database)?;
    let customer: Option<serde_json::Value> = row.try_get("customer_ref").map_err(database)?;
    let subject: Option<serde_json::Value> = row.try_get("subject_ref").map_err(database)?;
    let link = match (customer, subject) {
        (Some(customer), None) => FieldServiceCustomerLink::PinnedCustomer(
            serde_json::from_value(customer)
                .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        ),
        (None, Some(subject)) => FieldServiceCustomerLink::Subject(
            serde_json::from_value(subject)
                .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        ),
        _ => return Err(AuthorityProjectionError::MalformedEvidence),
    };
    Ok((
        record_id,
        FieldServiceCustomerEvidence { record_type, link },
    ))
}

fn field_service_portal_expiry(
    descriptor: &ItemDescriptor,
    candidate: &Candidate,
    descriptors: &[ItemDescriptor],
    bindings: &[PortalBindingEvidence],
    records: &BTreeMap<RecordId, FieldServiceCustomerEvidence>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if candidate.evidence.realm != RecordPolicyRealm::CustomerPortal
        || !matches!(
            candidate.evidence.principal_kind,
            RecordPolicyPrincipalKind::Human | RecordPolicyPrincipalKind::Service
        )
        || !matches!(
            descriptor.record().record_type().as_str(),
            "field-service.agreement" | "field-service.request" | "field-service.visit"
        )
    {
        return None;
    }
    let customer = resolve_field_service_customer(descriptor, records)?;
    let customer_descriptor = descriptors.iter().find(|value| {
        value.record().id() == customer.record().id()
            && value.record().owner_context()
                == erpx_integration_contracts::catalog::ContextId::CustomerAuthority
            && value.record().record_type().as_str() == "customer.account"
    })?;
    if customer_descriptor.descriptor_version() != customer.version()
        || customer_descriptor.lifecycle_state() != "active"
        || !customer_descriptor.scope().contains(descriptor.scope())
    {
        return None;
    }
    bindings
        .iter()
        .filter(|binding| {
            !binding.binding_id.is_nil()
                && binding.version > 0
                && binding.customer == customer
                && binding.principal_id == candidate.evidence.principal_id
                && binding.principal_kind == candidate.evidence.principal_kind
                && &binding.scope == descriptor.scope()
                && binding.permits_field_service
                && binding.valid_from <= now
                && now < binding.valid_until
                && binding.ended_at.is_none()
                && binding.status == "active"
                && binding.approved_by.is_some()
                && binding.approved_by != Some(binding.proposed_by)
                && binding.verified_identity_version > 0
                && binding.verified_identity_digest.len() == 64
                && binding
                    .verified_identity_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
        })
        .map(|binding| binding.valid_until)
        .max()
}

fn resolve_field_service_customer(
    descriptor: &ItemDescriptor,
    records: &BTreeMap<RecordId, FieldServiceCustomerEvidence>,
) -> Option<RecordVersionRef> {
    let mut record_id = descriptor.record().id();
    let mut expected_type = descriptor.record().record_type().as_str();
    let mut visited = BTreeSet::new();
    for _ in 0..8 {
        if !visited.insert(record_id) {
            return None;
        }
        let evidence = records.get(&record_id)?;
        if evidence.record_type != expected_type {
            return None;
        }
        match &evidence.link {
            FieldServiceCustomerLink::PinnedCustomer(customer) => return Some(customer.clone()),
            FieldServiceCustomerLink::Subject(subject)
                if subject.owner_context()
                    == erpx_integration_contracts::catalog::ContextId::FieldService =>
            {
                record_id = subject.id();
                expected_type = subject.record_type().as_str();
            }
            // An unversioned direct customer subject is insufficient for an
            // external grant: the Field Service owner must pin customer state.
            FieldServiceCustomerLink::Subject(_) => return None,
        }
    }
    None
}

fn external_owner_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<(RecordId, ExternalOwnerEvidence), AuthorityProjectionError> {
    let record_id = RecordId::try_from_uuid(row.try_get("record_id").map_err(database)?)
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let principal_id =
        PrincipalId::try_from_uuid(row.try_get("owner_principal_id").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let record_type = row.try_get("record_type").map_err(database)?;
    let realm = match row
        .try_get::<String, _>("owner_realm")
        .map_err(database)?
        .as_str()
    {
        "guest" => ExternalOwnerRealm::Guest,
        "customer" => ExternalOwnerRealm::Customer,
        "candidate" => ExternalOwnerRealm::Candidate,
        _ => return Err(AuthorityProjectionError::MalformedEvidence),
    };
    Ok((
        record_id,
        ExternalOwnerEvidence {
            record_type,
            principal_id,
            realm,
        },
    ))
}

fn future_external_owner_allows(
    record_type: &str,
    record_id: RecordId,
    principal_id: PrincipalId,
    principal_realm: RecordPolicyRealm,
    permission: &str,
    owners: &BTreeMap<RecordId, ExternalOwnerEvidence>,
) -> bool {
    if principal_realm != RecordPolicyRealm::CustomerPortal {
        return true;
    }
    if record_type == "commerce.catalog-publication" {
        return matches!(
            permission,
            "commerce.customer.catalog.read" | "commerce.guest.catalog.read"
        );
    }
    // Field Service does not yet expose an authoritative customer-account to
    // portal-principal binding. Keep those records neutral rather than infer
    // authority from their organizational scope.
    let Some(owner) = owners.get(&record_id) else {
        return false;
    };
    owner.record_type == record_type
        && owner.principal_id == principal_id
        && owner.realm.expected_permission(record_type) == Some(permission)
}

fn collaboration_authority_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<(RecordId, CollaborationAuthorityEvidence), AuthorityProjectionError> {
    let record_uuid = Uuid::parse_str(
        row.try_get::<String, _>("record_id")
            .map_err(database)?
            .as_str(),
    )
    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let record_id = RecordId::try_from_uuid(record_uuid)
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let anchor_record_id =
        RecordId::try_from_uuid(row.try_get("anchor_record_id").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let audience = serde_json::from_value(row.try_get("audience_document").map_err(database)?)
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let record_kind: String = row.try_get("record_kind").map_err(database)?;
    Ok((
        record_id,
        CollaborationAuthorityEvidence {
            anchor_record_id,
            audience,
            internal_only: record_kind == "moderation_case",
        },
    ))
}

fn collaboration_audience_allows(
    evidence: &CollaborationAuthorityEvidence,
    principal_id: PrincipalId,
    realm: erpx_integration_contracts::RecordPolicyRealm,
) -> bool {
    if evidence.internal_only {
        return matches!(
            realm,
            erpx_integration_contracts::RecordPolicyRealm::InternalWorkforce
                | erpx_integration_contracts::RecordPolicyRealm::PlatformWorkforce
        );
    }
    match &evidence.audience {
        Audience::Internal { principals, .. } => {
            matches!(
                realm,
                erpx_integration_contracts::RecordPolicyRealm::InternalWorkforce
                    | erpx_integration_contracts::RecordPolicyRealm::PlatformWorkforce
            ) && (principals.is_empty()
                || principals
                    .iter()
                    .any(|principal| principal.id() == principal_id))
        }
        Audience::ExternalPortal { participants, .. } => {
            realm == erpx_integration_contracts::RecordPolicyRealm::CustomerPortal
                && participants
                    .iter()
                    .any(|principal| principal.id() == principal_id)
        }
    }
}

fn candidate_from_row(
    row: &sqlx::postgres::PgRow,
    membership: Option<MembershipEvidence>,
) -> Result<Candidate, AuthorityProjectionError> {
    candidate_from_row_with_names(row, "valid_from", "valid_until", "revoked_at", membership)
}

fn candidate_from_row_with_names(
    row: &sqlx::postgres::PgRow,
    valid_from: &str,
    valid_until: &str,
    revoked_at: &str,
    membership: Option<MembershipEvidence>,
) -> Result<Candidate, AuthorityProjectionError> {
    Ok(Candidate {
        evidence: IamAuthorityEvidence {
            principal_id: PrincipalId::try_from_uuid(
                row.try_get("principal_id").map_err(database)?,
            )
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
            principal_kind: parse_principal_kind(row.try_get("principal_kind").map_err(database)?)?,
            realm: parse_realm(row.try_get("realm").map_err(database)?)?,
            principal_active: true,
            principal_version: to_u64(row.try_get("principal_version").map_err(database)?)?,
            role_version: to_u64(row.try_get("role_version").map_err(database)?)?,
            assignment_version: to_u64(row.try_get("assignment_version").map_err(database)?)?,
            permission: String::new(),
            assignment_scope: serde_json::from_value(
                row.try_get("assignment_scope").map_err(database)?,
            )
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
            classification_ceiling: parse_classification(
                row.try_get("classification_ceiling").map_err(database)?,
            )?,
            valid_from: row.try_get(valid_from).map_err(database)?,
            valid_until: row.try_get(valid_until).map_err(database)?,
            revoked_at: row.try_get(revoked_at).map_err(database)?,
            membership,
        },
        permissions: row.try_get("permissions").map_err(database)?,
    })
}

fn sharing_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<SharingEvidence, AuthorityProjectionError> {
    Ok(SharingEvidence {
        id: row.try_get("sharing_agreement_id").map_err(database)?,
        version: to_u64(row.try_get("agreement_version").map_err(database)?)?,
        source_scope: serde_json::from_value(row.try_get("source_scope").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        target_scope: serde_json::from_value(row.try_get("target_scope").map_err(database)?)
            .map_err(|_| AuthorityProjectionError::MalformedEvidence)?,
        direction: row.try_get("direction").map_err(database)?,
        permits_open_record: row.try_get("permits_open_record").map_err(database)?,
        source_approved: row.try_get("source_approved").map_err(database)?,
        target_approved: row.try_get("target_approved").map_err(database)?,
        valid_from: row.try_get("valid_from").map_err(database)?,
        valid_until: row.try_get("valid_until").map_err(database)?,
        revoked_at: row.try_get("revoked_at").map_err(database)?,
        status: row.try_get("sharing_status").map_err(database)?,
    })
}

fn organization_lineage_from_row(
    row: sqlx::postgres::PgRow,
    holding_id: HoldingId,
    source_event_position: u64,
) -> Result<(OrganizationLineageKey, OrganizationLineageWitness), AuthorityProjectionError> {
    let key = OrganizationLineageKey::try_new(
        holding_id.as_uuid(),
        row.try_get("node_id").map_err(database)?,
    )
    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let aggregate_version = to_u64(row.try_get("aggregate_version").map_err(database)?)?;
    let projected_source_position =
        to_u64(row.try_get("source_event_position").map_err(database)?)?;
    if projected_source_position > source_event_position {
        return Err(AuthorityProjectionError::MalformedEvidence);
    }
    let kind = parse_organization_node_kind(row.try_get("node_kind").map_err(database)?)?;
    let status = parse_organization_node_status(row.try_get("node_status").map_err(database)?)?;
    let witness = OrganizationLineageWitness::try_new(
        key,
        kind,
        status,
        row.try_get("parent_node_id").map_err(database)?,
        row.try_get("lineage_node_ids").map_err(database)?,
        aggregate_version,
    )
    .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    Ok((key, witness))
}

fn target_lineage_for_scope(
    scope: &AuthorityScope,
    holding_id: HoldingId,
    organization_nodes: &BTreeMap<OrganizationLineageKey, OrganizationLineageWitness>,
) -> Result<Option<OrganizationLineageWitness>, AuthorityProjectionError> {
    let AuthorityScope::Subtree {
        holding_id: scope_holding_id,
        organization_node_id,
    } = scope
    else {
        return Ok(None);
    };
    if *scope_holding_id != holding_id.as_uuid() {
        return Ok(None);
    }
    let key = OrganizationLineageKey::try_new(*scope_holding_id, *organization_node_id)
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let Some(witness) = organization_nodes.get(&key) else {
        return Ok(None);
    };
    Ok(active_lineage_chain_is_complete(witness, organization_nodes).then(|| witness.clone()))
}

fn active_lineage_chain_is_complete(
    witness: &OrganizationLineageWitness,
    organization_nodes: &BTreeMap<OrganizationLineageKey, OrganizationLineageWitness>,
) -> bool {
    if witness.validate().is_err() || witness.status() != OrganizationNodeStatus::Active {
        return false;
    }
    let Some(expected_kinds) =
        expected_ancestor_kinds(witness.kind(), witness.lineage_node_ids().len())
    else {
        return false;
    };
    if !matches!(
        witness.kind(),
        OrganizationNodeKind::Department | OrganizationNodeKind::Team
    ) || expected_kinds.len() != witness.lineage_node_ids().len()
    {
        return false;
    }

    let holding_id = witness.key().holding_id().as_uuid();
    witness
        .lineage_node_ids()
        .iter()
        .zip(expected_kinds)
        .enumerate()
        .all(|(index, (ancestor_id, expected_kind))| {
            let Ok(ancestor_key) =
                OrganizationLineageKey::try_new(holding_id, ancestor_id.as_uuid())
            else {
                return false;
            };
            organization_nodes
                .get(&ancestor_key)
                .is_some_and(|ancestor| {
                    ancestor.status() == OrganizationNodeStatus::Active
                        && ancestor.kind() == *expected_kind
                        && ancestor.lineage_node_ids() == &witness.lineage_node_ids()[..index]
                })
        })
}

fn expected_ancestor_kinds(
    kind: OrganizationNodeKind,
    lineage_length: usize,
) -> Option<&'static [OrganizationNodeKind]> {
    use OrganizationNodeKind::{Branch, Company, Department, Holding};

    match (kind, lineage_length) {
        (OrganizationNodeKind::Department, 2) => Some(&[Holding, Company]),
        (OrganizationNodeKind::Department, 3) => Some(&[Holding, Company, Branch]),
        (OrganizationNodeKind::Team, 3) => Some(&[Holding, Company, Department]),
        (OrganizationNodeKind::Team, 4) => Some(&[Holding, Company, Branch, Department]),
        _ => None,
    }
}

fn organization_evidence(
    descriptor: &ItemDescriptor,
    actor_scope: &AuthorityScope,
    published_to_descendants: bool,
    sharing: &[SharingEvidence],
    now: DateTime<Utc>,
) -> Option<OrganizationScopeEvidence> {
    if published_to_descendants && contains_scope(descriptor.scope(), actor_scope, None) {
        return Some(OrganizationScopeEvidence {
            kind: OrganizationScopeEvidenceKind::PublishedAncestor,
            actor_scope: actor_scope.clone(),
            record_scope: descriptor.scope().clone(),
            permits_open_record: true,
            version: descriptor.authorization_policy_version(),
            valid_from: now,
            valid_until: now + Duration::minutes(15),
            revoked_at: None,
            sharing_agreement_id: None,
        });
    }
    sharing.iter().find_map(|agreement| {
        let forward =
            &agreement.source_scope == actor_scope && &agreement.target_scope == descriptor.scope();
        let reverse = agreement.direction == "mutual"
            && &agreement.target_scope == actor_scope
            && &agreement.source_scope == descriptor.scope();
        (agreement.status == "active"
            && agreement.permits_open_record
            && agreement.source_approved
            && agreement.target_approved
            && agreement.valid_from <= now
            && now < agreement.valid_until
            && agreement.revoked_at.is_none()
            && (forward || reverse))
            .then(|| OrganizationScopeEvidence {
                kind: OrganizationScopeEvidenceKind::TwoSidedSharingAgreement,
                actor_scope: actor_scope.clone(),
                record_scope: descriptor.scope().clone(),
                permits_open_record: true,
                version: agreement.version,
                valid_from: agreement.valid_from,
                valid_until: agreement.valid_until,
                revoked_at: agreement.revoked_at,
                sharing_agreement_id: Some(agreement.id),
            })
    })
}

pub fn decode_organization_lineage(
    event: &ProjectionEvent,
) -> Result<OrganizationLineageWitness, AuthorityProjectionError> {
    if event.event_type != ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE
        || event.event_version != u32::from(ORGANIZATION_LINEAGE_CHANGED_VERSION)
    {
        return Err(AuthorityProjectionError::UnsupportedContract);
    }
    let payload: organization::Event = serde_json::from_value(event.payload.clone())
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let organization::Event::OrganizationLineageChanged(witness) = payload else {
        return Err(AuthorityProjectionError::MalformedEvidence);
    };
    witness
        .validate()
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let key = witness.key();
    let node_id = key.node_id().as_uuid();
    let expected_partition = format!("organization.node:{}", node_id.simple());
    let expected_scope = organization_lineage_scope(&witness)?;
    if event.context != "organization"
        || event.aggregate_type != "organization_node"
        || event.aggregate_id.as_uuid() != node_id
        || event.aggregate_version != witness.aggregate_version()
        || event.partition != expected_partition
        || event.active_scope != expected_scope
        || event.global_sequence == 0
    {
        return Err(AuthorityProjectionError::EnvelopeMismatch);
    }
    Ok(witness)
}

fn organization_lineage_scope(
    witness: &OrganizationLineageWitness,
) -> Result<AuthorityScope, AuthorityProjectionError> {
    let key = witness.key();
    let holding_id = key.holding_id().as_uuid();
    let node_id = key.node_id().as_uuid();
    match witness.kind() {
        OrganizationNodeKind::Holding => Ok(AuthorityScope::Holding { holding_id }),
        OrganizationNodeKind::Company => Ok(AuthorityScope::Company {
            holding_id,
            company_id: node_id,
        }),
        OrganizationNodeKind::Headquarters => Ok(AuthorityScope::Company {
            holding_id,
            company_id: witness
                .company_id()
                .ok_or(AuthorityProjectionError::MalformedEvidence)?
                .as_uuid(),
        }),
        OrganizationNodeKind::Branch => Ok(AuthorityScope::Branch {
            holding_id,
            company_id: witness
                .company_id()
                .ok_or(AuthorityProjectionError::MalformedEvidence)?
                .as_uuid(),
            branch_id: node_id,
        }),
        OrganizationNodeKind::Department | OrganizationNodeKind::Team => {
            Ok(AuthorityScope::Subtree {
                holding_id,
                organization_node_id: node_id,
            })
        }
    }
}

pub fn decode_authority_evidence(
    event: &ProjectionEvent,
) -> Result<AuthorityEvidenceFact, AuthorityProjectionError> {
    if event.event_type != AUTHORITY_EVIDENCE_EVENT_TYPE
        || event.event_version != AUTHORITY_EVIDENCE_VERSION
    {
        return Err(AuthorityProjectionError::UnsupportedContract);
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum AuthorityEvidenceWire {
        Direct(AuthorityEvidenceFact),
        PayloadKind(AuthorityEvidencePayloadKind),
    }

    #[derive(serde::Deserialize)]
    struct AuthorityEvidencePayloadKind {
        payload_kind: AuthorityEvidencePayloadKindTag,
        payload: AuthorityEvidenceFact,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum AuthorityEvidencePayloadKindTag {
        Authority,
    }

    let fact = match serde_json::from_value::<AuthorityEvidenceWire>(event.payload.clone())
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?
    {
        AuthorityEvidenceWire::Direct(fact) => fact,
        AuthorityEvidenceWire::PayloadKind(AuthorityEvidencePayloadKind {
            payload_kind: AuthorityEvidencePayloadKindTag::Authority,
            payload,
        }) => payload,
    };
    fact.validate()
        .map_err(|_| AuthorityProjectionError::MalformedEvidence)?;
    let matches_envelope = match &fact {
        AuthorityEvidenceFact::Principal(value) => {
            event.context == "identity_access"
                && event.aggregate_type == "access_principal"
                && event.aggregate_id.as_uuid() == value.principal_id.as_uuid()
                && event.aggregate_version == value.principal_version
        }
        AuthorityEvidenceFact::Role(value) => {
            event.context == "identity_access"
                && event.aggregate_type == "access_role"
                && event.aggregate_id.as_uuid() == value.role_id
                && event.aggregate_version == value.role_version
        }
        AuthorityEvidenceFact::GroupMembership(value) => {
            event.context == "identity_access"
                && event.aggregate_type == "authority_group"
                && event.aggregate_id.as_uuid() == value.group_id.as_uuid()
                && event.aggregate_version == value.group_version
        }
        AuthorityEvidenceFact::OrganizationSharing(value) => {
            event.context == "organization"
                && event.aggregate_type == "organization_sharing_agreement"
                && event.aggregate_id.as_uuid() == value.sharing_agreement_id
                && event.aggregate_version == value.agreement_version
        }
        AuthorityEvidenceFact::CustomerPortalBinding(value) => {
            event.context == "customer_authority"
                && event.aggregate_type == "customer_portal_principal_binding"
                && event.aggregate_id.as_uuid() == value.binding_id
                && event.aggregate_version == value.binding_version
        }
    };
    if !matches_envelope || event.global_sequence == 0 {
        return Err(AuthorityProjectionError::EnvelopeMismatch);
    }
    Ok(fact)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthorityGrantReconciliationReport {
    pub descriptors: usize,
    pub candidates: usize,
    pub changed: usize,
    pub revoked: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AuthorityProjectionError {
    #[error("unsupported authority evidence contract")]
    UnsupportedContract,
    #[error("authority evidence is malformed")]
    MalformedEvidence,
    #[error("authority evidence does not match its immutable outbox envelope")]
    EnvelopeMismatch,
    #[error("authority reconciliation exceeded its explicit fanout bound")]
    FanoutLimit,
    #[error("authority evidence projection is unavailable")]
    Unavailable,
}

fn scope_json(scope: &AuthorityScope) -> Result<serde_json::Value, AuthorityProjectionError> {
    serde_json::to_value(scope).map_err(|_| AuthorityProjectionError::MalformedEvidence)
}

fn to_i64(value: u64) -> Result<i64, AuthorityProjectionError> {
    i64::try_from(value).map_err(|_| AuthorityProjectionError::MalformedEvidence)
}

fn to_u64(value: i64) -> Result<u64, AuthorityProjectionError> {
    u64::try_from(value).map_err(|_| AuthorityProjectionError::MalformedEvidence)
}

fn database(_error: impl std::fmt::Display) -> AuthorityProjectionError {
    AuthorityProjectionError::Unavailable
}

const fn principal_kind_name(value: AuthorityPrincipalKind) -> &'static str {
    match value {
        AuthorityPrincipalKind::Human => "human",
        AuthorityPrincipalKind::Service => "service",
        AuthorityPrincipalKind::Agent => "agent",
        AuthorityPrincipalKind::HumanGroup => "human_group",
        AuthorityPrincipalKind::AgentPool => "agent_pool",
    }
}

fn parse_principal_kind(
    value: &str,
) -> Result<erpx_integration_contracts::RecordPolicyPrincipalKind, AuthorityProjectionError> {
    match value {
        "human" => Ok(erpx_integration_contracts::RecordPolicyPrincipalKind::Human),
        "service" => Ok(erpx_integration_contracts::RecordPolicyPrincipalKind::Service),
        "agent" => Ok(erpx_integration_contracts::RecordPolicyPrincipalKind::Agent),
        "human_group" => Ok(erpx_integration_contracts::RecordPolicyPrincipalKind::HumanGroup),
        "agent_pool" => Ok(erpx_integration_contracts::RecordPolicyPrincipalKind::AgentPool),
        _ => Err(AuthorityProjectionError::MalformedEvidence),
    }
}

const fn realm_name(value: AuthorityRealm) -> &'static str {
    match value {
        AuthorityRealm::InternalWorkforce => "internal_workforce",
        AuthorityRealm::PlatformWorkforce => "platform_workforce",
        AuthorityRealm::CustomerPortal => "customer_portal",
    }
}

fn parse_realm(
    value: &str,
) -> Result<erpx_integration_contracts::RecordPolicyRealm, AuthorityProjectionError> {
    match value {
        "internal_workforce" => {
            Ok(erpx_integration_contracts::RecordPolicyRealm::InternalWorkforce)
        }
        "platform_workforce" => {
            Ok(erpx_integration_contracts::RecordPolicyRealm::PlatformWorkforce)
        }
        "customer_portal" => Ok(erpx_integration_contracts::RecordPolicyRealm::CustomerPortal),
        _ => Err(AuthorityProjectionError::MalformedEvidence),
    }
}

const fn classification_name(value: DataClassification) -> &'static str {
    match value {
        DataClassification::Public => "public",
        DataClassification::Internal => "internal",
        DataClassification::Confidential => "confidential",
        DataClassification::Restricted => "restricted",
    }
}

fn parse_classification(value: &str) -> Result<DataClassification, AuthorityProjectionError> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(AuthorityProjectionError::MalformedEvidence),
    }
}

const fn sharing_direction_name(value: AuthoritySharingDirection) -> &'static str {
    match value {
        AuthoritySharingDirection::SourceToTarget => "source_to_target",
        AuthoritySharingDirection::Mutual => "mutual",
    }
}

const fn sharing_status_name(value: AuthoritySharingStatus) -> &'static str {
    match value {
        AuthoritySharingStatus::Proposed => "proposed",
        AuthoritySharingStatus::Active => "active",
        AuthoritySharingStatus::Rejected => "rejected",
        AuthoritySharingStatus::Revoked => "revoked",
        AuthoritySharingStatus::Expired => "expired",
    }
}

const fn organization_node_kind_name(value: OrganizationNodeKind) -> &'static str {
    match value {
        OrganizationNodeKind::Holding => "holding",
        OrganizationNodeKind::Company => "company",
        OrganizationNodeKind::Headquarters => "headquarters",
        OrganizationNodeKind::Branch => "branch",
        OrganizationNodeKind::Department => "department",
        OrganizationNodeKind::Team => "team",
    }
}

fn parse_organization_node_kind(
    value: &str,
) -> Result<OrganizationNodeKind, AuthorityProjectionError> {
    match value {
        "holding" => Ok(OrganizationNodeKind::Holding),
        "company" => Ok(OrganizationNodeKind::Company),
        "headquarters" => Ok(OrganizationNodeKind::Headquarters),
        "branch" => Ok(OrganizationNodeKind::Branch),
        "department" => Ok(OrganizationNodeKind::Department),
        "team" => Ok(OrganizationNodeKind::Team),
        _ => Err(AuthorityProjectionError::MalformedEvidence),
    }
}

const fn organization_node_status_name(value: OrganizationNodeStatus) -> &'static str {
    match value {
        OrganizationNodeStatus::Active => "active",
        OrganizationNodeStatus::Suspended => "suspended",
        OrganizationNodeStatus::Archived => "archived",
    }
}

fn parse_organization_node_status(
    value: &str,
) -> Result<OrganizationNodeStatus, AuthorityProjectionError> {
    match value {
        "active" => Ok(OrganizationNodeStatus::Active),
        "suspended" => Ok(OrganizationNodeStatus::Suspended),
        "archived" => Ok(OrganizationNodeStatus::Archived),
        _ => Err(AuthorityProjectionError::MalformedEvidence),
    }
}

const fn portal_use_case_name(
    value: erpx_integration_contracts::AuthorityPortalUseCase,
) -> &'static str {
    match value {
        erpx_integration_contracts::AuthorityPortalUseCase::FieldServiceRecordAccess => {
            "field_service_record_access"
        }
    }
}

const fn portal_binding_status_name(
    value: erpx_integration_contracts::AuthorityPortalBindingStatus,
) -> &'static str {
    match value {
        erpx_integration_contracts::AuthorityPortalBindingStatus::Proposed => "proposed",
        erpx_integration_contracts::AuthorityPortalBindingStatus::Active => "active",
        erpx_integration_contracts::AuthorityPortalBindingStatus::Rejected => "rejected",
        erpx_integration_contracts::AuthorityPortalBindingStatus::Revoked => "revoked",
        erpx_integration_contracts::AuthorityPortalBindingStatus::Expired => "expired",
    }
}

#[cfg(test)]
mod tests {
    use erpx_collaboration::PrincipalRef as CollaborationPrincipalRef;
    use erpx_integration_contracts::{
        AuthorityAssignmentEvidence, PrincipalAuthorityEvidence, RecordTypeId,
        catalog::ContextId,
        organization::{OrganizationLineageKey, OrganizationLineageWitness, OrganizationNodeKind},
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AggregateId, CommandId, CorrelationId, EventId, PrincipalKind,
    };

    use super::*;

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn lineage_event() -> (ProjectionEvent, OrganizationLineageWitness) {
        let holding_id = id();
        let company_id = id();
        let branch_id = id();
        let department_id = id();
        let team_id = id();
        let witness = OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(holding_id, team_id).unwrap(),
            OrganizationNodeKind::Team,
            OrganizationNodeStatus::Active,
            Some(department_id),
            vec![holding_id, company_id, branch_id, department_id],
            3,
        )
        .unwrap();
        let event = ProjectionEvent {
            event_id: EventId::try_from_uuid(id()).unwrap(),
            global_sequence: 9,
            context: "organization".into(),
            partition: format!("organization.node:{}", team_id.simple()),
            command_id: CommandId::try_from_uuid(id()).unwrap(),
            event_type: ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE.into(),
            event_version: u32::from(ORGANIZATION_LINEAGE_CHANGED_VERSION),
            aggregate_type: "organization_node".into(),
            aggregate_id: AggregateId::try_from_uuid(team_id).unwrap(),
            aggregate_version: 3,
            active_scope: AuthorityScope::Subtree {
                holding_id,
                organization_node_id: team_id,
            },
            actor: ActorAttribution {
                principal_ref: "worker:organization-lineage".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: CorrelationId::try_from_uuid(id()).unwrap(),
            causation_id: None,
            payload: serde_json::to_value(organization::Event::OrganizationLineageChanged(
                witness.clone(),
            ))
            .unwrap(),
            occurred_at: Utc::now(),
        };
        (event, witness)
    }

    fn lineage_id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn cached_witness(
        node_id: u128,
        kind: OrganizationNodeKind,
        status: OrganizationNodeStatus,
        parent_node_id: Option<u128>,
        lineage_node_ids: &[u128],
    ) -> OrganizationLineageWitness {
        OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(lineage_id(1), lineage_id(node_id)).unwrap(),
            kind,
            status,
            parent_node_id.map(lineage_id),
            lineage_node_ids.iter().copied().map(lineage_id).collect(),
            2,
        )
        .unwrap()
    }

    fn cached_lineage_tree() -> BTreeMap<OrganizationLineageKey, OrganizationLineageWitness> {
        [
            cached_witness(
                1,
                OrganizationNodeKind::Holding,
                OrganizationNodeStatus::Active,
                None,
                &[],
            ),
            cached_witness(
                2,
                OrganizationNodeKind::Company,
                OrganizationNodeStatus::Active,
                Some(1),
                &[1],
            ),
            cached_witness(
                3,
                OrganizationNodeKind::Branch,
                OrganizationNodeStatus::Active,
                Some(2),
                &[1, 2],
            ),
            cached_witness(
                4,
                OrganizationNodeKind::Department,
                OrganizationNodeStatus::Active,
                Some(3),
                &[1, 2, 3],
            ),
            cached_witness(
                5,
                OrganizationNodeKind::Team,
                OrganizationNodeStatus::Active,
                Some(4),
                &[1, 2, 3, 4],
            ),
        ]
        .into_iter()
        .map(|witness| (witness.key(), witness))
        .collect()
    }

    #[test]
    fn organization_lineage_decoder_binds_the_validated_witness_to_its_owner_envelope() {
        let (event, witness) = lineage_event();
        assert_eq!(decode_organization_lineage(&event).unwrap(), witness);

        let mut wrong_scope = event.clone();
        wrong_scope.active_scope = AuthorityScope::Holding {
            holding_id: witness.key().holding_id().as_uuid(),
        };
        assert_eq!(
            decode_organization_lineage(&wrong_scope),
            Err(AuthorityProjectionError::EnvelopeMismatch)
        );

        let mut wrong_version = event;
        wrong_version.aggregate_version += 1;
        assert_eq!(
            decode_organization_lineage(&wrong_version),
            Err(AuthorityProjectionError::EnvelopeMismatch)
        );
    }

    #[test]
    fn organization_lineage_cache_is_holding_scoped_monotonic_and_contract_shaped() {
        let source = include_str!("item_directory_authority.rs")
            .split("pub async fn apply_organization_lineage")
            .nth(1)
            .expect("lineage projection")
            .split("pub async fn reconcile_grants")
            .next()
            .expect("lineage projection boundary");
        assert!(source.contains("erpx_authority_read.organization_nodes"));
        assert!(
            source.contains("aggregate_version\n                   < EXCLUDED.aggregate_version")
        );
        assert!(source.contains(
            "source_event_position\n                   < EXCLUDED.source_event_position"
        ));
        assert!(!source.contains("company_id"));
        assert!(!source.contains("branch_id"));

        let ddl = include_str!(
            "../../../crates/migration/migrations_holding_read/0003_authority_lineage_forward_fix.sql"
        );
        let table = ddl
            .split("CREATE TABLE erpx_authority_read.organization_nodes")
            .nth(1)
            .expect("organization lineage cache")
            .split("CREATE TABLE erpx_authority_read.policy_decision_checkpoint")
            .next()
            .expect("organization lineage cache boundary");
        assert!(table.contains("PRIMARY KEY (holding_id,node_id)"));
        assert!(table.contains("lineage_node_ids UUID[] NOT NULL"));
        assert!(table.contains("cardinality(lineage_node_ids) BETWEEN 0 AND 4"));
        assert!(!table.contains("company_id UUID"));
        assert!(!table.contains("branch_id UUID"));
    }

    #[test]
    fn reconciliation_requires_a_complete_active_and_ordered_ancestor_chain() {
        let holding_id = HoldingId::try_from_uuid(lineage_id(1)).unwrap();
        let team_scope = AuthorityScope::Subtree {
            holding_id: lineage_id(1),
            organization_node_id: lineage_id(5),
        };
        let complete = cached_lineage_tree();
        assert_eq!(
            target_lineage_for_scope(&team_scope, holding_id, &complete)
                .unwrap()
                .map(|witness| witness.key()),
            Some(OrganizationLineageKey::try_new(lineage_id(1), lineage_id(5)).unwrap())
        );

        let branch_key = OrganizationLineageKey::try_new(lineage_id(1), lineage_id(3)).unwrap();
        let mut missing = complete.clone();
        missing.remove(&branch_key);
        assert_eq!(
            target_lineage_for_scope(&team_scope, holding_id, &missing).unwrap(),
            None
        );

        let mut wrong_kind = complete.clone();
        wrong_kind.insert(
            branch_key,
            cached_witness(
                3,
                OrganizationNodeKind::Headquarters,
                OrganizationNodeStatus::Active,
                Some(2),
                &[1, 2],
            ),
        );
        assert_eq!(
            target_lineage_for_scope(&team_scope, holding_id, &wrong_kind).unwrap(),
            None
        );

        let mut wrong_lineage = complete.clone();
        wrong_lineage.insert(
            branch_key,
            OrganizationLineageWitness::try_new(
                branch_key,
                OrganizationNodeKind::Branch,
                OrganizationNodeStatus::Active,
                Some(lineage_id(9)),
                vec![lineage_id(1), lineage_id(9)],
                2,
            )
            .unwrap(),
        );
        assert_eq!(
            target_lineage_for_scope(&team_scope, holding_id, &wrong_lineage).unwrap(),
            None
        );

        for status in [
            OrganizationNodeStatus::Suspended,
            OrganizationNodeStatus::Archived,
        ] {
            let mut inactive_ancestor = complete.clone();
            inactive_ancestor.insert(
                branch_key,
                cached_witness(3, OrganizationNodeKind::Branch, status, Some(2), &[1, 2]),
            );
            assert_eq!(
                target_lineage_for_scope(&team_scope, holding_id, &inactive_ancestor).unwrap(),
                None,
                "{status:?} ancestor"
            );
        }
    }

    #[test]
    fn authority_decoder_binds_fact_to_exact_owner_envelope() {
        let principal = PrincipalId::try_from_uuid(id()).unwrap();
        let now = Utc::now();
        let fact = AuthorityEvidenceFact::Principal(PrincipalAuthorityEvidence {
            principal_id: principal,
            principal_kind: AuthorityPrincipalKind::Human,
            realm: AuthorityRealm::InternalWorkforce,
            active: true,
            principal_version: 2,
            scope: AuthorityScope::Holding { holding_id: id() },
            assignments: vec![AuthorityAssignmentEvidence {
                assignment_id: id(),
                role_id: id(),
                assignment_version: 2,
                scope: AuthorityScope::Holding { holding_id: id() },
                valid_from: now,
                valid_until: now + Duration::minutes(5),
                revoked_at: None,
            }],
            group_memberships: Vec::new(),
            changed_at: now,
        });
        let mut event = ProjectionEvent {
            event_id: EventId::try_from_uuid(id()).unwrap(),
            global_sequence: 5,
            context: "identity_access".into(),
            partition: "principal".into(),
            command_id: CommandId::try_from_uuid(id()).unwrap(),
            event_type: AUTHORITY_EVIDENCE_EVENT_TYPE.into(),
            event_version: AUTHORITY_EVIDENCE_VERSION,
            aggregate_type: "access_principal".into(),
            aggregate_id: AggregateId::try_from_uuid(principal.as_uuid()).unwrap(),
            aggregate_version: 2,
            active_scope: AuthorityScope::Platform,
            actor: ActorAttribution {
                principal_ref: "worker:test".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: CorrelationId::try_from_uuid(id()).unwrap(),
            causation_id: None,
            payload: serde_json::to_value(&fact).unwrap(),
            occurred_at: now,
        };
        assert_eq!(decode_authority_evidence(&event).unwrap(), fact);
        event.payload = serde_json::json!({
            "payload_kind": "authority",
            "payload": fact,
        });
        assert_eq!(decode_authority_evidence(&event).unwrap(), fact);
        event.aggregate_version = 3;
        assert_eq!(
            decode_authority_evidence(&event),
            Err(AuthorityProjectionError::EnvelopeMismatch)
        );
    }

    #[test]
    fn authority_decoder_accepts_current_identity_access_role_wire() {
        let holding_id = id();
        let role_id = id();
        let now = Utc::now();
        let event = ProjectionEvent {
            event_id: EventId::try_from_uuid(id()).unwrap(),
            global_sequence: 45,
            context: "identity_access".into(),
            partition: "role".into(),
            command_id: CommandId::try_from_uuid(id()).unwrap(),
            event_type: AUTHORITY_EVIDENCE_EVENT_TYPE.into(),
            event_version: AUTHORITY_EVIDENCE_VERSION,
            aggregate_type: "access_role".into(),
            aggregate_id: AggregateId::try_from_uuid(role_id).unwrap(),
            aggregate_version: 1,
            active_scope: AuthorityScope::Platform,
            actor: ActorAttribution {
                principal_ref: "worker:test".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: CorrelationId::try_from_uuid(id()).unwrap(),
            causation_id: None,
            payload: serde_json::json!({
                "payload_kind": "authority",
                "payload": {
                    "authority_fact": "role",
                    "fact": {
                        "role_id": role_id,
                        "role_version": 1,
                        "scope": {"Holding": {"holding_id": holding_id}},
                        "permissions": ["identity_access.read", "security.read"],
                        "classification_ceiling": "restricted",
                        "active": true,
                        "changed_at": now,
                    }
                }
            }),
            occurred_at: now,
        };
        assert!(matches!(
            decode_authority_evidence(&event),
            Ok(AuthorityEvidenceFact::Role(_))
        ));
    }

    #[test]
    fn production_queries_are_holding_exact_and_never_infer_group_or_sibling_grants() {
        let source = include_str!("item_directory_authority.rs");
        let source = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(source.contains("WHERE holding_id=$1"));
        assert!(source.contains("gm.group_kind='human_group'"));
        assert!(!source.contains("gm.group_kind IN ('human_group','agent_pool')"));
        assert!(source.contains("source_approved"));
        assert!(source.contains("target_approved"));
    }

    #[test]
    fn collaboration_authority_intersects_anchor_realm_and_exact_audience() {
        let included = PrincipalId::try_from_uuid(id()).unwrap();
        let excluded = PrincipalId::try_from_uuid(id()).unwrap();
        let evidence = CollaborationAuthorityEvidence {
            anchor_record_id: RecordId::try_from_uuid(id()).unwrap(),
            audience: Audience::Internal {
                scope: AuthorityScope::Holding { holding_id: id() },
                principals: vec![CollaborationPrincipalRef::new(
                    included,
                    PrincipalKind::Human,
                )],
            },
            internal_only: false,
        };
        assert!(collaboration_audience_allows(
            &evidence,
            included,
            erpx_integration_contracts::RecordPolicyRealm::InternalWorkforce,
        ));
        assert!(!collaboration_audience_allows(
            &evidence,
            excluded,
            erpx_integration_contracts::RecordPolicyRealm::InternalWorkforce,
        ));
        assert!(!collaboration_audience_allows(
            &evidence,
            included,
            erpx_integration_contracts::RecordPolicyRealm::CustomerPortal,
        ));
    }

    #[test]
    fn external_future_records_require_exact_owner_realm_and_permission() {
        let record_id = RecordId::try_from_uuid(id()).unwrap();
        let owner = PrincipalId::try_from_uuid(id()).unwrap();
        let stranger = PrincipalId::try_from_uuid(id()).unwrap();
        let mut owners = BTreeMap::new();
        owners.insert(
            record_id,
            ExternalOwnerEvidence {
                record_type: "commerce.order".to_owned(),
                principal_id: owner,
                realm: ExternalOwnerRealm::Guest,
            },
        );
        assert!(future_external_owner_allows(
            "commerce.order",
            record_id,
            owner,
            RecordPolicyRealm::CustomerPortal,
            "commerce.guest.order.read",
            &owners,
        ));
        assert!(!future_external_owner_allows(
            "commerce.order",
            record_id,
            stranger,
            RecordPolicyRealm::CustomerPortal,
            "commerce.guest.order.read",
            &owners,
        ));
        assert!(!future_external_owner_allows(
            "commerce.order",
            record_id,
            owner,
            RecordPolicyRealm::CustomerPortal,
            "commerce.customer.order.read",
            &owners,
        ));
        assert!(!future_external_owner_allows(
            "field-service.agreement",
            RecordId::try_from_uuid(id()).unwrap(),
            owner,
            RecordPolicyRealm::CustomerPortal,
            "field_service.customer.read",
            &owners,
        ));
        assert!(future_external_owner_allows(
            "commerce.catalog-publication",
            RecordId::try_from_uuid(id()).unwrap(),
            owner,
            RecordPolicyRealm::CustomerPortal,
            "commerce.customer.catalog.read",
            &owners,
        ));
    }

    fn descriptor(
        context: ContextId,
        record_type: &str,
        record_id: RecordId,
        scope: AuthorityScope,
        version: u64,
    ) -> ItemDescriptor {
        let record = StableRecordRef::try_new(
            record_id,
            context,
            RecordTypeId::try_new(record_type).unwrap(),
        )
        .unwrap();
        let policy = record_policy(record_type).unwrap();
        ItemDescriptor::try_new(
            record,
            scope,
            "active",
            policy.policy_id(),
            DataClassification::Confidential,
            version,
            version,
        )
        .unwrap()
    }

    #[test]
    fn field_service_portal_grant_requires_exact_binding_customer_version_and_ttl() {
        let now = Utc::now();
        let scope = AuthorityScope::Company {
            holding_id: id(),
            company_id: id(),
        };
        let customer_id = RecordId::try_from_uuid(id()).unwrap();
        let agreement_id = RecordId::try_from_uuid(id()).unwrap();
        let customer_descriptor = descriptor(
            ContextId::CustomerAuthority,
            "customer.account",
            customer_id,
            scope.clone(),
            7,
        );
        let agreement_descriptor = descriptor(
            ContextId::FieldService,
            "field-service.agreement",
            agreement_id,
            scope.clone(),
            3,
        );
        let customer = RecordVersionRef::try_new(customer_descriptor.record().clone(), 7).unwrap();
        let portal_principal = PrincipalId::try_from_uuid(id()).unwrap();
        let candidate = Candidate {
            evidence: IamAuthorityEvidence {
                principal_id: portal_principal,
                principal_kind: RecordPolicyPrincipalKind::Human,
                realm: RecordPolicyRealm::CustomerPortal,
                principal_active: true,
                principal_version: 2,
                role_version: 2,
                assignment_version: 2,
                permission: "field_service.customer.read".into(),
                assignment_scope: scope.clone(),
                classification_ceiling: DataClassification::Confidential,
                valid_from: now - Duration::minutes(1),
                valid_until: now + Duration::minutes(20),
                revoked_at: None,
                membership: None,
            },
            permissions: vec!["field_service.customer.read".into()],
        };
        let binding_expiry = now + Duration::minutes(10);
        let binding = PortalBindingEvidence {
            binding_id: id(),
            version: 2,
            customer: customer.clone(),
            principal_id: portal_principal,
            principal_kind: RecordPolicyPrincipalKind::Human,
            scope: scope.clone(),
            verified_identity_digest: "a".repeat(64),
            verified_identity_version: 1,
            permits_field_service: true,
            valid_from: now - Duration::minutes(1),
            valid_until: binding_expiry,
            proposed_by: PrincipalId::try_from_uuid(id()).unwrap(),
            approved_by: Some(PrincipalId::try_from_uuid(id()).unwrap()),
            ended_at: None,
            status: "active".into(),
        };
        let mut records = BTreeMap::new();
        records.insert(
            agreement_id,
            FieldServiceCustomerEvidence {
                record_type: "field-service.agreement".into(),
                link: FieldServiceCustomerLink::PinnedCustomer(customer),
            },
        );
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[customer_descriptor.clone(), agreement_descriptor.clone()],
                std::slice::from_ref(&binding),
                &records,
                now,
            ),
            Some(binding_expiry)
        );

        let mut wrong_principal = binding.clone();
        wrong_principal.principal_id = PrincipalId::try_from_uuid(id()).unwrap();
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[customer_descriptor.clone(), agreement_descriptor.clone()],
                &[wrong_principal],
                &records,
                now,
            ),
            None
        );
        let mut revoked = binding.clone();
        revoked.status = "revoked".into();
        revoked.ended_at = Some(now);
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[customer_descriptor.clone(), agreement_descriptor.clone()],
                &[revoked],
                &records,
                now,
            ),
            None
        );
        let mut expired = binding.clone();
        expired.valid_until = now;
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[customer_descriptor.clone(), agreement_descriptor.clone()],
                &[expired],
                &records,
                now,
            ),
            None
        );
        let mut wrong_customer_version = binding.clone();
        wrong_customer_version.customer = RecordVersionRef::try_new(
            customer_descriptor.record().clone(),
            customer_descriptor.descriptor_version() + 1,
        )
        .unwrap();
        let mut wrong_records = records.clone();
        wrong_records.insert(
            agreement_id,
            FieldServiceCustomerEvidence {
                record_type: "field-service.agreement".into(),
                link: FieldServiceCustomerLink::PinnedCustomer(
                    wrong_customer_version.customer.clone(),
                ),
            },
        );
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[customer_descriptor.clone(), agreement_descriptor.clone()],
                &[wrong_customer_version],
                &wrong_records,
                now,
            ),
            None
        );

        let customer_policy = record_policy("customer.account").unwrap();
        let closed_customer = ItemDescriptor::try_new(
            customer_descriptor.record().clone(),
            customer_descriptor.scope().clone(),
            "merged",
            customer_policy.policy_id(),
            DataClassification::Confidential,
            customer_descriptor.descriptor_version(),
            customer_descriptor.source_event_position() + 1,
        )
        .unwrap();
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &candidate,
                &[closed_customer, agreement_descriptor.clone()],
                &[binding],
                &records,
                now,
            ),
            None
        );

        let mut internal = candidate;
        internal.evidence.realm = RecordPolicyRealm::InternalWorkforce;
        assert_eq!(
            field_service_portal_expiry(
                &agreement_descriptor,
                &internal,
                &[],
                &[],
                &BTreeMap::new(),
                now,
            ),
            None
        );
    }

    #[test]
    fn field_service_customer_chain_rejects_cycle_and_unversioned_customer_subject() {
        let scope = AuthorityScope::Company {
            holding_id: id(),
            company_id: id(),
        };
        let request_id = RecordId::try_from_uuid(id()).unwrap();
        let visit_id = RecordId::try_from_uuid(id()).unwrap();
        let request = descriptor(
            ContextId::FieldService,
            "field-service.request",
            request_id,
            scope,
            1,
        );
        let mut records = BTreeMap::new();
        records.insert(
            request_id,
            FieldServiceCustomerEvidence {
                record_type: "field-service.request".into(),
                link: FieldServiceCustomerLink::Subject(
                    StableRecordRef::try_new(
                        visit_id,
                        ContextId::FieldService,
                        RecordTypeId::try_new("field-service.visit").unwrap(),
                    )
                    .unwrap(),
                ),
            },
        );
        records.insert(
            visit_id,
            FieldServiceCustomerEvidence {
                record_type: "field-service.visit".into(),
                link: FieldServiceCustomerLink::Subject(request.record().clone()),
            },
        );
        assert_eq!(resolve_field_service_customer(&request, &records), None);

        records.insert(
            request_id,
            FieldServiceCustomerEvidence {
                record_type: "field-service.request".into(),
                link: FieldServiceCustomerLink::Subject(
                    StableRecordRef::try_new(
                        RecordId::try_from_uuid(id()).unwrap(),
                        ContextId::CustomerAuthority,
                        RecordTypeId::try_new("customer.account").unwrap(),
                    )
                    .unwrap(),
                ),
            },
        );
        assert_eq!(resolve_field_service_customer(&request, &records), None);
    }
}
