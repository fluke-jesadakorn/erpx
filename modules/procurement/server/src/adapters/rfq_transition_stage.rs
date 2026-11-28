//! Unmounted owner-local RFQ transition write, not a complete command.
//! Composition must bind current Identity, purchase, Records, Workflow and
//! Collaboration facts and effects in the same protected transaction.
use crate::domain::rfq_transition::{RfqTransitionAction, RfqTransitionDecision};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Static owner predicate for the Workflow assertion, with $1/$2 reserved
/// for command key and step. Scope placeholders follow the TS guard order.
pub fn rfq_version_assertion_predicate() -> &'static str {
    "EXISTS(SELECT 1 FROM procurement_rfqs r WHERE r.id=$3 AND r.version=$4 AND r.status=$5 AND r.company_id=$6 AND ($7::text IS NULL OR r.branch_id=$7) AND ($8::text IS NULL OR r.department_id=$8))"
}

pub(crate) struct TransitionWrite<'a> {
    pub id: &'a str,
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub expected_status: &'a str,
    pub expected_version: i64,
    pub actor_id: &'a str,
    pub command_key: &'a str,
    pub evidence: &'a str,
    pub now: i64,
    pub action: RfqTransitionAction,
    pub decision: &'a RfqTransitionDecision,
}

/// Source locks are acquired before the transaction's first data snapshot,
/// after earlier owners in the global lock order. This is not authorization.
pub(crate) async fn lock_transition_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RFQ_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.procurement_rfqs,public.procurement_rfq_events IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// Only a staged write: caller must discard the whole transaction on any
/// failure and must not commit until all cross-owner effects are verified.
pub(crate) async fn stage_transition(
    c: &mut PgConnection,
    input: &TransitionWrite<'_>,
) -> Result<bool, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("RFQ_SERIALIZABLE_REQUIRED").into());
    }
    let locks: i64 = sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('public.procurement_rfqs'::regclass,'public.procurement_rfq_events'::regclass)")
        .fetch_one(&mut *c).await?;
    if locks != 2 {
        return Err(PersistenceError::new("RFQ_SOURCE_LOCKS_REQUIRED").into());
    }
    let action = match input.action {
        RfqTransitionAction::Submit => "submit",
        RfqTransitionAction::Approve => "approve",
        RfqTransitionAction::Return => "return",
        RfqTransitionAction::Cancel => "cancel",
    };
    let valid = match input.action {
        RfqTransitionAction::Submit => {
            input.decision.status == "review" && input.decision.selected_offer_id.is_some()
        }
        RfqTransitionAction::Approve => {
            input.decision.status == "approved" && input.decision.selected_offer_id.is_some()
        }
        RfqTransitionAction::Return => {
            input.decision.status == "collecting" && input.decision.selected_offer_id.is_none()
        }
        RfqTransitionAction::Cancel => input.decision.status == "cancelled",
    };
    if !valid {
        return Err(PersistenceError::new("RFQ_TRANSITION_DECISION_INVALID").into());
    }
    let event_version = input
        .expected_version
        .checked_add(1)
        .ok_or_else(|| PersistenceError::new("RFQ_VERSION_INVALID"))?;
    // The original assertion compares scope/status/version before the two
    // updates. Its event offer is a JS truthiness projection, not a predicate.
    {
        let guarded:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.procurement_rfqs WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4) AND status=$5 AND version=$6)")
            .bind(input.id).bind(input.company_id).bind(input.branch_id).bind(input.department_id).bind(input.expected_status).bind(input.expected_version).fetch_one(&mut *c).await?;
        if !guarded { return Ok(false); }
    }
    // Preserve TS statement boundaries: each UPDATE may fire independent
    // work-cache and business triggers. Cancel has no action-specific UPDATE.
    let changed = match input.action {
        RfqTransitionAction::Submit => sqlx::query("UPDATE public.procurement_rfqs SET selected_offer_id=$1,selection_reason=$2,submitted_by=$3,approved_by=NULL WHERE id=$4")
            .bind(input.decision.selected_offer_id.as_deref()).bind(input.evidence).bind(input.actor_id).bind(input.id).execute(&mut *c).await?.rows_affected(),
        RfqTransitionAction::Approve => sqlx::query("UPDATE public.procurement_rfqs SET approved_by=$1 WHERE id=$2")
            .bind(input.actor_id).bind(input.id).execute(&mut *c).await?.rows_affected(),
        RfqTransitionAction::Return => sqlx::query("UPDATE public.procurement_rfqs SET selected_offer_id=NULL,selection_reason=NULL,submitted_by=NULL,approved_by=NULL WHERE id=$1")
            .bind(input.id).execute(&mut *c).await?.rows_affected(),
        RfqTransitionAction::Cancel => 1,
    };
    if changed != 1 {
        return Err(PersistenceError::new("RFQ_STAGE_ROW_MISSING").into());
    }
    let status_changed = sqlx::query(
        "UPDATE public.procurement_rfqs SET status=$1,version=version+1,updated_at=$2 WHERE id=$3",
    )
    .bind(input.decision.status)
    .bind(input.now)
    .bind(input.id)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if status_changed != 1 {
        return Err(PersistenceError::new("RFQ_STAGE_ROW_MISSING").into());
    }
    sqlx::query("INSERT INTO public.procurement_rfq_events(id,rfq_id,version,action,actor_id,evidence,selected_offer_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(format!("rfq-event-{}",input.command_key)).bind(input.id).bind(event_version).bind(action)
        .bind(input.actor_id).bind(input.evidence).bind(input.decision.selected_offer_id.as_deref()).bind(input.now).execute(c).await?;
    Ok(true)
}
