//! Application boundary for the durable G4 support workflow.
//!
//! The application layer validates the complete immutable grant before an
//! infrastructure adapter is allowed to persist it.  It carries no SQLx
//! types, bearer material, or transport DTOs.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_domain::{
    contracts::AuthorityScope as Scope,
    support::{
        SupportCapability, SupportDeviceSessionBinding, SupportError, SupportMode, SupportSession,
    },
};
use uuid::Uuid;

use crate::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportWorkflowReceipt {
    pub session_id: Uuid,
    pub capability_id: Uuid,
    pub session_version: u64,
    pub capability_version: u64,
    pub event_ids: Vec<Uuid>,
    pub replayed: bool,
}

/// Complete non-secret input for one approved support grant.  The raw
/// capability bearer is intentionally not part of this command; the domain
/// capability already contains only its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistApprovedSupportGrantCommand {
    pub session: SupportSession,
    pub device: SupportDeviceSessionBinding,
    pub capability: SupportCapability,
    pub actor_scope: Scope,
    pub requested_scope: Scope,
    pub sod_passed: bool,
    pub approval_ref: String,
    pub at: DateTime<Utc>,
}

impl PersistApprovedSupportGrantCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: SupportSession,
        device: SupportDeviceSessionBinding,
        capability: SupportCapability,
        actor_scope: Scope,
        requested_scope: Scope,
        sod_passed: bool,
        approval_ref: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, ApplicationError> {
        let approval_ref = approval_ref.into();
        if approval_ref.trim().is_empty()
            || approval_ref.len() > 128
            || approval_ref.chars().any(char::is_control)
        {
            return Err(ApplicationError::Repository(
                "support approval reference is invalid".into(),
            ));
        }
        session.authorize_device(&device, &actor_scope, &requested_scope, sod_passed, at)?;
        if session.approved_at.is_none() {
            return Err(SupportError::NotApproved.into());
        }
        if capability.issued_at < session.approved_at.expect("checked above") {
            return Err(SupportError::InvalidCapabilityBinding.into());
        }
        if capability.binding.session_id != session.id
            || capability.binding.ticket_id != session.ticket_id
            || capability.binding.customer_account_id != session.customer_account_id
            || capability.binding.agent_ref != session.agent_ref
            || capability.binding.device_ref != device.device_ref
            || capability.binding.mode != session.mode
            || capability.binding.scope != requested_scope.canonical_key()
            || capability.binding.module != session.module
            || capability.expires_at > session.expires_at
            || !capability.is_active(at)
        {
            return Err(SupportError::InvalidCapabilityBinding.into());
        }
        if !session
            .permissions
            .iter()
            .all(|permission| capability.binding.permissions.contains(permission))
        {
            return Err(SupportError::PermissionNotGranted.into());
        }
        Ok(Self {
            session,
            device,
            capability,
            actor_scope,
            requested_scope,
            sod_passed,
            approval_ref,
            at,
        })
    }

    pub fn is_high_risk(&self) -> bool {
        self.session.mode.requires_high_risk_mfa()
    }

    pub fn idempotency_identity(&self) -> (Uuid, Uuid, String) {
        (
            self.session.id,
            self.capability.id,
            self.approval_ref.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeSupportGrantCommand {
    pub session_id: Uuid,
    pub actor_ref: String,
    pub reason: String,
    pub at: DateTime<Utc>,
}

impl RevokeSupportGrantCommand {
    pub fn new(
        session_id: Uuid,
        actor_ref: impl Into<String>,
        reason: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, ApplicationError> {
        let actor_ref = actor_ref.into();
        let reason = reason.into();
        if session_id.is_nil()
            || actor_ref.trim().is_empty()
            || reason.trim().is_empty()
            || actor_ref.len() > 255
            || reason.len() > 256
            || actor_ref.chars().any(char::is_control)
            || reason.chars().any(char::is_control)
        {
            return Err(ApplicationError::Repository(
                "support revocation command is invalid".into(),
            ));
        }
        Ok(Self {
            session_id,
            actor_ref,
            reason,
            at,
        })
    }
}

#[async_trait]
pub trait SupportWorkflowPort: Send + Sync {
    async fn persist_approved_grant(
        &self,
        command: PersistApprovedSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, ApplicationError>;

    async fn revoke_grant(
        &self,
        command: RevokeSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, ApplicationError>;
}

/// Thin application service used by server/application adapters.  All
/// durable state changes, outbox events, and read-model work remain in the
/// infrastructure implementation of [`SupportWorkflowPort`].
#[derive(Debug, Clone)]
pub struct SupportWorkflowService<P> {
    port: Arc<P>,
}

impl<P> SupportWorkflowService<P>
where
    P: SupportWorkflowPort + 'static,
{
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }

    pub async fn persist_approved_grant(
        &self,
        command: PersistApprovedSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, ApplicationError> {
        self.port.persist_approved_grant(command).await
    }

    pub async fn revoke_grant(
        &self,
        command: RevokeSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, ApplicationError> {
        self.port.revoke_grant(command).await
    }
}

pub const SUPPORT_WORKFLOW_HIGH_RISK_MODES: &[SupportMode] =
    &[SupportMode::HoldingMutation, SupportMode::Impersonation];

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use erpx_domain::contracts::AuthorityScope as Scope;

    use super::*;

    fn command_parts(
        at: DateTime<Utc>,
        scope: Scope,
        mfa_verified_at: Option<DateTime<Utc>>,
    ) -> (
        SupportSession,
        SupportDeviceSessionBinding,
        SupportCapability,
    ) {
        let ticket_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();
        let mut session = SupportSession::request_with_module(
            ticket_id,
            customer_id,
            "support-agent-1",
            SupportMode::HoldingMutation,
            vec!["ticket.read".into()],
            scope.canonical_key(),
            "support",
            "approved diagnostic test",
            at - Duration::minutes(2),
            Duration::minutes(30),
            None,
        )
        .expect("session");
        session
            .approve("customer-approver-1", at - Duration::minutes(1))
            .expect("approval");
        let bound_at = mfa_verified_at.unwrap_or(at - Duration::minutes(1));
        let device = SupportDeviceSessionBinding::new(
            session.id,
            "support-agent-1",
            "device-1",
            mfa_verified_at,
            bound_at,
        )
        .expect("device binding");
        let binding = erpx_domain::support::SupportCapabilityBinding::new(
            customer_id,
            "support-agent-1",
            "device-1",
            session.id,
            ticket_id,
            SupportMode::HoldingMutation,
            vec!["ticket.read".into()],
            scope.canonical_key(),
            "support",
        )
        .expect("capability binding");
        let (capability, _) =
            SupportCapability::issue(binding, at - Duration::seconds(30), Duration::minutes(10))
                .expect("capability");
        (session, device, capability)
    }

    #[test]
    fn high_risk_support_fails_closed_for_stale_mfa() {
        let at = Utc::now();
        let scope = Scope::Holding {
            holding_id: Uuid::new_v4(),
        };
        let (session, device, capability) =
            command_parts(at, scope.clone(), Some(at - Duration::minutes(6)));
        let result = PersistApprovedSupportGrantCommand::new(
            session,
            device,
            capability,
            scope.clone(),
            scope,
            true,
            "approval-ref-stale-mfa",
            at,
        );
        assert!(matches!(
            result,
            Err(ApplicationError::Support(SupportError::HighRiskMfaExpired))
        ));
    }

    #[test]
    fn high_risk_support_rejects_scope_escape_and_failed_sod() {
        let at = Utc::now();
        let holding = Uuid::new_v4();
        let requested = Scope::Holding {
            holding_id: holding,
        };
        let actor = Scope::Company {
            holding_id: holding,
            company_id: Uuid::new_v4(),
        };
        let (session, device, capability) =
            command_parts(at, requested.clone(), Some(at - Duration::minutes(1)));
        let scope_escape = PersistApprovedSupportGrantCommand::new(
            session,
            device,
            capability,
            actor,
            requested.clone(),
            true,
            "approval-ref-escape",
            at,
        );
        assert!(matches!(
            scope_escape,
            Err(ApplicationError::Support(SupportError::SupportScopeDenied))
        ));

        let (session, device, capability) =
            command_parts(at, requested.clone(), Some(at - Duration::minutes(1)));
        let denied = PersistApprovedSupportGrantCommand::new(
            session,
            device,
            capability,
            Scope::Holding {
                holding_id: holding,
            },
            requested,
            false,
            "approval-ref-sod",
            at,
        );
        assert!(matches!(
            denied,
            Err(ApplicationError::Support(
                SupportError::SupportSegregationOfDutiesDenied
            ))
        ));
    }
}
