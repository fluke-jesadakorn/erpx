//! Native PMS dashboard/report read use cases.
//!
//! The repository owns only PMS tables and is read-only here: no serializable
//! unit, no lock, no journal and no source purchase. The retained response is
//! split into one bounded range summary and one bounded entry ledger, both
//! scoped to the actor's Company and (when selected) branch exactly as the
//! retained `pmsRangeReport` scope guard does.
//!
//! Omitted by design, never invented: the Finance-backed `balances` block
//! (`receivable`/`advance`/`deposit`/`refundPayable`/`unallocated` journal
//! lines, required port `pms_reporting_facts` from `server-finance`), the
//! booking-observation `pace`/`channels` windows, and the Finance/POS/Workflow
//! day-control counts behind the night-audit blocker list.
use crate::domain::night_audit::{PmsNightAuditFacts, night_blockers};
use crate::domain::report_projection::{
    PAGE_SIZE, PmsReportBalances, PmsReportData, PmsReportDay, PmsReportEntries,
    PmsReportEntriesData, PmsReportEntry, PmsReportFlow, PmsReportNightAudit, PmsReportRange,
    in_scope, not_found, read_invalid, read_limit, require_report, summarize, validate_days,
};
use crate::domain::room_board::{PmsBoardActor, PmsPropertyRow};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PmsReportFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// The night-audit facts a report may read plus the two persisted states the
/// retained night-audit screen shows. Day-control counts are not part of it.
#[derive(Clone, Debug)]
pub struct PmsReportNightRead {
    pub night_state: Option<String>,
    pub job_state: Option<String>,
    pub facts: PmsNightAuditFacts,
}

/// PMS-owned report repository. Every method is a bounded read.
pub trait PmsReportRepository: Send {
    type Error: From<BusinessError> + Send;
    fn property<'a>(
        &'a mut self,
        company_id: &'a str,
        property_id: &'a str,
    ) -> PmsReportFuture<'a, Option<PmsPropertyRow>, Self::Error>;
    fn days<'a>(
        &'a mut self,
        property: &'a PmsPropertyRow,
        from: &'a str,
        to: &'a str,
    ) -> PmsReportFuture<'a, Vec<PmsReportDay>, Self::Error>;
    /// Retained dashboard stay counts anchored on `business_date` (`to`).
    fn flow<'a>(
        &'a mut self,
        property_id: &'a str,
        business_date: &'a str,
    ) -> PmsReportFuture<'a, PmsReportFlow, Self::Error>;
    /// Retained `controlBalances`: the PMS-owned guest-account projection.
    fn balances<'a>(
        &'a mut self,
        property_id: &'a str,
    ) -> PmsReportFuture<'a, PmsReportBalances, Self::Error>;
    fn night_read<'a>(
        &'a mut self,
        property: &'a PmsPropertyRow,
    ) -> PmsReportFuture<'a, PmsReportNightRead, Self::Error>;
    /// `limit + 1` rows of the retained entry ledger, ordered by
    /// `updated_at, id` so the boundary is deterministic.
    fn entries<'a>(
        &'a mut self,
        property_id: &'a str,
        date: &'a str,
        offset: i64,
    ) -> PmsReportFuture<'a, Vec<PmsReportEntry>, Self::Error>;
}

/// Both retained input shapes select a hotel; `property` is required because a
/// report is never guessed from the first property of a holding.
async fn scoped_property<R: PmsReportRepository>(
    actor: &PmsBoardActor,
    property_id: &str,
    repository: &mut R,
) -> Result<PmsPropertyRow, R::Error> {
    let row = repository
        .property(&actor.company_id, property_id)
        .await?
        .ok_or_else(|| R::Error::from(not_found()))?;
    if !in_scope(actor, &row) {
        return Err(not_found().into());
    }
    Ok(row)
}

/// Retained `pmsRangeReport` without the cross-owner blocks.
pub async fn pms_range_report<R: PmsReportRepository>(
    actor: &PmsBoardActor,
    selection: &PmsReportRange,
    repository: &mut R,
    now: i64,
) -> Result<PmsReportData, R::Error> {
    require_report(actor, now)?;
    selection.validate()?;
    let property = scoped_property(actor, &selection.property_id, repository).await?;
    let view = property.view().map_err(|_| read_invalid())?;
    let currency = view
        .config
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(read_invalid)?
        .to_owned();
    let days = repository
        .days(&property, &selection.from, &selection.to)
        .await?;
    validate_days(&selection.from, &selection.to, &days)?;
    let totals = summarize(&days);
    let flow = repository.flow(&property.id, &selection.to).await?;
    let balances = repository.balances(&property.id).await?;
    let night = repository.night_read(&property).await?;
    if night.facts.property.id != property.id {
        return Err(read_invalid().into());
    }
    let blockers = night_blockers(&night.facts).map_err(R::Error::from)?;
    Ok(PmsReportData {
        property: property.summary(),
        business_date: property.business_date.clone(),
        currency,
        from: selection.from.clone(),
        to: selection.to.clone(),
        days,
        totals,
        flow,
        balances,
        night_audit: PmsReportNightAudit {
            business_date: property.business_date.clone(),
            night_state: night.night_state,
            job_state: night.job_state,
            ready: blockers.is_empty(),
            blockers,
        },
    })
}

/// Retained `pmsReportEntries`: 50 rows per page, one extra row for `hasNext`.
pub async fn pms_report_entries<R: PmsReportRepository>(
    actor: &PmsBoardActor,
    selection: &PmsReportEntries,
    repository: &mut R,
    now: i64,
) -> Result<PmsReportEntriesData, R::Error> {
    require_report(actor, now)?;
    selection.validate()?;
    let property = scoped_property(actor, &selection.property_id, repository).await?;
    let mut rows = repository
        .entries(&property.id, &selection.date, selection.offset())
        .await?;
    if rows.len() > PAGE_SIZE + 1 {
        return Err(read_limit("The hotel entry ledger exceeds its bounded page.").into());
    }
    let has_next = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    Ok(PmsReportEntriesData {
        date: selection.date.clone(),
        page: selection.page,
        rows,
        has_next,
    })
}
