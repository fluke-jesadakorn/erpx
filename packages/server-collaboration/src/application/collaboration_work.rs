//! A Collaboration mutation runs inside the caller's one retained owner unit.
use crate::domain::collaboration_work::{
    AuthorizedActor, CardAction, Change, CurrentCard, CurrentRelation, OwnerPlan, Receipt,
    WriterPartition, authorize_card_change, card_partition, js_trim, relation_partition,
    relation_sources, validate_text,
};
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::future::Future;

/// Collaboration owns this port; composition delegates it to Records and the source owner.
pub trait CollaborationSourcePort: Send {
    type Error: From<BusinessError> + Send;
    type Witness: Send;
    fn prepare(
        &mut self,
        source: &SourceReference,
    ) -> impl Future<Output = Result<(SourceRecord, Self::Witness), Self::Error>> + Send;
    fn require_current(
        &mut self,
        witness: &Self::Witness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
struct PreparedSource<W> {
    record: SourceRecord,
    witness: W,
}
impl<W> PreparedSource<W> {
    async fn require_current<P: CollaborationSourcePort<Witness = W>>(
        &self,
        port: &mut P,
    ) -> Result<(), P::Error> {
        port.require_current(&self.witness).await
    }
}
pub trait CollaborationUnit: CollaborationSourcePort {
    /// Must establish current records.write, Collaboration module and selected IAM.
    fn authorize(&mut self) -> impl Future<Output = Result<AuthorizedActor, Self::Error>> + Send;
    fn replay(
        &mut self,
        change: &Change,
    ) -> impl Future<Output = Result<Option<Receipt>, Self::Error>> + Send;
    fn relation(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<CurrentRelation, Self::Error>> + Send;
    fn card(&mut self, id: &str) -> impl Future<Output = Result<CurrentCard, Self::Error>> + Send;
    /// Stage owner rows, notices, events, ledger/audit/outbox; never commit here.
    fn apply(
        &mut self,
        actor: &AuthorizedActor,
        plan: OwnerPlan,
    ) -> impl Future<Output = Result<Receipt, Self::Error>> + Send;
    /// Flush deferred database constraints and owner effects before currentness.
    fn flush_deferred(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Revalidate the complete selected IAM and all Collaboration-owned witnesses.
    fn require_unit_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_INPUT_INVALID",
        "Provide a valid collaboration value.",
    )
}
async fn source<P: CollaborationUnit>(
    p: &mut P,
    a: &AuthorizedActor,
    r: &SourceReference,
) -> Result<PreparedSource<P::Witness>, P::Error> {
    if !matches!(
        r.kind,
        SourceKind::FieldServiceOrder | SourceKind::SupportTicket
    ) {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED",
            "This source is not part of the native Collaboration capability.",
        )
        .into());
    }
    let (record, witness) = p.prepare(r).await?;
    if record.kind != r.kind || record.id != r.id || record.company_id != a.company_id {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SOURCE_RECORD_NOT_FOUND",
            "Source record is unavailable in this work scope.",
        )
        .into());
    }
    if r.expected_version.is_some_and(|v| v != record.version) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SOURCE_RECORD_CHANGED",
            "The source version changed. Reload its owning workspace before handing it off.",
        )
        .into());
    }
    Ok(PreparedSource { record, witness })
}
/// No transaction is opened or committed here. All reads, effects and retained
/// witnesses must use the same caller-owned serializable transaction.
pub async fn execute<P: CollaborationUnit>(p: &mut P, change: Change) -> Result<Receipt, P::Error> {
    let actor = p.authorize().await?;
    let reason = match &change {
        Change::CreateRelation { reason, .. }
        | Change::RemoveRelation { reason, .. }
        | Change::CreateCard { reason, .. }
        | Change::ChangeCard { reason, .. } => reason,
    };
    validate_text(reason, 3, 3000)?;
    let supported = match &change {
        Change::CreateRelation { left, right, .. } => {
            relation_partition(left.kind, right.kind) == WriterPartition::NativeField
        }
        Change::CreateCard { source, .. } => {
            card_partition(source.kind) == WriterPartition::NativeField
        }
        // Existing records must be classified from their immutable stored source.
        Change::RemoveRelation { .. } | Change::ChangeCard { .. } => true,
    };
    if !supported {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED",
            "This source is not part of the native Collaboration capability.",
        )
        .into());
    }
    let prior = p.replay(&change).await?;
    let (plan, guards) = match &change {
        Change::CreateRelation {
            left,
            right,
            relation,
            reason,
        } => {
            let l = source(p, &actor, left).await?;
            let r = source(p, &actor, right).await?;
            let (left, right) = relation_sources(l.record.clone(), r.record.clone(), *relation)?;
            (
                OwnerPlan::CreateRelation {
                    left,
                    right,
                    relation: *relation,
                    reason: js_trim(reason).into(),
                },
                vec![l, r],
            )
        }
        Change::RemoveRelation {
            id,
            expected_version,
            reason,
        } => {
            if *expected_version <= 0 {
                return Err(invalid().into());
            }
            let current = p.relation(id).await?;
            if relation_partition(current.left.kind, current.right.kind)
                != WriterPartition::NativeField
            {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED",
                    "This stored record belongs to another Collaboration capability.",
                )
                .into());
            }
            let l = source(p, &actor, &current.left).await?;
            let r = source(p, &actor, &current.right).await?;
            if prior.is_none() && (!current.active || current.version != *expected_version) {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "RECORD_RELATION_CHANGED",
                    "Refresh the relation.",
                )
                .into());
            }
            (
                OwnerPlan::RemoveRelation {
                    current,
                    reason: reason.clone(),
                },
                vec![l, r],
            )
        }
        Change::CreateCard {
            source: reference,
            title,
            owner_id,
            due_at,
            reason,
        } => {
            validate_text(title, 3, 160)?;
            if owner_id.is_empty() || *due_at <= 0 {
                return Err(invalid().into());
            }
            let s = source(p, &actor, reference).await?;
            (
                OwnerPlan::CreateCard {
                    source: s.record.clone(),
                    title: js_trim(title).into(),
                    owner_id: owner_id.clone(),
                    due_at: *due_at,
                    reason: js_trim(reason).into(),
                },
                vec![s],
            )
        }
        Change::ChangeCard {
            id,
            expected_version,
            action,
            owner_id,
            due_at,
            evidence,
            reason,
        } => {
            if *expected_version <= 0 {
                return Err(invalid().into());
            }
            match action {
                CardAction::Complete => {
                    validate_text(evidence.as_deref().ok_or_else(invalid)?, 3, 4000)?
                }
                CardAction::Reassign => {
                    if owner_id.as_deref().is_none_or(str::is_empty)
                        || due_at.is_none_or(|n| n <= 0)
                    {
                        return Err(invalid().into());
                    }
                }
                CardAction::Cancel => {}
            }
            let current = p.card(id).await?;
            if card_partition(current.source.kind) != WriterPartition::NativeField {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "COLLABORATION_NATIVE_SOURCE_UNSUPPORTED",
                    "This stored record belongs to another Collaboration capability.",
                )
                .into());
            }
            let s = source(p, &actor, &current.source).await?;
            if prior.is_none() {
                authorize_card_change(&actor, &current, *action, *expected_version)?;
            }
            (
                OwnerPlan::ChangeCard {
                    current,
                    action: *action,
                    owner_id: if *action == CardAction::Reassign {
                        owner_id.clone()
                    } else {
                        None
                    },
                    due_at: if *action == CardAction::Reassign {
                        *due_at
                    } else {
                        None
                    },
                    evidence: if *action == CardAction::Complete {
                        evidence.as_ref().map(|v| js_trim(v).into())
                    } else {
                        None
                    },
                    reason: js_trim(reason).into(),
                },
                vec![s],
            )
        }
    };
    let result = match prior {
        Some(receipt) => receipt,
        None => p.apply(&actor, plan).await?,
    };
    p.flush_deferred().await?;
    for guard in &guards {
        guard.require_current(p).await?;
    }
    p.require_unit_current().await?;
    Ok(result)
}
