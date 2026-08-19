use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuthorityScope, CommandId, CorrelationId, EventId, HoldingId,
};
use erpx_projects::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, DependencyAgreement, DependencyId,
    HandoffId, PortError, Project, ProjectHandoff, ProjectId, ProjectsAggregate, ProjectsCommit,
    StoredCommit, WorkflowTemplate, WorkflowTemplateId, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, DEPENDENCY_AGGREGATE_TYPE, EVENT_VERSION, HANDOFF_AGGREGATE_TYPE,
    PROJECT_AGGREGATE_TYPE, PROJECT_PROJECTION_FACT_EVENT, WORKFLOW_AGGREGATE_TYPE,
    dependency_partition, handoff_partition, project_partition, projection::ProjectsOutboxPayload,
    workflow_partition,
};

#[derive(Debug, Clone)]
pub struct ProjectsWriteRepository {
    pool: WritePool,
}

impl ProjectsWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    async fn load(
        &self,
        authority: &AuthorityScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<ProjectsAggregate>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let document =
            load_normalized_aggregate(&mut transaction, partition, aggregate_type, id).await?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(document) = document else {
            return Ok(None);
        };
        let aggregate: ProjectsAggregate =
            serde_json::from_value(document).map_err(|_| PortError::Internal)?;
        if aggregate.id_uuid() != id
            || aggregate.aggregate_type() != aggregate_type
            || !valid_aggregate(&aggregate)
        {
            return Err(PortError::Internal);
        }
        if !authority.contains(aggregate.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(aggregate))
    }
}

#[async_trait]
impl WriteRepository for ProjectsWriteRepository {
    async fn load_project(
        &self,
        authority: &AuthorityScope,
        id: ProjectId,
    ) -> Result<Option<Project>, PortError> {
        match self
            .load(
                authority,
                &project_partition(id),
                PROJECT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(ProjectsAggregate::Project(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_workflow(
        &self,
        authority: &AuthorityScope,
        id: WorkflowTemplateId,
    ) -> Result<Option<WorkflowTemplate>, PortError> {
        match self
            .load(
                authority,
                &workflow_partition(id),
                WORKFLOW_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(ProjectsAggregate::Workflow(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_dependency(
        &self,
        authority: &AuthorityScope,
        id: DependencyId,
    ) -> Result<Option<DependencyAgreement>, PortError> {
        match self
            .load(
                authority,
                &dependency_partition(id),
                DEPENDENCY_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(ProjectsAggregate::Dependency(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_handoff(
        &self,
        authority: &AuthorityScope,
        id: HandoffId,
    ) -> Result<Option<ProjectHandoff>, PortError> {
        match self
            .load(
                authority,
                &handoff_partition(id),
                HANDOFF_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(ProjectsAggregate::Handoff(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

async fn load_normalized_aggregate(
    transaction: &mut Transaction<'_, Postgres>,
    partition: &str,
    aggregate_type: &str,
    id: Uuid,
) -> Result<Option<serde_json::Value>, PortError> {
    let expected_partition = match aggregate_type {
        PROJECT_AGGREGATE_TYPE => format!("projects.project:{}", id.simple()),
        WORKFLOW_AGGREGATE_TYPE => format!("projects.workflow:{}", id.simple()),
        DEPENDENCY_AGGREGATE_TYPE => format!("projects.dependency:{}", id.simple()),
        HANDOFF_AGGREGATE_TYPE => format!("projects.handoff:{}", id.simple()),
        _ => return Err(PortError::Internal),
    };
    if partition != expected_partition {
        return Err(PortError::Internal);
    }
    let sql = match aggregate_type {
        PROJECT_AGGREGATE_TYPE => PROJECT_DOCUMENT_SQL,
        WORKFLOW_AGGREGATE_TYPE => WORKFLOW_DOCUMENT_SQL,
        DEPENDENCY_AGGREGATE_TYPE => DEPENDENCY_DOCUMENT_SQL,
        HANDOFF_AGGREGATE_TYPE => HANDOFF_DOCUMENT_SQL,
        _ => unreachable!("aggregate type checked above"),
    };
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|_| PortError::Unavailable)
}

const PROJECT_DOCUMENT_SQL: &str = r#"
SELECT jsonb_build_object('Project', jsonb_build_object(
 'id',p.project_id,'version',p.aggregate_version,'scope',p.active_scope,
 'code',p.project_code,'name',p.project_name,'class',p.project_class,
 'policy_ref',p.policy_ref,
 'workflow',jsonb_build_object('template_id',p.workflow_template_id,'version',p.workflow_version),
 'owner_id',p.owner_id,'status',p.project_status,
 'nodes',COALESCE((SELECT jsonb_object_agg(n.node_id::text,jsonb_build_object(
   'id',n.node_id,'parent_id',n.parent_node_id,'kind',n.node_kind,'title',n.title,
   'owner_id',n.owner_id,
   'assignees',COALESCE((SELECT jsonb_agg(a.assignee_id ORDER BY a.assignee_id) FROM erpx_projects_write.project_node_assignees a WHERE a.holding_id=n.holding_id AND a.project_id=n.project_id AND a.node_id=n.node_id),'[]'::jsonb),
   'planned_start',n.planned_start,'planned_end',n.planned_end,'actual_start',n.actual_start,'actual_end',n.actual_end,
   'workflow_state',n.workflow_state,'core_state',n.core_state,
   'promotion_source',CASE WHEN n.promotion_thread_id IS NULL THEN NULL ELSE jsonb_build_object('thread_id',n.promotion_thread_id,'comment_id',n.promotion_comment_id,'revision',n.promotion_revision,'content_digest',n.promotion_digest) END,
   'attachments',COALESCE((SELECT jsonb_agg(jsonb_build_object('attachment_id',a.attachment_id,'object_ref',a.object_ref,'content_digest',a.content_digest) ORDER BY a.attachment_id) FROM erpx_projects_write.project_node_attachments a WHERE a.holding_id=n.holding_id AND a.project_id=n.project_id AND a.node_id=n.node_id),'[]'::jsonb),
   'references',COALESCE((SELECT jsonb_agg(r.reference_document ORDER BY r.record_id,r.owner_context,r.record_type) FROM erpx_projects_write.project_node_references r WHERE r.holding_id=n.holding_id AND r.project_id=n.project_id AND r.node_id=n.node_id),'[]'::jsonb),
   'time_facts',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',f.fact_id,'worker_id',f.worker_id,'work_date',f.work_date,'minutes',f.minutes,'source',f.source_record,'source_version',f.source_version) ORDER BY f.fact_id) FROM erpx_projects_write.project_time_facts f WHERE f.holding_id=n.holding_id AND f.project_id=n.project_id AND f.node_id=n.node_id),'[]'::jsonb),
   'cost_facts',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',f.fact_id,'amount',jsonb_build_object('minor_units',f.amount_minor,'currency',f.currency),'source',f.source_record,'source_version',f.source_version,'reversed_fact',f.reversed_fact_id) ORDER BY f.fact_id) FROM erpx_projects_write.project_cost_facts f WHERE f.holding_id=n.holding_id AND f.project_id=n.project_id AND f.node_id=n.node_id),'[]'::jsonb),
   'version',n.node_version,'created_at',n.created_at,'updated_at',n.updated_at
 )) FROM erpx_projects_write.project_node_guards n WHERE n.holding_id=p.holding_id AND n.project_id=p.project_id),'{}'::jsonb),
 'created_at',p.created_at,'updated_at',p.updated_at
)) FROM erpx_projects_write.project_guards p
WHERE p.holding_id=erpx_projects_write.current_holding_id() AND p.project_id=$1
"#;

const WORKFLOW_DOCUMENT_SQL: &str = r#"
SELECT jsonb_build_object('Workflow',jsonb_build_object(
 'id',w.template_id,'version',w.template_version,'scope',w.active_scope,'name',w.template_name,
 'states',COALESCE((SELECT jsonb_agg(jsonb_build_object('code',s.state_code,'core_state',s.core_state) ORDER BY s.state_code) FROM erpx_projects_write.workflow_states s WHERE s.holding_id=w.holding_id AND s.template_id=w.template_id AND s.template_version=w.template_version),'[]'::jsonb),
 'transitions',COALESCE((SELECT jsonb_agg(jsonb_build_object('from',t.from_state,'to',t.to_state) ORDER BY t.from_state,t.to_state) FROM erpx_projects_write.workflow_transitions t WHERE t.holding_id=w.holding_id AND t.template_id=w.template_id AND t.template_version=w.template_version),'[]'::jsonb),
 'status',w.template_status,'created_at',w.created_at,'updated_at',w.published_at
)) FROM erpx_projects_write.workflow_template_versions w
WHERE w.holding_id=erpx_projects_write.current_holding_id() AND w.template_id=$1
ORDER BY w.template_version DESC LIMIT 1
"#;

const DEPENDENCY_DOCUMENT_SQL: &str = r#"
SELECT jsonb_build_object('Dependency',jsonb_build_object(
 'id',d.dependency_id,'version',d.aggregate_version,'terms_version',d.terms_version,
 'source',jsonb_build_object('project_id',d.source_project_id,'project_version',d.source_project_version,'node_id',d.source_node_id,'node_version',d.source_node_version,'scope',d.source_scope,'owner_id',d.source_owner_id),
 'target',jsonb_build_object('project_id',d.target_project_id,'project_version',d.target_project_version,'node_id',d.target_node_id,'node_version',d.target_node_version,'scope',d.target_scope,'owner_id',d.target_owner_id),
 'kind',d.dependency_kind,'status',d.dependency_status,
 'source_acceptance',(SELECT jsonb_build_object('side',a.dependency_side,'owner_id',a.owner_id,'terms_version',a.terms_version,'accepted_at',a.accepted_at) FROM erpx_projects_write.dependency_acceptances a WHERE a.holding_id=d.holding_id AND a.dependency_id=d.dependency_id AND a.dependency_side='source'),
 'target_acceptance',(SELECT jsonb_build_object('side',a.dependency_side,'owner_id',a.owner_id,'terms_version',a.terms_version,'accepted_at',a.accepted_at) FROM erpx_projects_write.dependency_acceptances a WHERE a.holding_id=d.holding_id AND a.dependency_id=d.dependency_id AND a.dependency_side='target'),
 'proposed_by',d.proposed_by,'created_at',d.created_at,'updated_at',d.updated_at
)) FROM erpx_projects_write.dependency_guards d
WHERE d.holding_id=erpx_projects_write.current_holding_id() AND d.dependency_id=$1
"#;

const HANDOFF_DOCUMENT_SQL: &str = r#"
SELECT jsonb_build_object('Handoff',jsonb_build_object(
 'id',h.handoff_id,'version',h.aggregate_version,'project_id',h.project_id,'item_id',h.item_id,
 'scope',h.active_scope,'kind',h.handoff_kind,'payload_digest',h.payload_digest,
 'idempotency_key',h.idempotency_key,'correlation_id',h.correlation_id,'proposed_by',h.proposed_by,
 'approved_by',h.approved_by,'status',h.handoff_status,
 'outcome',CASE WHEN h.outcome_receipt_id IS NULL THEN NULL ELSE jsonb_build_object('owner_context',h.outcome_owner_context,'owner_receipt_id',h.outcome_receipt_id,'owner_record',h.outcome_record,'accepted',h.outcome_accepted,'reason_code',h.outcome_reason_code,'reconciled_at',h.reconciled_at) END,
 'created_at',h.created_at,'updated_at',h.updated_at
)) FROM erpx_projects_write.handoff_guards h
WHERE h.holding_id=erpx_projects_write.current_holding_id() AND h.handoff_id=$1
"#;

#[derive(Debug, Clone)]
pub struct ProjectsCommandAdapter {
    store: CommandStore,
    pool: WritePool,
    holding_id: HoldingId,
}

impl ProjectsCommandAdapter {
    pub fn new(pool: WritePool, holding_id: HoldingId) -> Self {
        Self {
            store: CommandStore::new(pool.clone()),
            pool,
            holding_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct ProjectsReceiptResponse {
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
}

#[async_trait]
impl CommandUnitOfWork for ProjectsCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, aggregate_type) = fingerprint_target(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<ProjectsReceiptResponse>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        stored_from_normalized_receipt(&self.pool, receipt, aggregate_type)
            .await
            .map(Some)
    }

    async fn commit(&self, request: ProjectsCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = request.aggregate.aggregate_type();
        let occurred_at = request.aggregate.occurred_at();
        let command_id = request.identity.command_id;
        let mut events = request
            .events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: EVENT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: ProjectsOutboxPayload::Domain(event.clone()),
            })
            .collect::<Vec<_>>();
        match &request.aggregate {
            ProjectsAggregate::Project(project) => events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    PROJECT_PROJECTION_FACT_EVENT,
                    events.len(),
                ),
                event_type: PROJECT_PROJECTION_FACT_EVENT.into(),
                event_version: EVENT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: ProjectsOutboxPayload::ProjectProjection(Box::new(project.clone())),
            }),
            ProjectsAggregate::Workflow(_) | ProjectsAggregate::Handoff(_) => {
                events.push(OutboxEventWrite {
                    event_id: deterministic_event_id(
                        command_id,
                        crate::PROJECT_RECORD_PROJECTION_FACT_EVENT,
                        events.len(),
                    ),
                    event_type: crate::PROJECT_RECORD_PROJECTION_FACT_EVENT.into(),
                    event_version: EVENT_VERSION,
                    partition: partition.clone(),
                    causation_id: request.causation_id,
                    occurred_at,
                    payload: ProjectsOutboxPayload::RecordProjection(Box::new(
                        request.aggregate.clone(),
                    )),
                });
            }
            ProjectsAggregate::Dependency(_) => {}
        }
        let aggregate = request.aggregate;
        let aggregate_id =
            AggregateId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
        let guard = ProjectsAtomicGuard {
            aggregate: aggregate.clone(),
            holding_id: self.holding_id,
            command_id,
            correlation_id: request.correlation_id,
            actor: request.actor.clone(),
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: ProjectsReceiptResponse {
                aggregate_type: aggregate_type.into(),
                aggregate_id: aggregate.id_uuid(),
                aggregate_version: aggregate.version(),
            },
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"projects-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "aggregate_id": aggregate_id.as_uuid(),
                    "aggregate_type": aggregate_type,
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
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = if replayed {
            stored_from_normalized_receipt(&self.pool, receipt, aggregate_type).await?
        } else {
            stored_from_receipt_and_aggregate(receipt, aggregate, aggregate_type)?
        };
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct ProjectsAtomicGuard {
    aggregate: ProjectsAggregate,
    holding_id: HoldingId,
    command_id: CommandId,
    correlation_id: CorrelationId,
    actor: ActorAttribution,
}

#[async_trait]
impl AtomicCommandGuard for ProjectsAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        set_holding(transaction, self.holding_id).await?;
        guard_version(transaction, self.holding_id, &self.aggregate)
            .await?
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        set_holding(transaction, self.holding_id).await?;
        let current = guard_version(transaction, self.holding_id, &self.aggregate).await?;
        match (context.expected_version, current) {
            (None, Some(_)) => Err(CqrsSqlxError::GuardRejected(
                "Projects aggregate already exists",
            )),
            (Some(_), None) => Err(CqrsSqlxError::GuardRejected(
                "Projects aggregate guard is missing",
            )),
            _ => Ok(()),
        }
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        set_holding(transaction, self.holding_id).await?;
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Projects applied guard metadata mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            ProjectsAggregate::Project(value) => {
                persist_project(transaction, self.holding_id, value, version).await?
            }
            ProjectsAggregate::Workflow(value) => {
                persist_workflow(transaction, self.holding_id, value, version).await?
            }
            ProjectsAggregate::Dependency(value) => {
                persist_dependency(transaction, self.holding_id, value, version).await?
            }
            ProjectsAggregate::Handoff(value) => {
                persist_handoff(transaction, self.holding_id, value, version).await?
            }
        }
        for draft in crate::directory::projects_directory_drafts(&self.aggregate)? {
            crate::directory::append_directory_draft(
                transaction,
                self.holding_id,
                self.command_id,
                self.correlation_id,
                &self.actor,
                &draft,
                self.aggregate.occurred_at(),
            )
            .await?;
        }
        Ok(())
    }
}

async fn persist_project(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    value: &Project,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    let changed = sqlx::query(
        "INSERT INTO erpx_projects_write.project_guards \
         (holding_id,project_id,active_scope,scope_key,project_code,project_name,project_class,project_status,policy_ref,workflow_template_id,workflow_version,owner_id,aggregate_version,created_at,updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) \
         ON CONFLICT (holding_id,project_id) DO UPDATE SET active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,project_code=EXCLUDED.project_code,project_name=EXCLUDED.project_name,project_class=EXCLUDED.project_class,project_status=EXCLUDED.project_status,policy_ref=EXCLUDED.policy_ref,workflow_template_id=EXCLUDED.workflow_template_id,workflow_version=EXCLUDED.workflow_version,owner_id=EXCLUDED.owner_id,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at \
         WHERE erpx_projects_write.project_guards.aggregate_version + 1 = EXCLUDED.aggregate_version",
    )
    .bind(holding_id.as_uuid())
    .bind(value.id().as_uuid())
    .bind(serde_json::to_value(value.scope()).map_err(encoding_error)?)
    .bind(scope_key(value.scope())?)
    .bind(value.code())
    .bind(value.name())
    .bind(enum_name(value.class())?)
    .bind(enum_name(value.status())?)
    .bind(value.policy_ref())
    .bind(value.workflow().template_id.as_uuid())
    .bind(to_i64(value.workflow().version)?)
    .bind(value.owner_id())
    .bind(version)
    .bind(value.created_at())
    .bind(value.updated_at())
    .execute(&mut **transaction)
    .await?;
    require_one(changed.rows_affected())?;

    for table in [
        "project_node_assignees",
        "project_node_attachments",
        "project_node_references",
        "project_time_facts",
        "project_cost_facts",
        "project_node_guards",
    ] {
        let statement = format!(
            "DELETE FROM erpx_projects_write.{table} WHERE holding_id=$1 AND project_id=$2"
        );
        sqlx::query(&statement)
            .bind(holding_id.as_uuid())
            .bind(value.id().as_uuid())
            .execute(&mut **transaction)
            .await?;
    }
    for node in value.nodes().values() {
        let source = node.promotion_source();
        sqlx::query(
            "INSERT INTO erpx_projects_write.project_node_guards \
             (holding_id,project_id,node_id,parent_node_id,node_kind,title,owner_id,planned_start,planned_end,actual_start,actual_end,workflow_state,core_state,promotion_thread_id,promotion_comment_id,promotion_revision,promotion_digest,node_version,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)",
        )
        .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid())
        .bind(node.parent_id().map(|id| id.as_uuid())).bind(enum_name(node.kind())?)
        .bind(node.title()).bind(node.owner_id()).bind(node.planned_start()).bind(node.planned_end())
        .bind(node.actual_start()).bind(node.actual_end()).bind(node.workflow_state())
        .bind(enum_name(node.core_state())?).bind(source.map(|v| v.thread_id))
        .bind(source.map(|v| v.comment_id)).bind(source.map(|v| to_i64(v.revision)).transpose()?)
        .bind(source.map(|v| v.content_digest.as_str())).bind(to_i64(node.version())?)
        .bind(node.created_at()).bind(node.updated_at()).execute(&mut **transaction).await?;
        for assignee in node.assignees() {
            sqlx::query("INSERT INTO erpx_projects_write.project_node_assignees (holding_id,project_id,node_id,assignee_id) VALUES ($1,$2,$3,$4)")
                .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid()).bind(assignee)
                .execute(&mut **transaction).await?;
        }
        for attachment in node.attachments() {
            sqlx::query("INSERT INTO erpx_projects_write.project_node_attachments (holding_id,project_id,node_id,attachment_id,object_ref,content_digest) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid())
                .bind(attachment.attachment_id).bind(&attachment.object_ref).bind(&attachment.content_digest)
                .execute(&mut **transaction).await?;
        }
        for reference in node.references() {
            sqlx::query("INSERT INTO erpx_projects_write.project_node_references (holding_id,project_id,node_id,record_id,owner_context,record_type,reference_document) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid())
                .bind(reference.id().as_uuid()).bind(enum_name(reference.owner_context())?)
                .bind(reference.record_type().as_str()).bind(serde_json::to_value(reference).map_err(encoding_error)?)
                .execute(&mut **transaction).await?;
        }
        for fact in node.time_facts() {
            sqlx::query("INSERT INTO erpx_projects_write.project_time_facts (holding_id,project_id,node_id,fact_id,worker_id,work_date,minutes,source_record,source_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid())
                .bind(fact.id.as_uuid()).bind(fact.worker_id).bind(fact.work_date)
                .bind(i32::try_from(fact.minutes).map_err(|_| CqrsSqlxError::NumericRange)?)
                .bind(serde_json::to_value(&fact.source).map_err(encoding_error)?).bind(to_i64(fact.source_version)?)
                .execute(&mut **transaction).await?;
        }
        for fact in node.cost_facts() {
            sqlx::query("INSERT INTO erpx_projects_write.project_cost_facts (holding_id,project_id,node_id,fact_id,amount_minor,currency,source_record,source_version,reversed_fact_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(node.id().as_uuid())
                .bind(fact.id.as_uuid()).bind(fact.amount.minor_units()).bind(fact.amount.currency().as_str())
                .bind(serde_json::to_value(&fact.source).map_err(encoding_error)?).bind(to_i64(fact.source_version)?)
                .bind(fact.reversed_fact.map(|id| id.as_uuid())).execute(&mut **transaction).await?;
        }
    }
    Ok(())
}

async fn persist_workflow(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    value: &WorkflowTemplate,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    sqlx::query("INSERT INTO erpx_projects_write.workflow_template_versions (holding_id,template_id,template_version,active_scope,scope_key,template_name,template_status,created_at,published_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(version)
        .bind(serde_json::to_value(value.scope()).map_err(encoding_error)?).bind(scope_key(value.scope())?)
        .bind(value.name()).bind(enum_name(value.status())?).bind(value.created_at()).bind(value.updated_at())
        .execute(&mut **transaction).await?;
    for state in value.states() {
        sqlx::query("INSERT INTO erpx_projects_write.workflow_states (holding_id,template_id,template_version,state_code,core_state) VALUES ($1,$2,$3,$4,$5)")
            .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(version).bind(&state.code)
            .bind(enum_name(state.core_state)?).execute(&mut **transaction).await?;
    }
    for transition in value.transitions() {
        sqlx::query("INSERT INTO erpx_projects_write.workflow_transitions (holding_id,template_id,template_version,from_state,to_state) VALUES ($1,$2,$3,$4,$5)")
            .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(version)
            .bind(&transition.from).bind(&transition.to).execute(&mut **transaction).await?;
    }
    Ok(())
}

async fn persist_dependency(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    value: &DependencyAgreement,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    let source = value.source();
    let target = value.target();
    let changed = sqlx::query("INSERT INTO erpx_projects_write.dependency_guards (holding_id,dependency_id,terms_version,source_project_id,source_project_version,source_node_id,source_node_version,source_scope,source_owner_id,target_project_id,target_project_version,target_node_id,target_node_version,target_scope,target_owner_id,dependency_kind,dependency_status,proposed_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21) ON CONFLICT (holding_id,dependency_id) DO UPDATE SET dependency_status=EXCLUDED.dependency_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_projects_write.dependency_guards.aggregate_version + 1 = EXCLUDED.aggregate_version")
        .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(to_i64(value.terms_version())?)
        .bind(source.project_id.as_uuid()).bind(to_i64(source.project_version)?).bind(source.node_id.as_uuid()).bind(to_i64(source.node_version)?)
        .bind(serde_json::to_value(&source.scope).map_err(encoding_error)?).bind(source.owner_id)
        .bind(target.project_id.as_uuid()).bind(to_i64(target.project_version)?).bind(target.node_id.as_uuid()).bind(to_i64(target.node_version)?)
        .bind(serde_json::to_value(&target.scope).map_err(encoding_error)?).bind(target.owner_id)
        .bind(enum_name(value.kind())?).bind(enum_name(value.status())?).bind(value.proposed_by()).bind(version)
        .bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
    require_one(changed.rows_affected())?;
    sqlx::query("DELETE FROM erpx_projects_write.dependency_acceptances WHERE holding_id=$1 AND dependency_id=$2")
        .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).execute(&mut **transaction).await?;
    for acceptance in [value.source_acceptance(), value.target_acceptance()]
        .into_iter()
        .flatten()
    {
        sqlx::query("INSERT INTO erpx_projects_write.dependency_acceptances (holding_id,dependency_id,dependency_side,owner_id,terms_version,accepted_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(enum_name(acceptance.side)?)
            .bind(acceptance.owner_id).bind(to_i64(acceptance.terms_version)?).bind(acceptance.accepted_at)
            .execute(&mut **transaction).await?;
    }
    Ok(())
}

async fn persist_handoff(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    value: &ProjectHandoff,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    let outcome = value.outcome();
    let changed = sqlx::query("INSERT INTO erpx_projects_write.handoff_guards (holding_id,handoff_id,project_id,item_id,active_scope,scope_key,handoff_kind,owner_context,handoff_status,idempotency_key,correlation_id,payload_digest,proposed_by,approved_by,outcome_owner_context,outcome_receipt_id,outcome_record,outcome_accepted,outcome_reason_code,reconciled_at,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23) ON CONFLICT (holding_id,handoff_id) DO UPDATE SET handoff_status=EXCLUDED.handoff_status,approved_by=EXCLUDED.approved_by,outcome_owner_context=EXCLUDED.outcome_owner_context,outcome_receipt_id=EXCLUDED.outcome_receipt_id,outcome_record=EXCLUDED.outcome_record,outcome_accepted=EXCLUDED.outcome_accepted,outcome_reason_code=EXCLUDED.outcome_reason_code,reconciled_at=EXCLUDED.reconciled_at,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at WHERE erpx_projects_write.handoff_guards.aggregate_version + 1 = EXCLUDED.aggregate_version")
        .bind(holding_id.as_uuid()).bind(value.id().as_uuid()).bind(value.project_id().as_uuid()).bind(value.item_id().as_uuid())
        .bind(serde_json::to_value(value.scope()).map_err(encoding_error)?).bind(scope_key(value.scope())?)
        .bind(enum_name(value.kind())?).bind(enum_name(value.kind().owner_context())?).bind(enum_name(value.status())?)
        .bind(value.idempotency_key()).bind(value.correlation_id().as_uuid()).bind(value.payload_digest())
        .bind(value.proposed_by()).bind(value.approved_by()).bind(outcome.map(|v| enum_name(v.owner_context)).transpose()?)
        .bind(outcome.map(|v| v.owner_receipt_id.as_str())).bind(outcome.and_then(|v| v.owner_record.as_ref()).map(serde_json::to_value).transpose().map_err(encoding_error)?)
        .bind(outcome.map(|v| v.accepted)).bind(outcome.map(|v| v.reason_code.as_str())).bind(outcome.map(|v| v.reconciled_at))
        .bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
    require_one(changed.rows_affected())
}

fn encoding_error(_: serde_json::Error) -> CqrsSqlxError {
    CqrsSqlxError::InvalidInput("Projects normalized value cannot be encoded")
}

fn require_one(rows: u64) -> Result<(), CqrsSqlxError> {
    (rows == 1)
        .then_some(())
        .ok_or(CqrsSqlxError::GuardRejected(
            "Projects normalized authority version is not exact +1",
        ))
}

async fn guard_version(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    aggregate: &ProjectsAggregate,
) -> Result<Option<i64>, CqrsSqlxError> {
    let (statement, id) = match aggregate {
        ProjectsAggregate::Project(value) => (
            "SELECT aggregate_version FROM erpx_projects_write.project_guards WHERE holding_id=$1 AND project_id=$2 FOR UPDATE",
            value.id().as_uuid(),
        ),
        ProjectsAggregate::Workflow(value) => (
            "SELECT template_version FROM erpx_projects_write.workflow_template_versions WHERE holding_id=$1 AND template_id=$2 ORDER BY template_version DESC LIMIT 1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        ProjectsAggregate::Dependency(value) => (
            "SELECT aggregate_version FROM erpx_projects_write.dependency_guards WHERE holding_id=$1 AND dependency_id=$2 FOR UPDATE",
            value.id().as_uuid(),
        ),
        ProjectsAggregate::Handoff(value) => (
            "SELECT aggregate_version FROM erpx_projects_write.handoff_guards WHERE holding_id=$1 AND handoff_id=$2 FOR UPDATE",
            value.id().as_uuid(),
        ),
    };
    sqlx::query_scalar(statement)
        .bind(holding_id.as_uuid())
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(Into::into)
}

async fn set_holding(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), CqrsSqlxError> {
    let expected = holding_id.as_uuid().to_string();
    let actual: String = sqlx::query_scalar("SELECT set_config('app.holding_id', $1, true)")
        .bind(&expected)
        .fetch_one(&mut **transaction)
        .await?;
    (actual == expected)
        .then_some(())
        .ok_or(CqrsSqlxError::DatabasePlaneMismatch)
}

fn validate_guard_context(
    aggregate: &ProjectsAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context == CONTEXT_NAME
        && context.aggregate_type == aggregate.aggregate_type()
        && context.aggregate_id.as_uuid() == aggregate.id_uuid()
        && context.partition == aggregate_partition(aggregate)
    {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(
            "Projects guard metadata mismatch",
        ))
    }
}

fn validate_commit(request: &ProjectsCommit) -> Result<(), PortError> {
    let (partition, aggregate_type) = fingerprint_target(&request.identity.fingerprint);
    if request.identity.fingerprint.aggregate_id().as_uuid() != request.aggregate.id_uuid()
        || partition != aggregate_partition(&request.aggregate)
        || aggregate_type != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
        || !valid_aggregate(&request.aggregate)
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

fn valid_aggregate(value: &ProjectsAggregate) -> bool {
    match value {
        ProjectsAggregate::Project(value) => value.validate().is_ok(),
        ProjectsAggregate::Workflow(value) => value.validate().is_ok(),
        ProjectsAggregate::Dependency(value) => {
            value.version() > 0 && value.source().project_id != value.target().project_id
        }
        ProjectsAggregate::Handoff(value) => value.validate().is_ok(),
    }
}

fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::CreateProject { project_id, .. }
        | CommandFingerprint::ChangeProject { project_id, .. } => {
            (project_partition(*project_id), PROJECT_AGGREGATE_TYPE)
        }
        CommandFingerprint::PublishWorkflow { workflow_id, .. } => {
            (workflow_partition(*workflow_id), WORKFLOW_AGGREGATE_TYPE)
        }
        CommandFingerprint::ProposeDependency { dependency_id, .. }
        | CommandFingerprint::DecideDependency { dependency_id, .. } => (
            dependency_partition(*dependency_id),
            DEPENDENCY_AGGREGATE_TYPE,
        ),
        CommandFingerprint::ProposeHandoff { handoff_id, .. }
        | CommandFingerprint::ChangeHandoff { handoff_id, .. } => {
            (handoff_partition(*handoff_id), HANDOFF_AGGREGATE_TYPE)
        }
    }
}

fn aggregate_partition(value: &ProjectsAggregate) -> String {
    match value {
        ProjectsAggregate::Project(value) => project_partition(value.id()),
        ProjectsAggregate::Workflow(value) => workflow_partition(value.id()),
        ProjectsAggregate::Dependency(value) => dependency_partition(value.id()),
        ProjectsAggregate::Handoff(value) => handoff_partition(value.id()),
    }
}

fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateProject { .. } => "projects.project.create",
        CommandFingerprint::ChangeProject { .. } => "projects.project.change",
        CommandFingerprint::PublishWorkflow { .. } => "projects.workflow.publish",
        CommandFingerprint::ProposeDependency { .. } => "projects.dependency.propose",
        CommandFingerprint::DecideDependency { .. } => "projects.dependency.decide",
        CommandFingerprint::ProposeHandoff { .. } => "projects.handoff.propose",
        CommandFingerprint::ChangeHandoff { .. } => "projects.handoff.change",
    }
}

fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.projects.command\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut label = kind.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &label))
        .expect("derived Projects event identifier")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn enum_name<T: serde::Serialize>(value: T) -> Result<String, CqrsSqlxError> {
    serde_json::to_value(value)
        .map_err(|_| CqrsSqlxError::InvalidInput("Projects enum cannot be encoded"))?
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput(
            "Projects enum must encode as text",
        ))
}

pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CqrsSqlxError::InvalidInput("Projects scope must be exact")),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn stored_from_receipt_and_aggregate(
    receipt: erpx_cqrs::CommandReceipt<ProjectsReceiptResponse>,
    aggregate: ProjectsAggregate,
    aggregate_type: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != aggregate_type
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.aggregate_id
        || receipt.aggregate.version != receipt.response.aggregate_version
        || receipt.response.aggregate_type != aggregate_type
        || receipt.response.aggregate_id != aggregate.id_uuid()
        || receipt.response.aggregate_version != aggregate.version()
        || !valid_aggregate(&aggregate)
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

async fn stored_from_normalized_receipt(
    pool: &WritePool,
    receipt: erpx_cqrs::CommandReceipt<ProjectsReceiptResponse>,
    aggregate_type: &str,
) -> Result<StoredCommit, PortError> {
    let partition = match aggregate_type {
        PROJECT_AGGREGATE_TYPE => format!(
            "projects.project:{}",
            receipt.response.aggregate_id.simple()
        ),
        WORKFLOW_AGGREGATE_TYPE => format!(
            "projects.workflow:{}",
            receipt.response.aggregate_id.simple()
        ),
        DEPENDENCY_AGGREGATE_TYPE => format!(
            "projects.dependency:{}",
            receipt.response.aggregate_id.simple()
        ),
        HANDOFF_AGGREGATE_TYPE => format!(
            "projects.handoff:{}",
            receipt.response.aggregate_id.simple()
        ),
        _ => return Err(PortError::Internal),
    };
    let mut transaction = pool.begin().await.map_err(map_store_error)?;
    let document = load_normalized_aggregate(
        &mut transaction,
        &partition,
        aggregate_type,
        receipt.response.aggregate_id,
    )
    .await?
    .ok_or(PortError::Internal)?;
    transaction
        .commit()
        .await
        .map_err(|_| PortError::Unavailable)?;
    let aggregate: ProjectsAggregate =
        serde_json::from_value(document).map_err(|_| PortError::Internal)?;
    stored_from_receipt_and_aggregate(receipt, aggregate, aggregate_type)
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
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_target_and_hash_are_stable() {
        let id = ProjectId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        let value = CommandFingerprint::ChangeProject {
            project_id: id,
            request_digest: "d".repeat(64),
        };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(
            fingerprint_target(&value),
            (project_partition(id), PROJECT_AGGREGATE_TYPE)
        );
    }

    #[test]
    fn guard_updates_share_the_atomic_command_transaction() {
        let source = include_str!("command.rs");
        assert!(source.contains("commit_normalized(command, &guard)"));
        assert!(source.contains("project_node_guards"));
        assert!(source.contains("handoff_guards"));
    }
}
