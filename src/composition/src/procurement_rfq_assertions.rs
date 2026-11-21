//! Unmounted assertion stage. It does not authorize, translate errors, write
//! effects, or clear assertions. The command must retain raw errors through
//! its replay/Finance/conflict handling, including deferred commit failures
//! (removalTaskId: retire.procurement.rfq-writers).
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_identity_access::public::rfq_request_membership::require_rfq_request_actor_unit;
use erpx_server_procurement::public::rfq_target_source::CurrentRfqTarget;
use erpx_server_procurement::public::rfq_transition::{
    RfqTransitionAction, rfq_version_assertion_predicate,
};
use erpx_server_procurement::public::rfq_transition_normalization::NormalizedRfqTransition;
use erpx_server_records::public::rfq_request_source::rfq_request_approval_assertion_predicate;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::rfq_command_receipt::rfq_assertion_statement;
use sqlx::{AssertSqlSafe, PgConnection};

pub(crate) struct RfqAssertionInput<'a> {
    pub key: &'a str,
    pub id: &'a str,
    pub version: i64,
    pub status: &'a str,
    pub company: &'a str,
    pub branch: Option<&'a str>,
    pub department: Option<&'a str>,
    /// None for cancel or an unlinked RFQ, as selected by the caller.
    pub request: Option<RfqRequestAssertion<'a>>,
}
pub(crate) struct RfqRequestAssertion<'a> {
    pub id: &'a str,
    pub evidence_version: Option<i64>,
    pub items_json: Option<&'a str>,
}

pub(crate) async fn stage_rfq_assertions(
    c: &mut PgConnection,
    input: &RfqAssertionInput<'_>,
) -> Result<(), sqlx::Error> {
    let sql = rfq_assertion_statement(rfq_version_assertion_predicate());
    // Both fragments are static owner code; all command data is bound.
    sqlx::query(AssertSqlSafe(sql.as_str()))
        .bind(input.key)
        .bind("rfq-version")
        .bind(input.id)
        .bind(input.version)
        .bind(input.status)
        .bind(input.company)
        .bind(input.branch)
        .bind(input.department)
        .execute(&mut *c)
        .await?;
    if let Some(request) = &input.request {
        let sql = rfq_assertion_statement(rfq_request_approval_assertion_predicate());
        sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(input.key)
            .bind("rfq-request-approval")
            .bind(request.id)
            .bind(input.company)
            .bind(request.evidence_version)
            .bind(request.items_json)
            .execute(c)
            .await?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum RfqAssertionWitnessError {
    Source(PgTransactionError<BusinessError>),
    InvalidTarget,
    Sql(sqlx::Error),
}

/// Historical assertion observation: construction executes applicable Workflow
/// assertions against the same target and actor unit. It does not establish that
/// the linked Records source is still current, and cannot authorize a writer.
/// Receipt/replay, current Identity, approval, policy and error handling remain
/// required before an effect stage can consume an eventual command proof.
pub(crate) struct RfqAssertionWitness<'a> {
    actor: &'a AuthorizedWorkContext,
    target: &'a CurrentRfqTarget,
    input: &'a NormalizedRfqTransition,
    key: String,
    request_step: bool,
}
impl RfqAssertionWitness<'_> {
    pub(crate) async fn require_for(
        &self,
        c: &mut PgConnection,
        actor: &AuthorizedWorkContext,
        target: &CurrentRfqTarget,
        input: &NormalizedRfqTransition,
        key: &str,
    ) -> Result<(), RfqAssertionWitnessError> {
        if !std::ptr::eq(actor, self.actor)
            || !std::ptr::eq(target, self.target)
            || !std::ptr::eq(input, self.input)
        {
            return Err(RfqAssertionWitnessError::InvalidTarget);
        }
        self.require_same_observation(c, key).await
    }
    pub(crate) async fn require_same_observation(
        &self,
        c: &mut PgConnection,
        key: &str,
    ) -> Result<(), RfqAssertionWitnessError> {
        if key != self.key {
            return Err(RfqAssertionWitnessError::InvalidTarget);
        }
        require_rfq_request_actor_unit(&mut *c, self.actor)
            .await
            .map_err(RfqAssertionWitnessError::Source)?;
        self.target
            .require_current(&mut *c)
            .await
            .map_err(RfqAssertionWitnessError::Source)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id=$1 AND (step='rfq-version' OR ($2 AND step='rfq-request-approval')) AND ok=1")
            .bind(key).bind(self.request_step).fetch_one(c).await.map_err(RfqAssertionWitnessError::Sql)?;
        if count != if self.request_step { 2 } else { 1 } {
            return Err(RfqAssertionWitnessError::InvalidTarget);
        }
        Ok(())
    }
}

pub(crate) async fn prepare_rfq_assertion_witness<'a>(
    c: &mut PgConnection,
    actor: &'a AuthorizedWorkContext,
    target: &'a CurrentRfqTarget,
    key: &str,
    input: &'a NormalizedRfqTransition,
) -> Result<RfqAssertionWitness<'a>, RfqAssertionWitnessError> {
    require_rfq_request_actor_unit(&mut *c, actor)
        .await
        .map_err(RfqAssertionWitnessError::Source)?;
    target
        .require_current(&mut *c)
        .await
        .map_err(RfqAssertionWitnessError::Source)?;
    if !target.facts().within_selected_scope(actor) {
        return Err(RfqAssertionWitnessError::InvalidTarget);
    }
    let image: serde_json::Value = serde_json::from_str(&target.facts().image_json)
        .map_err(|_| RfqAssertionWitnessError::InvalidTarget)?;
    let row = image
        .get("record")
        .and_then(|v| v.as_object())
        .ok_or(RfqAssertionWitnessError::InvalidTarget)?;
    let status = row
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or(RfqAssertionWitnessError::InvalidTarget)?;
    let request_id = row
        .get("request_id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty());
    let request = if input.action() == RfqTransitionAction::Cancel {
        None
    } else {
        request_id.map(|id| RfqRequestAssertion {
            id,
            evidence_version: row.get("request_evidence_version").and_then(|v| v.as_i64()),
            items_json: row.get("request_items_json").and_then(|v| v.as_str()),
        })
    };
    let workspace = actor.selection().workspace();
    let expected_version = input
        .json()
        .root()
        .property("expectedVersion")
        .and_then(|v| v.number())
        .ok_or(RfqAssertionWitnessError::InvalidTarget)? as i64;
    let assertion = RfqAssertionInput {
        key,
        id: &target.facts().id,
        version: expected_version,
        status,
        company: workspace.company_id.as_str(),
        branch: workspace.branch_id.as_ref().map(|v| v.as_str()),
        department: workspace.department_id.as_ref().map(|v| v.as_str()),
        request,
    };
    let request_step = assertion.request.is_some();
    stage_rfq_assertions(&mut *c, &assertion)
        .await
        .map_err(RfqAssertionWitnessError::Sql)?;
    let witness = RfqAssertionWitness {
        actor,
        target,
        input,
        key: key.into(),
        request_step,
    };
    witness.require_same_observation(c, key).await?;
    Ok(witness)
}
