use super::super::*;

pub struct SupportIntersectionService;

impl SupportIntersectionService {
    pub fn authorize(
        &self,
        request: &SupportIntersectionRequest<'_>,
    ) -> Result<(), SupportIntersectionDenial> {
        evaluate_support_intersection(request)
    }
}

#[async_trait]
pub trait SupportStore: Send + Sync {
    async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError>;
    async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError>;
    async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError>;
    async fn create_session(&self, session: SupportSession) -> Result<(), ApplicationError>;
    async fn get_session(&self, id: Uuid) -> Result<Option<SupportSession>, ApplicationError>;
    async fn save_session(&self, session: SupportSession) -> Result<(), ApplicationError>;
}

pub struct SupportService<S, C> {
    store: Arc<S>,
    clock: Arc<C>,
    audit: Arc<dyn AuditSink>,
}

/// Support command orchestration that writes through the application command
/// unit of work. The legacy `SupportService` remains available to isolated
/// unit tests and read-only paths while production command handlers use this
/// adapter so aggregate state, outbox, idempotency, and audit commit together.
pub struct AuthoritativeSupportService<S, C, U> {
    store: Arc<S>,
    clock: Arc<C>,
    commands: Arc<U>,
}

impl<S, C, U> AuthoritativeSupportService<S, C, U>
where
    S: SupportStore + 'static,
    C: Clock + 'static,
    U: CommandUnitOfWork + 'static,
{
    pub fn new(store: Arc<S>, clock: Arc<C>, commands: Arc<U>) -> Self {
        Self {
            store,
            clock,
            commands,
        }
    }

    pub async fn open_ticket(
        &self,
        context: &CommandContext,
        customer_account_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let ticket = SupportTicket {
            id: stable_support_ticket_id(context, "new", customer_account_id, None),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::New,
            severity: severity.into(),
            description: description.into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let receipt = execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            1,
            "support_ticket.created",
            &ticket,
            customer_account_id,
            "support_ticket_created",
        )
        .await?;
        if receipt.replayed {
            return self
                .store
                .get_ticket(receipt.aggregate_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::Repository(
                        "replayed support ticket command has no rehydratable aggregate".into(),
                    )
                });
        }
        Ok(ticket)
    }

    /// Create a new support aggregate linked to a closed predecessor. The
    /// predecessor remains terminal and the new ticket is emitted through the
    /// same authoritative command/outbox unit of work as ordinary creation.
    pub async fn open_ticket_recurrence(
        &self,
        context: &CommandContext,
        closed_ticket_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let closed = self
            .store
            .get_ticket(closed_ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let ticket = closed.recurrence(
            stable_support_ticket_id(
                context,
                "recurrence",
                closed.customer_account_id,
                Some(closed.id),
            ),
            severity,
            description,
        )?;
        let receipt = execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            1,
            "support_ticket.recurrence_created",
            &ticket,
            ticket.customer_account_id,
            "support_ticket_recurrence_created",
        )
        .await?;
        if receipt.replayed {
            return self
                .store
                .get_ticket(receipt.aggregate_id)
                .await?
                .ok_or_else(|| {
                    ApplicationError::Repository(
                        "replayed support ticket command has no rehydratable aggregate".into(),
                    )
                });
        }
        Ok(ticket)
    }

    /// Advance a support ticket through the frozen lifecycle using the same
    /// authoritative command/outbox unit of work as ticket creation.
    pub async fn transition_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        next_status: erpx_domain::support::TicketStatus,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Transition { next_status },
        )
        .await
    }

    /// Apply a support-ticket metadata or lifecycle command through the
    /// authoritative command unit of work. The aggregate is validated before
    /// the command is emitted, so the persisted JSON payload and outbox event
    /// carry the same audience-separated state.
    pub async fn apply_ticket_command(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        command: SupportTicketCommand,
    ) -> Result<SupportTicket, ApplicationError> {
        let mut ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        let event_type = command.event_type();
        let action = command.audit_action();
        ticket.apply_command(command, &context.active_scope, self.clock.now())?;
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_ticket",
            ticket.id,
            aggregate_version,
            event_type,
            &ticket,
            ticket.customer_account_id,
            action,
        )
        .await?;
        Ok(ticket)
    }

    pub async fn add_customer_reply(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        body: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::CustomerReply {
                message_id: Uuid::new_v4(),
                author_ref: context.actor.principal_ref.clone(),
                body: body.into(),
                created_at: self.clock.now(),
            },
        )
        .await
    }

    pub async fn add_internal_note(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        body: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::InternalNote {
                message_id: Uuid::new_v4(),
                author_ref: context.actor.principal_ref.clone(),
                body: body.into(),
                created_at: self.clock.now(),
            },
        )
        .await
    }

    pub async fn assign_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        assignee_ref: impl Into<String>,
        assignment_scope: erpx_domain::contracts::AuthorityScope,
    ) -> Result<SupportTicket, ApplicationError> {
        let assignment = SupportTicketAssignment::new(
            assignee_ref,
            context.actor.principal_ref.clone(),
            assignment_scope,
            self.clock.now(),
        )?;
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Assign { assignment },
        )
        .await
    }

    pub async fn escalate_ticket(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        level: u8,
        target_ref: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let escalation = SupportTicketEscalation::new(
            level,
            target_ref,
            reason,
            context.actor.principal_ref.clone(),
            self.clock.now(),
        )?;
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::Escalate { escalation },
        )
        .await
    }

    pub async fn set_sla_deadline(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        deadline: DateTime<Utc>,
    ) -> Result<SupportTicket, ApplicationError> {
        self.apply_ticket_command(
            context,
            ticket_id,
            SupportTicketCommand::SetSlaDeadline { deadline },
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_session(
        &self,
        context: &CommandContext,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: &str,
        mode: erpx_domain::support::SupportMode,
        permissions: Vec<String>,
        scope: &str,
        module: &str,
        reason: &str,
        duration: Duration,
        represented_holding_user: Option<String>,
    ) -> Result<SupportSession, ApplicationError> {
        let ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if ticket.customer_account_id != customer_account_id {
            return Err(ApplicationError::Authorization(
                "ticket/customer scope mismatch".into(),
            ));
        }
        let session = SupportSession::request_with_module(
            ticket_id,
            customer_account_id,
            agent_ref,
            mode,
            permissions,
            scope,
            module,
            reason,
            self.clock.now(),
            duration,
            represented_holding_user,
        )?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            1,
            "support_session.requested",
            &session,
            customer_account_id,
            "support_session_requested",
        )
        .await?;
        Ok(session)
    }

    pub async fn approve_session(
        &self,
        context: &CommandContext,
        session_id: Uuid,
        customer_approver: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.approve(customer_approver, self.clock.now())?;
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            aggregate_version,
            "support_session.approved",
            &session,
            session.customer_account_id,
            "support_session_approved",
        )
        .await?;
        Ok(session)
    }

    pub async fn revoke_session(
        &self,
        context: &CommandContext,
        session_id: Uuid,
    ) -> Result<(), ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.revoke(self.clock.now());
        let aggregate_version = next_command_version(context)?;
        execute_support_command(
            &self.commands,
            context,
            "support_session",
            session.id,
            aggregate_version,
            "support_session.revoked",
            &session,
            session.customer_account_id,
            "support_session_revoked",
        )
        .await
        .map(|_| ())
    }
}

fn next_command_version(context: &CommandContext) -> Result<u64, ApplicationError> {
    context
        .expected_aggregate_version
        .and_then(|version| version.checked_add(1))
        .ok_or_else(|| {
            ApplicationError::Conflict(
                "an expected aggregate version is required for an existing support aggregate"
                    .into(),
            )
        })
}

/// Derive a stable aggregate identity from non-secret command identity and
/// ticket lineage. The raw idempotency key is never persisted in the UUID or
/// exposed; only its SHA-256 digest contributes to the UUID bytes. Repeating
/// a create or recurrence command with the same idempotency key and lineage
/// therefore targets the same aggregate even if a transport rebuilds the
/// surrounding command context.
fn stable_support_ticket_id(
    context: &CommandContext,
    kind: &str,
    customer_account_id: Uuid,
    predecessor_id: Option<Uuid>,
) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx.support.ticket.aggregate");
    digest.update([0]);
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(context.idempotency_key.as_bytes());
    digest.update([0]);
    digest.update(customer_account_id.as_bytes());
    if let Some(predecessor_id) = predecessor_id {
        digest.update([0]);
        digest.update(predecessor_id.as_bytes());
    }
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // UUID v4/variant bits keep this deterministic identifier valid for
    // systems that validate UUID version metadata.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[allow(clippy::too_many_arguments)]
async fn execute_support_command<U, T>(
    commands: &Arc<U>,
    context: &CommandContext,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    state: &T,
    customer_account_id: Uuid,
    action: &str,
) -> Result<CommandReceipt, ApplicationError>
where
    U: CommandUnitOfWork + 'static,
    T: serde::Serialize,
{
    let aggregate_state = serde_json::to_value(state)
        .map_err(|error| ApplicationError::Repository(error.to_string()))?;
    let request_material = serde_json::json!({
        // The idempotency key is the retry identity. Command IDs may be
        // regenerated by a transport retry and must not turn a replay into a
        // conflicting request hash.
        "idempotency_key": context.idempotency_key,
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
    let request_hash = hex::encode(digest.finalize());
    let audit = AuditEvent {
        actor_ref: context.actor.principal_ref.clone(),
        customer_account_id: Some(customer_account_id),
        action: action.into(),
        purpose: context.audit.purpose.clone(),
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
            request_hash,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_aggregate_version: context.expected_aggregate_version,
            event,
            event_version: 1,
            active_scope: Some(
                serde_json::to_value(&context.active_scope)
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

impl<S, C> SupportService<S, C>
where
    S: SupportStore + 'static,
    C: Clock + 'static,
{
    pub fn new(store: Arc<S>, clock: Arc<C>, audit: Arc<dyn AuditSink>) -> Self {
        Self {
            store,
            clock,
            audit,
        }
    }

    pub async fn open_ticket(
        &self,
        customer_account_id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<SupportTicket, ApplicationError> {
        let ticket = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::New,
            severity: severity.into(),
            description: description.into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        self.store.create_ticket(ticket.clone()).await?;
        Ok(ticket)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_session(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: &str,
        mode: erpx_domain::support::SupportMode,
        permissions: Vec<String>,
        scope: &str,
        module: &str,
        reason: &str,
        duration: Duration,
        represented_holding_user: Option<String>,
    ) -> Result<SupportSession, ApplicationError> {
        let ticket = self
            .store
            .get_ticket(ticket_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if ticket.customer_account_id != customer_account_id {
            return Err(ApplicationError::Authorization(
                "ticket/customer scope mismatch".into(),
            ));
        }
        let session = SupportSession::request_with_module(
            ticket_id,
            customer_account_id,
            agent_ref,
            mode,
            permissions,
            scope,
            module,
            reason,
            self.clock.now(),
            duration,
            represented_holding_user,
        )?;
        self.store.create_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: agent_ref.into(),
                customer_account_id: Some(customer_account_id),
                action: "support_session_requested".into(),
                purpose: reason.into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(session)
    }

    pub async fn approve_session(
        &self,
        session_id: Uuid,
        customer_approver: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.approve(customer_approver, self.clock.now())?;
        self.store.save_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: customer_approver.into(),
                customer_account_id: Some(session.customer_account_id),
                action: "support_session_approved".into(),
                purpose: "customer_approval".into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(session)
    }

    pub async fn authorize_action(
        &self,
        session_id: Uuid,
        permission: &str,
        scope: &str,
    ) -> Result<SupportSession, ApplicationError> {
        let session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.authorize(self.clock.now(), permission, scope)?;
        Ok(session)
    }

    pub async fn revoke_session(
        &self,
        session_id: Uuid,
        actor_ref: &str,
    ) -> Result<(), ApplicationError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        session.revoke(self.clock.now());
        self.store.save_session(session.clone()).await?;
        self.audit
            .record(AuditEvent {
                actor_ref: actor_ref.into(),
                customer_account_id: Some(session.customer_account_id),
                action: "support_session_revoked".into(),
                purpose: "revocation".into(),
                correlation_id: Uuid::new_v4(),
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod command_replay_tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct ReplayTicketStore {
        tickets: Mutex<HashMap<Uuid, SupportTicket>>,
    }

    #[async_trait]
    impl SupportStore for ReplayTicketStore {
        async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
            self.tickets
                .lock()
                .expect("ticket store lock")
                .insert(ticket.id, ticket);
            Ok(())
        }

        async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError> {
            Ok(self
                .tickets
                .lock()
                .expect("ticket store lock")
                .get(&id)
                .cloned())
        }

        async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
            self.create_ticket(ticket).await
        }

        async fn create_session(&self, _session: SupportSession) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn get_session(&self, _id: Uuid) -> Result<Option<SupportSession>, ApplicationError> {
            Ok(None)
        }

        async fn save_session(&self, _session: SupportSession) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ReplayingSupportCommand {
        aggregate_id: Uuid,
    }

    #[async_trait]
    impl CommandUnitOfWork for ReplayingSupportCommand {
        async fn execute_command(
            &self,
            mutation: CommandMutation,
        ) -> Result<CommandReceipt, ApplicationError> {
            Ok(CommandReceipt {
                command_id: mutation.command_id,
                aggregate_type: mutation.aggregate_type,
                aggregate_id: self.aggregate_id,
                aggregate_version: 1,
                event_id: Uuid::new_v4(),
                response: serde_json::json!({"accepted": true, "aggregate_id": self.aggregate_id}),
                replayed: true,
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FixedSupportClock(DateTime<Utc>);

    impl Clock for FixedSupportClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[tokio::test]
    async fn recurrence_replay_rehydrates_receipt_aggregate_instead_of_returning_new_id() {
        let now = Utc::now();
        let customer_account_id = Uuid::new_v4();
        let holding_id = Uuid::new_v4();
        let closed = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id,
            status: erpx_domain::support::TicketStatus::Closed,
            severity: "sev2".into(),
            description: "original issue".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let context = CommandContext::try_new(
            "support-recurrence-replay",
            ActorAttribution {
                principal_ref: "support-agent".into(),
                kind: ActorKind::PlatformUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Holding { holding_id },
            "reopen recurring support issue",
        )
        .expect("context");
        let recurrence_id =
            stable_support_ticket_id(&context, "recurrence", customer_account_id, Some(closed.id));
        let stored_recurrence = closed
            .recurrence(recurrence_id, "sev1", "rehydrated stored recurrence")
            .expect("recurrence");
        let store = Arc::new(ReplayTicketStore::default());
        store
            .create_ticket(closed.clone())
            .await
            .expect("closed ticket");
        store
            .create_ticket(stored_recurrence.clone())
            .await
            .expect("stored recurrence");
        let commands = Arc::new(ReplayingSupportCommand {
            aggregate_id: recurrence_id,
        });
        let service =
            AuthoritativeSupportService::new(store, Arc::new(FixedSupportClock(now)), commands);

        let returned = service
            .open_ticket_recurrence(
                &context,
                closed.id,
                "sev1",
                "request payload must not win over replayed state",
            )
            .await
            .expect("replayed recurrence");
        assert_eq!(returned.id, recurrence_id);
        assert_eq!(returned.description, "rehydrated stored recurrence");
    }
}
