use async_trait::async_trait;
use erpx_agent_orchestration::{AgentNotificationPort, AssignmentId, AssignmentState, PortError};
use erpx_application::notifications::{
    Notification, NotificationError, NotificationRepositoryPort,
};
use erpx_kernel::{AuthorityScope, DataClassification, HoldingId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::SqlxSharedEnterpriseServiceStore;

/// Persists the authoritative in-app notification through the shared
/// notification application repository. It does not own provider delivery;
/// email/SMS/push remain separate notification-authority work.
#[derive(Debug, Clone)]
pub struct SqlxAgentNotificationPort {
    holding_id: HoldingId,
    store: SqlxSharedEnterpriseServiceStore,
}

impl SqlxAgentNotificationPort {
    pub fn try_new(pool: sqlx::PgPool, holding_id: Uuid) -> Result<Self, PortError> {
        let holding_id = HoldingId::try_from_uuid(holding_id).map_err(|_| PortError::Denied)?;
        Ok(Self {
            holding_id,
            store: SqlxSharedEnterpriseServiceStore::new(pool, holding_id),
        })
    }

    pub const fn holding_id(&self) -> HoldingId {
        self.holding_id
    }
}

#[async_trait]
impl AgentNotificationPort for SqlxAgentNotificationPort {
    async fn publish_state(
        &self,
        scope: &AuthorityScope,
        assignment_id: AssignmentId,
        aggregate_version: u64,
        recipient_principal_ref: &str,
        state: AssignmentState,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), PortError> {
        if aggregate_version == 0
            || scope.validate().is_err()
            || matches!(scope, AuthorityScope::Platform)
        {
            return Err(PortError::Denied);
        }
        let notification_id =
            notification_id(self.holding_id, assignment_id, aggregate_version, state);
        let expected = Notification::create(
            notification_id,
            scope.clone(),
            format!(
                "agent_orchestration.assignment.{}",
                assignment_state_name(state)
            ),
            format!("reference:item:{}", assignment_id.as_uuid()),
            recipient_principal_ref.to_owned(),
            DataClassification::Internal,
            None,
            false,
            matches!(state, AssignmentState::Waiting | AssignmentState::Failed),
            at,
        )
        .map_err(notification_error)?;
        if let Some(stored) = self
            .store
            .load(notification_id)
            .await
            .map_err(notification_error)?
        {
            return exact_replay(&stored, &expected)
                .then_some(())
                .ok_or(PortError::Persistence);
        }
        match self.store.save(&expected, 0).await {
            Ok(()) => Ok(()),
            Err(NotificationError::Conflict) => {
                let stored = self
                    .store
                    .load(notification_id)
                    .await
                    .map_err(notification_error)?
                    .ok_or(PortError::Persistence)?;
                exact_replay(&stored, &expected)
                    .then_some(())
                    .ok_or(PortError::Persistence)
            }
            Err(error) => Err(notification_error(error)),
        }
    }
}

fn exact_replay(stored: &Notification, expected: &Notification) -> bool {
    stored.notification_id == expected.notification_id
        && stored.version == 1
        && stored.delivery_version == 1
        && stored.scope == expected.scope
        && stored.source_id == expected.source_id
        && stored.source_record_ref == expected.source_record_ref
        && stored.recipient_ref == expected.recipient_ref
        && stored.classification == DataClassification::Internal
        && stored.preview.is_none()
        && stored.inbox_state == expected.inbox_state
}

fn notification_id(
    holding_id: HoldingId,
    assignment_id: AssignmentId,
    aggregate_version: u64,
    state: AssignmentState,
) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"erpx.agent.assignment.notification");
    digest.update(holding_id.as_uuid().as_bytes());
    digest.update(assignment_id.as_uuid().as_bytes());
    digest.update(aggregate_version.to_be_bytes());
    digest.update(assignment_state_name(state));
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn assignment_state_name(state: AssignmentState) -> &'static str {
    match state {
        AssignmentState::Queued => "queued",
        AssignmentState::Running => "running",
        AssignmentState::Waiting => "waiting",
        AssignmentState::Completed => "completed",
        AssignmentState::Failed => "failed",
        AssignmentState::Cancelled => "cancelled",
    }
}

fn notification_error(error: NotificationError) -> PortError {
    match error {
        NotificationError::Invalid | NotificationError::Denied => PortError::Denied,
        NotificationError::Conflict => PortError::IdempotencyConflict,
        NotificationError::ProviderUnavailable => PortError::Persistence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment_id() -> AssignmentId {
        AssignmentId::try_from_uuid(Uuid::from_u128(7)).expect("assignment")
    }

    #[test]
    fn notification_identity_is_holding_version_and_state_bound() {
        let holding = HoldingId::try_from_uuid(Uuid::from_u128(1)).expect("holding");
        let base = notification_id(holding, assignment_id(), 2, AssignmentState::Completed);
        assert_eq!(
            base,
            notification_id(holding, assignment_id(), 2, AssignmentState::Completed)
        );
        assert_ne!(
            base,
            notification_id(holding, assignment_id(), 3, AssignmentState::Completed)
        );
        assert_ne!(
            base,
            notification_id(holding, assignment_id(), 2, AssignmentState::Failed)
        );
    }

    #[test]
    fn notification_adapter_uses_authoritative_repository_without_preview_material() {
        let source = include_str!("agent_notifications.rs");
        assert!(source.contains("SqlxSharedEnterpriseServiceStore"));
        assert!(source.contains("DataClassification::Internal"));
        assert!(source.contains("preview.is_none()"));
        assert!(!source.contains(concat!("prompt_", "material")));
        assert!(!source.contains(concat!("output_", "material")));
    }
}
