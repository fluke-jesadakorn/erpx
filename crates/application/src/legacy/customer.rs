use super::super::*;

#[async_trait]
pub trait CustomerReadStore: Send + Sync {
    async fn list_customers(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<Customer360>, ApplicationError>;
    async fn get_customer(&self, id: Uuid) -> Result<Option<Customer360>, ApplicationError>;
}

/// Read-only secondary views for the platform Customer 360 projection. The
/// port intentionally returns a typed view envelope so transport adapters can
/// distinguish absent data, stale-but-usable projections, and views that have
/// no corresponding control read model yet. Authorization is outside this
/// read port.
#[async_trait]
pub trait Customer360DetailReadStore: Send + Sync {
    async fn get_customer_organization(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOrganizationReadDto>, ApplicationError>;
    async fn get_customer_contacts(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerContactsReadDto>, ApplicationError>;
    async fn get_customer_commercial(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerCommercialReadDto>, ApplicationError>;
    async fn get_customer_entitlements(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerEntitlementsReadDto>, ApplicationError>;
    async fn get_customer_onboarding(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOnboardingReadDto>, ApplicationError>;
    async fn get_customer_health(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerHealthReadDto>, ApplicationError>;
    async fn get_customer_support_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerSupportSummaryReadDto>, ApplicationError>;
    async fn get_customer_access_audit_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerAccessAuditSummaryReadDto>, ApplicationError>;
    async fn get_customer_notes(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerNotesReadDto>, ApplicationError>;
}

/// Authenticated, append-only Customer 360 note command. The scope is carried
/// into the durable event so the write adapter can enforce customer binding
/// before the aggregate/outbox transaction commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddCustomerNoteCommand {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub note: CustomerNote,
    pub purpose: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerNoteWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub idempotency_key: String,
    pub replayed: bool,
}

impl AddCustomerNoteCommand {
    pub fn new(
        command_id: Uuid,
        idempotency_key: impl Into<String>,
        note: CustomerNote,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, ApplicationError> {
        let idempotency_key = idempotency_key.into();
        let purpose = purpose.into();
        if command_id.is_nil() || correlation_id.is_nil() {
            return Err(ApplicationError::Repository(
                "customer note command IDs are required".into(),
            ));
        }
        if idempotency_key.trim().is_empty() || idempotency_key.len() > 255 {
            return Err(ApplicationError::Repository(
                "customer note idempotency key is invalid".into(),
            ));
        }
        if purpose.trim().is_empty() || purpose.chars().any(char::is_control) {
            return Err(ApplicationError::Repository(
                "customer note purpose is invalid".into(),
            ));
        }
        // Re-run the domain validation at the command boundary so a command
        // cannot be assembled from an unchecked deserialized note.
        CustomerNote::new(
            note.id,
            note.customer_id,
            note.author_ref.clone(),
            note.scope.clone(),
            note.created_at,
            note.body.clone(),
        )
        .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        Ok(Self {
            command_id,
            idempotency_key,
            note,
            purpose,
            correlation_id,
        })
    }

    /// Build the generic command mutation used by the SQLx control-plane
    /// adapter. Aggregate state, event payload, audit attribution, scope, and
    /// idempotency are intentionally derived from the same validated note.
    pub fn into_mutation(&self) -> Result<CommandMutation, ApplicationError> {
        let payload = serde_json::to_value(&self.note)
            .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        let request_material = serde_json::json!({
            "aggregate_type": "customer_note",
            "aggregate_id": self.note.id,
            "note": payload.clone(),
            "author_ref": self.note.author_ref.clone(),
            "scope": &self.note.scope,
            "purpose": self.purpose,
            "correlation_id": self.correlation_id,
        });
        let request_hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&request_material)
                .map_err(|error| ApplicationError::Repository(error.to_string()))?,
        ));
        let event_id = Uuid::new_v4();
        let response = CustomerNoteWriteReceipt {
            aggregate_type: "customer_note".into(),
            aggregate_id: self.note.id,
            aggregate_version: 1,
            event_id,
            idempotency_key: self.idempotency_key.clone(),
            replayed: false,
        };
        Ok(CommandMutation {
            command_id: self.command_id,
            idempotency_key: self.idempotency_key.clone(),
            request_hash,
            aggregate_type: "customer_note".into(),
            aggregate_id: self.note.id,
            expected_aggregate_version: None,
            event: OutboxEvent {
                id: event_id,
                aggregate_type: "customer_note".into(),
                aggregate_id: self.note.id,
                aggregate_version: 1,
                event_type: "customer.note_added".into(),
                payload,
                occurred_at: self.note.created_at,
            },
            event_version: 1,
            active_scope: Some(
                serde_json::to_value(&self.note.scope)
                    .map_err(|error| ApplicationError::Repository(error.to_string()))?,
            ),
            actor: Some(serde_json::json!({
                "principal_ref": self.note.author_ref,
            })),
            causation_id: None,
            audit: AuditEvent {
                actor_ref: self.note.author_ref.clone(),
                customer_account_id: Some(self.note.customer_id),
                action: "customer_note_added".into(),
                purpose: self.purpose.clone(),
                correlation_id: self.correlation_id,
            },
            response: serde_json::to_value(response)
                .map_err(|error| ApplicationError::Repository(error.to_string()))?,
        })
    }
}

#[async_trait]
pub trait Customer360NoteWritePort: Send + Sync {
    async fn append_customer_note(
        &self,
        command: AddCustomerNoteCommand,
    ) -> Result<CustomerNoteWriteReceipt, ApplicationError>;
}

#[derive(Debug, Clone)]
pub struct Customer360NoteService<W> {
    writer: Arc<W>,
}

impl<W> Customer360NoteService<W>
where
    W: Customer360NoteWritePort + 'static,
{
    pub fn new(writer: Arc<W>) -> Self {
        Self { writer }
    }

    pub async fn append(
        &self,
        command: AddCustomerNoteCommand,
    ) -> Result<CustomerNoteWriteReceipt, ApplicationError> {
        self.writer.append_customer_note(command).await
    }
}

pub const CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE: u32 = 100;

/// The customer security administrator's requested customer scope is explicit
/// in the query contract. This port carries scope for the authoritative
/// authorization boundary; the read repository does not make that decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerSecurityAdminScope {
    pub customer_id: Uuid,
    /// `true` is reserved for the platform-console scope. Holding security
    /// administrators always carry an explicit holding/company/branch scope.
    pub platform: bool,
    pub holding_id: Option<Uuid>,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditQueryContext {
    pub requester: GlobalReadActor,
    pub purpose: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailCursor {
    pub occurred_at: DateTime<Utc>,
    pub audit_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailQuery {
    pub customer_id: Uuid,
    pub security_admin_scope: CustomerSecurityAdminScope,
    pub audit: CustomerGlobalReadAuditQueryContext,
    pub before: Option<CustomerGlobalReadAuditDetailCursor>,
    pub limit: u32,
}

impl CustomerGlobalReadAuditDetailQuery {
    pub fn validate(&self) -> Result<(), String> {
        if self.customer_id.is_nil() {
            return Err("customer ID is required".into());
        }
        if self.security_admin_scope.customer_id != self.customer_id {
            return Err("security administrator scope does not match customer".into());
        }
        if self.security_admin_scope.platform
            && (self.security_admin_scope.holding_id.is_some()
                || self.security_admin_scope.company_id.is_some()
                || self.security_admin_scope.branch_id.is_some())
        {
            return Err("platform scope cannot carry organization identifiers".into());
        }
        if !self.security_admin_scope.platform {
            let valid_holding_scope = matches!(
                (
                    self.security_admin_scope.holding_id,
                    self.security_admin_scope.company_id,
                    self.security_admin_scope.branch_id,
                ),
                (Some(_), None, None) | (Some(_), Some(_), None) | (Some(_), Some(_), Some(_))
            );
            if !valid_holding_scope {
                return Err("holding security scope is incomplete".into());
            }
        }
        if self.audit.requester.principal_ref.trim().is_empty() {
            return Err("requester principal is required".into());
        }
        if self.audit.purpose.trim().is_empty() {
            return Err("audit purpose is required".into());
        }
        if self.audit.correlation_id.is_nil() {
            return Err("audit correlation ID is required".into());
        }
        Ok(())
    }

    pub fn bounded_limit(&self) -> u32 {
        self.limit
            .clamp(1, CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerGlobalReadAuditDetailPage {
    pub entries: Vec<CustomerGlobalReadAuditDetailReadDto>,
    pub next_before: Option<CustomerGlobalReadAuditDetailCursor>,
}

/// Read port for customer-security audit detail. Callers should use
/// `CustomerGlobalReadAuditDetailService` so the query itself is recorded in
/// the application audit sink before the source records are returned.
#[async_trait]
pub trait CustomerGlobalReadAuditDetailStore: Send + Sync {
    async fn get_customer_global_read_audit_details(
        &self,
        query: &CustomerGlobalReadAuditDetailQuery,
    ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError>;
}

pub struct CustomerGlobalReadAuditDetailService<R> {
    reader: Arc<R>,
    audit: Arc<dyn AuditSink>,
}

impl<R> CustomerGlobalReadAuditDetailService<R>
where
    R: CustomerGlobalReadAuditDetailStore + 'static,
{
    pub fn new(reader: Arc<R>, audit: Arc<dyn AuditSink>) -> Self {
        Self { reader, audit }
    }

    pub async fn list(
        &self,
        query: CustomerGlobalReadAuditDetailQuery,
    ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError> {
        query.validate().map_err(|error| {
            ApplicationError::Repository(format!("invalid audit query: {error}"))
        })?;
        self.audit
            .record(AuditEvent {
                actor_ref: query.audit.requester.principal_ref.clone(),
                customer_account_id: Some(query.customer_id),
                action: "customer_global_read_audit_detail".into(),
                purpose: query.audit.purpose.clone(),
                correlation_id: query.audit.correlation_id,
            })
            .await?;
        self.reader
            .get_customer_global_read_audit_details(&query)
            .await
    }
}

#[cfg(test)]
mod customer_audit_query_tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingAudit {
        events: Mutex<Vec<AuditEvent>>,
    }

    #[async_trait]
    impl AuditSink for RecordingAudit {
        async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError> {
            self.events.lock().expect("audit lock").push(event);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct EmptyAuditReader;

    #[async_trait]
    impl CustomerGlobalReadAuditDetailStore for EmptyAuditReader {
        async fn get_customer_global_read_audit_details(
            &self,
            query: &CustomerGlobalReadAuditDetailQuery,
        ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError> {
            Ok(CustomerReadView::absent(query.customer_id))
        }
    }

    fn query() -> CustomerGlobalReadAuditDetailQuery {
        CustomerGlobalReadAuditDetailQuery {
            customer_id: Uuid::from_u128(0x7001),
            security_admin_scope: CustomerSecurityAdminScope {
                customer_id: Uuid::from_u128(0x7001),
                platform: false,
                holding_id: Some(Uuid::from_u128(0x7101)),
                company_id: None,
                branch_id: None,
            },
            audit: CustomerGlobalReadAuditQueryContext {
                requester: GlobalReadActor {
                    principal_ref: "security-admin".into(),
                    session_ref: Some("security-session".into()),
                },
                purpose: "customer access review".into(),
                correlation_id: Uuid::from_u128(0x7201),
            },
            before: None,
            limit: 1_000,
        }
    }

    #[test]
    fn detail_query_requires_matching_customer_scope_and_bounds_page_size() {
        let query = query();
        assert!(query.validate().is_ok());
        assert_eq!(
            query.bounded_limit(),
            CUSTOMER_GLOBAL_READ_AUDIT_MAX_PAGE_SIZE
        );
        let mut mismatched = query;
        mismatched.security_admin_scope.customer_id = Uuid::from_u128(0x7002);
        assert!(mismatched.validate().is_err());
    }

    #[tokio::test]
    async fn detail_service_records_the_query_before_reading() {
        let audit = Arc::new(RecordingAudit::default());
        let service =
            CustomerGlobalReadAuditDetailService::new(Arc::new(EmptyAuditReader), audit.clone());
        let result = service.list(query()).await.expect("audited detail query");
        assert!(matches!(result, CustomerReadView::Absent { .. }));
        let events = audit.events.lock().expect("audit lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "customer_global_read_audit_detail");
        assert_eq!(events[0].customer_account_id, Some(Uuid::from_u128(0x7001)));
    }
}
