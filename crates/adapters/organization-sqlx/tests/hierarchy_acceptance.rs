use std::{collections::BTreeMap, env, error::Error, str::FromStr};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, IdempotencyKey,
    QueryConsistency, QueryEnvelope, QueryHandler,
};
use erpx_cqrs_sqlx::{
    ProjectionPool, ProjectionRunner, ProjectionRunnerConfig, ProjectionSource, ProjectionStore,
    ReadPool, WritePool,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, FixedClock, QueryId, SessionId,
};
use erpx_organization::{
    ActivateOrganizationNode, ArchiveOrganizationNode, AuthorizationPort, CreateOrganizationNode,
    CreateOrganizationNodeResult, EntitlementPort, GetOrganizationNode, ListOrganizationNodes,
    NodeList, NodeListFilter, NodeView, OrganizationAction, OrganizationCommandHandler,
    OrganizationMutationResult, OrganizationNodeId, OrganizationNodeKind, OrganizationNodeStatus,
    OrganizationPlacement, OrganizationQueryHandler, PortError, ReparentOrganizationNode,
    SuspendOrganizationNode, ports::AuthorizationRequest,
};
use erpx_organization_sqlx::{
    AGGREGATE_TYPE, CONTEXT_NAME, LEGACY_NODE_SNAPSHOT_EVENT_VERSION, NODE_SNAPSHOT_EVENT_VERSION,
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationNodeProjector,
    OrganizationOutboxPayload, OrganizationReadRepository, OrganizationSnapshot,
    OrganizationWriteRepository, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE, node_partition,
};
use serde_json::Value;
use sqlx::{
    Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const WRITE_DATABASE_URL: &str = "ERPX_TEST_HOLDING_WRITE_DATABASE_URL";
const READ_DATABASE_URL: &str = "ERPX_TEST_HOLDING_READ_DATABASE_URL";

type AcceptanceCommandHandler = OrganizationCommandHandler<
    OrganizationWriteRepository,
    OrganizationCommandAdapter,
    AcceptanceAccess,
    AcceptanceAccess,
    OrganizationHierarchyRepository,
    FixedClock,
>;
type AcceptanceQueryHandler =
    OrganizationQueryHandler<OrganizationReadRepository, AcceptanceAccess, AcceptanceAccess>;

#[test]
#[ignore = "requires disposable PostgreSQL databases prepared by cargo xtask verify-sqlx"]
fn organization_hierarchy_sqlx_acceptance() {
    let Some(write_url) = configured_url(WRITE_DATABASE_URL) else {
        eprintln!(
            "Organization hierarchy SQLx acceptance skipped: {WRITE_DATABASE_URL} is not set"
        );
        return;
    };
    let Some(read_url) = configured_url(READ_DATABASE_URL) else {
        eprintln!("Organization hierarchy SQLx acceptance skipped: {READ_DATABASE_URL} is not set");
        return;
    };

    // This harness intentionally leaves its randomly scoped evidence in place
    // for diagnosis. The xtask owns and drops these disposable databases after
    // the ignored suite; refusing any other database prevents retained writes.
    require_verify_database(&write_url, "erpx_verify_tw_")
        .expect("the write URL must target the verify-sqlx disposable database");
    require_verify_database(&read_url, "erpx_verify_tr_")
        .expect("the read URL must target the verify-sqlx disposable database");

    sqlx::__rt::test_block_on(async move {
        run_acceptance(&write_url, &read_url)
            .await
            .expect("Organization hierarchy SQLx acceptance must pass");
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
        return Err(
            format!("refusing non-disposable Organization acceptance database {database}").into(),
        );
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

    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_one_id = Uuid::new_v4();
    let branch_two_id = Uuid::new_v4();
    let department_one_id = Uuid::new_v4();
    let department_two_id = Uuid::new_v4();
    let team_one_id = Uuid::new_v4();
    let team_two_id = Uuid::new_v4();
    let nodes = [
        holding_id,
        company_id,
        branch_one_id,
        branch_two_id,
        department_one_id,
        department_two_id,
        team_one_id,
        team_two_id,
    ]
    .map(OrganizationNodeId::try_from_uuid)
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let [
        holding_node,
        company_node,
        branch_one,
        branch_two,
        department_one,
        department_two,
        team_one,
        team_two,
    ] = nodes.as_slice()
    else {
        unreachable!("the fixed Organization node set has eight entries")
    };
    let (holding_node, company_node, branch_one, branch_two) =
        (*holding_node, *company_node, *branch_one, *branch_two);
    let (department_one, department_two, team_one, team_two) =
        (*department_one, *department_two, *team_one, *team_two);

    let holding_scope = AuthorityScope::Holding { holding_id };
    let actor = actor();
    let audit = audit();
    let write_pool = WritePool::for_holding(write_database.clone(), holding_id)?;
    let projection_pool = ProjectionPool::for_holding(read_database.clone(), holding_id)?;
    let read_pool = ReadPool::for_holding(read_database.clone(), holding_id)?;
    let write_repository = OrganizationWriteRepository::new(write_pool.clone());
    let command_adapter = OrganizationCommandAdapter::new(write_pool.clone());
    let now = Utc::now();
    let create_handler = OrganizationCommandHandler::new(
        write_repository.clone(),
        command_adapter.clone(),
        AcceptanceAccess,
        AcceptanceAccess,
        OrganizationHierarchyRepository,
        FixedClock::new(now),
    );

    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        holding_node,
        OrganizationPlacement::Holding,
        "HOLDING",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        company_node,
        OrganizationPlacement::Company { holding_id },
        "COMPANY",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        branch_one,
        OrganizationPlacement::Branch {
            holding_id,
            company_id,
        },
        "BRANCH-1",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        branch_two,
        OrganizationPlacement::Branch {
            holding_id,
            company_id,
        },
        "BRANCH-2",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        department_one,
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
        },
        "DEPARTMENT-1",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        department_two,
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
        },
        "DEPARTMENT-2",
    )
    .await?;
    create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        team_one,
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
            department_id: department_one_id,
        },
        "TEAM-1",
    )
    .await?;
    let final_create = create_node(
        &create_handler,
        &holding_scope,
        &actor,
        &audit,
        team_two,
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
            department_id: department_two_id,
        },
        "TEAM-2",
    )
    .await?;
    let initial_projection_target = final_create
        .projection_target()
        .expect("a Team create commits a v2 snapshot event");

    let expected_scopes = BTreeMap::from([
        (holding_id, AuthorityScope::Holding { holding_id }),
        (
            company_id,
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
        ),
        (
            branch_one_id,
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id: branch_one_id,
            },
        ),
        (
            branch_two_id,
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id: branch_two_id,
            },
        ),
        (department_one_id, exact_scope(holding_id, department_one)),
        (department_two_id, exact_scope(holding_id, department_two)),
        (team_one_id, exact_scope(holding_id, team_one)),
        (team_two_id, exact_scope(holding_id, team_two)),
    ]);
    assert_v2_snapshot_rows(&write_pool, &expected_scopes).await?;

    // A v1 Department snapshot used its Branch as the scope witness. Append
    // one private legacy event inside this random holding, then let the real
    // projector prove the bounded compatibility upcast without fabricating a
    // read row or changing the normalized command authority.
    let legacy_department = OrganizationNodeId::try_from_uuid(Uuid::new_v4())?;
    let legacy_sequence = append_legacy_department_snapshot(
        &write_pool,
        legacy_department,
        holding_id,
        company_id,
        branch_one_id,
        &actor,
        now,
    )
    .await?;
    assert!(
        legacy_sequence > initial_projection_target.global_sequence,
        "the manual legacy replay follows the handler-created v2 snapshots"
    );

    let projector = OrganizationNodeProjector::new();
    let runner = ProjectionRunner::new(
        ProjectionSource::new(write_pool.clone()),
        ProjectionStore::new(projection_pool),
        ProjectionRunnerConfig {
            page_size: 1_000,
            ..ProjectionRunnerConfig::default()
        },
    )?;
    run_active_through(&runner, &projector, legacy_sequence).await?;

    let projected_department = projected_node(&read_pool, department_one).await?;
    assert_eq!(projected_department.source_version, 1);
    assert_eq!(
        projected_department.holding_scope,
        exact_scope(holding_id, department_one)
    );
    assert_eq!(
        projected_department.view.scope,
        exact_scope(holding_id, department_one)
    );
    assert_eq!(
        projected_department.scope_key,
        format!("h:{holding_id}/c:{company_id}/b:{branch_one_id}/d:{department_one_id}")
    );
    let projected_team = projected_node(&read_pool, team_one).await?;
    assert_eq!(projected_team.source_version, 1);
    assert_eq!(
        projected_team.holding_scope,
        exact_scope(holding_id, team_one)
    );
    assert_eq!(projected_team.view.scope, exact_scope(holding_id, team_one));

    let read_repository = OrganizationReadRepository::new(read_pool.clone())?;
    let query_handler =
        OrganizationQueryHandler::new(read_repository, AcceptanceAccess, AcceptanceAccess);
    let projected_legacy = projected_node(&read_pool, legacy_department).await?;
    assert_eq!(projected_legacy.source_version, 1);
    assert_eq!(
        projected_legacy.holding_scope,
        exact_scope(holding_id, legacy_department),
        "a v1 Branch witness is persisted as an exact Department scope"
    );
    assert_eq!(
        projected_legacy.view.scope,
        exact_scope(holding_id, legacy_department)
    );
    let legacy_scope = exact_scope(holding_id, legacy_department);
    assert_eq!(
        get_node(
            &query_handler,
            legacy_department,
            &legacy_scope,
            &actor,
            &audit,
        )
        .await?
        .expect("the upcast v1 Department is available at its exact scope")
        .node_id,
        legacy_department
    );
    assert_eq!(
        sorted_ids(
            &list_nodes(&query_handler, &legacy_scope, &actor, &audit)
                .await?
                .nodes,
        ),
        vec![legacy_department.as_uuid()]
    );

    let branch_scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id: branch_one_id,
    };
    assert_eq!(
        get_node(
            &query_handler,
            department_one,
            &branch_scope,
            &actor,
            &audit,
        )
        .await?
        .expect("a Branch GET reaches its Department")
        .node_id,
        department_one
    );
    assert_eq!(
        get_node(&query_handler, team_one, &branch_scope, &actor, &audit,)
            .await?
            .expect("a Branch GET reaches its Department's Team")
            .node_id,
        team_one
    );
    assert!(
        get_node(&query_handler, branch_two, &branch_scope, &actor, &audit,)
            .await?
            .is_none(),
        "a Branch scope does not disclose its sibling Branch"
    );
    let expected_branch_nodes = sorted_node_ids([
        branch_one,
        department_one,
        department_two,
        team_one,
        team_two,
        legacy_department,
    ]);
    assert_eq!(
        sorted_ids(
            &list_nodes(&query_handler, &branch_scope, &actor, &audit)
                .await?
                .nodes,
        ),
        expected_branch_nodes,
        "a Branch LIST reaches only its own Department and Team descendants"
    );

    let mut paged_branch_ids = Vec::new();
    let mut cursor = None;
    for _ in 0..8 {
        let page = list_nodes_with_filter(
            &query_handler,
            &branch_scope,
            &actor,
            &audit,
            NodeListFilter {
                page_size: 2,
                cursor,
                ..NodeListFilter::default()
            },
        )
        .await?;
        assert!(page.nodes.len() <= 2);
        paged_branch_ids.extend(page.nodes.iter().map(|node| node.node_id.as_uuid()));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert!(
        cursor.is_none(),
        "the bounded Branch cursor walk must finish"
    );
    paged_branch_ids.sort_unstable();
    assert_eq!(paged_branch_ids, expected_branch_nodes);

    let department_scope = exact_scope(holding_id, department_one);
    assert_eq!(
        get_node(
            &query_handler,
            department_one,
            &department_scope,
            &actor,
            &audit,
        )
        .await?
        .expect("an exact Department GET returns the Department")
        .node_id,
        department_one
    );
    assert_eq!(
        sorted_ids(
            &list_nodes(&query_handler, &department_scope, &actor, &audit)
                .await?
                .nodes,
        ),
        sorted_node_ids([department_one, team_one]),
        "an exact Department LIST includes only itself and its descendants"
    );
    assert!(
        get_node(
            &query_handler,
            department_two,
            &department_scope,
            &actor,
            &audit,
        )
        .await?
        .is_none(),
        "a sibling Department is hidden instead of disclosed"
    );

    let team_scope = exact_scope(holding_id, team_one);
    assert_eq!(
        get_node(&query_handler, team_one, &team_scope, &actor, &audit,)
            .await?
            .expect("an exact Team GET returns the Team")
            .node_id,
        team_one
    );
    assert_eq!(
        sorted_ids(
            &list_nodes(&query_handler, &team_scope, &actor, &audit)
                .await?
                .nodes,
        ),
        vec![team_one_id],
        "an exact Team LIST does not disclose a sibling Team"
    );
    assert!(
        get_node(&query_handler, team_two, &team_scope, &actor, &audit,)
            .await?
            .is_none(),
        "a sibling Team is hidden instead of disclosed"
    );

    let reparent_handler = OrganizationCommandHandler::new(
        write_repository,
        command_adapter,
        AcceptanceAccess,
        AcceptanceAccess,
        OrganizationHierarchyRepository,
        FixedClock::new(now + Duration::seconds(1)),
    );
    let parent_conflict = reparent_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        department_one,
        OrganizationPlacement::Department {
            holding_id,
            company_id,
            branch_id: Some(branch_two_id),
        },
        1,
    )
    .await
    .expect_err("a Department with a Team child cannot be reparented");
    assert_eq!(parent_conflict.category(), ErrorCategory::Conflict);
    match parent_conflict {
        CqrsError::Operation(error) => {
            assert_eq!(error.code, "organization.conflict");
            assert_eq!(
                error.safe_message,
                "The Organization node cannot be reparented while it has children."
            );
        }
        other => panic!("expected a typed Organization conflict, got {other:?}"),
    }
    assert_eq!(
        authority_state(&write_pool, department_one).await?,
        (1, Some(branch_one_id), Some(branch_one_id)),
        "the rejected parent reparent must leave normalized authority unchanged"
    );

    let leaf_reparent = reparent_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        team_one,
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
            department_id: department_two_id,
        },
        1,
    )
    .await?;
    assert_eq!(leaf_reparent.aggregate.version, 2);
    assert_eq!(leaf_reparent.response.node.parent_id, Some(department_two));
    let reparent_target = leaf_reparent
        .projection_target()
        .expect("the leaf reparent commits a v2 snapshot event");
    assert_reparent_snapshot_rows(&write_pool, team_one).await?;
    run_active_through(&runner, &projector, reparent_target.global_sequence).await?;

    let reprojected_team = projected_node(&read_pool, team_one).await?;
    assert_eq!(reprojected_team.source_version, 2);
    assert_eq!(reprojected_team.view.parent_id, Some(department_two));
    assert_eq!(
        reprojected_team.view.placement,
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
            department_id: department_two_id,
        }
    );
    assert_eq!(
        reprojected_team.scope_key,
        format!(
            "h:{holding_id}/c:{company_id}/b:{branch_one_id}/d:{department_two_id}/t:{team_one_id}"
        )
    );
    assert!(
        get_node(&query_handler, team_one, &department_scope, &actor, &audit,)
            .await?
            .is_none(),
        "the old Department scope loses the reparented Team"
    );
    let department_two_scope = exact_scope(holding_id, department_two);
    assert_eq!(
        get_node(
            &query_handler,
            team_one,
            &department_two_scope,
            &actor,
            &audit,
        )
        .await?
        .expect("the new Department scope gains the reparented Team")
        .node_id,
        team_one
    );
    assert_eq!(
        sorted_ids(
            &list_nodes(&query_handler, &department_two_scope, &actor, &audit,)
                .await?
                .nodes,
        ),
        sorted_node_ids([department_two, team_one, team_two])
    );

    // The application preflight intentionally does not trust a separate read
    // of the parent. Prove the SQL unit of work revalidates the current parent
    // in its own transaction: a suspended child cannot be reactivated after
    // its parent has become archived.
    let guarded_team = OrganizationNodeId::try_from_uuid(Uuid::new_v4())?;
    create_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        guarded_team,
        OrganizationPlacement::Team {
            holding_id,
            company_id,
            branch_id: Some(branch_one_id),
            department_id: department_one_id,
        },
        "TEAM-ACTIVATION-GUARD",
    )
    .await?;
    let suspended_child = suspend_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        guarded_team,
        1,
    )
    .await?;
    assert_eq!(
        suspended_child.response.node.status,
        OrganizationNodeStatus::Suspended
    );
    let archived_parent = archive_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        department_one,
        1,
    )
    .await?;
    assert_eq!(
        archived_parent.response.node.status,
        OrganizationNodeStatus::Archived
    );

    let inactive_parent = activate_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        guarded_team,
        2,
    )
    .await
    .expect_err("an archived parent must reject child activation atomically");
    assert_eq!(inactive_parent.category(), ErrorCategory::Invalid);
    assert_eq!(
        authority_lifecycle_state(&write_pool, guarded_team).await?,
        (2, "suspended".to_owned()),
        "the rejected activation must leave the normalized child suspended"
    );

    // A Holding has no parent. It still carries the Placement guard so the
    // adapter contract is uniform, while validate_parent exits successfully.
    suspend_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        holding_node,
        1,
    )
    .await?;
    let active_root = activate_node(
        &reparent_handler,
        &holding_scope,
        &actor,
        &audit,
        holding_node,
        2,
    )
    .await?;
    assert_eq!(
        active_root.response.node.status,
        OrganizationNodeStatus::Active
    );
    assert_eq!(active_root.aggregate.version, 3);

    write_database.close().await;
    read_database.close().await;
    Ok(())
}

async fn create_node(
    handler: &AcceptanceCommandHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    node_id: OrganizationNodeId,
    placement: OrganizationPlacement,
    code: &str,
) -> Result<CommandReceipt<CreateOrganizationNodeResult>, CqrsError> {
    let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("a random command ID is set");
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "organization-hierarchy-create-{command_id}"
            ))
            .expect("the bounded idempotency key is valid"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            causation_id: None,
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            expected_aggregate_version: None,
            command: CreateOrganizationNode {
                node_id,
                placement,
                code: code.into(),
                name: format!("Acceptance {code}"),
            },
        })
        .await
}

async fn reparent_node(
    handler: &AcceptanceCommandHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    node_id: OrganizationNodeId,
    placement: OrganizationPlacement,
    expected_version: u64,
) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
    let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("a random command ID is set");
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "organization-hierarchy-reparent-{command_id}"
            ))
            .expect("the bounded idempotency key is valid"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            causation_id: None,
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            expected_aggregate_version: Some(expected_version),
            command: ReparentOrganizationNode { node_id, placement },
        })
        .await
}

async fn suspend_node(
    handler: &AcceptanceCommandHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    node_id: OrganizationNodeId,
    expected_version: u64,
) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
    let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("a random command ID is set");
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "organization-hierarchy-suspend-{command_id}"
            ))
            .expect("the bounded idempotency key is valid"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            causation_id: None,
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            expected_aggregate_version: Some(expected_version),
            command: SuspendOrganizationNode { node_id },
        })
        .await
}

async fn activate_node(
    handler: &AcceptanceCommandHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    node_id: OrganizationNodeId,
    expected_version: u64,
) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
    let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("a random command ID is set");
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "organization-hierarchy-activate-{command_id}"
            ))
            .expect("the bounded idempotency key is valid"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            causation_id: None,
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            expected_aggregate_version: Some(expected_version),
            command: ActivateOrganizationNode { node_id },
        })
        .await
}

async fn archive_node(
    handler: &AcceptanceCommandHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    node_id: OrganizationNodeId,
    expected_version: u64,
) -> Result<CommandReceipt<OrganizationMutationResult>, CqrsError> {
    let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("a random command ID is set");
    handler
        .handle(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id,
            idempotency_key: IdempotencyKey::try_new(format!(
                "organization-hierarchy-archive-{command_id}"
            ))
            .expect("the bounded idempotency key is valid"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            causation_id: None,
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            expected_aggregate_version: Some(expected_version),
            command: ArchiveOrganizationNode { node_id },
        })
        .await
}

async fn get_node(
    handler: &AcceptanceQueryHandler,
    node_id: OrganizationNodeId,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
) -> Result<Option<NodeView>, CqrsError> {
    Ok(handler
        .handle(QueryEnvelope {
            contract_version: CONTRACT_VERSION,
            query_id: QueryId::try_from_uuid(Uuid::new_v4()).expect("a random query ID is set"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            consistency: QueryConsistency::Eventual,
            query: GetOrganizationNode { node_id },
        })
        .await?
        .value
        .node)
}

async fn list_nodes(
    handler: &AcceptanceQueryHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
) -> Result<NodeList, CqrsError> {
    list_nodes_with_filter(
        handler,
        authority_scope,
        actor,
        audit,
        NodeListFilter {
            page_size: 100,
            ..NodeListFilter::default()
        },
    )
    .await
}

async fn list_nodes_with_filter(
    handler: &AcceptanceQueryHandler,
    authority_scope: &AuthorityScope,
    actor: &ActorAttribution,
    audit: &AuditContext,
    filter: NodeListFilter,
) -> Result<NodeList, CqrsError> {
    Ok(handler
        .handle(QueryEnvelope {
            contract_version: CONTRACT_VERSION,
            query_id: QueryId::try_from_uuid(Uuid::new_v4()).expect("a random query ID is set"),
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4())
                .expect("a random correlation ID is set"),
            actor: actor.clone(),
            active_scope: authority_scope.clone(),
            audit: audit.clone(),
            consistency: QueryConsistency::Eventual,
            query: ListOrganizationNodes { filter },
        })
        .await?
        .value)
}

async fn assert_v2_snapshot_rows(
    pool: &WritePool,
    expected_scopes: &BTreeMap<Uuid, AuthorityScope>,
) -> Result<(), Box<dyn Error>> {
    let node_ids = expected_scopes.keys().copied().collect::<Vec<_>>();
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query(
        "SELECT aggregate_id,event_version,active_scope,payload
         FROM erpx_cqrs_write.outbox_events
         WHERE context_name='organization' AND event_type=$1
           AND aggregate_id=ANY($2::uuid[])
         ORDER BY aggregate_id",
    )
    .bind(SNAPSHOT_EVENT_TYPE)
    .bind(&node_ids)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    assert_eq!(rows.len(), expected_scopes.len());
    for row in rows {
        let node_id: Uuid = row.try_get("aggregate_id")?;
        let event_version: i32 = row.try_get("event_version")?;
        let active_scope: AuthorityScope =
            serde_json::from_value(row.try_get::<Value, _>("active_scope")?)?;
        assert_eq!(event_version, i32::try_from(NODE_SNAPSHOT_EVENT_VERSION)?);
        assert_eq!(Some(&active_scope), expected_scopes.get(&node_id));
        let payload: OrganizationOutboxPayload =
            serde_json::from_value(row.try_get::<Value, _>("payload")?)?;
        let OrganizationOutboxPayload::Snapshot(snapshot) = payload else {
            panic!("the v2 snapshot event carried a non-snapshot payload");
        };
        assert_eq!(snapshot.node.node_id.as_uuid(), node_id);
        assert_eq!(snapshot.node.scope, active_scope);
    }
    Ok(())
}

async fn append_legacy_department_snapshot(
    pool: &WritePool,
    node_id: OrganizationNodeId,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    actor: &ActorAttribution,
    occurred_at: chrono::DateTime<Utc>,
) -> Result<u64, Box<dyn Error>> {
    let placement = OrganizationPlacement::Department {
        holding_id,
        company_id,
        branch_id: Some(branch_id),
    };
    let legacy_scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let view = NodeView {
        node_id,
        version: 1,
        code: "LEGACY-DEPARTMENT".into(),
        name: "Acceptance legacy Department".into(),
        kind: OrganizationNodeKind::Department,
        parent_id: placement.parent_id(node_id),
        placement,
        scope: legacy_scope.clone(),
        status: OrganizationNodeStatus::Active,
        created_at: occurred_at,
        updated_at: occurred_at,
    };
    let payload =
        serde_json::to_value(OrganizationOutboxPayload::Snapshot(OrganizationSnapshot {
            node: view,
        }))?;
    let mut transaction = pool.begin().await?;
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,
             correlation_id,causation_id,payload,occurred_at,available_at,holding_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1,$9,$10,$11,NULL,$12,$13,now(),
                 erpx_cqrs_write.current_holding_id())
         RETURNING global_sequence",
    )
    .bind(Uuid::new_v4())
    .bind(CONTEXT_NAME)
    .bind(node_partition(node_id))
    .bind(Uuid::new_v4())
    .bind(SNAPSHOT_EVENT_TYPE)
    .bind(i32::try_from(LEGACY_NODE_SNAPSHOT_EVENT_VERSION)?)
    .bind(AGGREGATE_TYPE)
    .bind(node_id.as_uuid())
    .bind(serde_json::to_value(legacy_scope)?)
    .bind(serde_json::to_value(actor)?)
    .bind(Uuid::new_v4())
    .bind(payload)
    .bind(occurred_at)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(u64::try_from(sequence)?)
}

async fn assert_reparent_snapshot_rows(
    pool: &WritePool,
    node_id: OrganizationNodeId,
) -> Result<(), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let rows: Vec<(i64, i32)> = sqlx::query_as(
        "SELECT aggregate_version,event_version
         FROM erpx_cqrs_write.outbox_events
         WHERE context_name='organization' AND event_type=$1 AND aggregate_id=$2
         ORDER BY aggregate_version",
    )
    .bind(SNAPSHOT_EVENT_TYPE)
    .bind(node_id.as_uuid())
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    assert_eq!(
        rows,
        vec![
            (1, i32::try_from(NODE_SNAPSHOT_EVENT_VERSION)?),
            (2, i32::try_from(NODE_SNAPSHOT_EVENT_VERSION)?),
        ]
    );
    Ok(())
}

struct ProjectedNode {
    source_version: i64,
    holding_scope: AuthorityScope,
    scope_key: String,
    view: NodeView,
}

async fn projected_node(
    pool: &ReadPool,
    node_id: OrganizationNodeId,
) -> Result<ProjectedNode, Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT projection.source_aggregate_version,projection.holding_scope,
                projection.scope_key,projection.state
         FROM erpx_organization_read.node_projection projection
         JOIN erpx_cqrs_read.projection_generations generation
           ON generation.holding_id=projection.holding_id
          AND generation.projection_name=projection.projection_name
          AND generation.generation=projection.generation
          AND generation.status='active'
         WHERE projection.holding_id=erpx_cqrs_read.current_holding_id()
           AND projection.projection_name=$1 AND projection.node_id=$2",
    )
    .bind(PROJECTION_NAME)
    .bind(node_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    let projected = ProjectedNode {
        source_version: row.try_get("source_aggregate_version")?,
        holding_scope: serde_json::from_value(row.try_get::<Value, _>("holding_scope")?)?,
        scope_key: row.try_get("scope_key")?,
        view: serde_json::from_value(row.try_get::<Value, _>("state")?)?,
    };
    transaction.commit().await?;
    Ok(projected)
}

async fn authority_state(
    pool: &WritePool,
    node_id: OrganizationNodeId,
) -> Result<(i64, Option<Uuid>, Option<Uuid>), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        "SELECT aggregate_version,parent_node_id,branch_id
         FROM erpx_organization_write.organization_node_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1",
    )
    .bind(node_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    let state = (
        row.try_get("aggregate_version")?,
        row.try_get("parent_node_id")?,
        row.try_get("branch_id")?,
    );
    transaction.commit().await?;
    Ok(state)
}

async fn authority_lifecycle_state(
    pool: &WritePool,
    node_id: OrganizationNodeId,
) -> Result<(i64, String), Box<dyn Error>> {
    let mut transaction = pool.begin().await?;
    let state = sqlx::query_as(
        "SELECT aggregate_version,node_status
         FROM erpx_organization_write.organization_node_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND node_id=$1",
    )
    .bind(node_id.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(state)
}

async fn run_active_through(
    runner: &ProjectionRunner,
    projector: &OrganizationNodeProjector,
    target: u64,
) -> Result<(), erpx_cqrs_sqlx::CqrsSqlxError> {
    for _ in 0..128 {
        let run = runner.run_active(projector).await?;
        if run.cursor >= target {
            return Ok(());
        }
        assert!(run.fetched > 0, "projection source ended before the target");
    }
    panic!("active Organization projection did not reach its bounded target");
}

fn exact_scope(holding_id: Uuid, node_id: OrganizationNodeId) -> AuthorityScope {
    AuthorityScope::Subtree {
        holding_id,
        organization_node_id: node_id.as_uuid(),
    }
}

fn sorted_ids(nodes: &[NodeView]) -> Vec<Uuid> {
    let mut values = nodes
        .iter()
        .map(|node| node.node_id.as_uuid())
        .collect::<Vec<_>>();
    values.sort_unstable();
    values
}

fn sorted_node_ids<const N: usize>(nodes: [OrganizationNodeId; N]) -> Vec<Uuid> {
    let mut values = nodes.map(OrganizationNodeId::as_uuid).to_vec();
    values.sort_unstable();
    values
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "organization-hierarchy-acceptance".into(),
        kind: ActorKind::HoldingUser,
        session_id: Some(
            SessionId::try_from_uuid(Uuid::new_v4()).expect("an authenticated session is set"),
        ),
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn audit() -> AuditContext {
    AuditContext {
        purpose: "Organization hierarchy SQLx acceptance".into(),
        customer_visible: false,
        module: Some("organization".into()),
    }
}

#[derive(Debug, Clone, Copy)]
struct AcceptanceAccess;

#[async_trait]
impl AuthorizationPort for AcceptanceAccess {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        let same_holding = match request.authority_scope {
            AuthorityScope::Platform => true,
            authority => scope_holding_id(authority) == scope_holding_id(request.target_scope),
        };
        if same_holding
            && matches!(
                request.action,
                OrganizationAction::Create
                    | OrganizationAction::Reparent
                    | OrganizationAction::Suspend
                    | OrganizationAction::Activate
                    | OrganizationAction::Archive
                    | OrganizationAction::View
                    | OrganizationAction::List
            )
        {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

fn scope_holding_id(scope: &AuthorityScope) -> Option<Uuid> {
    match scope {
        AuthorityScope::Platform => None,
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

#[async_trait]
impl EntitlementPort for AcceptanceAccess {
    async fn ensure_organization_enabled(
        &self,
        scope: &AuthorityScope,
        _write: bool,
    ) -> Result<(), PortError> {
        scope.validate().map_err(|_| PortError::EntitlementDenied)
    }
}
