//! Fresh RFQ transition decision bound to actual owner and Identity sources.
//! Not a receipt, purchase grant, writer, HTTP route or completion claim.
use crate::procurement_rfq_request::checked_rfq_source_status;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_procurement::public::{
    rfq_request_status::ApprovalJsonError,
    rfq_target_source::CurrentRfqTarget,
    rfq_transition::{
        RfqCurrentOffer, RfqTransitionDecision, RfqTransitionFacts, decide_rfq_transition,
    },
    rfq_transition_normalization::NormalizedRfqTransition,
};
use erpx_server_records::public::rfq_request_source::CurrentRfqRequest;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
use std::collections::HashSet;
type Error = PgTransactionError<BusinessError>;
fn invalid() -> Error {
    PersistenceError::new("RFQ_TARGET_IMAGE_INVALID").into()
}

pub(crate) enum RfqDecisionPreparation {
    Decision(RfqTransitionDecision),
    InvalidApprovalJson(ApprovalJsonError),
    /// Detail must call Records' full quotation-source path before making a
    /// transition decision for an approved, linked RFQ. The narrow
    /// sourceBlocked witness cannot reproduce its non-403/404/409 failures.
    QuotationSourceRequired,
}

/// Call only for a fresh request after original purchase admission and replay.
/// An invalid stored approval JSON is returned separately so the eventual HTTP
/// bridge can preserve the TypeScript error mapping. No write is performed.
pub(crate) async fn prepare_rfq_decision(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &CurrentRfqTarget,
    source: Option<&CurrentRfqRequest>,
    input: &NormalizedRfqTransition,
    today: &str,
) -> Result<RfqDecisionPreparation, Error> {
    current.require_permission("commerce.purchase")?;
    current.require_permission("commerce.read")?;
    target.require_current(&mut *c).await?;
    if !target.facts().within_selected_scope(current) {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "RFQ_NOT_FOUND",
            "Quotation request not found in this scope.",
        )
        .into());
    }
    let image: serde_json::Value =
        serde_json::from_str(&target.facts().image_json).map_err(|_| invalid())?;
    let record = image
        .get("record")
        .and_then(|v| v.as_object())
        .ok_or_else(invalid)?;
    let status = record
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(invalid)?;
    let request = record.get("request_id").and_then(|v| v.as_str());
    let saved_version = record
        .get("request_evidence_version")
        .and_then(|v| v.as_i64());
    let saved_items = record.get("request_items_json").and_then(|v| v.as_str());
    let blocked = match checked_rfq_source_status(
        &mut *c,
        &current.selection().workspace().company_id,
        request,
        status,
        saved_version,
        saved_items,
        source,
    )
    .await?
    {
        Ok(v) => v,
        Err(e) => return Ok(RfqDecisionPreparation::InvalidApprovalJson(e)),
    };
    if status == "approved"
        && request.is_some_and(|id| !id.is_empty())
        && source.is_some_and(|source| source.facts().is_some())
    {
        // Do not let an incomplete source projection silently authorize a
        // decision, including cancel, before the real Records lookup exists.
        return Ok(RfqDecisionPreparation::QuotationSourceRequired);
    }
    // A missing row in the selected Company makes the Records lookup return
    // REQUEST_NOT_FOUND, which currentQuotationSource deliberately swallows.
    let selected = record
        .get("selected_offer_id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    let creator = record
        .get("created_by")
        .and_then(|v| v.as_str())
        .ok_or_else(invalid)?;
    let submitter = record.get("submitted_by").and_then(|v| v.as_str());
    let offers = image
        .get("offers")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let selected_recorder = offers
        .iter()
        .find(|v| v.get("id").and_then(|v| v.as_str()) == selected.as_deref())
        .and_then(|v| v.get("recorded_by"))
        .and_then(|v| v.as_str());
    let independent = !current.same_person_user_raw(creator)
        && !submitter.is_some_and(|id| current.same_person_user_raw(id))
        && !selected_recorder.is_some_and(|id| current.same_person_user_raw(id));
    let mut sorted = offers
        .iter()
        .map(|o| {
            Ok((
                o.get("rfq_version")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(invalid)?,
                o.get("supplier_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(invalid)?,
                o.get("id").and_then(|v| v.as_str()).ok_or_else(invalid)?,
                o.get("valid_until")
                    .and_then(|v| v.as_str())
                    .ok_or_else(invalid)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    let mut suppliers = HashSet::new();
    let current_offers = sorted
        .into_iter()
        .filter(|(_, supplier, _, _)| suppliers.insert(*supplier))
        .map(|(_, _, id, valid)| RfqCurrentOffer {
            id: id.into(),
            valid_until: valid.into(),
        })
        .collect();
    let facts = RfqTransitionFacts {
        status: status.into(),
        source_blocked: blocked,
        selected_offer_id: selected,
        current_offers,
        can_approve: current.has_permission("commerce.purchase")
            && current.has_permission("commerce.approve")
            && independent
            && !blocked,
        creator_is_actor: current.same_person_user_raw(creator),
    };
    let offer = input
        .json()
        .root()
        .property("offerId")
        .and_then(|v| v.utf16())
        .and_then(|v| String::from_utf16(v).ok());
    let decision = decide_rfq_transition(&facts, input.action(), offer.as_deref(), today, || {
        current.require_permission("commerce.approve")
    })?;
    target.require_current(&mut *c).await?;
    if request.is_some_and(|id| !id.is_empty()) {
        if let Some(source) = source {
            source.require_current(c).await?;
        }
    }
    Ok(RfqDecisionPreparation::Decision(decision))
}
