//! Native PMS booking-refund orchestration.
//!
//! The repository owns only PMS rows under the caller's transaction: booking
//! payment/request/attempt/refund rows and the guest-account records the refund
//! moves. Provider authority, property purchase, mail delivery and Finance
//! journal/document posting are caller ports; this owner never mints a provider
//! result, a Company, a permission or a journal.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::refund::{
    MAX_REFUND_ROWS, PmsPosting, PmsProviderReceipt, PmsProviderRefund, PmsRefundAccount,
    PmsRefundDocument, PmsRefundEffects, PmsRefundFinanceRow, PmsRefundPart, PmsRefundPayment,
    PmsRefundPrior, PmsRefundRequest, PmsRefundResult, PmsRefundSelection, PmsRefundSource,
    account_facts, apply_refund_liability, apply_refund_settle, apply_unallocated_receipt,
    conflict, forbidden, invalid, not_found, parse_parts, parts_json, plan_refund_request,
    require_finance_scope, require_independent_approval, require_online_accounting,
    require_unallocated_receipt, require_verified_receipt, reserve_refund, unavailable,
    valid_reason,
};
use crate::domain::room_board::{PmsBoardActor, PmsRecord, record_not_found};
use erpx_server_shared::public::BusinessError;

/// One verified provider refund result.
pub trait PmsRefundProvider: Send + Sync {
    /// Query one provider refund for the original payment.
    fn provider_refund<'a>(
        &'a self,
        payment: &'a PmsRefundPayment,
        refund_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsProviderRefund, BusinessError>;
    /// Create one provider refund for an approved request.
    fn create_refund<'a>(
        &'a self,
        payment: &'a PmsRefundPayment,
        request: &'a PmsRefundRequest,
        part: &'a [PmsRefundPart],
        promptpay: bool,
        email: &'a str,
        idempotency_key: &'a str,
    ) -> PmsBoardFuture<'a, PmsProviderRefund, BusinessError>;
    /// Whether the original payment method is a PromptPay instruction refund.
    fn promptpay<'a>(
        &'a self,
        payment: &'a PmsRefundPayment,
    ) -> PmsBoardFuture<'a, bool, BusinessError>;
    /// Paid Checkout session and PaymentIntent facts of the original payment.
    fn receipt<'a>(
        &'a self,
        payment: &'a PmsRefundPayment,
    ) -> PmsBoardFuture<'a, PmsProviderReceipt, BusinessError>;
    /// Contact e-mail of the reservation guest, when one is retained.
    fn guest_email<'a>(
        &'a self,
        payment: &'a PmsRefundPayment,
    ) -> PmsBoardFuture<'a, String, BusinessError>;
}

/// Property/Company purchase authority for settle-existing work.
pub trait PmsRefundPurchaseGuard: Send + Sync {
    fn require_settle_existing<'a>(
        &'a self,
        property_id: &'a str,
        permission: Option<&'a str>,
        now: i64,
    ) -> PmsBoardFuture<'a, (), BusinessError>;
}

/// Refund mail queue side effect (the native mail producer owns the row).
pub trait PmsRefundMail: Send + Sync {
    fn queue_refund<'a>(
        &'a self,
        reservation_id: &'a str,
        holding_slug: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), BusinessError>;
}

pub trait PmsRefundRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Lock and load one booking payment of the property.
    fn payment<'a>(
        &'a mut self,
        payment_id: &'a str,
        property_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error>;
    fn payment_by_id<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error>;
    fn request<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error>;
    fn locked_request<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundRequest>, Self::Error>;
    fn sources<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundSource>, Self::Error>;
    fn previous<'a>(
        &'a mut self,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundPrior>, Self::Error>;
    fn account<'a>(
        &'a mut self,
        property_id: &'a str,
        account_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error>;
    fn entries<'a>(
        &'a mut self,
        property_id: &'a str,
        guest_account_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRecord>, Self::Error>;
    fn property<'a>(
        &'a mut self,
        property_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundProperty>, Self::Error>;
    fn save_account<'a>(
        &'a mut self,
        account: &'a PmsRecord,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn insert_entry<'a>(
        &'a mut self,
        property_id: &'a str,
        entry: &'a PmsRecord,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn insert_request<'a>(
        &'a mut self,
        request: &'a PmsRefundRequest,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn mark_payment_refund<'a>(
        &'a mut self,
        payment_id: &'a str,
        state: &'a str,
        requested_at: Option<i64>,
        approved_by: Option<&'a str>,
        refund_id: Option<&'a str>,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn update_request<'a>(
        &'a mut self,
        request_id: &'a str,
        state: &'a str,
        approved_by: Option<&'a str>,
        provider_id: Option<&'a str>,
        last_error: Option<&'a str>,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn insert_attempt<'a>(
        &'a mut self,
        request_id: &'a str,
        generation: i64,
        provider_id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    /// Set the provider refund on the request and payment exactly once.
    fn bind_provider_refund<'a>(
        &'a mut self,
        request_id: &'a str,
        payment_id: &'a str,
        provider_id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    fn finance_rows<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundFinanceRow>, Self::Error>;
    /// Requests awaiting provider settlement for one request id or payment id.
    fn refundable_requests<'a>(
        &'a mut self,
        request_id: &'a str,
    ) -> PmsBoardFuture<'a, Vec<PmsRefundRequest>, Self::Error>;
    /// Approved/requested refund row of one Company by request or payment id.
    fn approval_target<'a>(
        &'a mut self,
        id: &'a str,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<(String, String)>, Self::Error>;
    /// Original unassigned receipt awaiting Finance recovery.
    fn recovery_payment<'a>(
        &'a mut self,
        request_id: &'a str,
        company_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRefundPayment>, Self::Error>;
    /// Guest account of the reservation stay, when one is retained.
    fn stay_account<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<String>, Self::Error>;
    /// Entry tagged `online-payment:<payment>` for the original payment.
    fn receipt_entry<'a>(
        &'a mut self,
        property_id: &'a str,
        payment_id: &'a str,
    ) -> PmsBoardFuture<'a, Option<PmsRecord>, Self::Error>;
    /// Retain the verified provider success time and PaymentIntent of the
    /// recovered original receipt.
    fn mark_receipt_recovered<'a>(
        &'a mut self,
        payment_id: &'a str,
        provider_intent_id: &'a str,
        succeeded_at: i64,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    /// Append one reservation history line and bump its version.
    fn append_reservation_history<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
        actor_id: &'a str,
        action: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;
    /// `set_config('erpx.pms_write','on',true)` for the write-scoped unit.
    fn write_context<'a>(&'a mut self) -> PmsBoardFuture<'a, (), Self::Error>;
}

/// Property facts a refund step needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsRefundProperty {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub config_json: String,
}

/// Finance effects the caller must post after the repository commit.
#[derive(Clone, Debug, Default)]
pub struct PmsRefundEffectsOut {
    pub postings: Vec<PmsPosting>,
    pub documents: Vec<PmsRefundDocument>,
}

impl PmsRefundEffectsOut {
    pub fn from(effects: PmsRefundEffects) -> Self {
        Self {
            postings: effects.postings,
            documents: effects.documents,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PmsRefundOutcome {
    pub result: PmsRefundResult,
    pub effects: PmsRefundEffectsOut,
}

fn truncated_reason(reason: &str) -> String {
    reason.trim().chars().take(500).collect()
}

/// Reserve original receipt funding and record one refund request.
#[allow(clippy::too_many_arguments)]
pub async fn request_refund<R>(
    repository: &mut R,
    payment_id: &str,
    property_id: &str,
    wanted: &[PmsRefundSelection],
    reason: &str,
    requested_by: &str,
    request_id: &str,
    now: i64,
) -> Result<PmsRefundResult, R::Error>
where
    R: PmsRefundRepository,
{
    let payment = repository
        .payment(payment_id, property_id)
        .await?
        .ok_or_else(|| not_found(false))?;
    let prior = repository.request(request_id).await?;
    let sources = repository.sources(payment_id).await?;
    let previous = repository.previous(payment_id).await?;
    let parts = plan_refund_request(
        &payment,
        prior.as_ref(),
        &sources,
        &previous,
        wanted,
        reason,
        requested_by,
        request_id,
    )?;
    if prior.is_some() {
        return Ok(PmsRefundResult {
            id: request_id.to_owned(),
            state: prior
                .map(|value| value.state)
                .unwrap_or_else(|| "requested".to_owned()),
            amount_minor: Some(parts.iter().map(|part| part.amount).sum()),
            version: None,
        });
    }
    let amount: i64 = parts.iter().map(|part| part.amount).sum();
    let mut account_ids: Vec<&str> = Vec::new();
    for part in &parts {
        if !account_ids.iter().any(|value| value == &part.guest_account_id.as_str()) {
            account_ids.push(&part.guest_account_id);
        }
    }
    for account_id in account_ids {
        let mut account = repository
            .account(property_id, account_id)
            .await?
            .ok_or_else(record_not_found)?;
        for part in parts
            .iter()
            .filter(|part| part.guest_account_id == *account_id)
        {
            reserve_refund(&mut account, "advance", part.amount, &part.entry_id, now)?;
        }
        repository.save_account(&account).await?;
    }
    let request = PmsRefundRequest {
        id: request_id.to_owned(),
        payment_id: payment_id.to_owned(),
        amount_minor: amount,
        allocations_json: parts_json(&parts)?,
        reason: reason.to_owned(),
        requested_by: requested_by.to_owned(),
        approved_by: None,
        provider_id: None,
        state: "requested".to_owned(),
        generation: 0,
        legacy_review: false,
    };
    repository.insert_request(&request, now).await?;
    repository
        .mark_payment_refund(payment_id, "requested", Some(now), None, None, now)
        .await?;
    Ok(PmsRefundResult {
        id: request_id.to_owned(),
        state: "requested".to_owned(),
        amount_minor: Some(amount),
        version: None,
    })
}

/// Record the guest-account refund liability after an independent approval.
pub async fn approve_refund<R, G>(
    repository: &mut R,
    guard: &G,
    actor: &PmsBoardActor,
    id: &str,
    now: i64,
) -> Result<PmsRefundOutcome, R::Error>
where
    R: PmsRefundRepository,
    G: PmsRefundPurchaseGuard,
{
    actor.require_action("pms.approve", now)?;
    if actor.branch_id.is_none() || actor.department_id.is_some() {
        return Err(R::Error::from(forbidden(
            "PMS_BRANCH_REQUIRED",
            "Choose a Company or Branch.",
        )));
    }
    let (request_id, property_id) = repository
        .approval_target(id, &actor.company_id)
        .await?
        .ok_or_else(|| {
            conflict(
                "PMS_REFUND_NOT_REQUESTED",
                "No refund request awaits approval",
            )
        })?;
    guard
        .require_settle_existing(&property_id, Some("pms.approve"), now)
        .await?;
    let property = repository
        .property(&property_id)
        .await?
        .ok_or_else(|| not_found(false))?;
    if property.company_id != actor.company_id
        || actor
            .branch_id
            .as_deref()
            .is_some_and(|branch| branch != property.branch_id)
    {
        return Err(R::Error::from(not_found(false)));
    }
    let request = repository
        .locked_request(&request_id)
        .await?
        .ok_or_else(|| {
            conflict(
                "PMS_REFUND_NOT_REQUESTED",
                "No refund request awaits approval",
            )
        })?;
    if request.state != "requested" {
        return Err(R::Error::from(conflict(
            "PMS_INDEPENDENT_REFUND_APPROVAL",
            "A different PMS manager must approve the refund",
        )));
    }
    require_independent_approval(&actor.user_id, &request.requested_by)?;
    require_online_accounting(&property.config_json)?;
    let payment = repository
        .payment_by_id(&request.payment_id)
        .await?
        .ok_or_else(record_not_found)?;
    if payment.company_id != actor.company_id {
        return Err(R::Error::from(not_found(false)));
    }
    let parts = parse_parts(&request.allocations_json)?;
    let mut effects = PmsRefundEffects::default();
    if parts.is_empty() {
        // A legacy review row without reserved parts still needs the recovered
        // original unallocated receipt before Finance may approve it.
        let entries = match repository
            .stay_account(&property_id, &payment.reservation_id)
            .await?
        {
            Some(account_id) => repository.entries(&property_id, &account_id).await?,
            None => Vec::new(),
        };
        require_unallocated_receipt(&parts, &entries, &request.payment_id)?;
    }
    let mut accounts: Vec<PmsRecord> = Vec::new();
    for part in &parts {
        let mut account = match accounts
            .iter()
            .position(|account| account.id == part.guest_account_id)
        {
            Some(index) => accounts.remove(index),
            None => repository
                .account(&property_id, &part.guest_account_id)
                .await?
                .ok_or_else(record_not_found)?,
        };
        if request.legacy_review {
            reserve_refund(&mut account, "advance", part.amount, &part.entry_id, now)?;
        }
        let entry = apply_refund_liability(
            &mut account,
            part,
            &request.id,
            &property.business_date,
            &actor.user_id,
            now,
            &mut effects,
        )?;
        repository.save_account(&account).await?;
        accounts.push(account);
        repository.insert_entry(&property.id, &entry).await?;
    }
    repository
        .update_request(
            &request.id,
            "approved",
            Some(&actor.user_id),
            None,
            None,
            now,
        )
        .await?;
    repository
        .mark_payment_refund(
            &request.payment_id,
            "approved",
            None,
            Some(&actor.user_id),
            None,
            now,
        )
        .await?;
    Ok(PmsRefundOutcome {
        result: PmsRefundResult {
            id: request.id,
            state: "approved".to_owned(),
            amount_minor: Some(request.amount_minor),
            version: None,
        },
        effects: PmsRefundEffectsOut::from(effects),
    })
}

/// Company Finance review list. Branch and department workspaces see nothing.
pub async fn refunds_for_finance<R>(
    repository: &mut R,
    actor: &PmsBoardActor,
    now: i64,
) -> Result<Vec<PmsRefundFinanceRow>, R::Error>
where
    R: PmsRefundRepository,
{
    require_finance_scope(actor, now)?;
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Ok(Vec::new());
    }
    let rows = repository.finance_rows(&actor.company_id).await?;
    if rows.len() > MAX_REFUND_ROWS {
        return Err(R::Error::from(unavailable(
            "PMS_REFUND_READ_LIMIT",
            "There are more refunds than this view can read.",
        )));
    }
    Ok(rows)
}

/// Settle approved provider refunds and post the PMS refund entries.
pub async fn settle_refund<R, P, M, G>(
    repository: &mut R,
    provider: &P,
    purchase: &G,
    mail: &M,
    request_id: &str,
    now: i64,
) -> Result<Vec<PmsRefundOutcome>, R::Error>
where
    R: PmsRefundRepository,
    P: PmsRefundProvider,
    M: PmsRefundMail,
    G: PmsRefundPurchaseGuard,
{
    let requests = repository.refundable_requests(request_id).await?;
    let mut outcomes = Vec::new();
    for request in requests {
        let payment = repository
            .payment_by_id(&request.payment_id)
            .await?
            .ok_or_else(record_not_found)?;
        if payment.checkout_id.is_none() {
            return Err(R::Error::from(conflict(
                "PMS_REFUND_PROVIDER_MISSING",
                "Original Checkout session is missing",
            )));
        }
        purchase
            .require_settle_existing(&payment.property_id, None, now)
            .await?;
        let mut provider_id = request.provider_id.clone();
        if provider_id.is_none() {
            let parts = parse_parts(&request.allocations_json)?;
            let promptpay = provider.promptpay(&payment).await?;
            let email = if promptpay {
                provider.guest_email(&payment).await?
            } else {
                String::new()
            };
            let key = format!("pms-refund-{}-{}", request.id, request.generation);
            let created = provider
                .create_refund(&payment, &request, &parts, promptpay, &email, &key)
                .await?;
            provider_id = Some(created.id.clone());
            repository
                .insert_attempt(&request.id, request.generation, &created.id, now)
                .await?;
            repository
                .bind_provider_refund(&request.id, &payment.id, &created.id, now)
                .await?;
        }
        let Some(refund_id) = provider_id else {
            return Err(R::Error::from(conflict(
                "PMS_REFUND_PROVIDER_MISSING",
                "Original Checkout session is missing",
            )));
        };
        let current = provider.provider_refund(&payment, &refund_id).await?;
        crate::domain::refund::require_provider_refund(
            request.amount_minor,
            &current.status,
            &payment.id,
            &request.id,
            &current,
        )?;
        if current.failed() {
            repository
                .update_request(
                    &request.id,
                    "failed",
                    None,
                    None,
                    Some("Provider refund failed; liability remains outstanding"),
                    now,
                )
                .await?;
            repository
                .mark_payment_refund(&payment.id, "failed", None, None, None, now)
                .await?;
            continue;
        }
        if !current.succeeded() {
            let state = if current.status == "requires_action" {
                "requires_action"
            } else {
                "processing"
            };
            repository
                .update_request(&request.id, state, None, None, None, now)
                .await?;
            continue;
        }
        let property = repository
            .property(&payment.property_id)
            .await?
            .ok_or_else(|| not_found(false))?;
        let current = repository
            .locked_request(&request.id)
            .await?
            .ok_or_else(record_not_found)?;
        if current.state == "succeeded" {
            continue;
        }
        if !matches!(current.state.as_str(), "processing" | "requires_action") {
            return Err(R::Error::from(conflict(
                "PMS_REFUND_STATE",
                "Refund state changed",
            )));
        }
        repository.write_context().await?;
        let mut effects = PmsRefundEffects::default();
        let mut parts = parse_parts(&current.allocations_json)?;
        if parts.is_empty() {
            // A legacy review row settles the recovered unallocated receipt.
            let account_id = repository
                .stay_account(&payment.property_id, &payment.reservation_id)
                .await?
                .ok_or_else(|| {
                    conflict(
                        "PMS_UNALLOCATED_RECEIPT_MISSING",
                        "Unallocated receipt requires recovery",
                    )
                })?;
            let receipt = repository
                .receipt_entry(&payment.property_id, &payment.id)
                .await?
                .filter(|entry| {
                    entry
                        .data
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        == Some("payment.unallocated")
                })
                .ok_or_else(|| {
                    conflict(
                        "PMS_UNALLOCATED_RECEIPT_MISSING",
                        "Unallocated receipt requires recovery",
                    )
                })?;
            parts.push(PmsRefundPart {
                stay_id: String::new(),
                guest_account_id: account_id,
                entry_id: receipt.id,
                amount: current.amount_minor,
            });
        }
        for part in parts {
            let entry = apply_refund_settle(
                &part,
                &request.id,
                &refund_id,
                &property.business_date,
                "system:stripe",
                now,
                &mut effects,
            )?;
            repository
                .insert_entry(&payment.property_id, &entry)
                .await?;
        }
        repository
            .update_request(&request.id, "succeeded", None, None, None, now)
            .await?;
        repository
            .mark_payment_refund(&payment.id, "succeeded", None, None, None, now)
            .await?;
        mail.queue_refund(&payment.reservation_id, &payment.holding_slug, now)
            .await?;
        outcomes.push(PmsRefundOutcome {
            result: PmsRefundResult {
                id: request.id,
                state: "succeeded".to_owned(),
                amount_minor: Some(request.amount_minor),
                version: None,
            },
            effects: PmsRefundEffectsOut::from(effects),
        });
    }
    Ok(outcomes)
}

/// Recover the original unallocated receipt before Finance approves a legacy
/// refund review row. Never collects money again.
pub async fn recover_receipt<R, P, G>(
    repository: &mut R,
    provider: &P,
    purchase: &G,
    actor: &PmsBoardActor,
    request_id: &str,
    reason: &str,
    now: i64,
) -> Result<PmsRefundOutcome, R::Error>
where
    R: PmsRefundRepository,
    P: PmsRefundProvider,
    G: PmsRefundPurchaseGuard,
{
    actor.require_action("finance.approve", now)?;
    if actor.branch_id.is_some() || actor.department_id.is_some() || !valid_reason(reason) {
        return Err(R::Error::from(invalid(
            "PMS_RECEIPT_RECOVERY_INPUT",
            "Use company Finance and enter a recovery reason",
        )));
    }
    let payment = repository
        .recovery_payment(request_id, &actor.company_id)
        .await?
        .ok_or_else(|| {
            conflict(
                "PMS_RECEIPT_RECOVERY_UNAVAILABLE",
                "Select an original unassigned payment awaiting Finance review",
            )
        })?;
    if payment.checkout_id.is_none() {
        return Err(R::Error::from(conflict(
            "PMS_RECEIPT_RECOVERY_UNAVAILABLE",
            "Select an original unassigned payment awaiting Finance review",
        )));
    }
    purchase
        .require_settle_existing(&payment.property_id, Some("finance.approve"), now)
        .await?;
    let property = repository
        .property(&payment.property_id)
        .await?
        .ok_or_else(|| not_found(false))?;
    if property.company_id != actor.company_id {
        return Err(R::Error::from(not_found(false)));
    }
    let verified = provider.receipt(&payment).await?;
    require_verified_receipt(&payment, &verified)?;
    repository.write_context().await?;
    let mut effects = PmsRefundEffects::default();
    let parts = parse_parts(
        &repository
            .request(request_id)
            .await?
            .map(|request| request.allocations_json)
            .unwrap_or_else(|| "[]".to_owned()),
    )?;
    if let Some(existing) = repository
        .receipt_entry(&payment.property_id, &payment.id)
        .await?
    {
        let description = existing
            .data
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if description != "payment.unallocated" {
            return Err(R::Error::from(conflict(
                "PMS_RECEIPT_RECOVERY_UNAVAILABLE",
                "Recover the original allocated receipt through its owner",
            )));
        }
        return Ok(PmsRefundOutcome {
            result: PmsRefundResult {
                id: request_id.to_owned(),
                state: "receipt_recovered".to_owned(),
                amount_minor: None,
                version: None,
            },
            effects: PmsRefundEffectsOut::default(),
        });
    }
    let guest_account_id = if parts.is_empty() {
        repository
            .stay_account(&payment.property_id, &payment.reservation_id)
            .await?
    } else {
        Some(parts[0].guest_account_id.clone())
    };
    let Some(guest_account_id) = guest_account_id else {
        return Err(R::Error::from(conflict(
            "PMS_RECEIPT_TARGET_MISSING",
            "The original booking needs owner review",
        )));
    };
    let entry = apply_unallocated_receipt(
        &guest_account_id,
        &payment.id,
        payment.amount_minor,
        &verified.payment_intent_id,
        &property.business_date,
        "system:stripe",
        now,
        &mut effects,
    );
    repository
        .insert_entry(&payment.property_id, &entry)
        .await?;
    repository
        .append_reservation_history(
            &payment.property_id,
            &payment.reservation_id,
            &actor.user_id,
            &format!(
                "Recovered original unallocated receipt: {}",
                truncated_reason(reason)
            ),
            now,
        )
        .await?;
    repository
        .mark_receipt_recovered(
            &payment.id,
            &verified.payment_intent_id,
            verified.succeeded_at,
            now,
        )
        .await?;
    Ok(PmsRefundOutcome {
        result: PmsRefundResult {
            id: request_id.to_owned(),
            state: "receipt_recovered".to_owned(),
            amount_minor: Some(payment.amount_minor),
            version: None,
        },
        effects: PmsRefundEffectsOut::from(effects),
    })
}

/// Approve one retry only after the provider confirms the previous refund
/// failed or was cancelled.
pub async fn retry_refund<R, P, G>(
    repository: &mut R,
    provider: &P,
    purchase: &G,
    actor: &PmsBoardActor,
    id: &str,
    reason: &str,
    now: i64,
) -> Result<PmsRefundResult, R::Error>
where
    R: PmsRefundRepository,
    P: PmsRefundProvider,
    G: PmsRefundPurchaseGuard,
{
    actor.require_action("pms.approve", now)?;
    if actor.department_id.is_some() || !valid_reason(reason) {
        return Err(R::Error::from(invalid(
            "PMS_REFUND_RETRY_INPUT",
            "Use PMS and enter a recovery reason",
        )));
    }
    let request = repository
        .locked_request(id)
        .await?
        .ok_or_else(record_not_found)?;
    let payment = repository
        .payment_by_id(&request.payment_id)
        .await?
        .ok_or_else(record_not_found)?;
    if payment.company_id != actor.company_id {
        return Err(R::Error::from(not_found(false)));
    }
    let property_id = payment.property_id.clone();
    let Some(provider_id) = request.provider_id.clone() else {
        return Err(R::Error::from(conflict(
            "PMS_REFUND_RETRY_UNAVAILABLE",
            "An independent PMS reviewer must select a failed provider refund",
        )));
    };
    if request.state != "failed" {
        return Err(R::Error::from(conflict(
            "PMS_REFUND_RETRY_UNAVAILABLE",
            "An independent PMS reviewer must select a failed provider refund",
        )));
    }
    require_independent_approval(&actor.user_id, &request.requested_by)?;
    purchase
        .require_settle_existing(&property_id, Some("pms.approve"), now)
        .await?;
    let property = repository
        .property(&property_id)
        .await?
        .ok_or_else(|| not_found(false))?;
    if property.company_id != actor.company_id
        || actor
            .branch_id
            .as_deref()
            .is_some_and(|branch| branch != property.branch_id)
    {
        return Err(R::Error::from(not_found(false)));
    }
    let provider_state = provider.provider_refund(&payment, &provider_id).await?;
    if !provider_state.failed() {
        return Err(R::Error::from(conflict(
            "PMS_REFUND_PROVIDER_PENDING",
            "Recover the current provider refund without creating another",
        )));
    }
    repository
        .insert_attempt(&request.id, request.generation, &provider_id, now)
        .await?;
    repository
        .update_request(
            &request.id,
            "approved",
            None,
            None,
            Some(&format!(
                "Recovery approved by {}: {}",
                actor.user_id,
                truncated_reason(reason)
            )),
            now,
        )
        .await?;
    repository
        .mark_payment_refund(&request.payment_id, "approved", None, None, None, now)
        .await?;
    Ok(PmsRefundResult {
        id: request.id,
        state: "approved".to_owned(),
        amount_minor: Some(request.amount_minor),
        version: None,
    })
}

/// Read the current guest-account refund facts (used by the approval view).
pub fn account_refund_facts(record: &PmsRecord) -> Result<PmsRefundAccount, BusinessError> {
    account_facts(record)
}
