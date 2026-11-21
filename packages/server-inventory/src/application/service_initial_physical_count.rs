//! Owner-neutral requirements for a Service first Count in one Holding unit.
//! The caller must supply actual owner witnesses; this module provides no
//! default authority, Finance, Flow, Quality, Records, or ledger implementation.
use crate::domain::service_initial_physical_count::{
    CountAction, CountAuthority, CountStatus, InitialCountValue, transition_count,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountSelection {
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub count_date: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountFinanceBasis {
    pub chart_id: String,
    pub inventory_account_id: String,
    pub variance_account_id: String,
    pub currency: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountSourceProof {
    pub count_id: String,
    pub selection: InitialCountSelection,
    pub count_command_id: String,
    pub service_setup_command_id: String,
    pub service_setup_request_hash: String,
    pub finance: InitialCountFinanceBasis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountPosting {
    pub proof: InitialCountSourceProof,
    pub value: InitialCountValue,
    pub decision_reason: String,
    pub period_id: String,
}
/// Required participants operate on the same retained transaction. `check_*`
/// must retain a witness and recheck its image before commit. A successful
/// result from this interface is factual, not an independent permission grant.
pub trait InitialCountOwnerPorts: Send {
    type Error: From<BusinessError> + Send;
    fn check_current_authority_and_purchase(
        &mut self,
        action: CountAction,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn check_flow_actor(
        &mut self,
        count_id: &str,
        action: CountAction,
        version: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn check_quality_history_absent(
        &mut self,
        selection: &InitialCountSelection,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn check_finance_basis(
        &mut self,
        proof: &InitialCountSourceProof,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn bind_submission_evidence(
        &mut self,
        count_id: &str,
        version: i64,
        attachment_ids: &[String],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Finance owns journal, postings, events, and Money snapshots. The
    /// returned ID is None only when the counted variance has zero value.
    fn post_variance(
        &mut self,
        posting: &InitialCountPosting,
    ) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    fn record_flow_action(
        &mut self,
        count_id: &str,
        action: CountAction,
        version: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn require_all_current(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
/// This produces only a policy decision. No standalone effect or authority is
/// conferred; the coordinator must revalidate all owner witnesses and commit
/// Inventory, receipt, evidence, Flow and Finance effects together.
pub struct InitialCountActionAuthorization<'a> {
    pub count_id: &'a str,
    pub status: CountStatus,
    pub version: i64,
    pub expected: i64,
    pub action: CountAction,
    pub actor: &'a CountAuthority,
    pub reason: Option<&'a str>,
    pub attachments: &'a [String],
}
pub async fn authorize_initial_count_action<P: InitialCountOwnerPorts>(
    ports: &mut P,
    input: InitialCountActionAuthorization<'_>,
) -> Result<CountStatus, P::Error> {
    let InitialCountActionAuthorization {
        count_id,
        status,
        version,
        expected,
        action,
        actor,
        reason,
        attachments,
    } = input;
    ports.check_current_authority_and_purchase(action).await?;
    let next = transition_count(
        status,
        version,
        expected,
        action,
        actor,
        reason,
        attachments.len(),
    )?;
    ports.check_flow_actor(count_id, action, version).await?;
    Ok(next)
}

/// Facts supplied by Commerce after its scoped current-master witness. These
/// values alone are not authority and must remain witnessed through commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountProductSnapshot {
    pub version: i64,
    pub description: String,
    pub unit: String,
    pub tracking: crate::domain::consumption::TrackingMode,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountDraft {
    pub proof: InitialCountSourceProof,
    pub product: InitialCountProductSnapshot,
    pub reason: String,
    pub actor_id: String,
    pub value: InitialCountValue,
    pub created_at: i64,
}

/// Inventory's action effects only; permission, independent-person identity,
/// Flow, purchase, Finance, Records and receipt are mandatory external witnesses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountTransition {
    pub count_id: String,
    pub company_id: String,
    pub tracking: crate::domain::consumption::TrackingMode,
    pub expected_version: i64,
    pub action: CountAction,
    pub actor_id: String,
    pub reason: Option<String>,
    pub command_id: String,
    pub decision_id: String,
    pub journal_id: Option<String>,
    pub now: i64,
}
