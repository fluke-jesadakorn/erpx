//! Native Procurement supplier-bill command use cases.
//!
//! Repository SQL stays on the caller's retained SERIALIZABLE Holding
//! transaction. The application checks the current Account/IAM actor facts,
//! resolves an exact idempotency receipt before a fresh write, reads the
//! current Commerce/Finance source facts through the owner ports, validates
//! the complete bill plan with the exact retained denial codes, records the
//! immutable bill history and writes the command receipt/audit/outbox
//! evidence in the same unit. Every cross-owner effect (Commerce
//! receipt-version and invoice-link staging, the Finance draft-void effect
//! and the Collaboration notification fan-out) is an explicit repository
//! hook: composition must supply the owning ports, and they must run inside
//! this same caller-owned transaction. The purchase admission itself
//! (capability foundations, assignment authority, source images) runs in
//! composition ahead of this port, exactly like the promise path.
use crate::domain::supplier_bill_command::{
    CaptureSupplierBillCommand, DecideSupplierBillCommand, RequestedBillReceipt,
    SaveSupplierBillPolicyCommand, SupplierBillAuthority, SupplierBillCommandActor,
    SupplierBillReadback, SupplierBillRecordReceipt, decimal_to_minor, idempotency_conflict,
    incomplete_receipt, invalid_receipt, quantity_milli, requested_bill_receipts, source_changed,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind, TimestampMillis};
use std::future::Future;

pub type SupplierBillCommandFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredSupplierBillReceipt {
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub authority_json: Option<String>,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SupplierBillCommitPlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub actor_id: String,
    pub assignment_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub now: TimestampMillis,
}

#[derive(Clone, Debug)]
pub struct SupplierBillCapturePlan {
    pub commit: SupplierBillCommitPlan,
    pub match_id: String,
    pub receipts: Vec<RequestedBillReceipt>,
    pub now: TimestampMillis,
}

#[derive(Clone, Debug)]
pub struct SupplierBillDecidePlan {
    pub commit: SupplierBillCommitPlan,
    pub match_id: String,
    pub action: String,
    pub expected_version: i64,
    pub variance_account_id: Option<String>,
    pub reason: String,
    pub now: TimestampMillis,
}

#[derive(Clone, Debug)]
pub struct SupplierBillPolicyPlan {
    pub commit: SupplierBillCommitPlan,
    pub matching_mode: String,
    pub expected_version: i64,
    pub now: TimestampMillis,
}

/// Capture source facts, one row per selected goods receipt with its order,
// journal and posting context, exactly as the retained reads provide them.
#[derive(Clone, Debug)]
pub struct CaptureReceiptFact {
    pub id: String,
    pub order_id: String,
    pub fulfillment_date: String,
    pub version: i64,
    pub invoice_id: Option<String>,
    pub order_kind: String,
    pub order_party_id: String,
    pub order_currency: String,
    pub order_branch_id: Option<String>,
    pub order_department_id: Option<String>,
    pub journal_id: Option<String>,
    pub journal_status: Option<String>,
    pub journal_currency: Option<String>,
    pub journal_chart_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CaptureOrderFact {
    pub id: String,
    pub kind: String,
    pub party_id: String,
    pub currency: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

/// Fully validated capture attributes carried into the match insert,
/// literal mirror of the retained `BillCapturePlan` scalars.
#[derive(Clone, Debug)]
pub struct CaptureAttributes {
    pub order_branch_id: String,
    pub order_department_id: Option<String>,
    pub order_id: String,
    pub fulfillment_id: String,
    pub supplier_id: String,
    pub currency: String,
    pub chart_id: String,
    pub clearing_account_id: String,
    pub payable_account_id: String,
    pub tax_account_id: String,
    pub reference: String,
    pub invoice_date: String,
    pub due_date: String,
    pub evidence: String,
    pub expected_subtotal: i64,
    pub subtotal: i64,
    pub tax: i64,
    pub quantity_matches: bool,
    pub created_by: String,
}

#[derive(Clone, Debug)]
pub struct PostingConfig {
    pub chart_id: String,
    pub payable_account_id: String,
    pub input_tax_account_id: String,
}

#[derive(Clone, Debug)]
pub struct CapturePostingFact {
    pub account_id: String,
    pub credit_minor: i64,
}

#[derive(Clone, Debug)]
pub struct CaptureLineFact {
    pub id: String,
    pub fulfillment_id: String,
    pub quantity_milli: i64,
    pub subtotal_minor: i64,
}

#[derive(Clone, Debug)]
pub struct CapturePreparedLine {
    pub fulfillment_line_id: String,
    pub quantity: i64,
    pub expected_quantity: i64,
    pub price: i64,
    pub tax_bps: i64,
    pub expected_subtotal: i64,
    pub subtotal: i64,
    pub tax: i64,
}

#[derive(Clone, Debug)]
pub struct BillHead {
    pub id: String,
    pub created_by: String,
    pub invoice_created_by: Option<String>,
    pub status: String,
    pub invoice_status: Option<String>,
    pub quantity_matches: bool,
    pub version: i64,
    pub chart_id: String,
    pub variance_account_id: Option<String>,
    pub invoice_id: Option<String>,
    pub invoice_version: Option<i64>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PolicyHead {
    pub version: i64,
    pub matching_mode: String,
}

/// Owner persistence plus the explicit cross-owner staging hooks. The hooks
/// must be supplied by the owning crates (Commerce receipt-version and
/// invoice links, Finance draft-void, Collaboration notifications) and must
/// run inside the same caller-owned transaction.
pub trait SupplierBillCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<StoredSupplierBillReceipt>, Self::Error>;

    /// One selected goods receipt with journal and order context, company and
    /// frame scoped. `None` reads as `RECEIPT_NOT_FOUND`.
    fn capture_receipt<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureReceiptFact>, Self::Error>;

    /// One purchase order head for scope consolidation. `None` reads as
    /// `UNBILLED_PURCHASE_REQUIRED` (the retained missing-order denial).
    fn capture_order<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureOrderFact>, Self::Error>;

    /// Company posting configuration for receipt clearing. `None` reads
    /// as `BILL_POSTING_PROFILE_REQUIRED`.
    fn posting_config<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PostingConfig>, Self::Error>;

    /// Journal posting lines for one receipt journal, ordered.
    fn receipt_postings<'a>(
        &'a mut self,
        journal_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Vec<CapturePostingFact>, Self::Error>;

    /// Actual receipt lines for the selected fulfillments, ordered.
    fn receipt_lines<'a>(
        &'a mut self,
        fulfillment_ids: &'a [String],
    ) -> SupplierBillCommandFuture<'a, Vec<CaptureLineFact>, Self::Error>;

    /// Bill head for the decide path, company and frame scoped. `None`
    /// reads as the retained detail 404.
    fn bill<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<BillHead>, Self::Error>;

    /// Whether any bill line carries a price variance.
    fn bill_variance<'a>(
        &'a mut self,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Whether the account is an active expense account in the chart with
    /// matching functional currency.
    fn variance_account_ok<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        account_id: &'a str,
        chart_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Current Company matching policy, if any.
    fn policy<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PolicyHead>, Self::Error>;

    /// Whether the actor holds Company-wide Finance approver authority.
    fn company_authority<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Exact bill source image proof for a match id: the match row joined to
    /// its primary receipt and purchase order over the caller frame. Returns
    /// `None` when the source is unavailable in this scope.
    fn bill_image<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<serde_json::Value>, Self::Error>;

    /// Write-time receipt fence for the capture path, literal mirror of the
    /// retained per-receipt assertion (posted journal, expected version,
    /// uninvoiced fulfillment).
    fn verify_capture_receipts<'a>(
        &'a mut self,
        receipts: &'a [(String, String, i64)],
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Write-time review-state fence for the decide path, literal mirror of
    /// the retained bill-review assertion. Returns `false` instead of the
    /// exact row denial; the caller maps the miss to `SUPPLIER_BILL_CHANGED`.
    fn verify_decide_state<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
        bill_status: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Write-time policy-version fence, literal mirror of the retained
    /// policy-version assertion.
    fn verify_policy_version<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        current_version: i64,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    /// Insert the captured bill match row.
    fn insert_match<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        attributes: &'a CaptureAttributes,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Insert one captured bill receipt row.
    fn insert_match_receipt<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        index: usize,
        fulfillment_id: &'a str,
        expected_version: i64,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Insert captured bill lines in ordinal order.
    fn insert_match_lines<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        lines: &'a [CapturePreparedLine],
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Insert one bill event row. The id is the retained `bill-event-{key}`
    /// shape from the caller's command key.
    fn insert_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        match_id: &'a str,
        version: i64,
        action: &'a str,
        note: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Apply the decided status transition. Literal mirror of the retained
    /// decision update (no version CAS of its own: the verify fence owns
    /// currency, exactly like the retained assertion/update pair).
    fn apply_decision<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Deactivate the match receipts on return, literal mirror of the
    /// retained return update.
    fn deactivate_match_receipts<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Upsert the Company matching policy row.
    fn upsert_policy<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Insert one policy event row.
    fn insert_policy_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        previous: &'a str,
        reason: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        actor: &'a SupplierBillCommandActor,
        request_fingerprint: &'a str,
        record_id: &'a str,
        result_json: &'a str,
        authority_json: &'a str,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<SupplierBillReadback>, Self::Error>;
}

/// Cross-owner staging effects for the bill commands. Composition must supply
/// the Commerce receipt-version and invoice-link ports, the Finance draft-void
/// port and the Collaboration notification fan-out; every hook runs inside the
/// same caller-owned serializable transaction as the repository writes.
pub trait SupplierBillCommandStaging: Send {
    type Error: From<BusinessError> + Send;

    /// Commerce-owned receipt-version staging for one captured receipt.
    fn stage_receipt_version<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Commerce-owned invoice-link staging for the captured match.
    fn stage_receipt_links<'a>(
        &'a mut self,
        match_id: &'a str,
        invoice_id: Option<&'a str>,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Finance-owned unposted-AP draft-void staging for a returned match.
    fn stage_draft_void<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;

    /// Collaboration-owned notification fan-out for one bill event.
    fn notify_bill<'a>(
        &'a mut self,
        match_id: &'a str,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error>;
}

fn authority_matches(saved: &str, current: &SupplierBillAuthority) -> bool {
    serde_json::from_str::<SupplierBillAuthority>(saved)
        .is_ok_and(|saved| saved == *current)
}

pub(crate) fn replay(
    stored: Option<StoredSupplierBillReceipt>,
    actor: &SupplierBillCommandActor,
    record_id: &str,
    request_fingerprint: &str,
    operation: &str,
) -> Result<Option<SupplierBillRecordReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != operation
        || stored.actor_id != actor.user_id
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
    {
        return Err(idempotency_conflict());
    }
    let Some(authority) = stored.authority_json else {
        return Err(invalid_receipt());
    };
    if !authority_matches(&authority, &SupplierBillAuthority::from_actor(actor)) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
            "Your work role changed. Reload and review before retrying.",
        ));
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: SupplierBillRecordReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id || receipt.version < 1 {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn fingerprint(operation: &str, record_id: &str, input: &serde_json::Value) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    operation.hash(&mut hasher);
    record_id.hash(&mut hasher);
    input.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

async fn commit_evidence<R: SupplierBillCommandRepository>(
    repository: &mut R,
    actor: &SupplierBillCommandActor,
    plan: &SupplierBillCommitPlan,
    receipt: &SupplierBillRecordReceipt,
    request_fingerprint: &str,
    authority_json: &str,
) -> Result<(), R::Error> {
    if !repository
        .insert_receipt(
            plan.key.as_str(),
            &plan.operation,
            actor,
            request_fingerprint,
            &receipt.id,
            &serde_json::to_string(receipt).map_err(|_| invalid_receipt())?,
            authority_json,
            plan.now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    repository.audit(plan, receipt, authority_json).await?;
    repository.outbox(plan, receipt, authority_json).await?;
    Ok(())
}

fn receipt_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "RECEIPT_NOT_FOUND",
        "Goods receipt not found.",
    )
}

fn detail_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPLIER_BILL_NOT_FOUND",
        "Supplier invoice review not found in this scope.",
    )
}

/// Retained capture use case: validate, replay, prove the receipts, config
/// and postings, record the match with its receipts and lines, stage the
/// cross-owner effects, then persist the receipt/audit/outbox evidence and
/// validate the readback.
pub async fn capture_supplier_bill<R: SupplierBillCommandRepository, S: SupplierBillCommandStaging<Error = R::Error>>(
    actor: &SupplierBillCommandActor,
    repository: &mut R,
    staging: &mut S,
    input: &serde_json::Value,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, R::Error> {
    actor.require_capture(now).map_err(Into::into)?;
    let command = CaptureSupplierBillCommand::parse(input).map_err(Into::into)?;
    let validated = command.validate().map_err(Into::into)?;
    let match_id = format!("bill-match-{key}");
    let requested = requested_bill_receipts(
        validated.legacy_fulfillment_id.as_deref(),
        validated.legacy_expected_version,
        &validated.receipts,
        &validated.lines,
    )
    .map_err(Into::into)?;
    let request_fingerprint = fingerprint(OPERATION_CAPTURE, &match_id, input);
    if let Some(receipt) = replay(
        repository.receipt(key).await?,
        actor,
        &match_id,
        &request_fingerprint,
        OPERATION_CAPTURE,
    )? {
        return Ok(receipt);
    }
    // Receipt proofs, one per selected fulfillment, in request order.
    let mut facts = Vec::with_capacity(requested.len());
    for receipt in &requested {
        let fact = repository
            .capture_receipt(actor, &receipt.fulfillment_id)
            .await?
            .ok_or_else(receipt_not_found)?;
        if fact.version != receipt.expected_version {
            return Err(source_changed().into());
        }
        facts.push(fact);
    }
    // Order consolidation scope, literal mirror of `billReceiptScope`.
    let mut orders: Vec<CaptureOrderFact> = Vec::new();
    for fact in &facts {
        let order = repository
            .capture_order(&fact.order_id)
            .await?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Conflict,
                    "UNBILLED_PURCHASE_REQUIRED",
                    "Select posted, unbilled purchase receipts.",
                )
            })?;
        if !orders.iter().any(|known: &CaptureOrderFact| known.id == order.id) {
            orders.push(order);
        }
    }
    let primary = facts.first().ok_or_else(receipt_not_found)?;
    let primary_order = orders
        .iter()
        .find(|order| order.id == primary.order_id)
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "UNBILLED_PURCHASE_REQUIRED",
                "Select posted, unbilled purchase receipts.",
            )
        })?;
    if primary_order.kind != "purchase" || facts.iter().any(|fact| fact.invoice_id.is_some()) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "UNBILLED_PURCHASE_REQUIRED",
            "Select posted, unbilled purchase receipts.",
        )
        .into());
    }
    for fact in &facts {
        let order = orders.iter().find(|order| order.id == fact.order_id).ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "UNBILLED_PURCHASE_REQUIRED",
                "Select posted, unbilled purchase receipts.",
            )
        })?;
        if order.kind != "purchase"
            || order.party_id != primary_order.party_id
            || order.currency != primary_order.currency
            || order.branch_id != primary_order.branch_id
            || order.department_id != primary_order.department_id
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CONSOLIDATED_BILL_SCOPE_MISMATCH",
                "A consolidated supplier invoice must use the same supplier, currency, branch and department across its purchase orders.",
            )
            .into());
        }
    }
    let mut ordered: Vec<&CaptureReceiptFact> = facts.iter().collect();
    ordered.sort_by(|a, b| {
        a.fulfillment_date
            .cmp(&b.fulfillment_date)
            .then_with(|| a.id.cmp(&b.id))
    });
    let latest = ordered.last().ok_or_else(receipt_not_found)?;
    if command.invoice_date < latest.fulfillment_date || command.due_date < command.invoice_date {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "BILL_DATES_INVALID",
            "Invoice and due dates must follow every selected receipt.",
        )
        .into());
    }
    let config = repository.posting_config(actor).await?.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "BILL_POSTING_PROFILE_REQUIRED",
            "Review the Company posting profile before matching supplier invoices.",
        )
    })?;
    // Clearing source: every selected receipt posts through the same
    // reviewed clearing account.
    let mut clearing: Option<String> = None;
    for fact in ordered.iter() {
        let journal_id = fact.journal_id.as_deref().ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "BILL_POSTING_PROFILE_REQUIRED",
                "Post every selected receipt GL with a compatible Company posting profile first.",
            )
        })?;
        let postings = repository.receipt_postings(journal_id).await?;
        if fact.journal_status.as_deref() != Some("posted")
            || fact.journal_currency.as_deref() != Some(primary_order.currency.as_str())
            || fact.journal_chart_id.as_deref() != Some(config.chart_id.as_str())
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "BILL_POSTING_PROFILE_REQUIRED",
                "Post every selected receipt GL with a compatible Company posting profile first.",
            )
            .into());
        }
        let candidate = postings
            .iter()
            .find(|posting| posting.credit_minor > 0)
            .map(|posting| posting.account_id.clone());
        match (clearing.clone(), candidate) {
            (_, None) => {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "BILL_SOURCE_POSTING_INVALID",
                    "Selected receipts must use the same reviewed clearing account.",
                )
                .into());
            }
            (None, Some(account)) => clearing = Some(account),
            (Some(held), Some(account)) if held == account => {}
            _ => {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "BILL_SOURCE_POSTING_INVALID",
                    "Selected receipts must use the same reviewed clearing account.",
                )
                .into());
            }
        }
        if postings.len() != 2 {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "BILL_SOURCE_POSTING_INVALID",
                "Selected receipts must use the same reviewed clearing account.",
            )
            .into());
        }
    }
    let fulfillment_ids: Vec<String> = ordered.iter().map(|fact| fact.id.clone()).collect();
    let actual = repository.receipt_lines(&fulfillment_ids).await?;
    if validated.lines.len() != actual.len()
        || validated
            .lines
            .iter()
            .any(|line| !actual.iter().any(|row| row.id == line.fulfillment_line_id))
    {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "BILL_COMPLETE_RECEIPT_REQUIRED",
            "Match every line from every selected receipt exactly once.",
        )
        .into());
    }
    let mut prepared = Vec::with_capacity(validated.lines.len());
    for line in validated.lines.iter() {
        let row = actual
            .iter()
            .find(|row| row.id == line.fulfillment_line_id)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "BILL_LINE_INVALID",
                    "Use selected receipt lines and positive invoiced quantities.",
                )
            })?;
        let quantity = quantity_milli(&line.quantity).map_err(|_| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "BILL_LINE_INVALID",
                "Use selected receipt lines and positive invoiced quantities.",
            )
        })?;
        let price = decimal_to_minor(&line.price, 2).map_err(|_| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "BILL_LINE_INVALID",
                "Use selected receipt lines and positive invoiced quantities.",
            )
        })?;
        let (subtotal, tax) =
            crate::domain::supplier_bill_command::line_amounts(quantity, price, line.tax_bps)
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::InvalidInput,
                        "BILL_AMOUNT_INVALID",
                        "The supplier invoice must have a positive supported total.",
                    )
                })?;
        if subtotal <= 0 {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "BILL_LINE_INVALID",
                "Use selected receipt lines and positive invoiced quantities.",
            )
            .into());
        }
        prepared.push(CapturePreparedLine {
            fulfillment_line_id: row.id.clone(),
            quantity,
            expected_quantity: row.quantity_milli,
            price,
            tax_bps: line.tax_bps,
            expected_subtotal: row.subtotal_minor,
            subtotal,
            tax,
        });
    }
    let subtotal: i64 = prepared.iter().map(|row| row.subtotal).sum();
    let tax: i64 = prepared.iter().map(|row| row.tax).sum();
    let expected: i64 = actual.iter().map(|row| row.subtotal_minor).sum();
    let quantity_matches = prepared
        .iter()
        .all(|row| row.quantity == actual.iter().find(|line| line.id == row.fulfillment_line_id).map(|line| line.quantity_milli).unwrap_or(-1));
    let total = subtotal.checked_add(tax).unwrap_or(i64::MAX);
    if total <= 0
        || total > crate::domain::supplier_bill_command::MAX_SAFE
        || prepared.iter().any(|row| row.subtotal <= 0)
    {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "BILL_AMOUNT_INVALID",
            "The supplier invoice must have a positive supported total.",
        )
        .into());
    }
    let plan = SupplierBillCapturePlan {
        commit: SupplierBillCommitPlan {
            key: CommandKey::new(key).map_err(|_| invalid_receipt())?,
            operation: crate::domain::supplier_bill_command::OPERATION_CAPTURE.to_owned(),
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            branch_id: actor.branch_id.clone(),
            department_id: actor.department_id.clone(),
            now,
        },
        match_id: match_id.clone(),
        receipts: requested.clone(),
        now,
    };
    let clearing = clearing.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "BILL_SOURCE_POSTING_INVALID",
            "Selected receipts must use the same reviewed clearing account.",
        )
    })?;
    let attributes = CaptureAttributes {
        order_branch_id: primary_order
            .branch_id
            .clone()
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Conflict,
                    "UNBILLED_PURCHASE_REQUIRED",
                    "Select posted, unbilled purchase receipts.",
                )
            })?,
        order_department_id: primary_order.department_id.clone(),
        order_id: primary_order.id.clone(),
        fulfillment_id: primary.id.clone(),
        supplier_id: primary_order.party_id.clone(),
        currency: primary_order.currency.clone(),
        chart_id: config.chart_id.clone(),
        clearing_account_id: clearing,
        payable_account_id: config.payable_account_id.clone(),
        tax_account_id: config.input_tax_account_id.clone(),
        reference: command.reference.trim().to_owned(),
        invoice_date: command.invoice_date.clone(),
        due_date: command.due_date.clone(),
        evidence: command.evidence.trim().to_owned(),
        expected_subtotal: expected,
        subtotal,
        tax,
        quantity_matches,
        created_by: actor.user_id.clone(),
    };
    let mut fence: Vec<(String, String, i64)> = Vec::with_capacity(facts.len());
    for fact in ordered.iter() {
        let expected = requested
            .iter()
            .find(|row| row.fulfillment_id == fact.id)
            .map(|row| row.expected_version)
            .ok_or_else(receipt_not_found)?;
        fence.push((fact.id.clone(), fact.order_id.clone(), expected));
    }
    if !repository.verify_capture_receipts(&fence).await? {
        return Err(source_changed().into());
    }
    repository.insert_match(&plan, &attributes).await?;
    for (index, receipt) in plan.receipts.iter().enumerate() {
        repository
            .insert_match_receipt(&plan, index, &receipt.fulfillment_id, receipt.expected_version)
            .await?;
        staging.stage_receipt_version(&receipt.fulfillment_id).await?;
    }
    repository.insert_match_lines(&plan, &prepared).await?;
    repository
        .insert_event(&plan.commit, &match_id, 1, "capture", command.evidence.trim())
        .await?;
    staging.stage_receipt_links(&match_id, None).await?;
    staging.notify_bill(&match_id, key).await?;
    let receipt = SupplierBillRecordReceipt {
        id: match_id.clone(),
        version: 1,
        receipt_count: Some(plan.receipts.len() as i64),
        status: None,
        voided_invoice_id: None,
        matching_mode: None,
    };
    let authority_json =
        serde_json::to_string(&SupplierBillAuthority::from_actor(actor)).map_err(|_| invalid_receipt())?;
    commit_evidence(repository, actor, &plan.commit, &receipt, &request_fingerprint, &authority_json).await?;
    let readback = repository
        .readback(&actor.company_id, &match_id)
        .await?
        .ok_or_else(invalid_receipt)?;
    if readback.id != match_id || readback.version < 1 {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

/// Retained decide use case: approve or return one reviewed bill with the
/// independent-reviewer, state and variance proofs.
pub async fn decide_supplier_bill<R: SupplierBillCommandRepository, S: SupplierBillCommandStaging<Error = R::Error>>(
    actor: &SupplierBillCommandActor,
    repository: &mut R,
    staging: &mut S,
    match_id: &str,
    input: &serde_json::Value,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, R::Error> {
    actor.require_decide(now).map_err(Into::into)?;
    let command = DecideSupplierBillCommand::parse(input).map_err(Into::into)?;
    command.validate().map_err(Into::into)?;
    let operation = if command.action == "approve" {
        crate::domain::supplier_bill_command::OPERATION_APPROVE
    } else {
        crate::domain::supplier_bill_command::OPERATION_RETURN
    };
    let request_fingerprint = fingerprint(operation, match_id, input);
    if let Some(receipt) = replay(
        repository.receipt(key).await?,
        actor,
        match_id,
        &request_fingerprint,
        operation,
    )? {
        return Ok(receipt);
    }
    let bill = repository.bill(actor, match_id).await?.ok_or_else(detail_not_found)?;
    assert_independent_review(actor, &bill.created_by, bill.invoice_created_by.as_deref())
        .map_err(Into::into)?;
    let return_draft = command.action == "return"
        && bill.status == "invoiced"
        && bill.invoice_status.as_deref() == Some("draft");
    assert_bill_state(&command.action, &bill.status, return_draft).map_err(Into::into)?;
    let variance = if command.action == "approve" {
        repository.bill_variance(match_id).await?
    } else {
        false
    };
    if command.action == "approve" {
        if !bill.quantity_matches {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "BILL_QUANTITY_VARIANCE",
                "Invoiced quantities differ from the selected receipts. Return the invoice for correction; do not approve unreceived quantities.",
            )
            .into());
        }
        if variance {
            let account = command.variance_account_id.as_deref().ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "PRICE_VARIANCE_ACCOUNT_REQUIRED",
                    "Choose the Company-approved expense account for purchase price variance.",
                )
            })?;
            if !repository.variance_account_ok(actor, account, &bill.chart_id).await? {
                return Err(BusinessError::new(
                    FailureKind::InvalidInput,
                    "PRICE_VARIANCE_ACCOUNT_INVALID",
                    "Use an active expense account in this Company chart's functional currency.",
                )
                .into());
            }
        }
    }
    let variance_account_id = if command.action == "approve" && variance {
        Some(
            command
                .variance_account_id
                .clone()
                .unwrap_or_else(|| bill.variance_account_id.clone().unwrap_or_default()),
        )
    } else {
        bill.variance_account_id.clone()
    };
    let plan = SupplierBillDecidePlan {
        commit: SupplierBillCommitPlan {
            key: CommandKey::new(key).map_err(|_| invalid_receipt())?,
            operation: operation.to_owned(),
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            branch_id: actor.branch_id.clone(),
            department_id: actor.department_id.clone(),
            now,
        },
        match_id: match_id.to_owned(),
        action: command.action.clone(),
        expected_version: command.expected_version,
        variance_account_id,
        reason: command.reason.trim().to_owned(),
        now,
    };
    if !repository.verify_decide_state(&plan, &bill.status).await? {
        return Err(source_changed().into());
    }
    repository.apply_decision(&plan).await?;
    if command.action == "return" {
        repository.deactivate_match_receipts(&plan).await?;
    }
    if return_draft {
        staging.stage_draft_void(&plan).await?;
        staging.stage_receipt_links(match_id, None).await?;
    }
    let event_action = if return_draft { "return-ap-draft" } else { command.action.as_str() };
    repository
        .insert_event(
            &plan.commit,
            &match_id,
            command.expected_version + 1,
            event_action,
            &command.reason,
        )
        .await?;
    staging.notify_bill(match_id, key).await?;
    let receipt = SupplierBillRecordReceipt {
        id: match_id.to_owned(),
        version: command.expected_version + 1,
        receipt_count: None,
        status: Some(if command.action == "approve" {
            "approved".to_owned()
        } else {
            "returned".to_owned()
        }),
        voided_invoice_id: if return_draft {
            bill.invoice_id.clone()
        } else {
            None
        },
        matching_mode: None,
    };
    let authority_json =
        serde_json::to_string(&SupplierBillAuthority::from_actor(actor)).map_err(|_| invalid_receipt())?;
    commit_evidence(repository, actor, &plan.commit, &receipt, &request_fingerprint, &authority_json).await?;
    let readback = repository
        .readback(&actor.company_id, match_id)
        .await?
        .ok_or_else(invalid_receipt)?;
    if readback.id != match_id || readback.version != command.expected_version + 1 {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

/// Retained policy use case: the Company matching-policy decision with the
/// approver/owner proof and optimistic version CAS.
pub async fn save_supplier_bill_policy<R: SupplierBillCommandRepository>(
    actor: &SupplierBillCommandActor,
    repository: &mut R,
    input: &serde_json::Value,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, R::Error> {
    actor.require_policy(now).map_err(Into::into)?;
    let command = SaveSupplierBillPolicyCommand::parse(input).map_err(Into::into)?;
    command.validate().map_err(Into::into)?;
    let record_id = actor.company_id.as_str().to_owned();
    let request_fingerprint = fingerprint(OPERATION_POLICY, &record_id, input);
    if let Some(receipt) = replay(
        repository.receipt(key).await?,
        actor,
        &record_id,
        &request_fingerprint,
        OPERATION_POLICY,
    )? {
        return Ok(receipt);
    }
    if !repository.company_authority(actor, now).await? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_FINANCE_AUTHORITY_REQUIRED",
            "A company-wide Finance approver or owner must change supplier invoice matching policy.",
        )
        .into());
    }
    let current = repository.policy(actor).await?;
    let current_version = current.as_ref().map(|policy| policy.version).unwrap_or(0);
    let current_mode = current.as_ref().map(|policy| policy.matching_mode.as_str());
    if command.expected_version != current_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "STALE_POLICY",
            "The supplier invoice policy changed. Reload and review its current version.",
        )
        .into());
    }
    if current_mode == Some(command.matching_mode.as_str()) {
        if command.matching_mode == "required" {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "POLICY_UNCHANGED",
                "Supplier invoice matching is already required.",
            )
            .into());
        }
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "POLICY_UNCHANGED",
            "Supplier invoice matching is already optional.",
        )
        .into());
    }
    let plan = SupplierBillPolicyPlan {
        commit: SupplierBillCommitPlan {
            key: CommandKey::new(key).map_err(|_| invalid_receipt())?,
            operation: OPERATION_POLICY.to_owned(),
            company_id: actor.company_id.clone(),
            actor_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            branch_id: actor.branch_id.clone(),
            department_id: actor.department_id.clone(),
            now,
        },
        matching_mode: command.matching_mode.clone(),
        expected_version: command.expected_version,
        now,
    };
    if !repository.verify_policy_version(&plan, current_version).await? {
        return Err(source_changed().into());
    }
    repository.upsert_policy(&plan).await?;
    repository
        .insert_policy_event(&plan, current_mode.unwrap_or("optional"), &command.reason)
        .await?;
    let receipt = SupplierBillRecordReceipt {
        id: record_id.clone(),
        version: current_version + 1,
        receipt_count: None,
        status: None,
        voided_invoice_id: None,
        matching_mode: Some(command.matching_mode.clone()),
    };
    let authority_json =
        serde_json::to_string(&SupplierBillAuthority::from_actor(actor)).map_err(|_| invalid_receipt())?;
    commit_evidence(repository, actor, &plan.commit, &receipt, &request_fingerprint, &authority_json).await?;
    Ok(receipt)
}

/// Read-only preconditions shared by the decide path validation. Kept beside
/// the use cases so reviewers see the exact denial mapping in one place.
pub fn assert_independent_review(
    actor: &SupplierBillCommandActor,
    created_by: &str,
    invoice_created_by: Option<&str>,
) -> Result<(), BusinessError> {
    if actor.same_person(created_by)
        || invoice_created_by.is_some_and(|id| actor.same_person(id))
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_BILL_REVIEW_REQUIRED",
            "A different Finance reviewer must accept or return the supplier invoice and its AP draft.",
        ));
    }
    Ok(())
}

pub fn assert_bill_state(action: &str, status: &str, return_draft: bool) -> Result<(), BusinessError> {
    if status != "review" && !(action == "return" && status == "approved") && !return_draft {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "BILL_REVIEW_REQUIRED",
            "Only unposted supplier invoices can be returned; posted invoices require the source credit workflow.",
        ));
    }
    Ok(())
}

const OPERATION_CAPTURE: &str = crate::domain::supplier_bill_command::OPERATION_CAPTURE;
const OPERATION_POLICY: &str = crate::domain::supplier_bill_command::OPERATION_POLICY;
