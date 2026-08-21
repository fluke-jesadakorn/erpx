//! Live acceptance proof for the Item Directory's Organization lineage cache.
//!
//! The verify harness owns and drops both databases. This test deliberately
//! leaves randomly holding-scoped evidence behind for diagnosis and refuses to
//! connect outside the erpx_verify_tw_ / erpx_verify_tr_ database fences.

use std::{env, error::Error, str::FromStr};

use chrono::{DateTime, Duration, Utc};
use erpx_collaboration_sqlx::{
    DirectoryAccess, DirectoryDescriptorPublication, DirectoryLookup, DirectorySearchRequest,
    ItemDirectoryProjectionAdapter, SqlxItemDirectory,
};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionPool, ProjectionSource, WritePool};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityAssignmentEvidence,
    AuthorityEvidenceFact, AuthorityPrincipalKind, AuthorityRealm, ItemDescriptor,
    PrincipalAuthorityEvidence, RecordTypeId, RoleAuthorityEvidence, StableRecordRef,
    catalog::ContextId,
    organization::{
        self, ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE, ORGANIZATION_LINEAGE_CHANGED_VERSION,
        OrganizationLineageKey, OrganizationLineageWitness, OrganizationNodeKind,
        OrganizationNodeStatus,
    },
    record_policy,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, AuthorityScope, CommandId, CorrelationId,
    DataClassification, EventId, HoldingId, PrincipalId, RecordId,
};
use erpx_worker::HoldingAuthorityEvidenceProjection;
use sqlx::{
    Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_HOLDING_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";

#[test]
#[ignore = "requires disposable PostgreSQL databases prepared by cargo xtask verify-sqlx"]
fn item_directory_lineage_sqlx_acceptance() {
    let Some(write_url) = configured_url(WRITE_DATABASE_URL) else {
        eprintln!(
            "Item Directory lineage SQLx acceptance skipped: {WRITE_DATABASE_URL} is not set"
        );
        return;
    };
    let Some(read_url) = configured_url(READ_DATABASE_URL) else {
        eprintln!("Item Directory lineage SQLx acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };
    require_verify_database(&write_url, "erpx_verify_tw_")
        .expect("the write URL must target the verify-sqlx disposable database");
    require_verify_database(&read_url, "erpx_verify_tr_")
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("Item Directory lineage SQLx acceptance must pass");
    });
}

fn configured_url(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn require_verify_database(url: &str, expected_prefix: &str) -> Result<(), Box<dyn Error>> {
    let options = PgConnectOptions::from_str(url)?;
    let database = options
        .get_database()
        .ok_or("the PostgreSQL URL does not name a database")?;
    if !database.starts_with(expected_prefix)
        || !database
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!(
            "refusing non-disposable Item Directory acceptance database {database}"
        )
        .into());
    }
    Ok(())
}

async fn run_acceptance(write_url: &str, read_url: &str) -> Result<(), Box<dyn Error>> {
    let write_database = PgPoolOptions::new()
        .max_connections(4)
        .connect(write_url)
        .await?;
    let read_database = PgPoolOptions::new()
        .max_connections(4)
        .connect(read_url)
        .await?;

    let fixture = Fixture::new()?;
    let holding_id = fixture.holding_id;
    let projection_pool = ProjectionPool::for_holding(read_database.clone(), holding_id.as_uuid())?;
    let write_pool = WritePool::for_holding(write_database.clone(), holding_id.as_uuid())?;
    let durable_source = ProjectionSource::new(write_pool.clone());
    let authority = HoldingAuthorityEvidenceProjection::new(
        holding_id,
        projection_pool.clone(),
        write_pool.clone(),
    );
    let directory = ItemDirectoryProjectionAdapter::new(read_database.clone());
    let query = SqlxItemDirectory::new(read_database.clone());
    let now = Utc::now();

    assert_delivery_retry_cannot_create_a_durable_cursor_hole(
        &write_pool,
        &durable_source,
        &fixture,
        now,
    )
    .await?;

    // A ready checkpoint lets the query adapter exercise its real fail-closed
    // authorization boundary instead of returning a projection-health result.
    directory.begin_rebuild(holding_id, 1, 1_000, now).await?;
    directory
        .complete_rebuild(holding_id, 1, 1_000, now)
        .await?;

    let mut initial_lineage = fixture.initial_lineage_events(now)?;
    let missing_branch_one = initial_lineage.remove(2);
    for event in initial_lineage {
        authority.apply_organization_lineage(&event).await?;
    }

    // The Team's aggregate v2 / source position 20 is current. Applying v1 /
    // position 19 afterwards must not roll the disposable cache backwards.
    authority
        .apply_organization_lineage(&fixture.lineage_event(
            fixture.team_witness(
                OrganizationNodeStatus::Active,
                fixture.department_one,
                &[
                    fixture.holding,
                    fixture.company_one,
                    fixture.branch_one,
                    fixture.department_one,
                ],
                1,
            )?,
            19,
            now - Duration::seconds(1),
        )?)
        .await?;
    assert_cached_team(&projection_pool, &fixture, 2, 20, fixture.department_one).await?;

    authority.apply(&fixture.role_event(now)?).await?;
    authority
        .apply(&fixture.principal_event(fixture.company_one_scope(), 1, 60, now)?)
        .await?;
    directory
        .apply_descriptor(&DirectoryDescriptorPublication {
            holding_id,
            descriptor: fixture.descriptor(70)?,
            authorized_search_text: "lineage acceptance purchase order".into(),
            projected_at: now,
        })
        .await?;

    let incomplete = authority.reconcile_grants(&directory, 75, now).await?;
    assert_eq!((incomplete.changed, incomplete.revoked), (0, 0));
    assert_grant(&projection_pool, &fixture, false).await?;
    assert_denied_before_payload_and_count(
        &query,
        &fixture.access(fixture.company_one_scope())?,
        &fixture,
        now,
    )
    .await?;

    // Supplying the one missing active ancestor completes the Organization
    // chain; the same assignment and descriptor can now produce a grant.
    authority
        .apply_organization_lineage(&missing_branch_one)
        .await?;
    let initial = authority.reconcile_grants(&directory, 80, now).await?;
    assert_eq!((initial.changed, initial.revoked), (1, 0));
    assert_grant(&projection_pool, &fixture, true).await?;
    assert_visible(
        &query,
        &fixture.access(fixture.company_one_scope())?,
        &fixture,
        now,
    )
    .await?;

    // Reparent the Team under Company 2. The stale effective grant still
    // exists here, so Neutral resolve and a zero authorized search count prove
    // that current lineage is checked before descriptor/search payloads.
    authority
        .apply_organization_lineage(&fixture.lineage_event(
            fixture.team_witness(
                OrganizationNodeStatus::Active,
                fixture.department_two,
                &[
                    fixture.holding,
                    fixture.company_two,
                    fixture.branch_two,
                    fixture.department_two,
                ],
                3,
            )?,
            90,
            now + Duration::seconds(1),
        )?)
        .await?;
    assert_grant(&projection_pool, &fixture, true).await?;
    assert_denied_before_payload_and_count(
        &query,
        &fixture.access(fixture.company_one_scope())?,
        &fixture,
        now,
    )
    .await?;

    let reparented = authority.reconcile_grants(&directory, 100, now).await?;
    assert_eq!((reparented.changed, reparented.revoked), (0, 1));
    assert_grant(&projection_pool, &fixture, false).await?;

    // A current Branch 2 assignment can see the reparented Team, but a later
    // Organization suspension again denies before the grant is reconciled.
    authority
        .apply(&fixture.principal_event(fixture.branch_two_scope(), 2, 101, now)?)
        .await?;
    let regranted = authority.reconcile_grants(&directory, 110, now).await?;
    assert_eq!((regranted.changed, regranted.revoked), (1, 0));
    let branch_two_access = fixture.access(fixture.branch_two_scope())?;
    assert_visible(&query, &branch_two_access, &fixture, now).await?;

    authority
        .apply_organization_lineage(&fixture.lineage_event(
            fixture.team_witness(
                OrganizationNodeStatus::Suspended,
                fixture.department_two,
                &[
                    fixture.holding,
                    fixture.company_two,
                    fixture.branch_two,
                    fixture.department_two,
                ],
                4,
            )?,
            120,
            now + Duration::seconds(2),
        )?)
        .await?;
    assert_grant(&projection_pool, &fixture, true).await?;
    assert_denied_before_payload_and_count(&query, &branch_two_access, &fixture, now).await?;

    let suspended = authority.reconcile_grants(&directory, 130, now).await?;
    assert_eq!((suspended.changed, suspended.revoked), (0, 1));
    assert_grant(&projection_pool, &fixture, false).await?;

    read_database.close().await;
    write_database.close().await;
    Ok(())
}

async fn assert_delivery_retry_cannot_create_a_durable_cursor_hole(
    write_pool: &WritePool,
    source: &ProjectionSource,
    fixture: &Fixture,
    now: DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    let delayed = fixture.lineage_event(
        fixture.witness(fixture.holding, OrganizationNodeKind::Holding, None, &[], 1)?,
        1,
        now,
    )?;
    let due = fixture.lineage_event(
        fixture.witness(
            fixture.company_one,
            OrganizationNodeKind::Company,
            Some(fixture.holding),
            &[fixture.holding],
            1,
        )?,
        2,
        now,
    )?;
    let delayed_sequence =
        insert_outbox_event(write_pool, &delayed, now + Duration::days(1)).await?;
    let due_sequence = insert_outbox_event(write_pool, &due, now - Duration::seconds(1)).await?;
    assert!(delayed_sequence < due_sequence);

    let event_types = vec![ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE.to_owned()];
    let delivery_page = source
        .page_between(0, Some(due_sequence), &event_types, 10)
        .await?;
    assert_eq!(
        delivery_page
            .events
            .iter()
            .map(|event| event.event_id)
            .collect::<Vec<_>>(),
        vec![due.event_id]
    );

    let durable_high_watermark = source.durable_high_watermark(&event_types).await?;
    assert_eq!(durable_high_watermark, due_sequence);
    let durable_page = source
        .page_durable_between(0, Some(durable_high_watermark), &event_types, 10)
        .await?;
    assert_eq!(
        durable_page
            .events
            .iter()
            .map(|event| event.event_id)
            .collect::<Vec<_>>(),
        vec![delayed.event_id, due.event_id]
    );
    Ok(())
}

async fn insert_outbox_event(
    pool: &WritePool,
    event: &ProjectionEvent,
    available_at: DateTime<Utc>,
) -> Result<u64, Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,
             correlation_id,causation_id,payload,occurred_at,available_at,holding_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                 erpx_cqrs_write.current_holding_id())
         RETURNING global_sequence",
    )
    .bind(event.event_id.as_uuid())
    .bind(&event.context)
    .bind(&event.partition)
    .bind(event.command_id.as_uuid())
    .bind(&event.event_type)
    .bind(i32::try_from(event.event_version)?)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id.as_uuid())
    .bind(i64::try_from(event.aggregate_version)?)
    .bind(serde_json::to_value(&event.active_scope)?)
    .bind(serde_json::to_value(&event.actor)?)
    .bind(event.correlation_id.as_uuid())
    .bind(event.causation_id.map(EventId::as_uuid))
    .bind(event.payload.clone())
    .bind(event.occurred_at)
    .bind(available_at)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(u64::try_from(sequence)?)
}

struct Fixture {
    holding_id: HoldingId,
    holding: Uuid,
    company_one: Uuid,
    branch_one: Uuid,
    department_one: Uuid,
    company_two: Uuid,
    branch_two: Uuid,
    department_two: Uuid,
    team: Uuid,
    principal_id: PrincipalId,
    role_id: Uuid,
    assignment_id: Uuid,
    record_id: RecordId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let holding = Uuid::new_v4();
        Ok(Self {
            holding_id: HoldingId::try_from_uuid(holding)?,
            holding,
            company_one: Uuid::new_v4(),
            branch_one: Uuid::new_v4(),
            department_one: Uuid::new_v4(),
            company_two: Uuid::new_v4(),
            branch_two: Uuid::new_v4(),
            department_two: Uuid::new_v4(),
            team: Uuid::new_v4(),
            principal_id: PrincipalId::try_from_uuid(Uuid::new_v4())?,
            role_id: Uuid::new_v4(),
            assignment_id: Uuid::new_v4(),
            record_id: RecordId::try_from_uuid(Uuid::new_v4())?,
        })
    }

    fn company_one_scope(&self) -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: self.holding,
            company_id: self.company_one,
        }
    }

    fn branch_two_scope(&self) -> AuthorityScope {
        AuthorityScope::Branch {
            holding_id: self.holding,
            company_id: self.company_two,
            branch_id: self.branch_two,
        }
    }

    fn team_scope(&self) -> AuthorityScope {
        AuthorityScope::Subtree {
            holding_id: self.holding,
            organization_node_id: self.team,
        }
    }

    fn access(&self, scope: AuthorityScope) -> Result<DirectoryAccess, Box<dyn Error>> {
        Ok(DirectoryAccess::try_from_authorization(
            self.holding_id,
            self.principal_id,
            scope,
        )?)
    }

    fn descriptor(&self, position: u64) -> Result<ItemDescriptor, Box<dyn Error>> {
        let record = StableRecordRef::try_new(
            self.record_id,
            ContextId::Procurement,
            RecordTypeId::try_new("procurement.purchase-order")?,
        )?;
        let policy = record_policy(record.record_type().as_str())
            .ok_or("the purchase-order record policy must exist")?;
        Ok(ItemDescriptor::try_new_with_display_label(
            record,
            self.team_scope(),
            "open",
            "Lineage purchase order",
            policy.policy_id(),
            DataClassification::Confidential,
            1,
            position,
        )?)
    }

    fn initial_lineage_events(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ProjectionEvent>, Box<dyn Error>> {
        let witnesses = [
            (
                self.witness(self.holding, OrganizationNodeKind::Holding, None, &[], 1)?,
                10,
            ),
            (
                self.witness(
                    self.company_one,
                    OrganizationNodeKind::Company,
                    Some(self.holding),
                    &[self.holding],
                    1,
                )?,
                11,
            ),
            (
                self.witness(
                    self.branch_one,
                    OrganizationNodeKind::Branch,
                    Some(self.company_one),
                    &[self.holding, self.company_one],
                    1,
                )?,
                12,
            ),
            (
                self.witness(
                    self.department_one,
                    OrganizationNodeKind::Department,
                    Some(self.branch_one),
                    &[self.holding, self.company_one, self.branch_one],
                    1,
                )?,
                13,
            ),
            (
                self.witness(
                    self.company_two,
                    OrganizationNodeKind::Company,
                    Some(self.holding),
                    &[self.holding],
                    1,
                )?,
                14,
            ),
            (
                self.witness(
                    self.branch_two,
                    OrganizationNodeKind::Branch,
                    Some(self.company_two),
                    &[self.holding, self.company_two],
                    1,
                )?,
                15,
            ),
            (
                self.witness(
                    self.department_two,
                    OrganizationNodeKind::Department,
                    Some(self.branch_two),
                    &[self.holding, self.company_two, self.branch_two],
                    1,
                )?,
                16,
            ),
            (
                self.team_witness(
                    OrganizationNodeStatus::Active,
                    self.department_one,
                    &[
                        self.holding,
                        self.company_one,
                        self.branch_one,
                        self.department_one,
                    ],
                    2,
                )?,
                20,
            ),
        ];
        witnesses
            .into_iter()
            .map(|(witness, position)| self.lineage_event(witness, position, now))
            .collect()
    }

    fn witness(
        &self,
        node_id: Uuid,
        kind: OrganizationNodeKind,
        parent_node_id: Option<Uuid>,
        lineage_node_ids: &[Uuid],
        aggregate_version: u64,
    ) -> Result<OrganizationLineageWitness, Box<dyn Error>> {
        Ok(OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(self.holding, node_id)?,
            kind,
            OrganizationNodeStatus::Active,
            parent_node_id,
            lineage_node_ids.to_vec(),
            aggregate_version,
        )?)
    }

    fn team_witness(
        &self,
        status: OrganizationNodeStatus,
        parent_node_id: Uuid,
        lineage_node_ids: &[Uuid],
        aggregate_version: u64,
    ) -> Result<OrganizationLineageWitness, Box<dyn Error>> {
        Ok(OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(self.holding, self.team)?,
            OrganizationNodeKind::Team,
            status,
            Some(parent_node_id),
            lineage_node_ids.to_vec(),
            aggregate_version,
        )?)
    }

    fn lineage_event(
        &self,
        witness: OrganizationLineageWitness,
        global_sequence: u64,
        occurred_at: DateTime<Utc>,
    ) -> Result<ProjectionEvent, Box<dyn Error>> {
        let node_id = witness.key().node_id().as_uuid();
        let active_scope = match witness.kind() {
            OrganizationNodeKind::Holding => AuthorityScope::Holding {
                holding_id: self.holding,
            },
            OrganizationNodeKind::Company => AuthorityScope::Company {
                holding_id: self.holding,
                company_id: node_id,
            },
            OrganizationNodeKind::Headquarters => AuthorityScope::Company {
                holding_id: self.holding,
                company_id: witness
                    .company_id()
                    .ok_or("headquarters witness must identify its company")?
                    .as_uuid(),
            },
            OrganizationNodeKind::Branch => AuthorityScope::Branch {
                holding_id: self.holding,
                company_id: witness
                    .company_id()
                    .ok_or("branch witness must identify its company")?
                    .as_uuid(),
                branch_id: node_id,
            },
            OrganizationNodeKind::Department | OrganizationNodeKind::Team => {
                AuthorityScope::Subtree {
                    holding_id: self.holding,
                    organization_node_id: node_id,
                }
            }
        };
        Ok(ProjectionEvent {
            event_id: EventId::try_from_uuid(Uuid::new_v4())?,
            global_sequence,
            context: "organization".into(),
            partition: format!("organization.node:{}", node_id.simple()),
            command_id: CommandId::try_from_uuid(Uuid::new_v4())?,
            event_type: ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE.into(),
            event_version: u32::from(ORGANIZATION_LINEAGE_CHANGED_VERSION),
            aggregate_type: "organization_node".into(),
            aggregate_id: AggregateId::try_from_uuid(node_id)?,
            aggregate_version: witness.aggregate_version(),
            active_scope,
            actor: worker_actor(),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())?,
            causation_id: None,
            payload: serde_json::to_value(organization::Event::OrganizationLineageChanged(
                witness,
            ))?,
            occurred_at,
        })
    }

    fn role_event(&self, now: DateTime<Utc>) -> Result<ProjectionEvent, Box<dyn Error>> {
        self.authority_event(
            AuthorityEvidenceFact::Role(RoleAuthorityEvidence {
                role_id: self.role_id,
                role_version: 1,
                scope: AuthorityScope::Holding {
                    holding_id: self.holding,
                },
                permissions: vec!["procurement.read".into()],
                classification_ceiling: DataClassification::Confidential,
                active: true,
                changed_at: now,
            }),
            self.role_id,
            "access_role",
            1,
            59,
            now,
        )
    }

    fn principal_event(
        &self,
        scope: AuthorityScope,
        version: u64,
        global_sequence: u64,
        now: DateTime<Utc>,
    ) -> Result<ProjectionEvent, Box<dyn Error>> {
        self.authority_event(
            AuthorityEvidenceFact::Principal(PrincipalAuthorityEvidence {
                principal_id: self.principal_id,
                principal_kind: AuthorityPrincipalKind::Human,
                realm: AuthorityRealm::InternalWorkforce,
                active: true,
                principal_version: version,
                scope: scope.clone(),
                assignments: vec![AuthorityAssignmentEvidence {
                    assignment_id: self.assignment_id,
                    role_id: self.role_id,
                    assignment_version: version,
                    scope,
                    valid_from: now - Duration::minutes(1),
                    valid_until: now + Duration::hours(1),
                    revoked_at: None,
                }],
                group_memberships: Vec::new(),
                changed_at: now,
            }),
            self.principal_id.as_uuid(),
            "access_principal",
            version,
            global_sequence,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn authority_event(
        &self,
        fact: AuthorityEvidenceFact,
        aggregate_id: Uuid,
        aggregate_type: &str,
        aggregate_version: u64,
        global_sequence: u64,
        occurred_at: DateTime<Utc>,
    ) -> Result<ProjectionEvent, Box<dyn Error>> {
        Ok(ProjectionEvent {
            event_id: EventId::try_from_uuid(Uuid::new_v4())?,
            global_sequence,
            context: "identity_access".into(),
            partition: format!("identity_access.authority:{}", aggregate_id.simple()),
            command_id: CommandId::try_from_uuid(Uuid::new_v4())?,
            event_type: AUTHORITY_EVIDENCE_EVENT_TYPE.into(),
            event_version: AUTHORITY_EVIDENCE_VERSION,
            aggregate_type: aggregate_type.into(),
            aggregate_id: AggregateId::try_from_uuid(aggregate_id)?,
            aggregate_version,
            active_scope: AuthorityScope::Holding {
                holding_id: self.holding,
            },
            actor: worker_actor(),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())?,
            causation_id: None,
            payload: serde_json::to_value(fact)?,
            occurred_at,
        })
    }
}

fn worker_actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "worker:item-directory-lineage-acceptance".into(),
        kind: ActorKind::SystemWorker,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

async fn assert_cached_team(
    pool: &ProjectionPool,
    fixture: &Fixture,
    expected_version: i64,
    expected_position: i64,
    expected_parent: Uuid,
) -> Result<(), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT aggregate_version,source_event_position,parent_node_id
         FROM erpx_authority_read.organization_nodes
         WHERE holding_id=$1 AND node_id=$2",
    )
    .bind(fixture.holding)
    .bind(fixture.team)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    assert_eq!(
        row.try_get::<i64, _>("aggregate_version")?,
        expected_version
    );
    assert_eq!(
        row.try_get::<i64, _>("source_event_position")?,
        expected_position
    );
    assert_eq!(row.try_get::<Uuid, _>("parent_node_id")?, expected_parent);
    Ok(())
}

async fn assert_grant(
    pool: &ProjectionPool,
    fixture: &Fixture,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM erpx_collaboration_read.item_directory_effective_grants
             WHERE holding_id=$1 AND record_id=$2 AND principal_id=$3
         )",
    )
    .bind(fixture.holding)
    .bind(fixture.record_id.as_uuid())
    .bind(fixture.principal_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    assert_eq!(exists, expected);
    Ok(())
}

async fn assert_visible(
    directory: &SqlxItemDirectory,
    access: &DirectoryAccess,
    fixture: &Fixture,
    now: DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    let resolved = directory.resolve(access, fixture.record_id, now).await?;
    assert!(matches!(resolved, DirectoryLookup::Visible { .. }));
    let searched = directory
        .search(
            access,
            &DirectorySearchRequest {
                query: "lineage".into(),
                page_size: 10,
                cursor: None,
            },
            now,
        )
        .await?;
    let DirectoryLookup::Visible { value, .. } = searched else {
        panic!("a complete active lineage must produce an authorized search page")
    };
    assert_eq!(value.authorized_count, 1);
    assert_eq!(value.items.len(), 1);
    Ok(())
}

async fn assert_denied_before_payload_and_count(
    directory: &SqlxItemDirectory,
    access: &DirectoryAccess,
    fixture: &Fixture,
    now: DateTime<Utc>,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        directory.resolve(access, fixture.record_id, now).await?,
        DirectoryLookup::Neutral
    );
    let searched = directory
        .search(
            access,
            &DirectorySearchRequest {
                query: "lineage".into(),
                page_size: 10,
                cursor: None,
            },
            now,
        )
        .await?;
    let DirectoryLookup::Visible { value, .. } = searched else {
        panic!("a fresh projection must return a neutral empty authorized page")
    };
    assert!(value.items.is_empty());
    assert_eq!(value.authorized_count, 0);
    Ok(())
}
