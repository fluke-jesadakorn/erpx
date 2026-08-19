use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_agent_orchestration::{
    ActionCardId, ActionCardProposal, Assignment, AssignmentFailure, AssignmentId,
    AssignmentRepository, BudgetLayer, BudgetLayerKind, BudgetLedgerPort, BudgetRequest,
    BudgetReservation, BudgetSubjects, Checkpoint, ClaimedAssignment, ExpiredLeaseRecovery,
    PortError, UsageReconciliation, WorkerAssignmentPort,
};
use erpx_kernel::AuthorityScope;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{command::append_assignment_state_fact, holding::HoldingBoundPool, normalized};

#[derive(Debug, Clone)]
pub struct SqlxAgentRuntime {
    pub(crate) database: HoldingBoundPool,
}

impl SqlxAgentRuntime {
    pub fn try_new(pool: PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }

    pub const fn holding_id(&self) -> Uuid {
        self.database.holding_id()
    }

    pub async fn load_action_card(
        &self,
        authority_scope: &AuthorityScope,
        id: ActionCardId,
    ) -> Result<Option<ActionCardProposal>, PortError> {
        let mut tx = self.database.begin().await?;
        let result = normalized::load_action_card(&mut tx, self.holding_id(), id).await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some((scope, card)) = result else {
            return Ok(None);
        };
        if !authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        Ok(Some(card))
    }
}

#[derive(Debug, Clone)]
pub struct SqlxWorkerQueue {
    database: HoldingBoundPool,
}

/// One opaque Agent notification fact claimed from the current holding
/// outbox. Prompt/output material and provider failure details are not part of
/// this contract.
#[derive(Debug, Clone)]
pub struct ClaimedAgentNotificationFact {
    pub global_sequence: i64,
    pub claim_token: Uuid,
    pub event_id: Uuid,
    pub event_version: u32,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub active_scope: serde_json::Value,
    pub actor: serde_json::Value,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub attempts: u32,
}

impl SqlxWorkerQueue {
    pub fn try_new(pool: PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        Ok(Self {
            database: HoldingBoundPool::try_new(pool, holding_id)?,
        })
    }

    pub const fn holding_id(&self) -> Uuid {
        self.database.holding_id()
    }

    async fn begin_holding_transaction(&self) -> Result<Transaction<'_, Postgres>, PortError> {
        self.database.begin().await
    }

    /// Claim at most one Agent notification fact from this exact holding. A
    /// worker never performs a global cross-holding outbox scan.
    pub async fn claim_notification_fact(
        &self,
        at: DateTime<Utc>,
        claim_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedAgentNotificationFact>, PortError> {
        if claim_until <= at || claim_until > at + chrono::Duration::minutes(15) {
            return Err(PortError::Denied);
        }
        let claim_token = Uuid::new_v4();
        let mut tx = self.begin_holding_transaction().await?;
        let row = sqlx::query(
            "WITH candidate AS (
                 SELECT global_sequence
                 FROM erpx_cqrs_write.outbox_events
                 WHERE holding_id=$1
                   AND context_name='agent_orchestration'
                   AND event_type='agent.assignment.state_notification.requested'
                   AND published_at IS NULL AND dead_lettered_at IS NULL
                   AND available_at<=$2
                   AND (claim_token IS NULL OR claim_until<=$2)
                 ORDER BY global_sequence
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE erpx_cqrs_write.outbox_events o
             SET claim_token=$3,claim_until=$4,publish_attempts=o.publish_attempts+1
             FROM candidate c
             WHERE o.holding_id=$1 AND o.global_sequence=c.global_sequence
             RETURNING o.global_sequence,o.event_id,o.event_version,o.aggregate_type,
                       o.aggregate_id,o.aggregate_version,o.active_scope,o.actor,
                       o.correlation_id,o.causation_id,o.payload,o.occurred_at,
                       o.available_at,o.publish_attempts",
        )
        .bind(self.holding_id())
        .bind(at)
        .bind(claim_token)
        .bind(claim_until)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let fact = row
            .map(|row| {
                use sqlx::Row;
                Ok(ClaimedAgentNotificationFact {
                    global_sequence: row
                        .try_get("global_sequence")
                        .map_err(|_| PortError::Persistence)?,
                    claim_token,
                    event_id: row
                        .try_get("event_id")
                        .map_err(|_| PortError::Persistence)?,
                    event_version: positive_u32(
                        row.try_get("event_version")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                    aggregate_type: row
                        .try_get("aggregate_type")
                        .map_err(|_| PortError::Persistence)?,
                    aggregate_id: row
                        .try_get("aggregate_id")
                        .map_err(|_| PortError::Persistence)?,
                    aggregate_version: positive_u64(
                        row.try_get("aggregate_version")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                    active_scope: row
                        .try_get("active_scope")
                        .map_err(|_| PortError::Persistence)?,
                    actor: row.try_get("actor").map_err(|_| PortError::Persistence)?,
                    correlation_id: row
                        .try_get("correlation_id")
                        .map_err(|_| PortError::Persistence)?,
                    causation_id: row
                        .try_get("causation_id")
                        .map_err(|_| PortError::Persistence)?,
                    payload: row.try_get("payload").map_err(|_| PortError::Persistence)?,
                    occurred_at: row
                        .try_get("occurred_at")
                        .map_err(|_| PortError::Persistence)?,
                    available_at: row
                        .try_get("available_at")
                        .map_err(|_| PortError::Persistence)?,
                    attempts: positive_u32(
                        row.try_get("publish_attempts")
                            .map_err(|_| PortError::Persistence)?,
                    )?,
                })
            })
            .transpose()?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(fact)
    }

    pub async fn complete_notification_fact(
        &self,
        global_sequence: i64,
        claim_token: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PortError> {
        if global_sequence <= 0 || claim_token.is_nil() {
            return Err(PortError::Denied);
        }
        let mut tx = self.begin_holding_transaction().await?;
        let updated = sqlx::query(
            "UPDATE erpx_cqrs_write.outbox_events
             SET published_at=$4,claim_token=NULL,claim_until=NULL,publish_error=NULL
             WHERE holding_id=$1 AND global_sequence=$2 AND claim_token=$3
               AND claim_until>$4 AND published_at IS NULL AND dead_lettered_at IS NULL",
        )
        .bind(self.holding_id())
        .bind(global_sequence)
        .bind(claim_token)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(updated.rows_affected())?;
        tx.commit().await.map_err(|_| PortError::Persistence)
    }

    pub async fn fail_notification_fact(
        &self,
        global_sequence: i64,
        claim_token: Uuid,
        error_digest: &str,
        retry_at: DateTime<Utc>,
        at: DateTime<Utc>,
        terminal: bool,
    ) -> Result<(), PortError> {
        if global_sequence <= 0
            || claim_token.is_nil()
            || error_digest.len() != 64
            || !error_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || (!terminal && retry_at <= at)
        {
            return Err(PortError::Denied);
        }
        let mut tx = self.begin_holding_transaction().await?;
        let updated = sqlx::query(
            "UPDATE erpx_cqrs_write.outbox_events
             SET claim_token=NULL,claim_until=NULL,publish_error=$4,
                 available_at=CASE WHEN $6 THEN available_at ELSE $5 END,
                 dead_lettered_at=CASE WHEN $6 THEN $7 ELSE NULL END
             WHERE holding_id=$1 AND global_sequence=$2 AND claim_token=$3
               AND claim_until>$7 AND published_at IS NULL AND dead_lettered_at IS NULL",
        )
        .bind(self.holding_id())
        .bind(global_sequence)
        .bind(claim_token)
        .bind(error_digest.to_ascii_lowercase())
        .bind(retry_at)
        .bind(terminal)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(updated.rows_affected())?;
        tx.commit().await.map_err(|_| PortError::Persistence)
    }

    /// Rechecks the exact running lease and layered reservation under the same
    /// holding RLS binding used by claim and terminal writes.
    pub async fn verify_execution(
        &self,
        claim: &ClaimedAssignment,
        at: DateTime<Utc>,
    ) -> Result<(), PortError> {
        let mut tx = self.begin_holding_transaction().await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM erpx_agent_write.agent_assignments a
                 JOIN erpx_agent_write.agent_budget_reservations r
                   ON r.reservation_id=a.budget_reservation_id
                  AND r.holding_id=a.holding_id
                 WHERE a.holding_id=$1 AND a.assignment_id=$2
                   AND a.assignment_status='running'
                   AND a.aggregate_version=$3 AND a.lease_token=$4
                   AND a.lease_expires_at>$5 AND r.reservation_status='reserved'
                   AND r.estimated_units=$6 AND r.estimated_cost_micros=$7
                   AND r.stop_after_units=$8 AND r.queue_priority=$9
                   AND (
                       SELECT count(*)
                       FROM erpx_agent_write.agent_budget_reservation_layers rl
                       JOIN erpx_agent_write.agent_budget_limits l
                         ON l.holding_id=rl.holding_id
                        AND l.layer_kind=rl.layer_kind AND l.subject_ref=rl.subject_ref
                       WHERE rl.holding_id=$1 AND rl.reservation_id=r.reservation_id
                         AND l.enabled=TRUE
                         AND l.active_concurrency BETWEEN 1 AND l.concurrency_limit
                   )=5
             )",
        )
        .bind(self.holding_id())
        .bind(claim.assignment.id().as_uuid())
        .bind(to_i64(claim.assignment.version())?)
        .bind(claim.lease_token)
        .bind(at)
        .bind(to_i64(claim.budget_request.estimated_units)?)
        .bind(to_i64(claim.budget_request.estimated_cost_micros)?)
        .bind(to_i64(claim.budget_request.stop_after_units)?)
        .bind(i32::from(claim.budget_request.queue_priority))
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        valid.then_some(()).ok_or(PortError::VersionConflict)
    }
}

#[async_trait]
impl AssignmentRepository for SqlxAgentRuntime {
    async fn load_assignment(
        &self,
        authority_scope: &AuthorityScope,
        id: AssignmentId,
    ) -> Result<Option<Assignment>, PortError> {
        let mut tx = self.database.begin().await?;
        let assignment = normalized::load_assignment(&mut tx, self.holding_id(), id).await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        let Some(assignment) = assignment else {
            return Ok(None);
        };
        if assignment.id() != id {
            return Err(PortError::Persistence);
        }
        if !authority_scope.contains(assignment.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(assignment))
    }

    async fn child_count(
        &self,
        root_id: AssignmentId,
        parent_id: AssignmentId,
    ) -> Result<u16, PortError> {
        let mut tx = self.database.begin().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM erpx_agent_write.assignment_delegations
             WHERE holding_id=$1 AND root_assignment_id=$2 AND parent_assignment_id=$3",
        )
        .bind(self.holding_id())
        .bind(root_id.as_uuid())
        .bind(parent_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        u16::try_from(count).map_err(|_| PortError::Persistence)
    }
}

#[async_trait]
impl BudgetLedgerPort for SqlxAgentRuntime {
    async fn quote_layers(
        &self,
        subjects: &BudgetSubjects,
        _request: &BudgetRequest,
        _at: DateTime<Utc>,
    ) -> Result<Vec<BudgetLayer>, PortError> {
        let subjects = [
            (BudgetLayerKind::Holding, &subjects.data_profile_key),
            (BudgetLayerKind::Workspace, &subjects.workspace_ref),
            (BudgetLayerKind::Agent, &subjects.agent_ref),
            (BudgetLayerKind::Principal, &subjects.principal_ref),
            (
                BudgetLayerKind::RootAssignment,
                &subjects.root_assignment_ref,
            ),
        ];
        let mut tx = self.database.begin().await?;
        let mut layers = Vec::with_capacity(subjects.len());
        for (kind, subject_ref) in subjects {
            let kind_name = budget_layer_name(kind);
            let row: Option<(i64, i64, i32, i32)> = sqlx::query_as(
                "SELECT remaining_units,remaining_cost_micros,active_concurrency,concurrency_limit
                 FROM erpx_agent_write.agent_budget_limits
                 WHERE holding_id=$1 AND layer_kind=$2 AND subject_ref=$3 AND enabled=TRUE",
            )
            .bind(self.holding_id())
            .bind(kind_name)
            .bind(subject_ref)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
            let Some((units, cost, active, limit)) = row else {
                return Err(PortError::BudgetUnavailable);
            };
            layers.push(BudgetLayer {
                kind,
                subject_ref: subject_ref.clone(),
                remaining_units: u64::try_from(units).map_err(|_| PortError::Persistence)?,
                remaining_cost_micros: u64::try_from(cost).map_err(|_| PortError::Persistence)?,
                active_concurrency: u32::try_from(active).map_err(|_| PortError::Persistence)?,
                concurrency_limit: u32::try_from(limit).map_err(|_| PortError::Persistence)?,
            });
        }
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(layers)
    }

    async fn reconcile_usage(
        &self,
        reservation: &BudgetReservation,
        usage: &UsageReconciliation,
        at: DateTime<Utc>,
    ) -> Result<(), PortError> {
        let mut tx = self.database.begin().await?;
        let changed = sqlx::query(
            "UPDATE erpx_agent_write.agent_budget_reservations
             SET actual_units=$2,actual_cost_micros=$3,reconciled_at=$4,
                 reservation_status=CASE WHEN $5 THEN 'over_budget' ELSE 'reconciled' END
             WHERE holding_id=$6 AND reservation_id=$1 AND reconciled_at IS NULL",
        )
        .bind(reservation.id.as_uuid())
        .bind(to_i64(usage.actual_units)?)
        .bind(to_i64(usage.actual_cost_micros)?)
        .bind(at)
        .bind(usage.exceeded_reservation)
        .bind(self.holding_id())
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(changed.rows_affected())?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(())
    }
}

#[async_trait]
impl WorkerAssignmentPort for SqlxWorkerQueue {
    async fn claim_due(
        &self,
        worker_ref: &str,
        at: DateTime<Utc>,
        lease_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedAssignment>, PortError> {
        let mut tx = self.begin_holding_transaction().await?;
        let row: Option<(Uuid, String, serde_json::Value, i64, i64, i64, i32)> = sqlx::query_as(
            "SELECT a.assignment_id,a.encrypted_prompt_ref,
                    a.payload_manifest,r.estimated_units,
                    r.estimated_cost_micros,r.stop_after_units,r.queue_priority
             FROM erpx_agent_write.agent_assignments a
             JOIN erpx_agent_write.agent_budget_reservations r
               ON r.reservation_id=a.budget_reservation_id AND r.holding_id=a.holding_id
             WHERE a.holding_id=$2 AND r.holding_id=$2
               AND a.assignment_status='queued' AND r.reservation_status='reserved'
               AND (a.not_before IS NULL OR a.not_before <= $1)
             ORDER BY a.queue_priority DESC,a.created_at,a.assignment_id
             LIMIT 1 FOR UPDATE OF a,r SKIP LOCKED",
        )
        .bind(at)
        .bind(self.holding_id())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        let Some((
            id,
            prompt_ref,
            payload,
            estimated_units,
            estimated_cost_micros,
            stop_after_units,
            queue_priority,
        )) = row
        else {
            tx.commit().await.map_err(|_| PortError::Persistence)?;
            return Ok(None);
        };
        let assignment_id = AssignmentId::try_from_uuid(id).map_err(|_| PortError::Persistence)?;
        let mut assignment = normalized::load_assignment(&mut tx, self.holding_id(), assignment_id)
            .await?
            .ok_or(PortError::Persistence)?;
        let previous_version = assignment.version();
        let lease_token = Uuid::new_v4();
        assignment
            .claim(lease_token, worker_ref.to_owned(), lease_until, at)
            .map_err(|_| PortError::VersionConflict)?;
        let updated = sqlx::query(
            "UPDATE erpx_agent_write.agent_assignments
             SET assignment_status='running',attempt=$2,lease_token=$3,lease_owner=$4,
                 lease_acquired_at=$5,lease_expires_at=$6,aggregate_version=$7,updated_at=$8
             WHERE assignment_id=$1 AND holding_id=$9 AND assignment_status='queued'
               AND aggregate_version=$10",
        )
        .bind(id)
        .bind(i32::from(assignment.attempt()))
        .bind(lease_token)
        .bind(worker_ref)
        .bind(at)
        .bind(lease_until)
        .bind(to_i64(assignment.version())?)
        .bind(at)
        .bind(self.holding_id())
        .bind(to_i64(previous_version)?)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        if updated.rows_affected() != 1 {
            return Err(PortError::VersionConflict);
        }
        append_assignment_state_fact(
            &mut tx,
            self.holding_id(),
            &assignment,
            worker_actor(worker_ref),
            at,
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(Some(ClaimedAssignment {
            assignment,
            lease_token,
            prompt_ref,
            payload_manifest: serde_json::from_value(payload)
                .map_err(|_| PortError::Persistence)?,
            budget_request: BudgetRequest {
                estimated_units: u64::try_from(estimated_units)
                    .map_err(|_| PortError::Persistence)?,
                estimated_cost_micros: u64::try_from(estimated_cost_micros)
                    .map_err(|_| PortError::Persistence)?,
                queue_priority: u16::try_from(queue_priority)
                    .map_err(|_| PortError::Persistence)?,
                stop_after_units: u64::try_from(stop_after_units)
                    .map_err(|_| PortError::Persistence)?,
            },
        }))
    }

    async fn load_prompt_material(&self, claim: &ClaimedAssignment) -> Result<String, PortError> {
        // The adapter stores only an opaque encrypted reference. Composition
        // must replace this fail-closed resolver with the approved secret/object
        // store and reauthorize the payload before provider egress.
        let _ = claim;
        Err(PortError::ProtectedMaterialUnavailable)
    }

    async fn save_checkpoint(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        checkpoint: Checkpoint,
    ) -> Result<(), PortError> {
        let mut tx = self.begin_holding_transaction().await?;
        let mut assignment = fence_running_assignment(
            &mut tx,
            self.holding_id(),
            assignment_id,
            expected_version,
            lease_token,
            checkpoint.created_at,
        )
        .await?;
        assignment
            .checkpoint(lease_token, checkpoint.clone(), checkpoint.created_at)
            .map_err(|_| PortError::VersionConflict)?;
        let result = sqlx::query(
            "INSERT INTO erpx_agent_write.assignment_checkpoints
                (holding_id,checkpoint_id,assignment_id,step,state_digest,retry_safe,lease_token,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(self.holding_id())
        .bind(checkpoint.checkpoint_id.as_uuid())
        .bind(assignment_id.as_uuid())
        .bind(i32::try_from(checkpoint.step).map_err(|_| PortError::Persistence)?)
        .bind(checkpoint.state_digest)
        .bind(checkpoint.retry_safe)
        .bind(lease_token)
        .bind(checkpoint.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        if result.rows_affected() != 1 {
            return Err(PortError::VersionConflict);
        }
        let assignment_update = sqlx::query(
            "UPDATE erpx_agent_write.agent_assignments
             SET aggregate_version=$2,updated_at=$3
             WHERE assignment_id=$1 AND holding_id=$4 AND aggregate_version=$5",
        )
        .bind(assignment_id.as_uuid())
        .bind(to_i64(assignment.version())?)
        .bind(checkpoint.created_at)
        .bind(self.holding_id())
        .bind(to_i64(expected_version)?)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(assignment_update.rows_affected())?;
        crate::publication::append_assignment_directory_draft(
            &mut tx,
            self.holding_id(),
            &assignment,
            assignment_actor(&assignment),
            checkpoint.created_at,
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        output_digest: &str,
        encrypted_output_ref: &str,
        proposed_actions: &[ActionCardProposal],
        usage: &UsageReconciliation,
        at: DateTime<Utc>,
    ) -> Result<(), PortError> {
        let mut tx = self.begin_holding_transaction().await?;
        let mut assignment = fence_running_assignment(
            &mut tx,
            self.holding_id(),
            assignment_id,
            expected_version,
            lease_token,
            at,
        )
        .await?;
        assignment
            .complete(lease_token, output_digest.to_owned(), at)
            .map_err(|_| PortError::VersionConflict)?;
        let completed = sqlx::query(
            "UPDATE erpx_agent_write.agent_assignments
             SET assignment_status='completed',output_digest=$2,encrypted_output_ref=$3,
                 actual_units=$4,actual_cost_micros=$5,lease_token=NULL,lease_owner=NULL,
                 lease_acquired_at=NULL,lease_expires_at=NULL,aggregate_version=$6,updated_at=$7
             WHERE assignment_id=$1 AND holding_id=$8 AND aggregate_version=$9",
        )
        .bind(assignment_id.as_uuid())
        .bind(output_digest)
        .bind(encrypted_output_ref)
        .bind(to_i64(usage.actual_units)?)
        .bind(to_i64(usage.actual_cost_micros)?)
        .bind(to_i64(assignment.version())?)
        .bind(at)
        .bind(self.holding_id())
        .bind(to_i64(expected_version)?)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(completed.rows_affected())?;
        for action in proposed_actions {
            sqlx::query(
                "INSERT INTO erpx_agent_write.agent_action_cards
                    (holding_id,action_card_id,assignment_id,agent_principal_ref,owner_context,
                     command_type,payload_digest,action_status,created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,'proposed',$8)",
            )
            .bind(self.holding_id())
            .bind(action.id.as_uuid())
            .bind(assignment_id.as_uuid())
            .bind(&action.agent_principal_ref)
            .bind(&action.owner_context)
            .bind(&action.command_type)
            .bind(&action.payload_digest)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(|_| PortError::Persistence)?;
        }
        reconcile_budget_tx(
            &mut tx,
            self.holding_id(),
            assignment.budget_reservation_id(),
            usage,
            at,
        )
        .await?;
        append_assignment_state_fact(
            &mut tx,
            self.holding_id(),
            &assignment,
            assignment_actor(&assignment),
            at,
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)
    }

    async fn fail(
        &self,
        assignment_id: AssignmentId,
        expected_version: u64,
        lease_token: Uuid,
        failure: &AssignmentFailure,
        at: DateTime<Utc>,
    ) -> Result<(), PortError> {
        let mut tx = self.begin_holding_transaction().await?;
        let mut assignment = fence_running_assignment(
            &mut tx,
            self.holding_id(),
            assignment_id,
            expected_version,
            lease_token,
            at,
        )
        .await?;
        assignment
            .fail(lease_token, failure.clone(), at)
            .map_err(|_| PortError::VersionConflict)?;
        let failed = sqlx::query(
            "UPDATE erpx_agent_write.agent_assignments
             SET assignment_status='failed',failure_code=$2,failure_retryable=$3,
                 failure_detail_digest=$4,failure_failed_at=$5,lease_token=NULL,lease_owner=NULL,
                 lease_acquired_at=NULL,lease_expires_at=NULL,aggregate_version=$6,updated_at=$7
             WHERE assignment_id=$1 AND holding_id=$8 AND aggregate_version=$9",
        )
        .bind(assignment_id.as_uuid())
        .bind(&failure.code)
        .bind(failure.retryable)
        .bind(&failure.detail_digest)
        .bind(failure.failed_at)
        .bind(to_i64(assignment.version())?)
        .bind(at)
        .bind(self.holding_id())
        .bind(to_i64(expected_version)?)
        .execute(&mut *tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(failed.rows_affected())?;
        if !failure.retryable {
            release_budget_tx(
                &mut tx,
                self.holding_id(),
                assignment.budget_reservation_id(),
                at,
            )
            .await?;
        }
        append_assignment_state_fact(
            &mut tx,
            self.holding_id(),
            &assignment,
            assignment_actor(&assignment),
            at,
        )
        .await?;
        tx.commit().await.map_err(|_| PortError::Persistence)
    }
}

pub(crate) async fn recover_one_expired_assignment(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    at: DateTime<Utc>,
) -> Result<bool, PortError> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT a.assignment_id,r.reservation_status
         FROM erpx_agent_write.agent_assignments a
         JOIN erpx_agent_write.agent_budget_reservations r
           ON r.holding_id=a.holding_id AND r.reservation_id=a.budget_reservation_id
         WHERE a.holding_id=$1 AND a.assignment_status='running'
           AND a.lease_expires_at<=$2
         ORDER BY a.lease_expires_at,a.assignment_id
         LIMIT 1 FOR UPDATE OF a,r SKIP LOCKED",
    )
    .bind(holding_id)
    .bind(at)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some((assignment_id, reservation_status)) = row else {
        return Ok(false);
    };
    let assignment_id =
        AssignmentId::try_from_uuid(assignment_id).map_err(|_| PortError::Persistence)?;
    let mut assignment = normalized::load_assignment(tx, holding_id, assignment_id)
        .await?
        .ok_or(PortError::Persistence)?;
    let previous_version = assignment.version();
    let recovery = assignment
        .recover_expired_lease(reservation_status == "reserved", at)
        .map_err(|_| PortError::VersionConflict)?;
    let (status, failure) = match recovery {
        ExpiredLeaseRecovery::Requeued => ("queued", None),
        ExpiredLeaseRecovery::FailedUnsafeCheckpoint
        | ExpiredLeaseRecovery::FailedAttemptLimit
        | ExpiredLeaseRecovery::FailedBudgetUnavailable => ("failed", assignment.failure()),
    };
    let updated = sqlx::query(
        "UPDATE erpx_agent_write.agent_assignments
         SET assignment_status=$3,lease_token=NULL,lease_owner=NULL,lease_acquired_at=NULL,
             lease_expires_at=NULL,failure_code=$4,failure_retryable=$5,
             failure_detail_digest=$6,failure_failed_at=$7,aggregate_version=$8,updated_at=$2
         WHERE holding_id=$1 AND assignment_id=$9 AND assignment_status='running'
           AND aggregate_version=$10",
    )
    .bind(holding_id)
    .bind(at)
    .bind(status)
    .bind(failure.map(|value| value.code.as_str()))
    .bind(failure.map(|value| value.retryable))
    .bind(failure.map(|value| value.detail_digest.as_str()))
    .bind(failure.map(|value| value.failed_at))
    .bind(to_i64(assignment.version())?)
    .bind(assignment_id.as_uuid())
    .bind(to_i64(previous_version)?)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    require_one(updated.rows_affected())?;
    if recovery != ExpiredLeaseRecovery::Requeued && reservation_status == "reserved" {
        release_budget_tx(tx, holding_id, assignment.budget_reservation_id(), at).await?;
    }
    append_assignment_state_fact(
        tx,
        holding_id,
        &assignment,
        worker_actor("system:expired-agent-lease-recovery"),
        at,
    )
    .await?;
    Ok(true)
}

fn worker_actor(principal_ref: &str) -> serde_json::Value {
    serde_json::json!({
        "principal_ref": principal_ref,
        "kind": "system_worker",
        "session_id": null,
        "represented_principal_ref": null,
        "support_capability_id": null,
    })
}

fn assignment_actor(assignment: &Assignment) -> serde_json::Value {
    serde_json::json!({
        "principal_ref": assignment.pin().agent_principal_ref,
        "kind": "agent",
        "session_id": null,
        "represented_principal_ref": null,
        "support_capability_id": null,
    })
}

async fn fence_running_assignment(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    assignment_id: AssignmentId,
    expected_version: u64,
    lease_token: Uuid,
    at: DateTime<Utc>,
) -> Result<Assignment, PortError> {
    let locked: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_agent_write.agent_assignments
         WHERE holding_id=$5 AND assignment_id=$1 AND assignment_status='running'
           AND aggregate_version=$2 AND lease_token=$3 AND lease_expires_at>$4
         FOR UPDATE",
    )
    .bind(assignment_id.as_uuid())
    .bind(to_i64(expected_version)?)
    .bind(lease_token)
    .bind(at)
    .bind(holding_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if locked.is_none() {
        return Err(PortError::VersionConflict);
    }
    let assignment = normalized::load_assignment(tx, holding_id, assignment_id)
        .await?
        .ok_or(PortError::VersionConflict)?;
    if assignment.id() != assignment_id || assignment.version() != expected_version {
        return Err(PortError::Persistence);
    }
    Ok(assignment)
}

async fn reconcile_budget_tx(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    reservation_id: erpx_agent_orchestration::BudgetReservationId,
    usage: &UsageReconciliation,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    let reservation: Option<(i64, i64)> = sqlx::query_as(
        "SELECT estimated_units,estimated_cost_micros
         FROM erpx_agent_write.agent_budget_reservations
         WHERE reservation_id=$1 AND holding_id=$2
           AND reservation_status='reserved' FOR UPDATE",
    )
    .bind(reservation_id.as_uuid())
    .bind(holding_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    let Some((estimated_units, estimated_cost)) = reservation else {
        return Err(PortError::VersionConflict);
    };
    let layers: Vec<(String, String)> = sqlx::query_as(
        "SELECT layer_kind,subject_ref
         FROM erpx_agent_write.agent_budget_reservation_layers
         WHERE reservation_id=$1 AND holding_id=$2 FOR UPDATE",
    )
    .bind(reservation_id.as_uuid())
    .bind(holding_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if layers.len() != BudgetLayerKind::ALL.len() {
        return Err(PortError::Persistence);
    }
    let actual_units = to_i64(usage.actual_units)?;
    let actual_cost = to_i64(usage.actual_cost_micros)?;
    let exceeded = usage.exceeded_reservation
        || actual_units > estimated_units
        || actual_cost > estimated_cost;
    for (kind, subject) in layers {
        let adjusted = sqlx::query(
            "UPDATE erpx_agent_write.agent_budget_limits SET
                remaining_units=GREATEST(0,remaining_units-GREATEST(0,$3-$5))
                    +GREATEST(0,$5-$3),
                remaining_cost_micros=GREATEST(0,remaining_cost_micros-GREATEST(0,$4-$6))
                    +GREATEST(0,$6-$4),
                active_concurrency=active_concurrency-1,updated_at=$7
             WHERE layer_kind=$1 AND subject_ref=$2 AND active_concurrency>0
               AND holding_id=$8",
        )
        .bind(kind)
        .bind(subject)
        .bind(actual_units)
        .bind(actual_cost)
        .bind(estimated_units)
        .bind(estimated_cost)
        .bind(at)
        // RLS is authoritative; the explicit predicate below prevents a
        // privileged connection from widening this holding-bound worker.
        .bind(holding_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(adjusted.rows_affected())?;
    }
    let reconciled = sqlx::query(
        "UPDATE erpx_agent_write.agent_budget_reservations
         SET actual_units=$2,actual_cost_micros=$3,reconciled_at=$4,
             reservation_status=CASE WHEN $5 THEN 'over_budget' ELSE 'reconciled' END
         WHERE reservation_id=$1 AND holding_id=$6",
    )
    .bind(reservation_id.as_uuid())
    .bind(actual_units)
    .bind(actual_cost)
    .bind(at)
    .bind(exceeded)
    .bind(holding_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    require_one(reconciled.rows_affected())
}

async fn release_budget_tx(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: Uuid,
    reservation_id: erpx_agent_orchestration::BudgetReservationId,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    let layers: Vec<(String, String, i64, i64)> = sqlx::query_as(
        "SELECT layer_kind,subject_ref,reserved_units,reserved_cost_micros
         FROM erpx_agent_write.agent_budget_reservation_layers
         WHERE holding_id=$1 AND reservation_id=$2 FOR UPDATE",
    )
    .bind(holding_id)
    .bind(reservation_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    if layers.len() != BudgetLayerKind::ALL.len() {
        return Err(PortError::Persistence);
    }
    let changed = sqlx::query(
        "UPDATE erpx_agent_write.agent_budget_reservations
         SET reservation_status='released',reconciled_at=$3
         WHERE holding_id=$1 AND reservation_id=$2 AND reservation_status='reserved'",
    )
    .bind(holding_id)
    .bind(reservation_id.as_uuid())
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(|_| PortError::Persistence)?;
    require_one(changed.rows_affected())?;
    for (kind, subject, units, cost) in layers {
        let restored = sqlx::query(
            "UPDATE erpx_agent_write.agent_budget_limits
             SET remaining_units=remaining_units+$4,
                 remaining_cost_micros=remaining_cost_micros+$5,
                 active_concurrency=active_concurrency-1,updated_at=$6
             WHERE holding_id=$1 AND layer_kind=$2 AND subject_ref=$3
               AND active_concurrency>0",
        )
        .bind(holding_id)
        .bind(kind)
        .bind(subject)
        .bind(units)
        .bind(cost)
        .bind(at)
        .execute(&mut **tx)
        .await
        .map_err(|_| PortError::Persistence)?;
        require_one(restored.rows_affected())?;
    }
    Ok(())
}

fn budget_layer_name(kind: BudgetLayerKind) -> &'static str {
    match kind {
        BudgetLayerKind::Holding => "holding",
        BudgetLayerKind::Workspace => "workspace",
        BudgetLayerKind::Agent => "agent",
        BudgetLayerKind::Principal => "principal",
        BudgetLayerKind::RootAssignment => "root_assignment",
    }
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Persistence)
}

fn positive_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PortError::Persistence)
}

fn positive_u32(value: i32) -> Result<u32, PortError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(PortError::Persistence)
}

fn require_one(rows_affected: u64) -> Result<(), PortError> {
    (rows_affected == 1)
        .then_some(())
        .ok_or(PortError::VersionConflict)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use erpx_agent_orchestration::PortError;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    use super::SqlxWorkerQueue;

    fn lazy_pool() -> sqlx::PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://erpx@127.0.0.1/erpx_agent_holding_test")
            .expect("syntactically valid lazy pool")
    }

    #[tokio::test]
    async fn worker_queue_requires_one_non_nil_holding_before_any_database_access() {
        assert!(matches!(
            SqlxWorkerQueue::try_new(lazy_pool(), Uuid::nil()),
            Err(erpx_agent_orchestration::PortError::Denied)
        ));
        let holding_id = Uuid::from_u128(1);
        let queue = SqlxWorkerQueue::try_new(lazy_pool(), holding_id).expect("holding-bound queue");
        assert_eq!(queue.holding_id(), holding_id);
    }

    #[test]
    fn every_worker_transaction_binds_rls_and_repeats_the_holding_predicate() {
        let source = include_str!("runtime.rs");
        let binding = include_str!("holding.rs");
        assert_eq!(binding.matches(concat!(".be", "gin()")).count(), 1);
        assert!(binding.contains("SELECT set_config('app.holding_id',$1,true)"));
        assert!(source.contains("WHERE a.holding_id=$2 AND s.holding_id=$2"));
        assert!(source.contains("WHERE a.holding_id=$5 AND s.holding_id=$5"));
        assert!(source.contains("WHERE assignment_id=$1 AND holding_id=$8"));
        assert!(source.contains("WHERE assignment_id=$1 AND holding_id=$7"));
        assert!(source.contains("WHERE holding_id=$1 AND assignment_id=$8"));
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }

    #[test]
    fn every_worker_state_change_appends_an_opaque_durable_notification_fact() {
        let source = include_str!("runtime.rs");
        assert!(source.matches("append_assignment_state_fact(").count() >= 4);
        assert!(source.contains("system:expired-agent-lease-recovery"));
        let fact = include_str!("command.rs")
            .split("pub(crate) async fn append_assignment_state_fact")
            .nth(1)
            .and_then(|value| value.split("fn stable_fact_uuid").next())
            .expect("notification fact function");
        assert!(fact.contains("erpx_cqrs_write.outbox_events"));
        assert!(fact.contains("state_notification.requested"));
        assert!(!fact.contains("prompt"));
        assert!(!fact.contains("output"));
        assert!(!fact.contains("failure_detail"));
    }

    #[test]
    fn execution_guard_is_exact_holding_lease_version_and_budget_bound() {
        let source = include_str!("runtime.rs");
        for fence in [
            "a.holding_id=$1 AND a.assignment_id=$2",
            "a.aggregate_version=$3 AND a.lease_token=$4",
            "a.lease_expires_at>$5 AND r.reservation_status='reserved'",
            "r.estimated_units=$6 AND r.estimated_cost_micros=$7",
            "r.stop_after_units=$8 AND r.queue_priority=$9",
            ")=5",
        ] {
            assert!(source.contains(fence), "missing execution fence: {fence}");
        }
    }

    #[test]
    fn claim_and_terminal_updates_are_lease_and_version_fenced() {
        let source = include_str!("runtime.rs");
        assert!(source.contains("FOR UPDATE OF a,s SKIP LOCKED"));
        assert!(source.contains("a.lease_expires_at>$4"));
        assert!(source.contains("FOR UPDATE OF a,s"));
        assert!(source.contains("aggregate_version=$2 AND lease_token=$3"));
        assert!(source.contains("assignment_status='completed'"));
        assert!(source.contains("assignment_status='failed'"));
        assert!(source.contains("recover_expired_lease"));
        assert!(source.contains("release_budget_tx"));
    }

    #[test]
    fn protected_prompt_resolver_fails_closed_until_composed() {
        let source = include_str!("runtime.rs");
        assert!(source.contains("ProtectedMaterialUnavailable"));
        assert!(!source.contains(concat!("SELECT prompt_", "material")));
    }

    #[tokio::test]
    #[ignore = "requires a disposable migrated PostgreSQL holding-write database"]
    async fn postgres_notification_claim_denies_missing_and_wrong_holding_bindings() {
        let database_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&database_url)
            .await
            .expect("disposable holding-write database");
        let holding_a = Uuid::new_v4();
        let holding_b = Uuid::new_v4();
        let queue_a = SqlxWorkerQueue::try_new(pool.clone(), holding_a).expect("holding A");
        let queue_b = SqlxWorkerQueue::try_new(pool.clone(), holding_b).expect("holding B");

        let mut unbound = pool.begin().await.expect("unbound transaction");
        let current: Option<Uuid> =
            sqlx::query_scalar("SELECT erpx_cqrs_write.current_holding_id()")
                .fetch_one(&mut *unbound)
                .await
                .expect("unbound holding lookup");
        assert!(current.is_none());
        unbound.rollback().await.expect("rollback");

        let event_id = Uuid::new_v4();
        let aggregate_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let at = Utc::now();
        let mut tx = queue_a
            .begin_holding_transaction()
            .await
            .expect("holding A transaction");
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events
                (event_id,context_name,partition_key,command_id,event_type,event_version,
                 aggregate_type,aggregate_id,aggregate_version,active_scope,actor,
                 correlation_id,payload,occurred_at,available_at)
             VALUES ($1,'agent_orchestration',$2,$3,
                     'agent.assignment.state_notification.requested',1,
                     'assignment',$4,1,$5,$6,$7,$8,$9,$9)",
        )
        .bind(event_id)
        .bind(format!("holding:{holding_a}"))
        .bind(Uuid::new_v4())
        .bind(aggregate_id)
        .bind(serde_json::json!({"holding":{"holding_id":Uuid::new_v4()}}))
        .bind(serde_json::json!({"principal_ref":"agent:test"}))
        .bind(correlation_id)
        .bind(serde_json::json!({
            "assignment_id": aggregate_id,
            "root_assignment_id": aggregate_id,
            "recipient_principal_ref": "human:test",
            "agent_principal_ref": "agent:test",
            "state": "failed",
            "aggregate_version": 1
        }))
        .bind(at)
        .execute(&mut *tx)
        .await
        .expect("insert holding A fact");
        tx.commit().await.expect("commit holding A fact");

        assert!(
            queue_b
                .claim_notification_fact(at, at + chrono::Duration::minutes(1))
                .await
                .expect("wrong holding scan")
                .is_none()
        );
        let claimed = queue_a
            .claim_notification_fact(at, at + chrono::Duration::minutes(1))
            .await
            .expect("holding A claim")
            .expect("holding A fact");
        assert!(matches!(
            queue_b
                .complete_notification_fact(
                    claimed.global_sequence,
                    claimed.claim_token,
                    at + chrono::Duration::seconds(1),
                )
                .await,
            Err(PortError::VersionConflict)
        ));
        queue_a
            .complete_notification_fact(
                claimed.global_sequence,
                claimed.claim_token,
                at + chrono::Duration::seconds(1),
            )
            .await
            .expect("holding A completion");

        let mut cleanup = queue_a
            .begin_holding_transaction()
            .await
            .expect("cleanup transaction");
        sqlx::query(
            "DELETE FROM erpx_cqrs_write.outbox_events WHERE holding_id=$1 AND event_id=$2",
        )
        .bind(holding_a)
        .bind(event_id)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup disposable fact");
        cleanup.commit().await.expect("cleanup commit");
        pool.close().await;
    }

    #[test]
    fn notification_outbox_is_holding_scoped_and_lease_fenced() {
        let source = include_str!("runtime.rs");
        for fence in [
            "WHERE holding_id=$1",
            "FOR UPDATE SKIP LOCKED",
            "o.holding_id=$1 AND o.global_sequence=c.global_sequence",
            "global_sequence=$2 AND claim_token=$3",
            "claim_until>$4",
            "claim_until>$7",
        ] {
            assert!(source.contains(fence), "missing outbox fence: {fence}");
        }
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }
}
