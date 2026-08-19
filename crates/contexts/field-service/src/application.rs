use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::{
    domain::{
        BillingHandoffId, FieldServiceDomainError, ServiceAgreement, ServiceOrder, ServiceOrderId,
        ServiceRequest,
    },
    ports::{
        ActorContext, AuthorizationPort, BillingHandoffPort, CommitReceipt, FieldServiceAction,
        FieldServiceAggregate, FieldServiceCommit, FieldServiceReadRepository,
        FieldServiceWriteRepository, MutationIdentity, PortError,
    },
};

pub struct FieldServiceCommandService<A, W, B> {
    authorization: A,
    write: W,
    billing: B,
}
impl<A, W, B> FieldServiceCommandService<A, W, B>
where
    A: AuthorizationPort,
    W: FieldServiceWriteRepository,
    B: BillingHandoffPort,
{
    pub const fn new(authorization: A, write: W, billing: B) -> Self {
        Self {
            authorization,
            write,
            billing,
        }
    }

    pub async fn activate_agreement(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        agreement: ServiceAgreement,
    ) -> Result<CommitReceipt, FieldServiceError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(
                actor,
                agreement.scope(),
                FieldServiceAction::ManageAgreement,
            )
            .await?;
        if self
            .write
            .load_agreement(&authority, agreement.id())
            .await?
            .is_some()
        {
            return Err(FieldServiceError::Conflict);
        }
        self.write
            .commit(FieldServiceCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "field_service.agreement_activated".into(),
                event_payload: serde_json::json!({"version": agreement.version()}),
                aggregate: FieldServiceAggregate::Agreement(Box::new(agreement)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ServiceOrderId,
        request: ServiceRequest,
        agreement_id: crate::ServiceAgreementId,
        agreement_version: u64,
        required_skills: std::collections::BTreeSet<String>,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, FieldServiceError> {
        identity.validate()?;
        request.validate()?;
        let authority = self
            .authorization
            .authorize(actor, &request.scope, FieldServiceAction::CreateOrder)
            .await?;
        if self.write.load_order(&authority, order_id).await?.is_some() {
            return Err(FieldServiceError::Conflict);
        }
        let agreement = self
            .write
            .load_agreement(&authority, agreement_id)
            .await?
            .ok_or(FieldServiceError::Neutral)?;
        if agreement.version() != agreement_version || agreement.scope() != &request.scope {
            return Err(FieldServiceError::Neutral);
        }
        let order = ServiceOrder::create(order_id, &request, &agreement, required_skills, at)?;
        self.write
            .commit(FieldServiceCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "field_service.order_created".into(),
                event_payload: serde_json::json!({"request_id": request.id.as_uuid(), "request_version": request.version}),
                aggregate: FieldServiceAggregate::Order(Box::new(order)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_order<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ServiceOrderId,
        scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        action: FieldServiceAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, FieldServiceError>
    where
        F: FnOnce(
            &mut ServiceOrder,
            Option<&ServiceAgreement>,
        ) -> Result<(), FieldServiceDomainError>,
    {
        identity.validate()?;
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut order = self
            .write
            .load_order(&authority, order_id)
            .await?
            .ok_or(FieldServiceError::Neutral)?;
        if order.scope() != scope || requested_version != Some(order.version()) {
            return Err(FieldServiceError::Conflict);
        }
        let agreement = self
            .write
            .load_agreement(&authority, order.agreement_id())
            .await?;
        let expected = order.version();
        change(&mut order, agreement.as_ref())?;
        self.write
            .commit(FieldServiceCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": order.version()}),
                aggregate: FieldServiceAggregate::Order(Box::new(order)),
            })
            .await
            .map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn release_billing(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ServiceOrderId,
        scope: &erpx_kernel::AuthorityScope,
        handoff_id: BillingHandoffId,
        amount: erpx_integration_contracts::MoneyAmount,
        requested_version: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, FieldServiceError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, FieldServiceAction::ReleaseBilling)
            .await?;
        let mut order = self
            .write
            .load_order(&authority, order_id)
            .await?
            .ok_or(FieldServiceError::Neutral)?;
        if order.scope() != scope || requested_version != Some(order.version()) {
            return Err(FieldServiceError::Conflict);
        }
        let expected = order.version();
        let handoff = order.billing_handoff(handoff_id, amount, identity.correlation_id)?;
        let response = self.billing.release(&authority, &handoff).await?;
        if !response.accepted {
            return Err(FieldServiceError::HandoffRejected(
                response
                    .rejection_reason
                    .unwrap_or_else(|| "Billing owner rejected handoff".into()),
            ));
        }
        order.mark_billing_handed_off(handoff_id, at)?;
        self.write.commit(FieldServiceCommit { identity, authority, expected_version: Some(expected), event_type: "field_service.billing_handoff_accepted".into(), event_payload: serde_json::json!({"handoff_id": handoff_id.as_uuid(), "owner_correlation_id": response.owner_correlation_id}), aggregate: FieldServiceAggregate::Order(Box::new(order)) }).await.map_err(Into::into)
    }

    pub async fn govern_record(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        record: crate::FieldServiceRecord,
        expected_version: Option<u64>,
        event_type: &str,
    ) -> Result<CommitReceipt, FieldServiceError> {
        identity.validate()?;
        record.validate()?;
        let action = match record.kind() {
            crate::FieldServiceRecordKind::Request => FieldServiceAction::CreateRequest,
            crate::FieldServiceRecordKind::Dispatch => FieldServiceAction::Dispatch,
            crate::FieldServiceRecordKind::Visit => FieldServiceAction::ExecuteVisit,
            crate::FieldServiceRecordKind::Handoff => FieldServiceAction::ReleaseBilling,
        };
        let authority = self
            .authorization
            .authorize(actor, record.scope(), action)
            .await?;
        let current = self.write.load_record(&authority, record.id()).await?;
        match (expected_version, current.as_ref()) {
            (None, None) if record.version() == 1 => {}
            (Some(expected), Some(current))
                if current.version() == expected
                    && current.kind() == record.kind()
                    && record.version()
                        == expected.checked_add(1).ok_or(FieldServiceError::Conflict)? => {}
            _ => return Err(FieldServiceError::Conflict),
        }
        self.write
            .commit(FieldServiceCommit {
                identity,
                authority,
                expected_version,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: FieldServiceAggregate::Record(Box::new(record)),
            })
            .await
            .map_err(Into::into)
    }
}
pub struct FieldServiceQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> FieldServiceQueryService<A, R>
where
    A: AuthorizationPort,
    R: FieldServiceReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }
    pub async fn get_order(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: ServiceOrderId,
    ) -> Result<crate::ports::ServiceOrderProjection, FieldServiceError> {
        let authority = self
            .authorization
            .authorize(actor, scope, FieldServiceAction::View)
            .await?;
        self.read
            .get_order(&authority, id)
            .await?
            .ok_or(FieldServiceError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::FieldServiceRecordProjection, FieldServiceError> {
        let authority = self
            .authorization
            .authorize(actor, scope, FieldServiceAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(FieldServiceError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FieldServiceError {
    #[error("Field Service item unavailable")]
    Neutral,
    #[error("Field Service conflict")]
    Conflict,
    #[error("invalid Field Service request: {0}")]
    Invalid(String),
    #[error("billing handoff rejected: {0}")]
    HandoffRejected(String),
    #[error("Field Service dependency unavailable")]
    Unavailable,
    #[error("Field Service operation failed")]
    Internal,
}
impl From<FieldServiceDomainError> for FieldServiceError {
    fn from(value: FieldServiceDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for FieldServiceError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied | PortError::NotFound => Self::Neutral,
            PortError::Conflict => Self::Conflict,
            PortError::Stale | PortError::Unavailable => Self::Unavailable,
            PortError::InvalidState => Self::Invalid("adapter rejected invalid state".into()),
            PortError::Internal => Self::Internal,
        }
    }
}
