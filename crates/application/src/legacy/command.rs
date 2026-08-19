use super::super::*;

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError>;
}

/// The application-level command mutation passed to a transactional unit of
/// work. It deliberately contains only stable domain values and JSON context;
/// persistence-specific transaction types remain in infrastructure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMutation {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub request_hash: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub expected_aggregate_version: Option<u64>,
    pub event: OutboxEvent,
    pub event_version: u32,
    pub active_scope: Option<Value>,
    pub actor: Option<Value>,
    pub causation_id: Option<Uuid>,
    pub audit: AuditEvent,
    pub response: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub command_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub response: Value,
    pub replayed: bool,
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn execute_command(
        &self,
        mutation: CommandMutation,
    ) -> Result<CommandReceipt, ApplicationError>;
}

#[derive(Debug, Clone)]
pub struct ApplicationCommandService<U> {
    unit_of_work: Arc<U>,
}

impl<U> ApplicationCommandService<U>
where
    U: CommandUnitOfWork + 'static,
{
    pub fn new(unit_of_work: Arc<U>) -> Self {
        Self { unit_of_work }
    }

    pub async fn execute(
        &self,
        mutation: CommandMutation,
    ) -> Result<CommandReceipt, ApplicationError> {
        self.unit_of_work.execute_command(mutation).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub actor_ref: String,
    pub customer_account_id: Option<Uuid>,
    pub action: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingUnitOfWork {
        received: Mutex<Vec<CommandMutation>>,
    }

    #[async_trait]
    impl CommandUnitOfWork for RecordingUnitOfWork {
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
            self.received.lock().expect("recording lock").push(mutation);
            Ok(receipt)
        }
    }

    #[tokio::test]
    async fn command_service_forwards_the_complete_atomic_mutation() {
        let unit_of_work = Arc::new(RecordingUnitOfWork::default());
        let service = ApplicationCommandService::new(unit_of_work.clone());
        let command_id = Uuid::new_v4();
        let aggregate_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let mutation = CommandMutation {
            command_id,
            idempotency_key: "command-1".into(),
            request_hash: "hash-1".into(),
            aggregate_type: "customer".into(),
            aggregate_id,
            expected_aggregate_version: None,
            event: OutboxEvent {
                id: event_id,
                aggregate_type: "customer".into(),
                aggregate_id,
                aggregate_version: 1,
                event_type: "customer.created".into(),
                payload: serde_json::json!({"status": "active"}),
                occurred_at: Utc::now(),
            },
            event_version: 1,
            active_scope: None,
            actor: None,
            causation_id: None,
            audit: AuditEvent {
                actor_ref: "test".into(),
                customer_account_id: None,
                action: "customer_created".into(),
                purpose: "test".into(),
                correlation_id: command_id,
            },
            response: serde_json::json!({"accepted": true}),
        };
        let receipt = service.execute(mutation).await.expect("command receipt");
        assert_eq!(receipt.command_id, command_id);
        assert_eq!(receipt.event_id, event_id);
        assert_eq!(
            unit_of_work.received.lock().expect("recording lock").len(),
            1
        );
    }
}
