use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use erpx_application::{
    ApplicationError, AuditEvent, AuditSink, CustomerReadStore, RoleAssignmentStore, SupportStore,
};
use erpx_domain::{
    access::RoleAssignment,
    cqrs::{OutboxEvent, ProjectionCheckpoint},
    customer::Customer360,
    support::{SupportSession, SupportTicket},
};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct InMemoryControlPlane {
    customers: RwLock<HashMap<Uuid, Customer360>>,
    tickets: RwLock<HashMap<Uuid, SupportTicket>>,
    sessions: RwLock<HashMap<Uuid, SupportSession>>,
    assignments: RwLock<HashMap<Uuid, RoleAssignment>>,
    audit: RwLock<Vec<AuditEvent>>,
}

impl InMemoryControlPlane {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn seed_customer(&self, customer: Customer360) {
        self.customers
            .write()
            .await
            .insert(customer.account.id, customer);
    }

    pub async fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.read().await.clone()
    }
}

#[async_trait]
impl CustomerReadStore for InMemoryControlPlane {
    async fn list_customers(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<Customer360>, ApplicationError> {
        let query = search.unwrap_or("").to_lowercase();
        Ok(self
            .customers
            .read()
            .await
            .values()
            .filter(|customer| {
                query.is_empty()
                    || customer
                        .account
                        .trading_name
                        .to_lowercase()
                        .contains(&query)
                    || customer
                        .account
                        .account_code
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect())
    }

    async fn get_customer(&self, id: Uuid) -> Result<Option<Customer360>, ApplicationError> {
        Ok(self.customers.read().await.get(&id).cloned())
    }
}

#[async_trait]
impl SupportStore for InMemoryControlPlane {
    async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
        self.tickets.write().await.insert(ticket.id, ticket);
        Ok(())
    }

    async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError> {
        Ok(self.tickets.read().await.get(&id).cloned())
    }

    async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
        self.tickets.write().await.insert(ticket.id, ticket);
        Ok(())
    }

    async fn create_session(&self, session: SupportSession) -> Result<(), ApplicationError> {
        self.sessions.write().await.insert(session.id, session);
        Ok(())
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<SupportSession>, ApplicationError> {
        Ok(self.sessions.read().await.get(&id).cloned())
    }

    async fn save_session(&self, session: SupportSession) -> Result<(), ApplicationError> {
        self.sessions.write().await.insert(session.id, session);
        Ok(())
    }
}

#[async_trait]
impl RoleAssignmentStore for InMemoryControlPlane {
    async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError> {
        self.assignments
            .write()
            .await
            .insert(assignment.id, assignment);
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError> {
        Ok(self.assignments.read().await.get(&id).cloned())
    }

    async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError> {
        Ok(self
            .assignments
            .read()
            .await
            .values()
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl AuditSink for InMemoryControlPlane {
    async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError> {
        self.audit.write().await.push(event);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ProjectionRunner {
    checkpoints: RwLock<HashMap<String, ProjectionCheckpoint>>,
}

impl ProjectionRunner {
    pub async fn should_apply(&self, projection: &str, event: &OutboxEvent) -> bool {
        self.checkpoints
            .read()
            .await
            .get(projection)
            .is_none_or(|checkpoint| checkpoint.accepts(event))
    }

    pub async fn checkpoint(&self, projection: &str, event: &OutboxEvent) {
        let mut checkpoints = self.checkpoints.write().await;
        let checkpoint = checkpoints
            .entry(projection.to_string())
            .or_insert_with(|| ProjectionCheckpoint {
                projection_name: projection.to_string(),
                last_event_id: None,
                last_aggregate_version: 0,
                updated_at: Utc::now(),
            });
        if checkpoint.accepts(event) {
            checkpoint.advance(event, Utc::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::{
        commercial::BillingState,
        customer::{CustomerAccount, CustomerHealth, CustomerStatus, HealthState},
    };

    #[tokio::test]
    async fn in_memory_adapter_is_available_only_to_unit_tests() {
        let store = InMemoryControlPlane::new();
        let id = Uuid::new_v4();
        store
            .seed_customer(Customer360 {
                account: CustomerAccount {
                    id,
                    account_code: "ACME".into(),
                    trading_name: "Acme Holding".into(),
                    legal_name: None,
                    industry: None,
                    country: "TH".into(),
                    status: CustomerStatus::Active,
                    billing_state: BillingState::Active,
                    renewal_at: None,
                    lifetime_value_minor: 0,
                },
                health: CustomerHealth {
                    customer_account_id: id,
                    state: HealthState::Healthy,
                    projection_lag_seconds: 0,
                    worker_failures: 0,
                    backup_status: "ok".into(),
                    last_activity_at: None,
                },
                holding_count: 1,
                company_count: 1,
                branch_count: 1,
                active_user_count: 1,
                expiring_role_count: 0,
                open_ticket_count: 0,
                module_count: 1,
            })
            .await;
        assert_eq!(store.list_customers(Some("acme")).await.unwrap().len(), 1);
    }
}
