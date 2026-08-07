use super::super::*;

#[async_trait]
pub trait RoleAssignmentStore: Send + Sync {
    async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError>;
    async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError>;
    async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError>;
}

pub struct RoleLifecycleService<R, C> {
    store: Arc<R>,
    clock: Arc<C>,
    audit: Arc<dyn AuditSink>,
}

pub struct AuthoritativeRoleLifecycleService<R, C, U> {
    store: Arc<R>,
    clock: Arc<C>,
    commands: Arc<U>,
}

impl<R, C, U> AuthoritativeRoleLifecycleService<R, C, U>
where
    R: RoleAssignmentStore + 'static,
    C: Clock + 'static,
    U: CommandUnitOfWork + 'static,
{
    pub fn new(store: Arc<R>, clock: Arc<C>, commands: Arc<U>) -> Self {
        Self {
            store,
            clock,
            commands,
        }
    }

    /// Build the review queue from authoritative role rows. The domain
    /// classifier is deterministic and the command path remains separate so
    /// a review read cannot grant or extend access by itself.
    pub async fn review_queue(
        &self,
        expiry_horizon: Duration,
    ) -> Result<RoleReviewQueue, ApplicationError> {
        let assignments = self.store.list(u32::MAX).await?;
        Ok(build_role_review_queue(
            &assignments,
            self.clock.now(),
            expiry_horizon,
        )?)
    }

    pub async fn renew(
        &self,
        context: &CommandContext,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew(self.clock.now(), valid_until, grantor, approver)?;
        renewed.validate_for_grant(parent.as_ref())?;
        let aggregate_state = serde_json::to_value(&renewed)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            renewed.id,
            1,
            "role_assignment.renewed",
            aggregate_state,
            None,
            "role_renewed",
            renewed.grant_reason.clone(),
        )
        .await?;
        Ok(renewed)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn renew_with_reviewer(
        &self,
        context: &CommandContext,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
        renewal_reviewer: &str,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew_with_reviewer(
            self.clock.now(),
            valid_until,
            grantor,
            approver,
            renewal_reviewer,
        )?;
        renewed.validate_for_grant(parent.as_ref())?;
        let aggregate_state = serde_json::to_value(&renewed)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            renewed.id,
            1,
            "role_assignment.renewed",
            aggregate_state,
            None,
            "role_renewed_reviewed",
            renewed.grant_reason.clone(),
        )
        .await?;
        Ok(renewed)
    }

    /// Recover the final Platform Root or Holding Owner only when the
    /// authoritative write-side role set confirms no active target remains.
    /// The requester must be the authenticated command actor; materialization
    /// enforces finite validity and independent high-risk approval.
    pub async fn recover_final_role(
        &self,
        context: &CommandContext,
        mut request: RoleRecoveryRequest,
    ) -> Result<RoleAssignment, ApplicationError> {
        if request.requester_ref != context.actor.principal_ref {
            return Err(ApplicationError::Authorization(
                "recovery requester does not match command actor".into(),
            ));
        }
        // Recovery always begins at the authoritative write-side clock. The
        // transport does not get to choose or backdate the start instant.
        request.valid_from = self.clock.now();
        // The request ID is also the deterministic assignment ID. A command
        // can commit its aggregate/outbox transaction immediately before the
        // separate recovery-review receipt transaction; on retry, rehydrate
        // that exact assignment instead of treating the committed recovery as
        // a new target conflict. Validate against every other assignment so a
        // later conflicting recovery still fails closed.
        if let Some(existing) = self.store.get(request.request_id).await? {
            let other_assignments = self
                .store
                .list(u32::MAX)
                .await?
                .into_iter()
                .filter(|assignment| assignment.id != existing.id)
                .collect::<Vec<_>>();
            let expected = request.materialize(&other_assignments, request.valid_from)?;
            if expected != existing {
                return Err(ApplicationError::Conflict(
                    "recovery request ID is bound to a different assignment".into(),
                ));
            }
            return Ok(existing);
        }
        let assignments = self.store.list(u32::MAX).await?;
        let recovered = request.materialize(&assignments, request.valid_from)?;
        let aggregate_state = serde_json::to_value(&recovered)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            recovered.id,
            1,
            "role_assignment.recovered",
            aggregate_state,
            None,
            "role_recovered",
            recovered.grant_reason.clone(),
        )
        .await?;
        Ok(recovered)
    }

    pub async fn grant(
        &self,
        context: &CommandContext,
        assignment: RoleAssignment,
        parent: Option<&RoleAssignment>,
    ) -> Result<RoleAssignment, ApplicationError> {
        assignment.validate_for_grant(parent)?;
        let aggregate_state = serde_json::to_value(&assignment)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        execute_generic_command(
            &self.commands,
            context,
            "role_assignment",
            assignment.id,
            1,
            "role_assignment.granted",
            aggregate_state,
            None,
            "role_granted",
            assignment.grant_reason.clone(),
        )
        .await?;
        Ok(assignment)
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_generic_command<U>(
    commands: &Arc<U>,
    context: &CommandContext,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    aggregate_state: Value,
    customer_account_id: Option<Uuid>,
    action: &str,
    purpose: String,
) -> Result<CommandReceipt, ApplicationError>
where
    U: CommandUnitOfWork + 'static,
{
    let request_material = serde_json::json!({
        "command_id": context.command_id,
        "aggregate_type": aggregate_type,
        "aggregate_id": aggregate_id,
        "aggregate_version": aggregate_version,
        "event_type": event_type,
        "state": aggregate_state,
    });
    let mut digest = Sha256::new();
    digest.update(
        serde_json::to_vec(&request_material)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?,
    );
    let audit = AuditEvent {
        actor_ref: context.actor.principal_ref.clone(),
        customer_account_id,
        action: action.into(),
        purpose,
        correlation_id: context.correlation_id,
    };
    let event = OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        aggregate_version,
        event_type: event_type.into(),
        payload: aggregate_state.clone(),
        occurred_at: Utc::now(),
    };
    commands
        .execute_command(CommandMutation {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            request_hash: hex::encode(digest.finalize()),
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_aggregate_version: context.expected_aggregate_version,
            aggregate_state,
            event,
            event_version: 1,
            tenant_scope: Some(
                serde_json::to_value(&context.tenant_scope)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            actor: Some(
                serde_json::to_value(&context.actor)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            causation_id: context.causation_id,
            audit,
            response: serde_json::json!({ "accepted": true, "aggregate_id": aggregate_id }),
        })
        .await
}

impl<R, C> RoleLifecycleService<R, C>
where
    R: RoleAssignmentStore + 'static,
    C: Clock + 'static,
{
    pub fn new(store: Arc<R>, clock: Arc<C>, audit: Arc<dyn AuditSink>) -> Self {
        Self {
            store,
            clock,
            audit,
        }
    }

    pub async fn grant(
        &self,
        assignment: RoleAssignment,
        parent: Option<&RoleAssignment>,
    ) -> Result<RoleAssignment, ApplicationError> {
        assignment.validate_for_grant(parent)?;
        self.store.create(assignment.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: assignment.grantor.clone(),
                customer_account_id: None,
                action: "role_granted".into(),
                purpose: assignment.grant_reason.clone(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(assignment)
    }

    pub async fn renew(
        &self,
        assignment_id: Uuid,
        valid_until: DateTime<Utc>,
        grantor: &str,
        approver: Option<String>,
    ) -> Result<RoleAssignment, ApplicationError> {
        let current = self
            .store
            .get(assignment_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let parent = match current.parent_assignment_id {
            Some(parent_id) => Some(
                self.store
                    .get(parent_id)
                    .await?
                    .ok_or(ApplicationError::NotFound)?,
            ),
            None => None,
        };
        let renewed = current.renew(self.clock.now(), valid_until, grantor, approver)?;
        renewed.validate_for_grant(parent.as_ref())?;
        self.store.create(renewed.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: grantor.into(),
                customer_account_id: None,
                action: "role_renewed".into(),
                purpose: renewed.grant_reason.clone(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(renewed)
    }

    pub async fn list_expiring(
        &self,
        within: Duration,
    ) -> Result<Vec<RoleAssignment>, ApplicationError> {
        let now = self.clock.now();
        let until = now + within;
        Ok(self
            .store
            .list(500)
            .await?
            .into_iter()
            .filter(|assignment| assignment.valid_until > now && assignment.valid_until <= until)
            .collect())
    }

    pub async fn review_queue(
        &self,
        expiry_horizon: Duration,
    ) -> Result<RoleReviewQueue, ApplicationError> {
        let assignments = self.store.list(10_000).await?;
        Ok(build_role_review_queue(
            &assignments,
            self.clock.now(),
            expiry_horizon,
        )?)
    }

    pub async fn list_expiring_in_scope(
        &self,
        within: Duration,
        requested_scope: &Scope,
    ) -> Result<Vec<RoleAssignment>, ApplicationError> {
        Ok(self
            .list_expiring(within)
            .await?
            .into_iter()
            .filter(|assignment| assignment.scope.contains(requested_scope))
            .collect())
    }
}

#[cfg(test)]
mod role_lifecycle_tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use erpx_domain::{
        access::{RiskTier, Scope},
        contracts::{ActorAttribution, ActorKind, TenantScope},
    };

    #[derive(Debug, Default)]
    struct RecordingRoleStore {
        assignments: Mutex<HashMap<Uuid, RoleAssignment>>,
    }

    #[async_trait]
    impl RoleAssignmentStore for RecordingRoleStore {
        async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError> {
            self.assignments
                .lock()
                .expect("role store lock")
                .insert(assignment.id, assignment);
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError> {
            Ok(self
                .assignments
                .lock()
                .expect("role store lock")
                .get(&id)
                .cloned())
        }

        async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError> {
            Ok(self
                .assignments
                .lock()
                .expect("role store lock")
                .values()
                .take(limit as usize)
                .cloned()
                .collect())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingRoleCommands {
        received: Mutex<Vec<CommandMutation>>,
    }

    #[async_trait]
    impl CommandUnitOfWork for RecordingRoleCommands {
        async fn execute_command(
            &self,
            mutation: CommandMutation,
        ) -> Result<CommandReceipt, ApplicationError> {
            let receipt = CommandReceipt {
                command_id: mutation.command_id,
                aggregate_type: mutation.aggregate_type.clone(),
                aggregate_id: mutation.aggregate_id,
                aggregate_version: mutation.event.aggregate_version,
                event_id: mutation.event.id,
                response: mutation.response.clone(),
                replayed: false,
            };
            self.received
                .lock()
                .expect("role command lock")
                .push(mutation);
            Ok(receipt)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[derive(Debug, Default)]
    struct NoopAudit;

    #[async_trait]
    impl AuditSink for NoopAudit {
        async fn record(&self, _event: AuditEvent) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    fn context() -> CommandContext {
        CommandContext::try_new(
            "role-lifecycle-test",
            ActorAttribution {
                principal_ref: "operator".into(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            TenantScope::Platform,
            "role lifecycle test",
        )
        .expect("valid command context")
    }

    fn assignment(
        id: Uuid,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        parent_assignment_id: Option<Uuid>,
    ) -> RoleAssignment {
        RoleAssignment {
            id,
            principal_ref: "operator".into(),
            role_key: "finance_reader".into(),
            permissions: vec!["finance.read".into()],
            functional_domain: "finance".into(),
            authority_level: 2,
            approval_limit_minor: 0,
            scope: Scope::Platform,
            valid_from: from,
            valid_until: until,
            timezone: "UTC".into(),
            risk_tier: RiskTier::Standard,
            grant_reason: "lifecycle test".into(),
            ticket_reference: None,
            grantor: "operator".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    #[tokio::test]
    async fn authoritative_renewal_rechecks_parent_and_emits_new_version() {
        let from = Utc::now();
        let parent_until = from + Duration::hours(2);
        let parent = assignment(Uuid::new_v4(), from, parent_until, None);
        let child = assignment(
            Uuid::new_v4(),
            from,
            from + Duration::hours(1),
            Some(parent.id),
        );
        let store = Arc::new(RecordingRoleStore::default());
        store.create(parent.clone()).await.expect("parent");
        store.create(child.clone()).await.expect("child");
        let commands = Arc::new(RecordingRoleCommands::default());
        let service = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(from + Duration::minutes(15))),
            commands.clone(),
        );

        let renewed = service
            .renew(&context(), child.id, parent_until, "operator", None)
            .await
            .expect("parent-contained renewal");
        assert_ne!(renewed.id, child.id);
        assert_eq!(renewed.assignment_version, 2);
        assert_eq!(renewed.valid_until, parent_until);
        let mutations = commands.received.lock().expect("role command lock");
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0].event.event_type, "role_assignment.renewed");
        assert_eq!(mutations[0].event.aggregate_version, 1);
    }

    #[tokio::test]
    async fn authoritative_grant_rejects_self_approved_high_risk_assignment() {
        let from = Utc::now();
        let mut assignment = assignment(Uuid::new_v4(), from, from + Duration::hours(1), None);
        assignment.risk_tier = RiskTier::High;
        assignment.grantor = "operator".into();
        assignment.approver = Some("operator".into());
        let service = AuthoritativeRoleLifecycleService::new(
            Arc::new(RecordingRoleStore::default()),
            Arc::new(FixedClock(from)),
            Arc::new(RecordingRoleCommands::default()),
        );

        assert_eq!(
            service
                .grant(&context(), assignment, None)
                .await
                .expect_err("self-approval must be rejected")
                .to_string(),
            "high-risk assignments require an approver other than the grantor"
        );
    }

    #[tokio::test]
    async fn legacy_lifecycle_grant_rejects_expired_parent_scope() {
        let from = Utc::now();
        let parent = assignment(Uuid::new_v4(), from, from + Duration::minutes(5), None);
        let child = assignment(
            Uuid::new_v4(),
            from + Duration::minutes(6),
            from + Duration::minutes(7),
            Some(parent.id),
        );
        let store = Arc::new(RecordingRoleStore::default());
        let service =
            RoleLifecycleService::new(store, Arc::new(FixedClock(from)), Arc::new(NoopAudit));
        assert_eq!(
            service
                .grant(child, Some(&parent))
                .await
                .expect_err("child cannot start after parent expiry")
                .to_string(),
            "child assignment outlives its parent"
        );
    }

    #[tokio::test]
    async fn review_queue_is_authoritative_and_recovery_requires_command_actor() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let store = Arc::new(RecordingRoleStore::default());
        store
            .create(assignment(
                Uuid::from_u128(90),
                at,
                at + Duration::hours(1),
                None,
            ))
            .await
            .expect("assignment");
        let audit = Arc::new(NoopAudit);
        let lifecycle = RoleLifecycleService::new(store.clone(), Arc::new(FixedClock(at)), audit);
        let queue = lifecycle
            .review_queue(Duration::hours(1))
            .await
            .expect("review queue");
        assert_eq!(queue.items.len(), 1);
        assert!(
            queue.items[0]
                .classifications
                .contains(&erpx_domain::access::RoleReviewClassification::ExpiringSoon)
        );

        let commands = Arc::new(RecordingRoleCommands::default());
        let authoritative = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(at)),
            commands.clone(),
        );
        let request = RoleRecoveryRequest {
            request_id: Uuid::from_u128(91),
            target: erpx_domain::access::RoleRecoveryTarget::PlatformRoot,
            principal_ref: "recovered-root".into(),
            permissions: vec!["platform.read".into()],
            functional_domain: "platform".into(),
            authority_level: 255,
            valid_from: at,
            valid_until: at + Duration::days(1),
            timezone: "UTC".into(),
            grant_reason: "final root recovery".into(),
            ticket_reference: None,
            requester_ref: "not-operator".into(),
            approver_ref: "approver".into(),
            reviewer_ref: None,
        };
        assert_eq!(
            authoritative
                .recover_final_role(&context(), request.clone())
                .await
                .expect_err("actor mismatch")
                .to_string(),
            "authorization denied: recovery requester does not match command actor"
        );
        let mut authorized_request = request;
        authorized_request.requester_ref = "operator".into();
        let recovered = authoritative
            .recover_final_role(&context(), authorized_request)
            .await
            .expect("recovery");
        assert_eq!(recovered.role_key, "platform_root");
        assert_eq!(
            commands
                .received
                .lock()
                .expect("command lock")
                .last()
                .expect("recovery command")
                .event
                .event_type,
            "role_assignment.recovered"
        );
    }

    #[tokio::test]
    async fn recovery_retry_rehydrates_committed_assignment_without_second_command() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");
        let request = RoleRecoveryRequest {
            request_id: Uuid::from_u128(92),
            target: erpx_domain::access::RoleRecoveryTarget::PlatformRoot,
            principal_ref: "recovered-root".into(),
            permissions: vec!["platform.read".into()],
            functional_domain: "platform".into(),
            authority_level: 255,
            valid_from: at,
            valid_until: at + Duration::days(1),
            timezone: "UTC".into(),
            grant_reason: "repair recovery receipt after commit".into(),
            ticket_reference: Some("RECOVERY-RETRY".into()),
            requester_ref: "operator".into(),
            approver_ref: "approver".into(),
            reviewer_ref: None,
        };
        let committed = request.materialize(&[], at).expect("recovery assignment");
        let store = Arc::new(RecordingRoleStore::default());
        store
            .create(committed.clone())
            .await
            .expect("committed role");
        let commands = Arc::new(RecordingRoleCommands::default());
        let service = AuthoritativeRoleLifecycleService::new(
            store,
            Arc::new(FixedClock(at)),
            commands.clone(),
        );

        let returned = service
            .recover_final_role(&context(), request)
            .await
            .expect("replay existing recovery");
        assert_eq!(returned, committed);
        assert!(
            commands
                .received
                .lock()
                .expect("role command lock")
                .is_empty()
        );
    }
}
