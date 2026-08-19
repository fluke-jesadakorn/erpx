use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_identity_access::{
    AccessPrincipal, AccessPrincipalState, AccessRole, AccessRoleState, AggregateKey,
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, GroupMembershipState,
    IdentityAccessAggregate, IdentityAccessCommit, NonHumanPrincipalGovernance, PermissionKey,
    PortError, PrincipalId, PrincipalKind, PrincipalRealm, PrincipalStatus, PrincipalView,
    RiskTier, RoleAssignment, RoleAssignmentId, RoleAssignmentState, RoleCode, RoleId, RoleView,
    StoredCommit, WriteRepository,
};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityAssignmentEvidence,
    AuthorityEvidenceFact, AuthorityPrincipalKind, AuthorityRealm,
    GroupMembershipAuthorityEvidence, ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE,
    IntegrationEvent, ItemDescriptorDraft, PrincipalAuthorityEvidence, RecordTypeId,
    RoleAuthorityEvidence, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, PRINCIPAL_AGGREGATE_TYPE, PRINCIPAL_SNAPSHOT_EVENT_TYPE,
    ROLE_AGGREGATE_TYPE, ROLE_SNAPSHOT_EVENT_TYPE, principal_partition,
    projection::{IdentityAccessOutboxPayload, IdentityAccessSnapshot},
    role_partition,
};

#[derive(Debug, Clone)]
pub struct IdentityAccessWriteRepository {
    pool: WritePool,
}

impl IdentityAccessWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WriteRepository for IdentityAccessWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        key: AggregateKey,
    ) -> Result<Option<IdentityAccessAggregate>, PortError> {
        match key {
            AggregateKey::Principal(id) => load_principal(&self.pool, authority_scope, id)
                .await
                .map(|value| value.map(IdentityAccessAggregate::Principal)),
            AggregateKey::Role(id) => load_role(&self.pool, authority_scope, id)
                .await
                .map(|value| value.map(IdentityAccessAggregate::Role)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityAccessCommandAdapter {
    store: CommandStore,
}

impl IdentityAccessCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for IdentityAccessCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let key = fingerprint_key(&identity.fingerprint);
        let (partition, _, _) = key_parts(key)?;
        let receipt = self
            .store
            .find_receipt::<IdentityAccessAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: IdentityAccessCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let key = fingerprint_key(&request.identity.fingerprint);
        let (partition, aggregate_type, aggregate_id) = key_parts(key)?;
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let idempotency_key = request.identity.idempotency_key.clone();
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: IdentityAccessOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        let (snapshot_type, snapshot) = match &request.aggregate {
            IdentityAccessAggregate::Principal(value) => (
                PRINCIPAL_SNAPSHOT_EVENT_TYPE,
                IdentityAccessSnapshot::Principal(PrincipalView::from(value)),
            ),
            IdentityAccessAggregate::Role(value) => (
                ROLE_SNAPSHOT_EVENT_TYPE,
                IdentityAccessSnapshot::Role(RoleView::from(value)),
            ),
        };
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_type, events.len()),
            event_type: snapshot_type.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: IdentityAccessOutboxPayload::Snapshot(snapshot),
        });
        let authority_fact = identity_access_authority_fact(&request.aggregate)?;
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(
                command_id,
                AUTHORITY_EVIDENCE_EVENT_TYPE,
                events.len(),
            ),
            event_type: AUTHORITY_EVIDENCE_EVENT_TYPE.into(),
            event_version: AUTHORITY_EVIDENCE_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: IdentityAccessOutboxPayload::Authority(authority_fact),
        });
        for directory_draft in identity_access_directory_drafts(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: IdentityAccessOutboxPayload::Directory(directory_draft),
            });
        }

        let active_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let guard = IdentityAccessGuard {
            aggregate: aggregate.clone(),
            fingerprint: request.identity.fingerprint.clone(),
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            active_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"identity-access-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_id": aggregate_id.as_uuid(),
                    "expected_version": request.expected_version,
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn identity_access_authority_fact(
    aggregate: &IdentityAccessAggregate,
) -> Result<AuthorityEvidenceFact, PortError> {
    let fact = match aggregate {
        IdentityAccessAggregate::Principal(value) => {
            let principal_id = erpx_kernel::PrincipalId::try_from_uuid(value.id().as_uuid())
                .map_err(|_| PortError::Internal)?;
            let assignments = value
                .assignments()
                .iter()
                .map(|assignment| AuthorityAssignmentEvidence {
                    assignment_id: assignment.id().as_uuid(),
                    role_id: assignment.role_id().as_uuid(),
                    assignment_version: value.version(),
                    scope: assignment.scope().clone(),
                    valid_from: assignment.valid_from(),
                    valid_until: assignment.valid_until(),
                    revoked_at: assignment.revoked_at(),
                })
                .collect();
            AuthorityEvidenceFact::Principal(PrincipalAuthorityEvidence {
                principal_id,
                principal_kind: match value.kind() {
                    erpx_identity_access::PrincipalKind::Human => AuthorityPrincipalKind::Human,
                    erpx_identity_access::PrincipalKind::Service => AuthorityPrincipalKind::Service,
                    erpx_identity_access::PrincipalKind::Agent => AuthorityPrincipalKind::Agent,
                    erpx_identity_access::PrincipalKind::HumanGroup => {
                        AuthorityPrincipalKind::HumanGroup
                    }
                    erpx_identity_access::PrincipalKind::AgentPool => {
                        AuthorityPrincipalKind::AgentPool
                    }
                },
                realm: match value.realm() {
                    erpx_identity_access::PrincipalRealm::InternalWorkforce => {
                        AuthorityRealm::InternalWorkforce
                    }
                    erpx_identity_access::PrincipalRealm::PlatformWorkforce => {
                        AuthorityRealm::PlatformWorkforce
                    }
                    erpx_identity_access::PrincipalRealm::CustomerPortal => {
                        AuthorityRealm::CustomerPortal
                    }
                },
                active: value.status() == erpx_identity_access::PrincipalStatus::Active,
                principal_version: value.version(),
                scope: value.scope().clone(),
                assignments,
                group_memberships: value
                    .group_memberships()
                    .iter()
                    .map(|membership| GroupMembershipAuthorityEvidence {
                        group_id: principal_id,
                        group_kind: match value.kind() {
                            erpx_identity_access::PrincipalKind::HumanGroup => {
                                AuthorityPrincipalKind::HumanGroup
                            }
                            erpx_identity_access::PrincipalKind::AgentPool => {
                                AuthorityPrincipalKind::AgentPool
                            }
                            _ => unreachable!("only group principals carry memberships"),
                        },
                        group_version: value.version(),
                        member_id: erpx_kernel::PrincipalId::try_from_uuid(
                            membership.member_principal_id().as_uuid(),
                        )
                        .expect("identity principal IDs are non-nil"),
                        membership_version: membership.version(),
                        scope: value.scope().clone(),
                        valid_from: membership.valid_from(),
                        valid_until: membership.valid_until(),
                        revoked_at: membership.revoked_at(),
                        changed_at: value.updated_at(),
                    })
                    .collect(),
                changed_at: value.updated_at(),
            })
        }
        IdentityAccessAggregate::Role(value) => {
            AuthorityEvidenceFact::Role(RoleAuthorityEvidence {
                role_id: value.id().as_uuid(),
                role_version: value.version(),
                scope: value.scope().clone(),
                permissions: value
                    .permissions()
                    .iter()
                    .map(|permission| permission.as_str().to_owned())
                    .collect(),
                classification_ceiling: DataClassification::Restricted,
                active: true,
                changed_at: value.updated_at(),
            })
        }
    };
    fact.validate().map_err(|_| PortError::Internal)?;
    Ok(fact)
}

fn identity_access_directory_drafts(
    aggregate: &IdentityAccessAggregate,
) -> Result<Vec<ItemDescriptorDraft>, PortError> {
    let record_id =
        RecordId::try_from_uuid(aggregate.aggregate_uuid()).map_err(|_| PortError::Internal)?;
    let (record_type, lifecycle_state, classification, search_text) = match aggregate {
        IdentityAccessAggregate::Principal(value) => (
            "identity.principal",
            match value.status() {
                erpx_identity_access::PrincipalStatus::Invited => "invited",
                erpx_identity_access::PrincipalStatus::Active => "active",
                erpx_identity_access::PrincipalStatus::Suspended => "suspended",
                erpx_identity_access::PrincipalStatus::Disabled => "disabled",
            },
            DataClassification::Restricted,
            value.display_name().to_owned(),
        ),
        IdentityAccessAggregate::Role(value) => (
            "identity.role",
            "active",
            DataClassification::Confidential,
            format!("{} {}", value.code().as_str(), value.display_name()),
        ),
    };
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::IdentityAccess,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification,
        descriptor_version: aggregate.version(),
        authorized_search_text: search_text,
    };
    value.validate().map_err(|_| PortError::Internal)?;
    let mut values = vec![value];
    if let IdentityAccessAggregate::Principal(principal) = aggregate {
        for assignment in principal.assignments() {
            let record = StableRecordRef::try_new(
                RecordId::try_from_uuid(assignment.id().as_uuid())
                    .map_err(|_| PortError::Internal)?,
                ContextId::IdentityAccess,
                RecordTypeId::try_new("identity.grant").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?;
            let at = principal.updated_at();
            let lifecycle_state = if assignment.revoked_at().is_some() {
                "revoked"
            } else if at < assignment.valid_from() {
                "scheduled"
            } else if at >= assignment.valid_until() {
                "expired"
            } else {
                "active"
            };
            let draft = ItemDescriptorDraft {
                authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
                record,
                scope: assignment.scope().clone(),
                lifecycle_state: lifecycle_state.into(),
                classification: DataClassification::Restricted,
                descriptor_version: principal.version(),
                authorized_search_text: format!("Authority grant {}", assignment.id().as_uuid()),
            };
            draft.validate().map_err(|_| PortError::Internal)?;
            values.push(draft);
        }
    }
    Ok(values)
}

#[derive(Debug, Clone)]
struct IdentityAccessGuard {
    aggregate: IdentityAccessAggregate,
    fingerprint: CommandFingerprint,
}

#[async_trait]
impl AtomicCommandGuard for IdentityAccessGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(
            context.context,
            context.aggregate_type,
            context.aggregate_id,
            &self.aggregate,
        )?;
        let (table, id_column) = match &self.aggregate {
            IdentityAccessAggregate::Principal(_) => ("principal_authority", "principal_id"),
            IdentityAccessAggregate::Role(_) => ("role_authority", "role_id"),
        };
        let query = format!(
            "SELECT aggregate_version FROM erpx_identity_access_write.{table}
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND {id_column}=$1 FOR UPDATE"
        );
        let version: Option<i64> = sqlx::query_scalar(&query)
            .bind(self.aggregate.aggregate_uuid())
            .fetch_optional(&mut **transaction)
            .await?;
        version
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            context.context,
            context.aggregate_type,
            context.aggregate_id,
            &self.aggregate,
        )?;
        match &self.aggregate {
            IdentityAccessAggregate::Role(role) => {
                let owner: Option<Uuid> = sqlx::query_scalar(
                    "SELECT role_id
                     FROM erpx_identity_access_write.role_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id()
                       AND scope_key = $1 AND role_code = $2
                     FOR UPDATE",
                )
                .bind(scope_key(role.scope()))
                .bind(role.code().as_str())
                .fetch_optional(&mut **transaction)
                .await?;
                if owner.is_some_and(|owner| owner != role.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "role code is already owned in this scope",
                    ));
                }
            }
            IdentityAccessAggregate::Principal(principal) => {
                if let CommandFingerprint::RecoverRole {
                    role_id,
                    scope,
                    valid_from,
                    ..
                } = &self.fingerprint
                {
                    let lock_key = format!(
                        "identity-access-recovery:{}:{}",
                        role_id.as_uuid(),
                        scope_key(scope)
                    );
                    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                        .bind(lock_key)
                        .execute(&mut **transaction)
                        .await?;
                    let role_exists: bool = sqlx::query_scalar(
                        "SELECT EXISTS(
                            SELECT 1
                            FROM erpx_identity_access_write.role_authority
                            WHERE holding_id=erpx_cqrs_write.current_holding_id()
                              AND role_id=$1 AND scope_key=$2
                        )",
                    )
                    .bind(role_id.as_uuid())
                    .bind(scope_key(scope))
                    .fetch_one(&mut **transaction)
                    .await?;
                    if !role_exists {
                        return Err(CqrsSqlxError::GuardRejected(
                            "recovery role is not defined in the command scope",
                        ));
                    }
                    let existing: Option<Uuid> = sqlx::query_scalar(
                        "SELECT assignment_id
                         FROM erpx_identity_access_write.assignment_authority
                         WHERE holding_id=erpx_cqrs_write.current_holding_id()
                           AND role_id=$1 AND scope_key=$2 AND revoked_at IS NULL
                           AND valid_from <= $3 AND valid_until > $3
                         ORDER BY assignment_id
                         LIMIT 1
                         FOR UPDATE",
                    )
                    .bind(role_id.as_uuid())
                    .bind(scope_key(scope))
                    .bind(*valid_from)
                    .fetch_optional(&mut **transaction)
                    .await?;
                    if existing.is_some() {
                        return Err(CqrsSqlxError::GuardRejected(
                            "recovery target already has an active role assignment",
                        ));
                    }
                }
                let existing_assignments = sqlx::query(
                    "SELECT assignment_id FROM erpx_identity_access_write.assignment_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id() AND principal_id=$1
                     FOR UPDATE",
                )
                .bind(principal.id().as_uuid())
                .fetch_all(&mut **transaction)
                .await?;
                if existing_assignments.iter().any(|row| {
                    row.try_get::<Uuid, _>("assignment_id")
                        .ok()
                        .and_then(|id| RoleAssignmentId::try_from_uuid(id).ok())
                        .is_none_or(|id| {
                            !principal
                                .assignments()
                                .iter()
                                .any(|assignment| assignment.id() == id)
                        })
                }) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "principal assignment history cannot be removed",
                    ));
                }
                let existing_members = sqlx::query(
                    "SELECT member_principal_id
                     FROM erpx_identity_access_write.group_membership_authority
                     WHERE holding_id=erpx_cqrs_write.current_holding_id() AND group_id=$1
                     FOR UPDATE",
                )
                .bind(principal.id().as_uuid())
                .fetch_all(&mut **transaction)
                .await?;
                if existing_members.iter().any(|row| {
                    row.try_get::<Uuid, _>("member_principal_id")
                        .ok()
                        .and_then(|id| PrincipalId::try_from_uuid(id).ok())
                        .is_none_or(|id| {
                            !principal
                                .group_memberships()
                                .iter()
                                .any(|membership| membership.member_principal_id() == id)
                        })
                }) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "group membership history cannot be removed",
                    ));
                }
                for assignment in principal.assignments() {
                    let owner: Option<Uuid> = sqlx::query_scalar(
                        "SELECT principal_id
                         FROM erpx_identity_access_write.assignment_authority
                         WHERE holding_id=erpx_cqrs_write.current_holding_id()
                           AND assignment_id = $1
                         FOR UPDATE",
                    )
                    .bind(assignment.id().as_uuid())
                    .fetch_optional(&mut **transaction)
                    .await?;
                    if owner.is_some_and(|owner| owner != principal.id().as_uuid()) {
                        return Err(CqrsSqlxError::GuardRejected(
                            "assignment id belongs to another principal",
                        ));
                    }
                    let duplicate: Option<Uuid> = sqlx::query_scalar(
                        "SELECT assignment_id
                         FROM erpx_identity_access_write.assignment_authority
                         WHERE holding_id=erpx_cqrs_write.current_holding_id()
                           AND principal_id = $1 AND role_id = $2 AND scope_key = $3
                           AND revoked_at IS NULL AND assignment_id <> $4
                         FOR UPDATE",
                    )
                    .bind(principal.id().as_uuid())
                    .bind(assignment.role_id().as_uuid())
                    .bind(scope_key(assignment.scope()))
                    .bind(assignment.id().as_uuid())
                    .fetch_optional(&mut **transaction)
                    .await?;
                    if duplicate.is_some() {
                        return Err(CqrsSqlxError::GuardRejected(
                            "an active role assignment already exists",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(
            context.context,
            context.aggregate_type,
            context.aggregate_id,
            &self.aggregate,
        )?;
        if context.aggregate_version != self.aggregate.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "guard aggregate version does not match snapshot",
            ));
        }
        match &self.aggregate {
            IdentityAccessAggregate::Principal(principal) => {
                apply_principal_authority(transaction, principal, context.aggregate_version)
                    .await?;
            }
            IdentityAccessAggregate::Role(role) => {
                apply_role_authority(transaction, role, context.aggregate_version).await?;
            }
        }
        match &self.aggregate {
            IdentityAccessAggregate::Role(role) => {
                sqlx::query(
                    "INSERT INTO erpx_identity_access_write.role_code_guards
                        (role_id, scope_key, role_code, aggregate_version, updated_at)
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (role_id) DO UPDATE
                     SET scope_key = EXCLUDED.scope_key, role_code = EXCLUDED.role_code,
                         aggregate_version = EXCLUDED.aggregate_version,
                         updated_at = EXCLUDED.updated_at",
                )
                .bind(role.id().as_uuid())
                .bind(scope_key(role.scope()))
                .bind(role.code().as_str())
                .bind(to_i64(context.aggregate_version)?)
                .bind(role.updated_at())
                .execute(&mut **transaction)
                .await?;
                sqlx::query(
                    "INSERT INTO erpx_authority_write.roles
                        (holding_id,role_id,role_scope,role_version,permissions,
                         classification_ceiling,active,changed_at)
                     VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,
                             'restricted',true,$5)
                     ON CONFLICT (holding_id,role_id) DO UPDATE SET
                        role_scope=EXCLUDED.role_scope,
                        role_version=EXCLUDED.role_version,
                        permissions=EXCLUDED.permissions,
                        classification_ceiling=EXCLUDED.classification_ceiling,
                        active=EXCLUDED.active,
                        changed_at=EXCLUDED.changed_at
                     WHERE erpx_authority_write.roles.holding_id
                           = erpx_authority_write.current_holding_id()
                       AND erpx_authority_write.roles.role_version < EXCLUDED.role_version",
                )
                .bind(role.id().as_uuid())
                .bind(serde_json::to_value(role.scope())?)
                .bind(to_i64(role.version())?)
                .bind(
                    role.permissions()
                        .iter()
                        .map(|permission| permission.as_str())
                        .collect::<Vec<_>>(),
                )
                .bind(role.updated_at())
                .execute(&mut **transaction)
                .await?;
            }
            IdentityAccessAggregate::Principal(principal) => {
                sqlx::query(
                    "INSERT INTO erpx_authority_write.principals
                        (holding_id,principal_id,principal_kind,realm,principal_scope,
                         active,principal_version,changed_at)
                     VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7)
                     ON CONFLICT (holding_id,principal_id) DO UPDATE SET
                        principal_kind=EXCLUDED.principal_kind,
                        realm=EXCLUDED.realm,
                        principal_scope=EXCLUDED.principal_scope,
                        active=EXCLUDED.active,
                        principal_version=EXCLUDED.principal_version,
                        changed_at=EXCLUDED.changed_at
                     WHERE erpx_authority_write.principals.holding_id
                           = erpx_authority_write.current_holding_id()
                       AND erpx_authority_write.principals.principal_version
                           < EXCLUDED.principal_version",
                )
                .bind(principal.id().as_uuid())
                .bind(principal_kind_name(principal.kind()))
                .bind(principal_realm_name(principal.realm()))
                .bind(serde_json::to_value(principal.scope())?)
                .bind(principal.status() == erpx_identity_access::PrincipalStatus::Active)
                .bind(to_i64(principal.version())?)
                .bind(principal.updated_at())
                .execute(&mut **transaction)
                .await?;
                for assignment in principal.assignments() {
                    let applied = sqlx::query(
                        "INSERT INTO erpx_identity_access_write.assignment_guards
                            (assignment_id, principal_id, role_id, scope_key, risk_tier,
                             valid_from, valid_until, revoked_at, principal_version, updated_at)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                         ON CONFLICT (assignment_id) DO UPDATE
                         SET role_id = EXCLUDED.role_id, scope_key = EXCLUDED.scope_key,
                             risk_tier = EXCLUDED.risk_tier,
                             valid_from = EXCLUDED.valid_from,
                             valid_until = EXCLUDED.valid_until,
                             revoked_at = EXCLUDED.revoked_at,
                             principal_version = EXCLUDED.principal_version,
                             updated_at = EXCLUDED.updated_at
                         WHERE erpx_identity_access_write.assignment_guards.principal_id
                               = EXCLUDED.principal_id",
                    )
                    .bind(assignment.id().as_uuid())
                    .bind(principal.id().as_uuid())
                    .bind(assignment.role_id().as_uuid())
                    .bind(scope_key(assignment.scope()))
                    .bind(risk_name(assignment.risk_tier()))
                    .bind(assignment.valid_from())
                    .bind(assignment.valid_until())
                    .bind(assignment.revoked_at())
                    .bind(to_i64(context.aggregate_version)?)
                    .bind(principal.updated_at())
                    .execute(&mut **transaction)
                    .await?;
                    if applied.rows_affected() != 1 {
                        return Err(CqrsSqlxError::GuardRejected(
                            "assignment guard owner changed",
                        ));
                    }
                    sqlx::query(
                        "INSERT INTO erpx_authority_write.assignments
                            (holding_id,assignment_id,principal_id,role_id,assignment_scope,
                             assignment_version,valid_from,valid_until,revoked_at,changed_at)
                         VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,
                                 $5,$6,$7,$8,$9)
                         ON CONFLICT (holding_id,assignment_id) DO UPDATE SET
                            role_id=EXCLUDED.role_id,
                            assignment_scope=EXCLUDED.assignment_scope,
                            assignment_version=EXCLUDED.assignment_version,
                            valid_from=EXCLUDED.valid_from,
                            valid_until=EXCLUDED.valid_until,
                            revoked_at=EXCLUDED.revoked_at,
                            changed_at=EXCLUDED.changed_at
                         WHERE erpx_authority_write.assignments.holding_id
                               = erpx_authority_write.current_holding_id()
                           AND erpx_authority_write.assignments.principal_id
                               = EXCLUDED.principal_id
                           AND erpx_authority_write.assignments.assignment_version
                               < EXCLUDED.assignment_version",
                    )
                    .bind(assignment.id().as_uuid())
                    .bind(principal.id().as_uuid())
                    .bind(assignment.role_id().as_uuid())
                    .bind(serde_json::to_value(assignment.scope())?)
                    .bind(to_i64(principal.version())?)
                    .bind(assignment.valid_from())
                    .bind(assignment.valid_until())
                    .bind(assignment.revoked_at())
                    .bind(principal.updated_at())
                    .execute(&mut **transaction)
                    .await?;
                }
                if matches!(
                    principal.kind(),
                    erpx_identity_access::PrincipalKind::HumanGroup
                        | erpx_identity_access::PrincipalKind::AgentPool
                ) {
                    for membership in principal.group_memberships() {
                        let applied = sqlx::query(
                            "INSERT INTO erpx_authority_write.group_memberships
                                (holding_id,group_id,member_principal_id,group_kind,group_version,
                                 membership_version,membership_scope,valid_from,valid_until,
                                 revoked_at,changed_at)
                             VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,$5,
                                     $6,$7,$8,$9,$10)
                             ON CONFLICT (holding_id,group_id,member_principal_id) DO UPDATE SET
                                group_kind=EXCLUDED.group_kind,
                                group_version=EXCLUDED.group_version,
                                membership_version=EXCLUDED.membership_version,
                                membership_scope=EXCLUDED.membership_scope,
                                valid_from=EXCLUDED.valid_from,
                                valid_until=EXCLUDED.valid_until,
                                revoked_at=EXCLUDED.revoked_at,
                                changed_at=EXCLUDED.changed_at
                             WHERE erpx_authority_write.group_memberships.holding_id
                                   = erpx_authority_write.current_holding_id()
                               AND erpx_authority_write.group_memberships.group_version
                                   < EXCLUDED.group_version",
                        )
                        .bind(principal.id().as_uuid())
                        .bind(membership.member_principal_id().as_uuid())
                        .bind(principal_kind_name(principal.kind()))
                        .bind(to_i64(principal.version())?)
                        .bind(to_i64(membership.version())?)
                        .bind(serde_json::to_value(principal.scope())?)
                        .bind(membership.valid_from())
                        .bind(membership.valid_until())
                        .bind(membership.revoked_at())
                        .bind(principal.updated_at())
                        .execute(&mut **transaction)
                        .await?;
                        if applied.rows_affected() != 1 {
                            return Err(CqrsSqlxError::GuardRejected(
                                "group membership owner or version changed",
                            ));
                        }
                    }
                } else if !principal.group_memberships().is_empty() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "non-group principal carries group membership authority",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ScopeColumns {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl ScopeColumns {
    fn from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }

    fn from_scope_store(scope: &AuthorityScope) -> Result<Self, CqrsSqlxError> {
        Self::from_scope(scope)
            .map_err(|_| CqrsSqlxError::InvalidInput("Identity scope is invalid"))
    }
}

const AUTHORITY_FILTER: &str = "
 AND ($2='platform' OR ($2='holding' AND holding_id=$3)
   OR ($2='company' AND holding_id=$3 AND company_id=$4)
   OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
   OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
       AND ((branch_id IS NULL AND cardinality($7::uuid[])=0) OR branch_id=ANY($7))))";

async fn load_principal(
    pool: &WritePool,
    authority_scope: &AuthorityScope,
    id: PrincipalId,
) -> Result<Option<AccessPrincipal>, PortError> {
    let authority = ScopeColumns::from_scope(authority_scope)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let query = format!(
        "SELECT aggregate_version,scope_mode,holding_id,company_id,branch_id,company_ids,
                branch_ids,display_name,principal_kind,principal_realm,governance_owner_ref,
                governance_purpose,governance_workload_binding,governance_credential_reference,
                principal_status,created_at,updated_at
         FROM erpx_identity_access_write.principal_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND principal_id=$1{AUTHORITY_FILTER}"
    );
    let row = sqlx::query(&query)
        .bind(id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let scope = scope_from_row(&row, "")?;
    let assignments = load_assignments(&mut tx, id, &scope).await?;
    let group_memberships = load_group_memberships(&mut tx, id).await?;
    let governance_fields = (
        row.try_get::<Option<String>, _>("governance_owner_ref")
            .map_err(|_| PortError::Internal)?,
        row.try_get::<Option<String>, _>("governance_purpose")
            .map_err(|_| PortError::Internal)?,
        row.try_get::<Option<String>, _>("governance_workload_binding")
            .map_err(|_| PortError::Internal)?,
        row.try_get::<Option<String>, _>("governance_credential_reference")
            .map_err(|_| PortError::Internal)?,
    );
    let non_human_governance = match governance_fields {
        (Some(owner), Some(purpose), Some(binding), Some(credential)) => Some(
            NonHumanPrincipalGovernance::new(owner, purpose, binding, credential)
                .map_err(|_| PortError::Internal)?,
        ),
        (None, None, None, None) => None,
        _ => return Err(PortError::Internal),
    };
    let value = AccessPrincipal::try_rehydrate(AccessPrincipalState {
        id,
        version: row_u64(&row, "aggregate_version")?,
        scope,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PortError::Internal)?,
        kind: parse_principal_kind(
            row.try_get("principal_kind")
                .map_err(|_| PortError::Internal)?,
        )?,
        realm: parse_principal_realm(
            row.try_get("principal_realm")
                .map_err(|_| PortError::Internal)?,
        )?,
        non_human_governance,
        status: parse_principal_status(
            row.try_get("principal_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        assignments,
        group_memberships,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    })
    .map_err(|_| PortError::Internal)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(Some(value))
}

async fn load_role(
    pool: &WritePool,
    authority_scope: &AuthorityScope,
    id: RoleId,
) -> Result<Option<AccessRole>, PortError> {
    let authority = ScopeColumns::from_scope(authority_scope)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let query = format!(
        "SELECT aggregate_version,scope_mode,holding_id,company_id,branch_id,company_ids,
                branch_ids,role_code,display_name,risk_tier,created_at,updated_at
         FROM erpx_identity_access_write.role_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND role_id=$1{AUTHORITY_FILTER}"
    );
    let row = sqlx::query(&query)
        .bind(id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let permission_rows = sqlx::query(
        "SELECT permission_key FROM erpx_identity_access_write.role_permission_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND role_id=$1
         ORDER BY permission_key",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let permissions = permission_rows
        .into_iter()
        .map(|row| {
            PermissionKey::try_new(
                row.try_get::<String, _>("permission_key")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let value = AccessRole::try_rehydrate(AccessRoleState {
        id,
        version: row_u64(&row, "aggregate_version")?,
        scope: scope_from_row(&row, "")?,
        code: RoleCode::try_new(
            row.try_get::<String, _>("role_code")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PortError::Internal)?,
        risk_tier: parse_risk(row.try_get("risk_tier").map_err(|_| PortError::Internal)?)?,
        permissions,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    })
    .map_err(|_| PortError::Internal)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(Some(value))
}

async fn load_assignments(
    tx: &mut Transaction<'_, Postgres>,
    principal_id: PrincipalId,
    principal_scope: &AuthorityScope,
) -> Result<Vec<RoleAssignment>, PortError> {
    let rows = sqlx::query(
        "SELECT assignment_id,role_id,scope_mode,holding_id,company_id,branch_id,
                company_ids,branch_ids,risk_tier,valid_from,valid_until,grant_reason,
                ticket_reference,grantor_ref,approver_ref,revoked_at
         FROM erpx_identity_access_write.assignment_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND principal_id=$1
         ORDER BY assignment_id",
    )
    .bind(principal_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    rows.into_iter()
        .map(|row| {
            RoleAssignment::try_rehydrate(
                principal_scope,
                RoleAssignmentState {
                    id: RoleAssignmentId::try_from_uuid(
                        row.try_get("assignment_id")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    role_id: RoleId::try_from_uuid(
                        row.try_get("role_id").map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    scope: scope_from_row(&row, "")?,
                    risk_tier: parse_risk(
                        row.try_get("risk_tier").map_err(|_| PortError::Internal)?,
                    )?,
                    valid_from: row.try_get("valid_from").map_err(|_| PortError::Internal)?,
                    valid_until: row
                        .try_get("valid_until")
                        .map_err(|_| PortError::Internal)?,
                    grant_reason: row
                        .try_get("grant_reason")
                        .map_err(|_| PortError::Internal)?,
                    ticket_reference: row
                        .try_get("ticket_reference")
                        .map_err(|_| PortError::Internal)?,
                    grantor_ref: row
                        .try_get("grantor_ref")
                        .map_err(|_| PortError::Internal)?,
                    approver_ref: row
                        .try_get("approver_ref")
                        .map_err(|_| PortError::Internal)?,
                    revoked_at: row.try_get("revoked_at").map_err(|_| PortError::Internal)?,
                },
            )
            .map_err(|_| PortError::Internal)
        })
        .collect()
}

async fn load_group_memberships(
    tx: &mut Transaction<'_, Postgres>,
    group_id: PrincipalId,
) -> Result<Vec<GroupMembershipState>, PortError> {
    let rows = sqlx::query(
        "SELECT member_principal_id,membership_version,valid_from,valid_until,revoked_at
         FROM erpx_identity_access_write.group_membership_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND group_id=$1
         ORDER BY member_principal_id",
    )
    .bind(group_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    rows.into_iter()
        .map(|row| {
            Ok(GroupMembershipState {
                member_principal_id: PrincipalId::try_from_uuid(
                    row.try_get("member_principal_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                version: row_u64(&row, "membership_version")?,
                valid_from: row.try_get("valid_from").map_err(|_| PortError::Internal)?,
                valid_until: row
                    .try_get("valid_until")
                    .map_err(|_| PortError::Internal)?,
                revoked_at: row.try_get("revoked_at").map_err(|_| PortError::Internal)?,
            })
        })
        .collect()
}

async fn apply_principal_authority(
    tx: &mut Transaction<'_, Postgres>,
    principal: &AccessPrincipal,
    version: u64,
) -> Result<(), CqrsSqlxError> {
    let scope = ScopeColumns::from_scope_store(principal.scope())?;
    let governance = principal.non_human_governance();
    let changed = sqlx::query(
        "INSERT INTO erpx_identity_access_write.principal_authority
            (holding_id,principal_id,aggregate_version,scope_key,scope_mode,holding_id,company_id,
             branch_id,company_ids,branch_ids,display_name,principal_kind,principal_realm,
             governance_owner_ref,governance_purpose,governance_workload_binding,
             governance_credential_reference,principal_status,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                 $11,$12,$13,$14,$15,$16,$17,$18,$19)
         ON CONFLICT (holding_id,principal_id) DO UPDATE SET
            aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
            scope_mode=EXCLUDED.scope_mode,holding_id=EXCLUDED.holding_id,
            company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
            company_ids=EXCLUDED.company_ids,branch_ids=EXCLUDED.branch_ids,
            display_name=EXCLUDED.display_name,principal_kind=EXCLUDED.principal_kind,
            principal_realm=EXCLUDED.principal_realm,
            governance_owner_ref=EXCLUDED.governance_owner_ref,
            governance_purpose=EXCLUDED.governance_purpose,
            governance_workload_binding=EXCLUDED.governance_workload_binding,
            governance_credential_reference=EXCLUDED.governance_credential_reference,
            principal_status=EXCLUDED.principal_status,created_at=EXCLUDED.created_at,
            updated_at=EXCLUDED.updated_at
         WHERE erpx_identity_access_write.principal_authority.holding_id=
                   erpx_cqrs_write.current_holding_id()
           AND erpx_identity_access_write.principal_authority.aggregate_version + 1=
                   EXCLUDED.aggregate_version",
    )
    .bind(principal.id().as_uuid())
    .bind(to_i64(version)?)
    .bind(scope_key(principal.scope()))
    .bind(scope.mode)
    .bind(scope.holding_id)
    .bind(scope.company_id)
    .bind(scope.branch_id)
    .bind(&scope.company_ids)
    .bind(&scope.branch_ids)
    .bind(principal.display_name())
    .bind(principal_kind_name(principal.kind()))
    .bind(principal_realm_name(principal.realm()))
    .bind(governance.map(NonHumanPrincipalGovernance::owner_ref))
    .bind(governance.map(NonHumanPrincipalGovernance::purpose))
    .bind(governance.map(NonHumanPrincipalGovernance::workload_binding))
    .bind(governance.map(NonHumanPrincipalGovernance::credential_reference))
    .bind(principal_status_name(principal.status()))
    .bind(principal.created_at())
    .bind(principal.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require_one_row(changed, "Identity principal version fence")?;

    for assignment in principal.assignments() {
        let assignment_scope = ScopeColumns::from_scope_store(assignment.scope())?;
        let changed = sqlx::query(
            "INSERT INTO erpx_identity_access_write.assignment_authority
                (holding_id,assignment_id,principal_id,role_id,scope_key,scope_mode,holding_id,
                 company_id,branch_id,company_ids,branch_ids,risk_tier,valid_from,valid_until,
                 grant_reason,ticket_reference,grantor_ref,approver_ref,revoked_at,principal_version)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                     $11,$12,$13,$14,$15,$16,$17,$18,$19)
             ON CONFLICT (holding_id,assignment_id) DO UPDATE SET
                revoked_at=EXCLUDED.revoked_at,principal_version=EXCLUDED.principal_version
             WHERE erpx_identity_access_write.assignment_authority.principal_id=EXCLUDED.principal_id
               AND erpx_identity_access_write.assignment_authority.role_id=EXCLUDED.role_id
               AND erpx_identity_access_write.assignment_authority.scope_key=EXCLUDED.scope_key
               AND erpx_identity_access_write.assignment_authority.risk_tier=EXCLUDED.risk_tier
               AND erpx_identity_access_write.assignment_authority.valid_from=EXCLUDED.valid_from
               AND erpx_identity_access_write.assignment_authority.valid_until=EXCLUDED.valid_until
               AND erpx_identity_access_write.assignment_authority.grant_reason=EXCLUDED.grant_reason
               AND erpx_identity_access_write.assignment_authority.ticket_reference IS NOT DISTINCT FROM EXCLUDED.ticket_reference
               AND erpx_identity_access_write.assignment_authority.grantor_ref=EXCLUDED.grantor_ref
               AND erpx_identity_access_write.assignment_authority.approver_ref IS NOT DISTINCT FROM EXCLUDED.approver_ref",
        )
        .bind(assignment.id().as_uuid())
        .bind(principal.id().as_uuid())
        .bind(assignment.role_id().as_uuid())
        .bind(scope_key(assignment.scope()))
        .bind(assignment_scope.mode)
        .bind(assignment_scope.holding_id)
        .bind(assignment_scope.company_id)
        .bind(assignment_scope.branch_id)
        .bind(&assignment_scope.company_ids)
        .bind(&assignment_scope.branch_ids)
        .bind(risk_name(assignment.risk_tier()))
        .bind(assignment.valid_from())
        .bind(assignment.valid_until())
        .bind(assignment.grant_reason())
        .bind(assignment.ticket_reference())
        .bind(assignment.grantor_ref())
        .bind(assignment.approver_ref())
        .bind(assignment.revoked_at())
        .bind(to_i64(version)?)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        require_one_row(changed, "Identity assignment immutable fields changed")?;
    }

    for membership in principal.group_memberships() {
        let changed = sqlx::query(
            "INSERT INTO erpx_identity_access_write.group_membership_authority
                (holding_id,group_id,member_principal_id,membership_version,valid_from,
                 valid_until,revoked_at,principal_version)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (holding_id,group_id,member_principal_id) DO UPDATE SET
                membership_version=EXCLUDED.membership_version,
                revoked_at=EXCLUDED.revoked_at,principal_version=EXCLUDED.principal_version
             WHERE erpx_identity_access_write.group_membership_authority.valid_from=EXCLUDED.valid_from
               AND erpx_identity_access_write.group_membership_authority.valid_until=EXCLUDED.valid_until
               AND erpx_identity_access_write.group_membership_authority.membership_version
                   <= EXCLUDED.membership_version",
        )
        .bind(principal.id().as_uuid())
        .bind(membership.member_principal_id().as_uuid())
        .bind(to_i64(membership.version())?)
        .bind(membership.valid_from())
        .bind(membership.valid_until())
        .bind(membership.revoked_at())
        .bind(to_i64(version)?)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        require_one_row(
            changed,
            "Identity group membership immutable fields changed",
        )?;
    }
    Ok(())
}

async fn apply_role_authority(
    tx: &mut Transaction<'_, Postgres>,
    role: &AccessRole,
    version: u64,
) -> Result<(), CqrsSqlxError> {
    let scope = ScopeColumns::from_scope_store(role.scope())?;
    let changed = sqlx::query(
        "INSERT INTO erpx_identity_access_write.role_authority
            (holding_id,role_id,aggregate_version,scope_key,scope_mode,holding_id,company_id,
             branch_id,company_ids,branch_ids,role_code,display_name,risk_tier,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT (holding_id,role_id) DO UPDATE SET
            aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
            scope_mode=EXCLUDED.scope_mode,holding_id=EXCLUDED.holding_id,
            company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
            company_ids=EXCLUDED.company_ids,branch_ids=EXCLUDED.branch_ids,
            role_code=EXCLUDED.role_code,display_name=EXCLUDED.display_name,
            risk_tier=EXCLUDED.risk_tier,created_at=EXCLUDED.created_at,
            updated_at=EXCLUDED.updated_at
         WHERE erpx_identity_access_write.role_authority.holding_id=
                   erpx_cqrs_write.current_holding_id()
           AND erpx_identity_access_write.role_authority.aggregate_version + 1=
                   EXCLUDED.aggregate_version",
    )
    .bind(role.id().as_uuid())
    .bind(to_i64(version)?)
    .bind(scope_key(role.scope()))
    .bind(scope.mode)
    .bind(scope.holding_id)
    .bind(scope.company_id)
    .bind(scope.branch_id)
    .bind(&scope.company_ids)
    .bind(&scope.branch_ids)
    .bind(role.code().as_str())
    .bind(role.display_name())
    .bind(risk_name(role.risk_tier()))
    .bind(role.created_at())
    .bind(role.updated_at())
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require_one_row(changed, "Identity role version fence")?;
    sqlx::query(
        "DELETE FROM erpx_identity_access_write.role_permission_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND role_id=$1",
    )
    .bind(role.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    for permission in role.permissions() {
        sqlx::query(
            "INSERT INTO erpx_identity_access_write.role_permission_authority
                (holding_id,role_id,permission_key,role_version)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3)",
        )
        .bind(role.id().as_uuid())
        .bind(permission.as_str())
        .bind(to_i64(version)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn scope_from_row(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<AuthorityScope, PortError> {
    let column = |name: &str| format!("{prefix}{name}");
    let mode: String = row
        .try_get(column("scope_mode").as_str())
        .map_err(|_| PortError::Internal)?;
    let holding_id: Option<Uuid> = row
        .try_get(column("holding_id").as_str())
        .map_err(|_| PortError::Internal)?;
    let company_id: Option<Uuid> = row
        .try_get(column("company_id").as_str())
        .map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row
        .try_get(column("branch_id").as_str())
        .map_err(|_| PortError::Internal)?;
    let company_ids: Vec<Uuid> = row
        .try_get(column("company_ids").as_str())
        .map_err(|_| PortError::Internal)?;
    let branch_ids: Vec<Uuid> = row
        .try_get(column("branch_ids").as_str())
        .map_err(|_| PortError::Internal)?;
    let scope = match mode.as_str() {
        "platform" if holding_id.is_none() && company_id.is_none() && branch_id.is_none() => {
            AuthorityScope::Platform
        }
        "holding" => AuthorityScope::Holding {
            holding_id: holding_id.ok_or(PortError::Internal)?,
        },
        "company" => AuthorityScope::Company {
            holding_id: holding_id.ok_or(PortError::Internal)?,
            company_id: company_id.ok_or(PortError::Internal)?,
        },
        "branch" => AuthorityScope::Branch {
            holding_id: holding_id.ok_or(PortError::Internal)?,
            company_id: company_id.ok_or(PortError::Internal)?,
            branch_id: branch_id.ok_or(PortError::Internal)?,
        },
        "subtree" => AuthorityScope::Subtree {
            holding_id: holding_id.ok_or(PortError::Internal)?,
            organization_node_id: single_subtree_node(&company_ids, &branch_ids)?,
        },
        _ => return Err(PortError::Internal),
    };
    scope.validate().map_err(|_| PortError::Internal)?;
    Ok(scope)
}

fn single_subtree_node(company_ids: &[Uuid], branch_ids: &[Uuid]) -> Result<Uuid, PortError> {
    let mut nodes = company_ids.iter().chain(branch_ids).copied();
    let node = nodes.next().ok_or(PortError::Internal)?;
    if node.is_nil() || nodes.next().is_some() {
        return Err(PortError::Internal);
    }
    Ok(node)
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn parse_principal_kind(value: &str) -> Result<PrincipalKind, PortError> {
    match value {
        "human" => Ok(PrincipalKind::Human),
        "service" => Ok(PrincipalKind::Service),
        "agent" => Ok(PrincipalKind::Agent),
        "human_group" => Ok(PrincipalKind::HumanGroup),
        "agent_pool" => Ok(PrincipalKind::AgentPool),
        _ => Err(PortError::Internal),
    }
}

fn parse_principal_realm(value: &str) -> Result<PrincipalRealm, PortError> {
    match value {
        "internal_workforce" => Ok(PrincipalRealm::InternalWorkforce),
        "platform_workforce" => Ok(PrincipalRealm::PlatformWorkforce),
        "customer_portal" => Ok(PrincipalRealm::CustomerPortal),
        _ => Err(PortError::Internal),
    }
}

fn parse_principal_status(value: &str) -> Result<PrincipalStatus, PortError> {
    match value {
        "invited" => Ok(PrincipalStatus::Invited),
        "active" => Ok(PrincipalStatus::Active),
        "suspended" => Ok(PrincipalStatus::Suspended),
        "disabled" => Ok(PrincipalStatus::Disabled),
        _ => Err(PortError::Internal),
    }
}

fn principal_status_name(value: PrincipalStatus) -> &'static str {
    match value {
        PrincipalStatus::Invited => "invited",
        PrincipalStatus::Active => "active",
        PrincipalStatus::Suspended => "suspended",
        PrincipalStatus::Disabled => "disabled",
    }
}

fn parse_risk(value: &str) -> Result<RiskTier, PortError> {
    match value {
        "platform_root" => Ok(RiskTier::PlatformRoot),
        "administrator" => Ok(RiskTier::Administrator),
        "high" => Ok(RiskTier::High),
        "standard" => Ok(RiskTier::Standard),
        "contractor" => Ok(RiskTier::Contractor),
        "support_diagnostic" => Ok(RiskTier::SupportDiagnostic),
        "support_mutation" => Ok(RiskTier::SupportMutation),
        "emergency" => Ok(RiskTier::Emergency),
        _ => Err(PortError::Internal),
    }
}

fn require_one_row(rows: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

const fn principal_kind_name(kind: erpx_identity_access::PrincipalKind) -> &'static str {
    match kind {
        erpx_identity_access::PrincipalKind::Human => "human",
        erpx_identity_access::PrincipalKind::Service => "service",
        erpx_identity_access::PrincipalKind::Agent => "agent",
        erpx_identity_access::PrincipalKind::HumanGroup => "human_group",
        erpx_identity_access::PrincipalKind::AgentPool => "agent_pool",
    }
}

const fn principal_realm_name(realm: erpx_identity_access::PrincipalRealm) -> &'static str {
    match realm {
        erpx_identity_access::PrincipalRealm::InternalWorkforce => "internal_workforce",
        erpx_identity_access::PrincipalRealm::PlatformWorkforce => "platform_workforce",
        erpx_identity_access::PrincipalRealm::CustomerPortal => "customer_portal",
    }
}

fn validate_guard_context(
    context: &str,
    aggregate_type: &str,
    aggregate_id: AggregateId,
    aggregate: &IdentityAccessAggregate,
) -> Result<(), CqrsSqlxError> {
    if context != CONTEXT_NAME
        || aggregate_type != aggregate.aggregate_type()
        || aggregate_id.as_uuid() != aggregate.aggregate_uuid()
    {
        return Err(CqrsSqlxError::GuardRejected(
            "guard received mismatched aggregate metadata",
        ));
    }
    Ok(())
}

fn validate_commit(request: &IdentityAccessCommit) -> Result<(), PortError> {
    validate_aggregate(&request.aggregate)?;
    let key = fingerprint_key(&request.identity.fingerprint);
    if key.as_uuid() != request.aggregate.aggregate_uuid()
        || key.aggregate_type() != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn validate_aggregate(aggregate: &IdentityAccessAggregate) -> Result<(), PortError> {
    aggregate
        .scope()
        .validate()
        .map_err(|_| PortError::Internal)?;
    match aggregate {
        IdentityAccessAggregate::Principal(principal) => {
            if principal.version() == 0 || principal.display_name().trim().is_empty() {
                return Err(PortError::Internal);
            }
            for assignment in principal.assignments() {
                if !principal.scope().contains(assignment.scope())
                    || assignment.valid_until() <= assignment.valid_from()
                {
                    return Err(PortError::Internal);
                }
            }
        }
        IdentityAccessAggregate::Role(role) => {
            if role.version() == 0
                || role.display_name().trim().is_empty()
                || role.permissions().is_empty()
            {
                return Err(PortError::Internal);
            }
        }
    }
    Ok(())
}

fn key_parts(key: AggregateKey) -> Result<(String, &'static str, AggregateId), PortError> {
    let (partition, aggregate_type, uuid) = match key {
        AggregateKey::Principal(id) => (
            principal_partition(id),
            PRINCIPAL_AGGREGATE_TYPE,
            id.as_uuid(),
        ),
        AggregateKey::Role(id) => (role_partition(id), ROLE_AGGREGATE_TYPE, id.as_uuid()),
    };
    Ok((
        partition,
        aggregate_type,
        AggregateId::try_from_uuid(uuid).map_err(|_| PortError::Internal)?,
    ))
}

fn fingerprint_key(fingerprint: &CommandFingerprint) -> AggregateKey {
    match fingerprint {
        CommandFingerprint::CreatePrincipal { principal_id, .. }
        | CommandFingerprint::CreateAuthorityGroup {
            group_id: principal_id,
            ..
        }
        | CommandFingerprint::ActivatePrincipal { principal_id }
        | CommandFingerprint::SuspendPrincipal { principal_id }
        | CommandFingerprint::ReactivatePrincipal { principal_id }
        | CommandFingerprint::DisablePrincipal { principal_id }
        | CommandFingerprint::AssignRole { principal_id, .. }
        | CommandFingerprint::RenewRole { principal_id, .. }
        | CommandFingerprint::RecoverRole { principal_id, .. }
        | CommandFingerprint::RevokeRole { principal_id, .. }
        | CommandFingerprint::AddGroupMember {
            group_id: principal_id,
            ..
        }
        | CommandFingerprint::RevokeGroupMember {
            group_id: principal_id,
            ..
        } => AggregateKey::Principal(*principal_id),
        CommandFingerprint::DefineRole { role_id, .. }
        | CommandFingerprint::SetRolePermissions { role_id, .. } => AggregateKey::Role(*role_id),
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.identity-access.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
        .expect("a SHA-256-derived UUID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::CreatePrincipal { .. } => "identity_access.principal.create",
        CommandFingerprint::CreateAuthorityGroup { .. } => "identity_access.authority_group.create",
        CommandFingerprint::ActivatePrincipal { .. } => "identity_access.principal.activate",
        CommandFingerprint::SuspendPrincipal { .. } => "identity_access.principal.suspend",
        CommandFingerprint::ReactivatePrincipal { .. } => "identity_access.principal.reactivate",
        CommandFingerprint::DisablePrincipal { .. } => "identity_access.principal.disable",
        CommandFingerprint::AssignRole { .. } => "identity_access.principal.assign_role",
        CommandFingerprint::RenewRole { .. } => "identity_access.principal.renew_role",
        CommandFingerprint::RecoverRole { .. } => "identity_access.principal.recover_role",
        CommandFingerprint::RevokeRole { .. } => "identity_access.principal.revoke_role",
        CommandFingerprint::AddGroupMember { .. } => "identity_access.authority_group.add_member",
        CommandFingerprint::RevokeGroupMember { .. } => {
            "identity_access.authority_group.revoke_member"
        }
        CommandFingerprint::DefineRole { .. } => "identity_access.role.define",
        CommandFingerprint::SetRolePermissions { .. } => "identity_access.role.set_permissions",
    }
}

fn aggregate_updated_at(aggregate: &IdentityAccessAggregate) -> chrono::DateTime<chrono::Utc> {
    match aggregate {
        IdentityAccessAggregate::Principal(value) => value.updated_at(),
        IdentityAccessAggregate::Role(value) => value.updated_at(),
    }
}

pub(crate) fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".into(),
        AuthorityScope::Holding { holding_id } => format!("holding:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("subtree:{holding_id}:{organization_node_id}"),
    }
}

pub(crate) const fn risk_name(risk: RiskTier) -> &'static str {
    match risk {
        RiskTier::PlatformRoot => "platform_root",
        RiskTier::Administrator => "administrator",
        RiskTier::High => "high",
        RiskTier::Standard => "standard",
        RiskTier::Contractor => "contractor",
        RiskTier::SupportDiagnostic => "support_diagnostic",
        RiskTier::SupportMutation => "support_mutation",
        RiskTier::Emergency => "emergency",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<IdentityAccessAggregate>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.aggregate_uuid()
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    validate_aggregate(&receipt.response)?;
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(error)
            if error
                .as_database_error()
                .and_then(|value| value.code())
                .is_some_and(|code| code == "23505") =>
        {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

#[cfg(test)]
mod tests {
    use erpx_identity_access::PrincipalId;

    use super::*;

    #[test]
    fn fingerprints_are_stable_and_payload_sensitive() {
        let principal_id = PrincipalId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let first = CommandFingerprint::CreatePrincipal {
            principal_id,
            scope: AuthorityScope::Platform,
            display_name: "Ada".into(),
        };
        let changed = CommandFingerprint::CreatePrincipal {
            principal_id,
            scope: AuthorityScope::Platform,
            display_name: "Grace".into(),
        };
        assert_eq!(request_hash(&first).unwrap(), request_hash(&first).unwrap());
        assert_ne!(
            request_hash(&first).unwrap(),
            request_hash(&changed).unwrap()
        );
    }

    #[test]
    fn canonical_scope_keys_are_order_insensitive_for_subtrees() {
        let left = AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(2),
        };
        let right = AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(2),
        };
        assert_eq!(scope_key(&left), scope_key(&right));
    }

    #[test]
    fn deterministic_ids_are_namespaced() {
        let command = CommandId::try_from_uuid(Uuid::from_u128(9)).unwrap();
        assert_eq!(
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0),
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0)
        );
        assert_ne!(
            deterministic_event_id(command, PRINCIPAL_SNAPSHOT_EVENT_TYPE, 0),
            deterministic_event_id(command, ROLE_SNAPSHOT_EVENT_TYPE, 0)
        );
    }
}
