use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, HumanDecisionProof};
use thiserror::Error;

use crate::{
    domain::{
        ManufacturingDomainError, ManufacturingSpecification, ProductionOrder, ProductionOrderId,
        SpecificationId, production_order_close_decision_ref,
        production_order_release_decision_ref, specification_publish_decision_ref,
    },
    ports::{
        ActorContext, AuthorizationPort, CommitReceipt, ManufacturingAction,
        ManufacturingAggregate, ManufacturingCommit, ManufacturingReadRepository,
        ManufacturingWriteRepository, MutationIdentity, PortError,
    },
    record::ManufacturingRecord,
};

pub struct ManufacturingCommandService<A, W> {
    authorization: A,
    write: W,
}

impl<A, W> ManufacturingCommandService<A, W>
where
    A: AuthorizationPort,
    W: ManufacturingWriteRepository,
{
    pub const fn new(authorization: A, write: W) -> Self {
        Self {
            authorization,
            write,
        }
    }

    pub async fn publish_specification(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        mut specification: ManufacturingSpecification,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        require_authenticated_human(actor)?;
        identity.validate()?;
        let human_decision = bind_human_decision(
            actor,
            specification.scope().clone(),
            specification_publish_decision_ref(
                specification.reference().specification_id,
                specification.reference().version,
                identity.command_id,
            ),
            at,
        )?;
        let authority = self
            .authorization
            .authorize(
                actor,
                specification.scope(),
                ManufacturingAction::PublishSpecification,
            )
            .await?;
        let existing = self
            .write
            .load_specification(&authority, specification.reference().specification_id)
            .await?;
        if existing.is_some() {
            return Err(ManufacturingError::Conflict);
        }
        specification.release(identity.command_id, &human_decision)?;
        let version = specification.reference().version;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: Some(human_decision),
                expected_version: None,
                event_type: "manufacturing.specification_released".into(),
                event_payload: serde_json::json!({"version": version}),
                aggregate: ManufacturingAggregate::Specification(specification),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &erpx_kernel::AuthorityScope,
        specification_id: SpecificationId,
        specification_version: u64,
        planned_output: erpx_integration_contracts::Quantity,
        correlation_id: erpx_kernel::CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, ManufacturingAction::CreateOrder)
            .await?;
        if self.write.load_order(&authority, order_id).await?.is_some() {
            return Err(ManufacturingError::Conflict);
        }
        let specification = self
            .write
            .load_specification(&authority, specification_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if specification.scope() != target_scope
            || specification.reference().version != specification_version
        {
            return Err(ManufacturingError::Neutral);
        }
        let order = ProductionOrder::draft(
            order_id,
            target_scope.clone(),
            &specification,
            planned_output,
            correlation_id,
            at,
        )?;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: None,
                expected_version: None,
                event_type: "manufacturing.order_created".into(),
                event_payload: serde_json::json!({
                    "specification_id": specification_id.as_uuid(),
                    "specification_version": specification_version,
                }),
                aggregate: ManufacturingAggregate::Order(order),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_order<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        action: ManufacturingAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, ManufacturingError>
    where
        F: FnOnce(&mut ProductionOrder) -> Result<(), ManufacturingDomainError>,
    {
        if matches!(
            action,
            ManufacturingAction::ReleaseOrder | ManufacturingAction::CloseOrder
        ) {
            return Err(ManufacturingError::HumanDecisionRequired);
        }
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, action)
            .await?;
        let mut order = self
            .write
            .load_order(&authority, order_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if order.scope() != target_scope {
            return Err(ManufacturingError::Neutral);
        }
        let expected = order.version();
        if requested_version != Some(expected) {
            return Err(ManufacturingError::Conflict);
        }
        change(&mut order)?;
        let correlation_id = order.correlation_id();
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: None,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": order.version()}),
                aggregate: ManufacturingAggregate::Order(order),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn release_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &AuthorityScope,
        requested_version: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        self.decide_order(
            actor,
            identity,
            order_id,
            target_scope,
            requested_version,
            at,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn close_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &AuthorityScope,
        requested_version: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, ManufacturingError> {
        self.decide_order(
            actor,
            identity,
            order_id,
            target_scope,
            requested_version,
            at,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn decide_order(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        order_id: ProductionOrderId,
        target_scope: &AuthorityScope,
        requested_version: Option<u64>,
        at: DateTime<Utc>,
        close: bool,
    ) -> Result<CommitReceipt, ManufacturingError> {
        require_authenticated_human(actor)?;
        identity.validate()?;
        let expected = requested_version.ok_or(ManufacturingError::Conflict)?;
        let decision_ref = if close {
            production_order_close_decision_ref(order_id, expected, identity.command_id)
        } else {
            production_order_release_decision_ref(order_id, expected, identity.command_id)
        };
        let human_decision = bind_human_decision(actor, target_scope.clone(), decision_ref, at)?;
        let action = if close {
            ManufacturingAction::CloseOrder
        } else {
            ManufacturingAction::ReleaseOrder
        };
        let authority = self
            .authorization
            .authorize(actor, target_scope, action)
            .await?;
        let mut order = self
            .write
            .load_order(&authority, order_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if order.scope() != target_scope {
            return Err(ManufacturingError::Neutral);
        }
        if order.version() != expected {
            return Err(ManufacturingError::Conflict);
        }
        if close {
            order.close(identity.command_id, &human_decision)?;
        } else {
            order.release(identity.command_id, &human_decision)?;
        }
        let correlation_id = order.correlation_id();
        let event_type = if close {
            "manufacturing.order_closed"
        } else {
            "manufacturing.order_released"
        };
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: Some(human_decision),
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": order.version()}),
                aggregate: ManufacturingAggregate::Order(order),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn govern_record<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        record_id: uuid::Uuid,
        target_scope: &erpx_kernel::AuthorityScope,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, ManufacturingError>
    where
        F: FnOnce(&mut ManufacturingRecord) -> Result<(), ManufacturingDomainError>,
    {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, target_scope, ManufacturingAction::GovernRecord)
            .await?;
        let mut record = self
            .write
            .load_record(&authority, record_id)
            .await?
            .ok_or(ManufacturingError::Neutral)?;
        if record.scope() != target_scope {
            return Err(ManufacturingError::Neutral);
        }
        let expected = record.version();
        change(&mut record)?;
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: None,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: ManufacturingAggregate::Record(record),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn create_record(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: erpx_kernel::CorrelationId,
        record: ManufacturingRecord,
        event_type: &str,
    ) -> Result<CommitReceipt, ManufacturingError> {
        identity.validate()?;
        record.validate()?;
        if record.version() != 1 {
            return Err(ManufacturingError::Conflict);
        }
        let authority = self
            .authorization
            .authorize(actor, record.scope(), ManufacturingAction::GovernRecord)
            .await?;
        if self
            .write
            .load_record(&authority, record.id())
            .await?
            .is_some()
        {
            return Err(ManufacturingError::Conflict);
        }
        self.write
            .commit(ManufacturingCommit {
                identity,
                correlation_id,
                authority,
                human_decision: None,
                expected_version: None,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: ManufacturingAggregate::Record(record),
            })
            .await
            .map_err(Into::into)
    }
}

pub struct ManufacturingQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> ManufacturingQueryService<A, R>
where
    A: AuthorizationPort,
    R: ManufacturingReadRepository,
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
        id: ProductionOrderId,
    ) -> Result<crate::ports::ManufacturingOrderProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_order(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }

    pub async fn get_specification(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: SpecificationId,
    ) -> Result<crate::ports::ManufacturingSpecificationProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_specification(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::ManufacturingRecordProjection, ManufacturingError> {
        let authority = self
            .authorization
            .authorize(actor, scope, ManufacturingAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(ManufacturingError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManufacturingError {
    #[error("manufacturing item is unavailable")]
    Neutral,
    #[error("manufacturing request conflicts with current state")]
    Conflict,
    #[error("an authenticated direct human decision is required")]
    HumanDecisionRequired,
    #[error("invalid manufacturing request: {0}")]
    Invalid(String),
    #[error("manufacturing dependency is unavailable")]
    Unavailable,
    #[error("manufacturing operation failed")]
    Internal,
}

fn require_authenticated_human(actor: &ActorContext) -> Result<(), ManufacturingError> {
    let actor = &actor.attribution;
    if actor.is_human()
        && actor.session_id.is_some()
        && actor.represented_principal_ref.is_none()
        && actor.support_capability_id.is_none()
    {
        Ok(())
    } else {
        Err(ManufacturingError::HumanDecisionRequired)
    }
}

fn bind_human_decision(
    actor: &ActorContext,
    scope: AuthorityScope,
    decision_ref: String,
    at: DateTime<Utc>,
) -> Result<HumanDecisionProof, ManufacturingError> {
    HumanDecisionProof::try_new(actor.attribution.clone(), scope, decision_ref, at)
        .map_err(|_| ManufacturingError::HumanDecisionRequired)
}
impl From<ManufacturingDomainError> for ManufacturingError {
    fn from(value: ManufacturingDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for ManufacturingError {
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

#[allow(dead_code)]
fn _ids(_: SpecificationId, _: ProductionOrderId) {}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use erpx_integration_contracts::{
        Quantity, StableRecordRef, catalog::ContextId, reference::RecordTypeId,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, CommandId, HoldingId, PrincipalId, RecordId, SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        domain::{ManufacturingMode, MaterialLine, MaterialRole, RoutingId, RoutingStep},
        ports::{AuthorizedManufacturingAccess, ManufacturingCommit},
    };

    fn uuid(value: u128) -> Uuid {
        let mut bytes = value.to_be_bytes();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    #[derive(Clone, Default)]
    struct NeverManufacturing {
        authorizations: Arc<AtomicUsize>,
        loads: Arc<AtomicUsize>,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AuthorizationPort for NeverManufacturing {
        async fn authorize(
            &self,
            actor: &ActorContext,
            target_scope: &AuthorityScope,
            action: ManufacturingAction,
        ) -> Result<AuthorizedManufacturingAccess, PortError> {
            self.authorizations.fetch_add(1, Ordering::SeqCst);
            Ok(AuthorizedManufacturingAccess {
                actor: actor.clone(),
                target_scope: target_scope.clone(),
                action,
            })
        }
    }

    #[async_trait]
    impl ManufacturingWriteRepository for NeverManufacturing {
        async fn load_specification(
            &self,
            _: &AuthorizedManufacturingAccess,
            _: SpecificationId,
        ) -> Result<Option<ManufacturingSpecification>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_order(
            &self,
            _: &AuthorizedManufacturingAccess,
            _: ProductionOrderId,
        ) -> Result<Option<ProductionOrder>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_record(
            &self,
            _: &AuthorizedManufacturingAccess,
            _: Uuid,
        ) -> Result<Option<ManufacturingRecord>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit(&self, _: ManufacturingCommit) -> Result<CommitReceipt, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Internal)
        }
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding {
            holding_id: uuid(1),
        }
    }

    fn actor(kind: ActorKind) -> ActorContext {
        ActorContext {
            holding_id: HoldingId::try_from_uuid(uuid(1)).unwrap(),
            principal_id: PrincipalId::try_from_uuid(uuid(2)).unwrap(),
            authority_scope: scope(),
            attribution: ActorAttribution {
                principal_ref: "principal:manufacturing-test".into(),
                kind,
                session_id: Some(SessionId::try_from_uuid(uuid(3)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
        }
    }

    fn identity() -> MutationIdentity {
        MutationIdentity {
            command_id: CommandId::try_from_uuid(uuid(4)).unwrap(),
            idempotency_key: "manufacturing-human-decision".into(),
            request_digest: "a".repeat(64),
        }
    }

    fn record(value: u128, context: ContextId, kind: &str) -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(uuid(value)).unwrap(),
            context,
            RecordTypeId::try_new(kind).unwrap(),
        )
        .unwrap()
    }

    fn specification() -> ManufacturingSpecification {
        ManufacturingSpecification::draft(
            SpecificationId::try_from_uuid(uuid(5)).unwrap(),
            scope(),
            ManufacturingMode::Discrete,
            record(6, ContextId::Inventory, "inventory.lot"),
            Quantity::try_new(1_000, 3).unwrap(),
            vec![MaterialLine {
                line: 1,
                item: record(7, ContextId::Inventory, "inventory.lot"),
                quantity: Quantity::try_new(2_000, 3).unwrap(),
                role: MaterialRole::Component,
                expected_scrap_basis_points: 0,
            }],
            RoutingId::try_from_uuid(uuid(8)).unwrap(),
            vec![RoutingStep {
                sequence: 1,
                operation_code: "ASSEMBLE".into(),
                work_center: record(9, ContextId::Assets, "asset.asset"),
                setup_seconds: 60,
                run_seconds_per_output: 1,
            }],
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn manufacturing_decisions_reject_invalid_actors_before_any_port() {
        let mut actors = vec![
            actor(ActorKind::Agent),
            actor(ActorKind::Service),
            actor(ActorKind::SystemWorker),
        ];
        let mut sessionless = actor(ActorKind::HoldingUser);
        sessionless.attribution.session_id = None;
        actors.push(sessionless);
        let mut represented = actor(ActorKind::HoldingUser);
        represented.attribution.represented_principal_ref = Some("principal:represented".into());
        actors.push(represented);
        let at = DateTime::from_timestamp(1_800_000_010, 0).unwrap();

        for actor in actors {
            for operation in 0..3 {
                let ports = NeverManufacturing::default();
                let service = ManufacturingCommandService::new(ports.clone(), ports.clone());
                let result = match operation {
                    0 => {
                        service
                            .publish_specification(
                                &actor,
                                identity(),
                                erpx_kernel::CorrelationId::try_from_uuid(uuid(10)).unwrap(),
                                specification(),
                                at,
                            )
                            .await
                    }
                    1 => {
                        service
                            .release_order(
                                &actor,
                                identity(),
                                ProductionOrderId::try_from_uuid(uuid(11)).unwrap(),
                                &scope(),
                                Some(1),
                                at,
                            )
                            .await
                    }
                    _ => {
                        service
                            .close_order(
                                &actor,
                                identity(),
                                ProductionOrderId::try_from_uuid(uuid(11)).unwrap(),
                                &scope(),
                                Some(8),
                                at,
                            )
                            .await
                    }
                };
                assert_eq!(
                    result.unwrap_err(),
                    ManufacturingError::HumanDecisionRequired
                );
                assert_eq!(ports.authorizations.load(Ordering::SeqCst), 0);
                assert_eq!(ports.loads.load(Ordering::SeqCst), 0);
                assert_eq!(ports.commits.load(Ordering::SeqCst), 0);
            }
        }
    }
}
