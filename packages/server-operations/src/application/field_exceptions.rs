//! The consuming Operations command retains Records and recipient witnesses
//! after owner effects. Composition owns the one transaction and Workflow key.
use crate::domain::field_exceptions::{Actor, Exception, Transition, TransitionInput, transition};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::future::Future;
pub trait FieldExceptionUnit: Send {
    type Error: From<BusinessError> + Send;
    type SourceWitness: Send;
    type RecipientWitness: Send;
    fn actor(&mut self, row: &Exception)
    -> impl Future<Output = Result<Actor, Self::Error>> + Send;
    fn source(
        &mut self,
        row: &Exception,
    ) -> impl Future<Output = Result<Self::SourceWitness, Self::Error>> + Send;
    /// Dated active People engagement AND scoped IAM permission. This must not use
    /// Collaboration's intentionally undated action-card recipient predicate.
    fn recipient(
        &mut self,
        row: &Exception,
        user: &str,
        review: bool,
    ) -> impl Future<Output = Result<Self::RecipientWitness, Self::Error>> + Send;
    fn apply(
        &mut self,
        row: &Exception,
        input: &TransitionInput,
        plan: &Transition,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn flush(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn require_current(
        &mut self,
        source: &Self::SourceWitness,
        recipients: &[Self::RecipientWitness],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub async fn apply_transition<P: FieldExceptionUnit>(
    unit: &mut P,
    row: &Exception,
    input: &TransitionInput,
) -> Result<Transition, P::Error> {
    if row.source_kind != "field_service_order" {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED",
            "This source requires its native owner capability.",
        )
        .into());
    }
    let actor = unit.actor(row).await?;
    let plan = transition(&actor, row, input)?;
    let source = unit.source(row).await?;
    let mut recipients = vec![
        unit.recipient(row, &actor.user, input.action.manages())
            .await?,
    ];
    use crate::domain::field_exceptions::Action;
    if input.action == Action::Assign {
        recipients.push(unit.recipient(row, &plan.owner, false).await?);
    }
    if input.action == Action::Escalate {
        recipients.push(
            unit.recipient(
                row,
                plan.escalation_owner
                    .as_deref()
                    .expect("validated escalation owner"),
                true,
            )
            .await?,
        );
    }
    unit.apply(row, input, &plan).await?;
    unit.flush().await?;
    unit.require_current(&source, &recipients).await?;
    Ok(plan)
}
