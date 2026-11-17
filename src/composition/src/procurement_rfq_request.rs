//! Source-bound RFQ request comparison. This does not authorize a command.
use erpx_server_procurement::public::rfq_request_status::{ApprovalJsonError, rfq_source_blocked};
use erpx_server_records::public::rfq_request_source::CurrentRfqRequest;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// Intermediate enabled-mode observation only; it carries no request access or
/// command authority. Raw SQL and JS-number signals must be translated by the
/// unfinished command boundary, not mapped to a public response here.
#[derive(Debug)]
pub(crate) enum RfqSourceAfterDetail {
    DetailNumberOutOfRange,
    SourceNumberOutOfRange,
    NumericMaterializationPending,
    Sql(sqlx::Error),
    SourceFence(Error),
    Blocked(Result<bool, ApprovalJsonError>),
}

/// The caller has already completed original purchase admission, replay and
/// RFQ row materialization. The four detail reads precede sourceState, whose
/// adapter number decoding precedes sourceBlocked. This is not a full payload.
pub(crate) async fn rfq_source_after_enabled_detail(
    c: &mut PgConnection,
    company: &CompanyId,
    rfq_id: &str,
    request_id: Option<&str>,
    status: &str,
    evidence_version: Option<i64>,
    items: Option<&str>,
    source: Option<&CurrentRfqRequest>,
) -> RfqSourceAfterDetail {
    use crate::procurement_rfq_materialization::{
        RfqDetailAndSourceProbe as Probe, RfqMaterialization, rfq_enabled_detail_then_source_state,
    };
    match rfq_enabled_detail_then_source_state(&mut *c, rfq_id, request_id, company.as_str()).await
    {
        Probe::DetailNumberOutOfRange => return RfqSourceAfterDetail::DetailNumberOutOfRange,
        Probe::NumericMaterializationPending => {
            return RfqSourceAfterDetail::NumericMaterializationPending;
        }
        Probe::Sql(error) => return RfqSourceAfterDetail::Sql(error),
        Probe::Source(Some(RfqMaterialization::NumberOutOfRange)) => {
            return RfqSourceAfterDetail::SourceNumberOutOfRange;
        }
        Probe::Source(Some(RfqMaterialization::UnsupportedNumeric)) => {
            return RfqSourceAfterDetail::NumericMaterializationPending;
        }
        Probe::Source(_) => {}
    }
    match checked_rfq_source_status(
        c,
        company,
        request_id,
        status,
        evidence_version,
        items,
        source,
    )
    .await
    {
        Ok(value) => RfqSourceAfterDetail::Blocked(value),
        Err(error) => RfqSourceAfterDetail::SourceFence(error),
    }
}

/// `request_id` and saved values must come from the current protected RFQ row.
/// The caller retains and revalidates the witness after all deferred effects.
pub(crate) async fn checked_rfq_source_status(
    c: &mut PgConnection,
    company: &CompanyId,
    request_id: Option<&str>,
    status: &str,
    evidence_version: Option<i64>,
    items: Option<&str>,
    source: Option<&CurrentRfqRequest>,
) -> Result<Result<bool, ApprovalJsonError>, Error> {
    let expected = request_id.filter(|id| !id.is_empty());
    let facts = if let Some(id) = expected {
        let source = source.ok_or_else(|| PersistenceError::new("RFQ_REQUEST_WITNESS_REQUIRED"))?;
        if source.company() != company || source.id() != id {
            return Err(PersistenceError::new("RFQ_REQUEST_WITNESS_MISMATCH").into());
        }
        source.require_current(c).await?;
        source.facts()
    } else {
        None
    };
    Ok(rfq_source_blocked(
        request_id,
        status,
        evidence_version,
        items,
        facts,
    ))
}
