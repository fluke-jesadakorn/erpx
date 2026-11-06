//! Native POS register-session, bill, void and refund commands.
//!
//! The repository owns POS tables only. Finance journals, periods, parties,
//! Commerce/Inventory stock and PMS room-charge facts are explicit ports; no
//! cross-owner SQL is constructed here.
#![allow(clippy::too_many_arguments)]
use crate::domain::commands::{
    CashApproveInput, PaymentCreateInput, PosActor, PosCashMovement, PosCatalogSnapshot, PosCommand,
    PosCommandInput, PosFulfillment, PosFulfillmentInsert, PosFulfillmentSplitFacts,
    PosFulfillmentStock, PosJournalLine, PosJournalRequest, PosLine, PosLineDetail,
    PosLineDiscountRow, PosLineInsert, PosLineSplitRow, PosOperation, PosPaymentInsert, PosReceipt,
    PosReceiptRequest, PosRefund, PosRegister, PosResult, PosSale, PosSettings, PosShift, PosStockRow,
    RefundRejectInput, RefundRequestInput, SaleAddInput, SaleCreateInput, SaleCustomerInput,
    SaleDiscountInput, SaleMergeInput, SaleMoveTableInput, SaleRemoveInput, SaleSplitInput,
    SaleUpdateLineInput, SaleVersionInput, ShiftCloseInput,
    ShiftOpenInput, ShiftReviewInput,
    accounting_review_required, conflict, day_closed, idempotency_conflict, independent_review,
    not_found, parse_receipt_result, period_closed, request_invalid, setup_required,
    source_invalid, valid_uuid, version_conflict,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type PosFuture<'a, T, E = BusinessError> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PosCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn digest<'a>(&'a mut self, value: &'a str) -> PosFuture<'a, String, Self::Error>;
    fn receipt<'a>(&'a mut self, key: &'a str) -> PosFuture<'a, Option<PosReceipt>, Self::Error>;
    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        company: &'a CompanyId,
        request_hash: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn audit<'a>(
        &'a mut self,
        actor: &'a PosActor,
        operation: &'a str,
        record_id: &'a str,
        input: &'a str,
        result: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn new_id(&mut self) -> PosFuture<'_, String, Self::Error>;
    fn business_date(&mut self, now: TimestampMillis) -> PosFuture<'_, String, Self::Error>;
    fn same_person<'a>(
        &'a mut self,
        company: &'a CompanyId,
        left: &'a str,
        right: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;

    fn settings<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PosFuture<'a, Option<PosSettings>, Self::Error>;
    fn register<'a>(
        &'a mut self,
        company: &'a CompanyId,
        register_id: &'a str,
    ) -> PosFuture<'a, Option<PosRegister>, Self::Error>;
    fn shift<'a>(
        &'a mut self,
        company: &'a CompanyId,
        shift_id: &'a str,
    ) -> PosFuture<'a, Option<PosShift>, Self::Error>;
    fn shift_open_exists<'a>(
        &'a mut self,
        register_id: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn day_writable<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn insert_shift<'a>(
        &'a mut self,
        id: &'a str,
        register: &'a PosRegister,
        actor: &'a PosActor,
        opening_minor: i64,
        business_date: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn open_sales<'a>(&'a mut self, shift_id: &'a str) -> PosFuture<'a, bool, Self::Error>;
    fn unapproved_cash<'a>(&'a mut self, shift_id: &'a str) -> PosFuture<'a, bool, Self::Error>;
    fn cash_sales<'a>(&'a mut self, shift_id: &'a str) -> PosFuture<'a, i64, Self::Error>;
    fn cash_refunds<'a>(&'a mut self, shift_id: &'a str) -> PosFuture<'a, i64, Self::Error>;
    fn approved_movements<'a>(&'a mut self, shift_id: &'a str) -> PosFuture<'a, i64, Self::Error>;
    fn close_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
        counted_minor: i64,
        expected_minor: i64,
        variance_minor: i64,
        denominations: Option<&'a str>,
        actor: &'a PosActor,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn cash_movement<'a>(
        &'a mut self,
        shift_id: &'a str,
        movement_id: &'a str,
    ) -> PosFuture<'a, Option<PosCashMovement>, Self::Error>;
    fn approve_cash<'a>(
        &'a mut self,
        movement_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn bump_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn review_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
        reason: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn insert_posting<'a>(
        &'a mut self,
        source_key: &'a str,
        company: &'a CompanyId,
        journal_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;

    fn sale<'a>(
        &'a mut self,
        company: &'a CompanyId,
        sale_id: &'a str,
    ) -> PosFuture<'a, Option<PosSale>, Self::Error>;
    fn sale_lines<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, Vec<PosLine>, Self::Error>;
    fn payment_started<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, bool, Self::Error>;
    fn set_sale_state<'a>(
        &'a mut self,
        sale_id: &'a str,
        expected_version: i64,
        state: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn sale_fulfillments<'a>(
        &'a mut self,
        sale_id: &'a str,
    ) -> PosFuture<'a, Vec<PosFulfillment>, Self::Error>;
    fn fulfillment_shared<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
        sale_id: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn cancel_fulfillment<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn cancel_sale<'a>(
        &'a mut self,
        sale_id: &'a str,
        expected_version: i64,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn table_exists<'a>(
        &'a mut self,
        register_id: &'a str,
        table_id: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn insert_sale<'a>(
        &'a mut self,
        id: &'a str,
        shift: &'a PosShift,
        register: &'a PosRegister,
        settings: &'a PosSettings,
        input: &'a SaleCreateInput,
        number: &'a str,
        issuer_json: &'a str,
        actor: &'a PosActor,
        business_date: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn insert_fulfillment<'a>(
        &'a mut self,
        fulfillment: &'a PosFulfillmentInsert,
    ) -> PosFuture<'a, (), Self::Error>;
    fn insert_line<'a>(&'a mut self, line: &'a PosLineInsert) -> PosFuture<'a, (), Self::Error>;
    fn refresh_sale<'a>(
        &'a mut self,
        sale_id: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn sale_line<'a>(
        &'a mut self,
        sale_id: &'a str,
        line_id: &'a str,
    ) -> PosFuture<'a, Option<PosLineDetail>, Self::Error>;
    fn update_fulfillment<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
        quantity_milli: i64,
        note: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn update_line<'a>(
        &'a mut self,
        line_id: &'a str,
        quantity_milli: i64,
        discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
        note: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn set_sale_party<'a>(
        &'a mut self,
        sale_id: &'a str,
        party_id: Option<&'a str>,
    ) -> PosFuture<'a, (), Self::Error>;
    fn set_sale_table<'a>(
        &'a mut self,
        sale_id: &'a str,
        table_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn delete_reservations<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn delete_line<'a>(&'a mut self, line_id: &'a str) -> PosFuture<'a, (), Self::Error>;
    fn sale_lines_discount<'a>(
        &'a mut self,
        sale_id: &'a str,
    ) -> PosFuture<'a, Vec<PosLineDiscountRow>, Self::Error>;
    fn update_line_discount<'a>(
        &'a mut self,
        line_id: &'a str,
        discount_minor: i64,
        bill_discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
    ) -> PosFuture<'a, (), Self::Error>;
    fn move_lines<'a>(
        &'a mut self,
        target_id: &'a str,
        source_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn mark_merged<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, (), Self::Error>;
    fn sale_line_split<'a>(
        &'a mut self,
        sale_id: &'a str,
        line_id: &'a str,
    ) -> PosFuture<'a, Option<PosLineSplitRow>, Self::Error>;
    fn fulfillment_split<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> PosFuture<'a, Option<PosFulfillmentSplitFacts>, Self::Error>;
    fn create_split_sale<'a>(
        &'a mut self,
        target_id: &'a str,
        source_id: &'a str,
        number: &'a str,
        user: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn move_line_to_sale<'a>(
        &'a mut self,
        target_id: &'a str,
        line_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn create_split_fulfillment<'a>(
        &'a mut self,
        new_id: &'a str,
        source_id: &'a str,
        quantity_milli: i64,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn reduce_fulfillment<'a>(
        &'a mut self,
        source_id: &'a str,
        quantity_milli: i64,
    ) -> PosFuture<'a, (), Self::Error>;
    fn create_split_line<'a>(
        &'a mut self,
        new_line_id: &'a str,
        target_id: &'a str,
        new_fulfillment_id: &'a str,
        source_line_id: &'a str,
        quantity_milli: i64,
        discount_minor: i64,
        bill_discount_minor: i64,
        subtotal_minor: i64,
        tax_minor: i64,
    ) -> PosFuture<'a, (), Self::Error>;
    fn reduce_line<'a>(
        &'a mut self,
        line_id: &'a str,
        quantity_milli: i64,
        subtotal_minor: i64,
        tax_minor: i64,
        discount_minor: i64,
        bill_discount_minor: i64,
    ) -> PosFuture<'a, (), Self::Error>;
    fn payment_reserved<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, i64, Self::Error>;
    fn sale_gross<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, i64, Self::Error>;
    fn sale_fulfillment_stock<'a>(
        &'a mut self,
        sale_id: &'a str,
    ) -> PosFuture<'a, Vec<PosFulfillmentStock>, Self::Error>;
    fn insert_payment<'a>(
        &'a mut self,
        payment: &'a PosPaymentInsert,
    ) -> PosFuture<'a, (), Self::Error>;
    fn settle_payment<'a>(
        &'a mut self,
        payment_id: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn attach_finance_payment<'a>(
        &'a mut self,
        payment_id: &'a str,
        finance_payment_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn reserve_stock<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
        warehouse_id: &'a str,
        product_id: &'a str,
        quantity_milli: i64,
    ) -> PosFuture<'a, (), Self::Error>;
    fn mark_fulfillment_reserved<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> PosFuture<'a, (), Self::Error>;
    fn bump_payment_pending<'a>(
        &'a mut self,
        sale_id: &'a str,
        now: TimestampMillis,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn prepared_day_close<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> PosFuture<'a, bool, Self::Error>;
    fn insert_refund<'a>(
        &'a mut self,
        id: &'a str,
        sale: &'a PosSale,
        input: &'a RefundRequestInput,
        kind: &'a str,
        lines_json: &'a str,
        amount_minor: i64,
        tax_minor: i64,
        accounting_date: &'a str,
        actor: &'a PosActor,
        now: TimestampMillis,
    ) -> PosFuture<'a, (), Self::Error>;
    fn refund<'a>(
        &'a mut self,
        company: &'a CompanyId,
        refund_id: &'a str,
    ) -> PosFuture<'a, Option<PosRefund>, Self::Error>;
    fn reject_refund<'a>(
        &'a mut self,
        refund_id: &'a str,
        expected_version: i64,
        actor: &'a PosActor,
    ) -> PosFuture<'a, bool, Self::Error>;
}

pub trait PosFinancePort: Send {
    fn open_period<'a>(&'a mut self, company: &'a CompanyId, date: &'a str) -> PosFuture<'a, ()>;
    fn post_journal<'a>(
        &'a mut self,
        request: &'a PosJournalRequest,
    ) -> PosFuture<'a, Option<String>>;
    fn party_exists<'a>(
        &'a mut self,
        company: &'a CompanyId,
        party_id: &'a str,
    ) -> PosFuture<'a, bool>;
    /// Record the Finance receipt row for a settled POS payment. The journal
    /// itself is posted separately on the same owner transaction.
    fn record_receipt<'a>(
        &'a mut self,
        request: &'a PosReceiptRequest,
    ) -> PosFuture<'a, ()>;
}
pub trait PosHospitalityPort: Send {
    fn hotel_day_open<'a>(
        &'a mut self,
        company: &'a CompanyId,
        property_id: &'a str,
        branch_id: &'a str,
        date: &'a str,
    ) -> PosFuture<'a, bool>;
    fn room_charge_exists<'a>(&'a mut self, sale_id: &'a str) -> PosFuture<'a, bool>;
}
pub trait PosOrganizationPort: Send {
    fn issuer_name<'a>(&'a mut self, company: &'a CompanyId) -> PosFuture<'a, String>;
}

struct Accounts {
    chart_id: String,
    cash: String,
    cash_transfer: String,
    cash_difference: String,
}
struct PaymentAccounts {
    chart_id: String,
    cash: String,
    edc: String,
    receivable: String,
    advance: String,
}
fn accounts(settings: &PosSettings) -> Result<Accounts, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(&settings.accounts_json).map_err(|_| {
        request_invalid(
            "POS_ACCOUNT_INVALID",
            "Configured POS accounts are invalid.",
        )
    })?;
    let text = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Ok(Accounts {
        chart_id: text("chartId").ok_or_else(|| {
            request_invalid("POS_ACCOUNT_INVALID", "Configured POS chart is invalid.")
        })?,
        cash: text("cash").ok_or_else(|| {
            request_invalid(
                "POS_ACCOUNT_INVALID",
                "Configured POS cash account is invalid.",
            )
        })?,
        cash_transfer: text("cashTransfer").ok_or_else(|| {
            request_invalid(
                "POS_ACCOUNT_INVALID",
                "Configured POS cash-transfer account is invalid.",
            )
        })?,
        cash_difference: text("cashDifference").ok_or_else(|| {
            request_invalid(
                "POS_ACCOUNT_INVALID",
                "Configured POS cash-difference account is invalid.",
            )
        })?,
    })
}
fn payment_accounts(settings: &PosSettings) -> Result<PaymentAccounts, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(&settings.accounts_json).map_err(|_| {
        request_invalid(
            "POS_ACCOUNT_INVALID",
            "Configured POS accounts are invalid.",
        )
    })?;
    let text = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                request_invalid(
                    "POS_ACCOUNT_INVALID",
                    "Configured POS accounts are incomplete.",
                )
            })
    };
    Ok(PaymentAccounts {
        chart_id: text("chartId")?,
        cash: text("cash")?,
        edc: text("edc")?,
        receivable: text("receivable")?,
        advance: text("advance")?,
    })
}

fn scope_shift(actor: &PosActor, shift: &PosShift) -> Result<(), BusinessError> {
    if shift.company_id != actor.company_id.as_str()
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| shift.branch_id != branch.as_str())
    {
        return Err(not_found());
    }
    Ok(())
}
fn scope_register(actor: &PosActor, register: &PosRegister) -> Result<(), BusinessError> {
    if register.company_id != actor.company_id.as_str()
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| register.branch_id != branch.as_str())
    {
        return Err(not_found());
    }
    Ok(())
}
fn scope_sale(actor: &PosActor, sale: &PosSale) -> Result<(), BusinessError> {
    if sale.company_id != actor.company_id.as_str()
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| sale.branch_id != branch.as_str())
    {
        return Err(not_found());
    }
    Ok(())
}
fn scope_refund(actor: &PosActor, refund: &PosRefund) -> Result<(), BusinessError> {
    if refund.company_id != actor.company_id.as_str()
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|branch| refund.branch_id != branch.as_str())
    {
        return Err(not_found());
    }
    Ok(())
}
fn version_matches(actual: i64, expected: i64) -> Result<(), BusinessError> {
    if actual != expected {
        return Err(version_conflict());
    }
    Ok(())
}
fn scaled(amount: i64, numerator: i64, denominator: i64) -> Result<i64, BusinessError> {
    if amount < 0 || numerator < 0 || denominator <= 0 {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Review the POS money calculation.",
        ));
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    i64::try_from(value)
        .map_err(|_| request_invalid("POS_COMMAND_INVALID", "The POS money amount is too large."))
}
fn portion(total: i64, previous: i64, quantity: i64, whole: i64) -> Result<i64, BusinessError> {
    if previous < 0 || quantity <= 0 || previous + quantity > whole {
        return Err(request_invalid(
            "POS_REFUND_QUANTITY",
            "Refund quantity exceeds the original item.",
        ));
    }
    Ok(scaled(total, previous + quantity, whole)? - scaled(total, previous, whole)?)
}

struct JournalRequest {
    account_debit: String,
    account_credit: String,
    amount: i64,
}

async fn post_pair<R, F>(
    repo: &mut R,
    finance: &mut F,
    actor: &PosActor,
    settings: &PosSettings,
    business_date: &str,
    source_key: &str,
    source_id: &str,
    pair: JournalRequest,
    now: TimestampMillis,
) -> Result<(), R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
{
    if pair.amount == 0 {
        return Ok(());
    }
    let accounts = accounts(settings).map_err(R::Error::from)?;
    let request = PosJournalRequest {
        company_id: actor.company_id.clone(),
        branch_id: actor.branch_id.clone(),
        chart_id: accounts.chart_id,
        currency: settings.currency.clone(),
        business_date: business_date.to_owned(),
        source_key: source_key.to_owned(),
        source_id: source_id.to_owned(),
        actor_id: actor.user_id.clone(),
        lines: vec![
            PosJournalLine {
                account_id: pair.account_debit,
                debit: pair.amount,
                credit: 0,
            },
            PosJournalLine {
                account_id: pair.account_credit,
                debit: 0,
                credit: pair.amount,
            },
        ],
        now,
    };
    if let Some(journal) = finance
        .post_journal(&request)
        .await
        .map_err(R::Error::from)?
    {
        repo.insert_posting(source_key, &actor.company_id, &journal)
            .await?;
    }
    Ok(())
}

pub async fn execute_pos_command<R, F, H, O>(
    actor: &PosActor,
    command: &PosCommand,
    repository: &mut R,
    catalog: Option<&PosCatalogSnapshot>,
    stock: Option<&[PosStockRow]>,
    finance: &mut F,
    hospitality: &mut H,
    organization: &mut O,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
    H: PosHospitalityPort,
    O: PosOrganizationPort,
{
    match command.operation {
        PosOperation::ShiftReview | PosOperation::CashApprove | PosOperation::RefundReject => {
            actor.require_approve(now)?;
        }
        _ => actor.require_sell(now)?,
    }
    let identity = command.receipt_identity(actor);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(source_invalid().into());
    }
    if let Some(receipt) = repository.receipt(command.key.as_str()).await? {
        if receipt.request_hash != hash {
            return Err(idempotency_conflict().into());
        }
        return parse_receipt_result(&receipt.result_json).map_err(Into::into);
    }
    let result = match &command.input {
        PosCommandInput::ShiftOpen(input) => {
            shift_open(actor, input, repository, finance, hospitality, now).await?
        }
        PosCommandInput::ShiftClose(input) => shift_close(actor, input, repository, now).await?,
        PosCommandInput::ShiftReview(input) => {
            shift_review(actor, input, repository, finance, now).await?
        }
        PosCommandInput::CashApprove(input) => {
            cash_approve(actor, input, repository, finance, now).await?
        }
        PosCommandInput::SaleAdd(input) => {
            sale_add(actor, input, catalog, repository, now).await?
        }
        PosCommandInput::SaleUpdateLine(input) => {
            sale_update_line(actor, input, repository, now).await?
        }
        PosCommandInput::SaleCustomer(input) => {
            sale_customer(actor, input, repository, finance, now).await?
        }
        PosCommandInput::SaleMoveTable(input) => {
            sale_move_table(actor, input, repository, now).await?
        }
        PosCommandInput::SaleRemove(input) => {
            sale_remove(actor, input, repository, now).await?
        }
        PosCommandInput::SaleDiscount(input) => {
            sale_discount(actor, input, repository, now).await?
        }
        PosCommandInput::SaleMerge(input) => {
            sale_merge(actor, input, repository, now).await?
        }
        PosCommandInput::SaleSplit(input) => {
            sale_split(actor, input, repository, now).await?
        }
        PosCommandInput::PaymentCreate(input) => {
            payment_create(actor, input, stock, repository, finance, now).await?
        }
        PosCommandInput::SaleCreate(input) => {
            sale_create(
                actor,
                input,
                repository,
                finance,
                hospitality,
                organization,
                now,
            )
            .await?
        }
        PosCommandInput::SaleVersion(input) => {
            sale_version(actor, command.operation, input, repository, now).await?
        }
        PosCommandInput::RefundRequest(input) => {
            refund_request(actor, input, repository, hospitality, now).await?
        }
        PosCommandInput::RefundReject(input) => refund_reject(actor, input, repository).await?,
    };
    let result_json = serde_json::to_string(&result.to_wire()).map_err(|_| source_invalid())?;
    if !repository
        .insert_receipt(
            command.key.as_str(),
            &actor.company_id,
            &hash,
            &result_json,
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    repository
        .audit(
            actor,
            command.operation(),
            &result.id,
            &command.canonical_input,
            &result_json,
            now,
        )
        .await?;
    Ok(result)
}

async fn shift_open<R, F, H>(
    actor: &PosActor,
    input: &ShiftOpenInput,
    repository: &mut R,
    finance: &mut F,
    hospitality: &mut H,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
    H: PosHospitalityPort,
{
    let settings = repository
        .settings(&actor.company_id)
        .await?
        .ok_or_else(setup_required)?;
    if settings
        .accounting_approved_by
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(accounting_review_required().into());
    }
    if !settings.enabled {
        return Err(conflict("POS_DISABLED", "POS transactions are not enabled.").into());
    }
    let register = repository
        .register(&actor.company_id, &input.register_id)
        .await?
        .ok_or_else(not_found)?;
    scope_register(actor, &register)?;
    if !register.active {
        return Err(conflict("POS_REGISTER_DISABLED", "Sales point is disabled.").into());
    }
    if repository.shift_open_exists(&register.id).await? {
        return Err(conflict(
            "POS_REGISTER_OPEN",
            "Close the current shift before opening another.",
        )
        .into());
    }
    let business_date = repository.business_date(now).await?;
    if let Some(property_id) = &register.property_id
        && !hospitality
            .hotel_day_open(
                &actor.company_id,
                property_id,
                &register.branch_id,
                &business_date,
            )
            .await?
    {
        return Err(conflict(
            "POS_HOTEL_DAY_CLOSED",
            "Finish or recover the hotel day close before opening a restaurant cash session.",
        )
        .into());
    }
    if !repository
        .day_writable(&actor.company_id, &register.branch_id, &business_date)
        .await?
    {
        return Err(day_closed().into());
    }
    finance
        .open_period(&actor.company_id, &business_date)
        .await
        .map_err(|_| R::Error::from(period_closed()))?;
    let id = repository.new_id().await?;
    if !valid_uuid(&id) {
        return Err(source_invalid().into());
    }
    repository
        .insert_shift(
            &id,
            &register,
            actor,
            input.opening_minor,
            &business_date,
            now,
        )
        .await?;
    Ok(PosResult { id, version: None })
}

async fn shift_close<R>(
    actor: &PosActor,
    input: &ShiftCloseInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let shift = repository
        .shift(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_shift(actor, &shift)?;
    version_matches(shift.version, input.expected_version)?;
    if shift.state != "open" {
        return Err(request_invalid("POS_SHIFT_INVALID", "Cash session is closed.").into());
    }
    if repository.open_sales(&shift.id).await? {
        return Err(conflict(
            "POS_OPEN_SALES",
            "Close or cancel open bills before closing the cash session.",
        )
        .into());
    }
    if repository.unapproved_cash(&shift.id).await? {
        return Err(conflict(
            "POS_CASH_REVIEW_PENDING",
            "There are unapproved cash movements.",
        )
        .into());
    }
    let expected = shift.opening_minor + repository.cash_sales(&shift.id).await?
        - repository.cash_refunds(&shift.id).await?
        + repository.approved_movements(&shift.id).await?;
    let variance = input.counted_minor - expected;
    let denominations = input.denominations.as_ref().map(|values| {
        format!(
            "{{{}}}",
            values
                .iter()
                .map(|(denomination, quantity)| format!(
                    "{}:{}",
                    serde_json::to_string(&denomination.to_string()).unwrap(),
                    quantity
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    });
    if !repository
        .close_shift(
            &shift.id,
            input.expected_version,
            input.counted_minor,
            expected,
            variance,
            denominations.as_deref(),
            actor,
            now,
        )
        .await?
    {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: shift.id,
        version: Some(input.expected_version + 1),
    })
}

async fn shift_review<R, F>(
    actor: &PosActor,
    input: &ShiftReviewInput,
    repository: &mut R,
    finance: &mut F,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
{
    let shift = repository
        .shift(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_shift(actor, &shift)?;
    version_matches(shift.version, input.expected_version)?;
    if shift.state != "closed" {
        return Err(conflict(
            "POS_SHIFT_INVALID",
            "Only a closed cash session can be reviewed.",
        )
        .into());
    }
    if repository
        .same_person(&actor.company_id, actor.user_id.as_str(), &shift.opened_by)
        .await?
    {
        return Err(independent_review().into());
    }
    if let Some(closed_by) = shift.closed_by.as_deref()
        && repository
            .same_person(&actor.company_id, actor.user_id.as_str(), closed_by)
            .await?
    {
        return Err(independent_review().into());
    }
    let settings = repository
        .settings(&actor.company_id)
        .await?
        .ok_or_else(setup_required)?;
    let variance = shift.variance_minor.ok_or_else(source_invalid)?;
    if variance != 0 {
        let accounts = accounts(&settings)?;
        let (debit, credit) = if variance > 0 {
            (accounts.cash, accounts.cash_difference)
        } else {
            (accounts.cash_difference, accounts.cash)
        };
        post_pair(
            repository,
            finance,
            actor,
            &settings,
            &shift.business_date,
            &format!("shift:{}", shift.id),
            &shift.id,
            JournalRequest {
                account_debit: debit,
                account_credit: credit,
                amount: variance.unsigned_abs() as i64,
            },
            now,
        )
        .await?;
    }
    if !repository
        .review_shift(&shift.id, input.expected_version, actor, &input.reason)
        .await?
    {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: shift.id,
        version: Some(input.expected_version + 1),
    })
}

async fn cash_approve<R, F>(
    actor: &PosActor,
    input: &CashApproveInput,
    repository: &mut R,
    finance: &mut F,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
{
    let shift = repository
        .shift(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_shift(actor, &shift)?;
    version_matches(shift.version, input.expected_version)?;
    if shift.state != "open" {
        return Err(conflict("POS_SHIFT_CLOSED", "Cash session is closed.").into());
    }
    let movement = repository
        .cash_movement(&shift.id, &input.movement_id)
        .await?
        .ok_or_else(independent_review)?;
    if movement.approved_by.is_some() {
        return Err(independent_review().into());
    }
    if repository
        .same_person(
            &actor.company_id,
            actor.user_id.as_str(),
            &movement.created_by,
        )
        .await?
    {
        return Err(independent_review().into());
    }
    let settings = repository
        .settings(&actor.company_id)
        .await?
        .ok_or_else(setup_required)?;
    let accounts = accounts(&settings)?;
    let (debit, credit) = if movement.amount_minor > 0 {
        (accounts.cash, accounts.cash_transfer)
    } else {
        (accounts.cash_transfer, accounts.cash)
    };
    post_pair(
        repository,
        finance,
        actor,
        &settings,
        &shift.business_date,
        &format!("cash:{}", movement.id),
        &shift.id,
        JournalRequest {
            account_debit: debit,
            account_credit: credit,
            amount: movement.amount_minor.unsigned_abs() as i64,
        },
        now,
    )
    .await?;
    if !repository
        .approve_cash(&movement.id, input.expected_version, actor)
        .await?
    {
        return Err(version_conflict().into());
    }
    if !repository
        .bump_shift(&shift.id, input.expected_version)
        .await?
    {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: shift.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_create<R, F, H, O>(
    actor: &PosActor,
    input: &SaleCreateInput,
    repository: &mut R,
    finance: &mut F,
    hospitality: &mut H,
    organization: &mut O,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
    H: PosHospitalityPort,
    O: PosOrganizationPort,
{
    let shift = repository
        .shift(&actor.company_id, &input.shift_id)
        .await?
        .ok_or_else(not_found)?;
    scope_shift(actor, &shift)?;
    if shift.state != "open" {
        return Err(conflict("POS_SHIFT_CLOSED", "Cash session is closed.").into());
    }
    let settings = repository
        .settings(&actor.company_id)
        .await?
        .ok_or_else(setup_required)?;
    if settings
        .accounting_approved_by
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(accounting_review_required().into());
    }
    if !settings.enabled {
        return Err(conflict("POS_DISABLED", "New transactions are temporarily paused.").into());
    }
    if let Some(currency) = &input.currency
        && currency != &settings.currency
    {
        return Err(request_invalid(
            "POS_CURRENCY_SETUP_REQUIRED",
            "The selected currency needs POS register, payment and pricing setup first.",
        )
        .into());
    }
    let register = repository
        .register(&actor.company_id, &shift.register_id)
        .await?
        .ok_or_else(not_found)?;
    scope_register(actor, &register)?;
    if !register.active {
        return Err(conflict("POS_REGISTER_DISABLED", "Sales point is disabled.").into());
    }
    let business_date = repository.business_date(now).await?;
    if shift.business_date != business_date {
        return Err(conflict(
            "POS_SHIFT_DATE",
            "Close the previous business day cash session before selling.",
        )
        .into());
    }
    if let Some(property_id) = &register.property_id
        && !hospitality
            .hotel_day_open(
                &actor.company_id,
                property_id,
                &register.branch_id,
                &business_date,
            )
            .await?
    {
        return Err(conflict(
            "POS_HOTEL_DAY_CLOSED",
            "Finish or recover the hotel day close before selling.",
        )
        .into());
    }
    if !repository
        .day_writable(&actor.company_id, &register.branch_id, &business_date)
        .await?
    {
        return Err(day_closed().into());
    }
    let retail = input.service_mode == "retail";
    if retail != (register.mode == "retail") {
        return Err(
            request_invalid("POS_MODE_INVALID", "Mode does not match the sales point.").into(),
        );
    }
    if let Some(table_id) = &input.table_id
        && !repository.table_exists(&register.id, table_id).await?
    {
        return Err(
            request_invalid("POS_TABLE_INVALID", "Table is not at this sales point.").into(),
        );
    }
    if let Some(party_id) = &input.party_id
        && !finance
            .party_exists(&actor.company_id, party_id)
            .await
            .map_err(R::Error::from)?
    {
        return Err(request_invalid("POS_CUSTOMER_INVALID", "Customer is not available.").into());
    }
    let id = repository.new_id().await?;
    if !valid_uuid(&id) {
        return Err(source_invalid().into());
    }
    let number = format!(
        "POS-{}-{}",
        business_date.replace('-', ""),
        id.chars().take(8).collect::<String>().to_ascii_uppercase()
    );
    let issuer = organization
        .issuer_name(&actor.company_id)
        .await
        .map_err(R::Error::from)?;
    let issuer_json = serde_json::json!({
        "companyName": issuer,
        "registerName": register.name,
        "branchId": register.branch_id,
    })
    .to_string();
    repository
        .insert_sale(
            &id,
            &shift,
            &register,
            &settings,
            input,
            &number,
            &issuer_json,
            actor,
            &business_date,
            now,
        )
        .await?;
    Ok(PosResult { id, version: None })
}

async fn sale_version<R>(
    actor: &PosActor,
    operation: PosOperation,
    input: &SaleVersionInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !["open", "held"].contains(&sale.state.as_str()) {
        return Err(conflict("POS_SALE_LOCKED", "This bill cannot be edited.").into());
    }
    if repository.payment_started(&sale.id).await? {
        return Err(conflict("POS_PAYMENT_STARTED", "This bill already has a payment.").into());
    }
    if operation == PosOperation::SaleCancel {
        for fulfillment in repository.sale_fulfillments(&sale.id).await? {
            if !["draft", "queued", "cancelled"].contains(&fulfillment.state.as_str()) {
                return Err(conflict(
                    "POS_KITCHEN_ACTIVE",
                    "Cancel the kitchen task and record waste before cancelling the bill.",
                )
                .into());
            }
            if repository
                .fulfillment_shared(&fulfillment.id, &sale.id)
                .await?
            {
                return Err(conflict(
                    "POS_SHARED_KITCHEN",
                    "This kitchen item is on a separate bill. Handle the item first.",
                )
                .into());
            }
            repository.cancel_fulfillment(&fulfillment.id).await?;
        }
        if !repository
            .cancel_sale(&sale.id, input.expected_version, now)
            .await?
        {
            return Err(version_conflict().into());
        }
    } else {
        let state = if operation == PosOperation::SaleHold {
            "held"
        } else {
            "open"
        };
        if !repository
            .set_sale_state(&sale.id, input.expected_version, state, now)
            .await?
        {
            return Err(version_conflict().into());
        }
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn refund_request<R, H>(
    actor: &PosActor,
    input: &RefundRequestInput,
    repository: &mut R,
    hospitality: &mut H,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    H: PosHospitalityPort,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    let business_date = repository.business_date(now).await?;
    if !repository
        .day_writable(&actor.company_id, &sale.branch_id, &business_date)
        .await?
    {
        return Err(day_closed().into());
    }
    if repository
        .prepared_day_close(&actor.company_id, &sale.branch_id, &sale.business_date)
        .await?
    {
        return Err(conflict(
            "POS_DAY_REVIEW_PENDING",
            "Finance must confirm the original day close before a refund can change its source records.",
        )
        .into());
    }
    if hospitality
        .room_charge_exists(&sale.id)
        .await
        .map_err(R::Error::from)?
    {
        return Err(conflict(
            "POS_ROOM_CHARGE_REVERSAL",
            "Reverse this charge on the hotel guest account; it has no POS payment to refund.",
        )
        .into());
    }
    if !["completed", "payment_pending"].contains(&sale.state.as_str()) {
        return Err(conflict("POS_REFUND_SALE", "Bill has no payment.").into());
    }
    if input
        .lines
        .iter()
        .map(|line| line.line_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len()
        != input.lines.len()
    {
        return Err(request_invalid("POS_DUPLICATE_LINE", "Duplicate refund item.").into());
    }
    let advance = sale.state != "completed";
    let lines = repository.sale_lines(&sale.id).await?;
    let mut amount = 0i64;
    let mut tax = 0i64;
    let mut snapshots = Vec::new();
    if advance {
        if input.amount_minor.is_none() || !input.lines.is_empty() {
            return Err(request_invalid(
                "POS_ADVANCE_REFUND",
                "Enter an advance payment refund amount without selecting items.",
            )
            .into());
        }
        amount = input.amount_minor.unwrap_or(0);
        if amount == 0 {
            return Err(request_invalid(
                "POS_REFUND_EMPTY",
                "Select an item with a refund amount.",
            )
            .into());
        }
    } else {
        for part in &input.lines {
            let line = lines
                .iter()
                .find(|line| line.id == part.line_id)
                .ok_or_else(|| {
                    request_invalid(
                        "POS_REFUND_QUANTITY",
                        "Refund quantity exceeds the original item.",
                    )
                })?;
            if line.returned_milli + part.quantity_milli > line.quantity_milli {
                return Err(request_invalid(
                    "POS_REFUND_QUANTITY",
                    "Refund quantity exceeds the original item.",
                )
                .into());
            }
            let subtotal = portion(
                line.subtotal_minor,
                line.returned_milli,
                part.quantity_milli,
                line.quantity_milli,
            )?;
            let line_tax = portion(
                line.tax_minor,
                line.returned_milli,
                part.quantity_milli,
                line.quantity_milli,
            )?;
            amount += subtotal + line_tax;
            tax += line_tax;
            snapshots.push(format!(
                "{{\"lineId\":{},\"quantityMilli\":{},\"restock\":{},\"subtotalMinor\":{},\"taxMinor\":{},\"previousReturned\":{}}}",
                serde_json::to_string(&part.line_id).unwrap(),
                part.quantity_milli,
                part.restock,
                subtotal,
                line_tax,
                line.returned_milli
            ));
        }
        if amount <= 0 {
            return Err(request_invalid(
                "POS_REFUND_EMPTY",
                "Select an item with a refund amount.",
            )
            .into());
        }
    }
    let id = repository.new_id().await?;
    if !valid_uuid(&id) {
        return Err(source_invalid().into());
    }
    let snapshots = format!("[{}]", snapshots.join(","));
    repository
        .insert_refund(
            &id,
            &sale,
            input,
            if advance { "advance" } else { "sale" },
            &snapshots,
            amount,
            tax,
            &business_date,
            actor,
            now,
        )
        .await?;
    Ok(PosResult { id, version: None })
}

async fn refund_reject<R>(
    actor: &PosActor,
    input: &RefundRejectInput,
    repository: &mut R,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let refund = repository
        .refund(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_refund(actor, &refund)?;
    version_matches(refund.version, input.expected_version)?;
    if refund.state != "requested" {
        return Err(independent_review().into());
    }
    if repository
        .same_person(
            &actor.company_id,
            actor.user_id.as_str(),
            &refund.requested_by,
        )
        .await?
    {
        return Err(independent_review().into());
    }
    if !repository
        .reject_refund(&refund.id, input.expected_version, actor)
        .await?
    {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: refund.id,
        version: None,
    })
}

fn sale_amounts(
    quantity_milli: i64,
    price_minor: i64,
    tax_bps: i64,
    tax_inclusive: bool,
    discount_minor: i64,
) -> Result<(i64, i64), BusinessError> {
    let gross = scaled(price_minor, quantity_milli, 1000)?;
    if discount_minor > gross {
        return Err(request_invalid(
            "POS_DISCOUNT_AMOUNT",
            "The line discount exceeds the line amount.",
        ));
    }
    let discounted = gross - discount_minor;
    if tax_inclusive {
        let subtotal = scaled(discounted, 10_000, 10_000 + tax_bps)?;
        Ok((subtotal, discounted - subtotal))
    } else {
        let tax = scaled(discounted, tax_bps, 10_000)?;
        Ok((discounted, tax))
    }
}

struct SelectedModifier {
    value: serde_json::Value,
    price_minor: i64,
}

fn selected_modifiers(
    modifiers_json: &str,
    ids: &[String],
) -> Result<Vec<SelectedModifier>, BusinessError> {
    let invalid = || request_invalid("POS_MODIFIER_INVALID", "Review the selected product options.");
    let value: serde_json::Value = serde_json::from_str(modifiers_json).map_err(|_| invalid())?;
    let all = value.as_array().ok_or_else(invalid)?;
    let mut selected = Vec::new();
    for id in ids {
        let modifier = all
            .iter()
            .find(|item| item.get("id").and_then(|value| value.as_str()) == Some(id.as_str()))
            .ok_or_else(invalid)?;
        let price_minor = modifier
            .get("priceMinor")
            .and_then(|value| value.as_i64())
            .ok_or_else(invalid)?;
        selected.push(SelectedModifier {
            value: modifier.clone(),
            price_minor,
        });
    }
    Ok(selected)
}

fn recipe_parts(recipe_json: &str) -> Result<Vec<serde_json::Value>, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(recipe_json).map_err(|_| {
        request_invalid("POS_PRODUCT_INVALID", "The product recipe is unavailable.")
    })?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| request_invalid("POS_PRODUCT_INVALID", "The product recipe is unavailable."))
}

fn portion_of(total: i64, previous: i64, quantity: i64, whole: i64) -> Result<i64, BusinessError> {
    if previous < 0 || quantity <= 0 || previous + quantity > whole {
        return Err(request_invalid("POS_SPLIT_INVALID", "Review the split quantity."));
    }
    let end = scaled(total, previous + quantity, whole)?;
    let start = scaled(total, previous, whole)?;
    end.checked_sub(start)
        .ok_or_else(|| request_invalid("POS_SPLIT_INVALID", "Review the split quantity."))
}

async fn sale_add<R>(
    actor: &PosActor,
    input: &SaleAddInput,
    catalog: Option<&PosCatalogSnapshot>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    let snapshot = catalog.ok_or_else(|| {
        request_invalid(
            "POS_PRODUCT_INVALID",
            "This product is not enabled for the sales point.",
        )
    })?;
    if snapshot.register_id != sale.register_id || snapshot.product.id != input.product_id {
        return Err(conflict(
            "POS_CATALOG_CHANGED",
            "The catalogue changed. Review the bill.",
        )
        .into());
    }
    let product = &snapshot.product;
    if input
        .expected_catalog_version
        .is_some_and(|version| version != product.catalog_version)
        || input
            .expected_price_minor
            .is_some_and(|price| price != product.price_minor)
    {
        return Err(conflict(
            "POS_CATALOG_CHANGED",
            "The catalogue price changed. Review the bill.",
        )
        .into());
    }
    let modifiers = selected_modifiers(&product.modifiers_json, &input.modifier_ids)?;
    let price = modifiers
        .iter()
        .try_fold(product.price_minor, |total, modifier| {
            total.checked_add(modifier.price_minor)
        })
        .ok_or_else(|| request_invalid("POS_COMMAND_INVALID", "Review the POS money fields."))?;
    let (subtotal, tax) = sale_amounts(
        input.quantity_milli,
        price,
        product.tax_bps,
        product.tax_inclusive,
        input.discount_minor,
    )?;
    if product.kind == "recipe" && sale.service_mode == "retail" {
        return Err(request_invalid(
            "POS_RESTAURANT_REQUIRED",
            "Prepared menus require a restaurant sales point.",
        )
        .into());
    }
    let mut recipe = match product.kind.as_str() {
        "stock" => vec![serde_json::json!({
            "productId": input.product_id,
            "quantityMilli": 1000,
        })],
        "recipe" => recipe_parts(&product.recipe_json)?,
        _ => Vec::new(),
    };
    for modifier in &modifiers {
        if let Some(ingredients) = modifier
            .value
            .get("ingredients")
            .and_then(|value| value.as_array())
        {
            recipe.extend(ingredients.iter().cloned());
        }
    }
    let register = repository
        .register(&actor.company_id, &sale.register_id)
        .await?
        .ok_or_else(not_found)?;
    let fulfillment_id = repository.new_id().await?;
    let line_id = repository.new_id().await?;
    if !valid_uuid(&fulfillment_id) || !valid_uuid(&line_id) {
        return Err(source_invalid().into());
    }
    let recipe_json = serde_json::to_string(&recipe).map_err(|_| source_invalid())?;
    let modifiers_json = serde_json::to_string(
        &modifiers
            .iter()
            .map(|modifier| modifier.value.clone())
            .collect::<Vec<_>>(),
    )
    .map_err(|_| source_invalid())?;
    repository
        .insert_fulfillment(&PosFulfillmentInsert {
            id: fulfillment_id.clone(),
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: sale.branch_id.clone(),
            warehouse_id: register.warehouse_id.clone(),
            product_id: input.product_id.clone(),
            quantity_milli: input.quantity_milli,
            kind: product.kind.clone(),
            station: product.station.clone(),
            note: input.note.clone(),
            recipe_json,
            accounts_json: sale.accounts_json.clone(),
            currency: sale.currency.clone(),
            business_date: sale.business_date.clone(),
        })
        .await?;
    repository
        .insert_line(&PosLineInsert {
            id: line_id,
            sale_id: sale.id.clone(),
            fulfillment_id,
            product_id: input.product_id.clone(),
            name: product.name.clone(),
            quantity_milli: input.quantity_milli,
            price_minor: price,
            tax_bps: product.tax_bps,
            tax_inclusive: product.tax_inclusive,
            discount_minor: input.discount_minor,
            subtotal_minor: subtotal,
            tax_minor: tax,
            modifiers_json,
            note: input.note.clone(),
        })
        .await?;
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_update_line<R>(
    actor: &PosActor,
    input: &SaleUpdateLineInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    let line = repository
        .sale_line(&sale.id, &input.line_id)
        .await?
        .ok_or_else(|| request_invalid("POS_LINE_INVALID", "Line is not on this bill."))?;
    if line.fulfillment_state != "draft" && input.quantity_milli != 0 {
        return Err(conflict(
            "POS_ITEM_IN_KITCHEN",
            "A line already sent to the kitchen cannot change quantity.",
        )
        .into());
    }
    if line.fulfillment_state == "draft" {
        repository
            .update_fulfillment(&line.fulfillment_id, input.quantity_milli, &input.note)
            .await?;
    }
    let (subtotal, tax) = sale_amounts(
        input.quantity_milli,
        line.price_minor,
        line.tax_bps,
        line.tax_inclusive,
        input.discount_minor.saturating_add(line.bill_discount_minor),
    )?;
    repository
        .update_line(
            &line.id,
            input.quantity_milli,
            input.discount_minor,
            subtotal,
            tax,
            &input.note,
        )
        .await?;
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_customer<R, F>(
    actor: &PosActor,
    input: &SaleCustomerInput,
    repository: &mut R,
    finance: &mut F,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    if let Some(party) = input.party_id.as_deref()
        && !finance
            .party_exists(&actor.company_id, party)
            .await
            .map_err(R::Error::from)?
    {
        return Err(request_invalid("POS_CUSTOMER_INVALID", "Customer is not available.").into());
    }
    repository
        .set_sale_party(&sale.id, input.party_id.as_deref())
        .await?;
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_move_table<R>(
    actor: &PosActor,
    input: &SaleMoveTableInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    if !repository
        .table_exists(&sale.register_id, &input.table_id)
        .await?
    {
        return Err(request_invalid("POS_TABLE_INVALID", "Table is not at this sales point.").into());
    }
    repository
        .set_sale_table(&sale.id, &input.table_id)
        .await?;
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_remove<R>(
    actor: &PosActor,
    input: &SaleRemoveInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    let line = repository
        .sale_line(&sale.id, &input.line_id)
        .await?
        .ok_or_else(|| request_invalid("POS_LINE_INVALID", "Line is not on this bill."))?;
    if !matches!(line.fulfillment_state.as_str(), "draft" | "cancelled") {
        return Err(conflict(
            "POS_ITEM_IN_KITCHEN",
            "Cancel the kitchen item before removing it from the bill.",
        )
        .into());
    }
    repository
        .delete_reservations(&line.fulfillment_id)
        .await?;
    repository.delete_line(&line.id).await?;
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_discount<R>(
    actor: &PosActor,
    input: &SaleDiscountInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held") {
        return Err(conflict(
            "POS_SALE_NOT_EDITABLE",
            "Only an open or held bill can change.",
        )
        .into());
    }
    let lines = repository.sale_lines_discount(&sale.id).await?;
    let weights = lines
        .iter()
        .map(|line| {
            scaled(line.price_minor, line.quantity_milli, 1000)
                .and_then(|amount| amount.checked_sub(line.discount_minor).ok_or_else(|| {
                    request_invalid("POS_DISCOUNT_AMOUNT", "Review the bill discount.")
                }))
                .and_then(|amount| amount.checked_add(line.bill_discount_minor).ok_or_else(|| {
                    request_invalid("POS_DISCOUNT_AMOUNT", "Review the bill discount.")
                }))
        })
        .collect::<Result<Vec<_>, BusinessError>>()?;
    let total = weights
        .iter()
        .try_fold(0i64, |total, weight| total.checked_add(*weight))
        .ok_or_else(|| request_invalid("POS_DISCOUNT_AMOUNT", "Review the bill discount."))?;
    if input.amount_minor > total {
        return Err(request_invalid(
            "POS_DISCOUNT_AMOUNT",
            "The discount exceeds the bill amount.",
        )
        .into());
    }
    let mut previous = 0i64;
    for (line, weight) in lines.iter().zip(weights.iter()) {
        let extra = if *weight > 0 && input.amount_minor > 0 {
            let end = previous + weight;
            scaled(input.amount_minor, end, total)?
                - scaled(input.amount_minor, previous, total)?
        } else {
            0
        };
        previous += weight;
        let discount = line
            .discount_minor
            .checked_sub(line.bill_discount_minor)
            .and_then(|base| base.checked_add(extra))
            .ok_or_else(|| request_invalid("POS_DISCOUNT_AMOUNT", "Review the bill discount."))?;
        let (subtotal, tax) = sale_amounts(
            line.quantity_milli,
            line.price_minor,
            line.tax_bps,
            line.tax_inclusive,
            discount,
        )?;
        repository
            .update_line_discount(&line.id, discount, extra, subtotal, tax)
            .await?;
    }
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: sale.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_merge<R>(
    actor: &PosActor,
    input: &SaleMergeInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let source = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &source)?;
    version_matches(source.version, input.expected_version)?;
    if !matches!(source.state.as_str(), "open" | "held")
        || repository.payment_started(&source.id).await?
    {
        return Err(conflict("POS_SALE_LOCKED", "This bill cannot be edited.").into());
    }
    if input.target_id == source.id {
        return Err(request_invalid("POS_MERGE_INVALID", "Select a different bill.").into());
    }
    let target = repository
        .sale(&actor.company_id, &input.target_id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &target)?;
    version_matches(target.version, input.target_version)?;
    if !matches!(target.state.as_str(), "open" | "held")
        || repository.payment_started(&target.id).await?
    {
        return Err(conflict("POS_SALE_LOCKED", "This bill cannot be edited.").into());
    }
    if target.register_id != source.register_id
        || target.shift_id != source.shift_id
        || target.party_id != source.party_id
    {
        return Err(request_invalid(
            "POS_MERGE_INVALID",
            "Bills must use the same sales point, cash session, and customer.",
        )
        .into());
    }
    repository.move_lines(&target.id, &source.id).await?;
    repository.mark_merged(&source.id).await?;
    if !repository.refresh_sale(&source.id, now).await? {
        return Err(version_conflict().into());
    }
    if !repository.refresh_sale(&target.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: source.id,
        version: Some(input.expected_version + 1),
    })
}

async fn sale_split<R>(
    actor: &PosActor,
    input: &SaleSplitInput,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
{
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held")
        || repository.payment_started(&sale.id).await?
    {
        return Err(conflict("POS_SALE_LOCKED", "This bill cannot be edited.").into());
    }
    let line = repository
        .sale_line_split(&sale.id, &input.line_id)
        .await?
        .ok_or_else(|| request_invalid("POS_SPLIT_INVALID", "Invalid split quantity."))?;
    if input.quantity_milli > line.quantity_milli {
        return Err(request_invalid("POS_SPLIT_INVALID", "Invalid split quantity.").into());
    }
    let facts = repository
        .fulfillment_split(&line.fulfillment_id)
        .await?
        .ok_or_else(|| request_invalid("POS_SPLIT_INVALID", "Invalid split quantity."))?;
    let target_id = repository.new_id().await?;
    if !valid_uuid(&target_id) {
        return Err(source_invalid().into());
    }
    let number = format!("POS-{}", target_id[..8].to_uppercase());
    repository
        .create_split_sale(
            &target_id,
            &sale.id,
            &number,
            actor.user_id.as_str(),
            now,
        )
        .await?;
    if input.quantity_milli == line.quantity_milli {
        repository
            .move_line_to_sale(&target_id, &line.id)
            .await?;
    } else {
        let mut new_fulfillment_id = line.fulfillment_id.clone();
        if facts.kind != "recipe" || facts.state == "draft" {
            if facts.reserved {
                return Err(conflict(
                    "POS_SPLIT_RESERVED",
                    "Reserved items cannot be split yet.",
                )
                .into());
            }
            let created = repository.new_id().await?;
            if !valid_uuid(&created) {
                return Err(source_invalid().into());
            }
            repository
                .create_split_fulfillment(
                    &created,
                    &line.fulfillment_id,
                    input.quantity_milli,
                    now,
                )
                .await?;
            repository
                .reduce_fulfillment(&line.fulfillment_id, input.quantity_milli)
                .await?;
            new_fulfillment_id = created;
        }
        let subtotal = portion_of(
            line.subtotal_minor,
            0,
            input.quantity_milli,
            line.quantity_milli,
        )?;
        let tax = portion_of(
            line.tax_minor,
            0,
            input.quantity_milli,
            line.quantity_milli,
        )?;
        let discount = portion_of(
            line.discount_minor,
            0,
            input.quantity_milli,
            line.quantity_milli,
        )?;
        let bill_discount = portion_of(
            line.bill_discount_minor,
            0,
            input.quantity_milli,
            line.quantity_milli,
        )?;
        let new_line_id = repository.new_id().await?;
        if !valid_uuid(&new_line_id) {
            return Err(source_invalid().into());
        }
        repository
            .create_split_line(
                &new_line_id,
                &target_id,
                &new_fulfillment_id,
                &line.id,
                input.quantity_milli,
                discount,
                bill_discount,
                subtotal,
                tax,
            )
            .await?;
        repository
            .reduce_line(
                &line.id,
                input.quantity_milli,
                subtotal,
                tax,
                discount,
                bill_discount,
            )
            .await?;
    }
    if !repository.refresh_sale(&target_id, now).await? {
        return Err(version_conflict().into());
    }
    if !repository.refresh_sale(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: target_id,
        version: None,
    })
}

fn ingredients(recipe_json: &str, quantity_milli: i64) -> Result<Vec<(String, i64)>, BusinessError> {
    let invalid = || {
        request_invalid(
            "POS_PRODUCT_INVALID",
            "The product ingredient recipe is unavailable.",
        )
    };
    let value: serde_json::Value = serde_json::from_str(recipe_json).map_err(|_| invalid())?;
    let parts = value.as_array().ok_or_else(invalid)?;
    parts
        .iter()
        .map(|part| {
            let product_id = part
                .get("productId")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty() && value.len() <= 128)
                .ok_or_else(invalid)?
                .to_owned();
            let base = part
                .get("quantityMilli")
                .and_then(|value| value.as_i64())
                .filter(|value| *value > 0)
                .ok_or_else(invalid)?;
            let quantity = scaled(base, quantity_milli, 1000)?;
            if quantity <= 0 {
                return Err(invalid());
            }
            Ok((product_id, quantity))
        })
        .collect()
}

async fn payment_create<R, F>(
    actor: &PosActor,
    input: &PaymentCreateInput,
    stock: Option<&[PosStockRow]>,
    repository: &mut R,
    finance: &mut F,
    now: TimestampMillis,
) -> Result<PosResult, R::Error>
where
    R: PosCommandRepository,
    F: PosFinancePort,
{
    if !["cash", "transfer", "edc"].contains(&input.method.as_str()) {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "POS_TENDER_UNAVAILABLE",
            "This tender needs a provider that is not mounted yet.",
        )
        .into());
    }
    let sale = repository
        .sale(&actor.company_id, &input.id)
        .await?
        .ok_or_else(not_found)?;
    scope_sale(actor, &sale)?;
    version_matches(sale.version, input.expected_version)?;
    if !matches!(sale.state.as_str(), "open" | "held" | "payment_pending") {
        return Err(conflict("POS_SALE_LOCKED", "Bill is not ready for payment.").into());
    }
    let settings = repository
        .settings(&actor.company_id)
        .await?
        .ok_or_else(setup_required)?;
    if settings
        .accounting_approved_by
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(accounting_review_required().into());
    }
    if !settings.enabled {
        return Err(conflict("POS_DISABLED", "New transactions are temporarily paused.").into());
    }
    finance
        .open_period(&actor.company_id, &sale.business_date)
        .await
        .map_err(R::Error::from)?;
    let shift = repository
        .shift(&actor.company_id, &sale.shift_id)
        .await?
        .ok_or_else(|| conflict("POS_SHIFT_CLOSED", "Cash session is closed."))?;
    if shift.state != "open" {
        return Err(conflict("POS_SHIFT_CLOSED", "Cash session is closed.").into());
    }
    let gross = repository.sale_gross(&sale.id).await?;
    if sale.discount_minor > scaled(gross, settings.cashier_discount_bps, 10_000)?
        && sale.discount_approved_by.is_none()
    {
        return Err(conflict(
            "POS_DISCOUNT_APPROVAL",
            "This discount must be approved before payment.",
        )
        .into());
    }
    let reserved = repository.payment_reserved(&sale.id).await?;
    let outstanding = sale
        .total_minor
        .checked_sub(reserved)
        .ok_or_else(|| {
            request_invalid("POS_PAYMENT_AMOUNT", "Review the payment amount.")
        })?;
    if input.amount_minor > outstanding
        || input.tendered_minor < input.amount_minor
        || (input.method != "cash" && input.tendered_minor != input.amount_minor)
    {
        return Err(request_invalid(
            "POS_PAYMENT_AMOUNT",
            "Review the payment or tendered amount.",
        )
        .into());
    }
    if ["transfer", "edc"].contains(&input.method.as_str())
        && input.reference.trim().chars().count() < 2
    {
        return Err(request_invalid(
            "POS_REFERENCE_REQUIRED",
            "Enter the payment reference.",
        )
        .into());
    }
    let fulfillments = repository.sale_fulfillment_stock(&sale.id).await?;
    let mut availability: std::collections::HashMap<(String, String), i64> =
        std::collections::HashMap::new();
    for row in stock.unwrap_or(&[]) {
        availability.insert(
            (row.warehouse_id.clone(), row.product_id.clone()),
            row.quantity_milli.saturating_sub(row.reserved_milli),
        );
    }
    let mut reserved_fulfillments = Vec::new();
    for fulfillment in &fulfillments {
        if fulfillment.state == "cancelled" {
            return Err(conflict(
                "POS_ITEM_CANCELLED",
                "Remove cancelled items before taking payment.",
            )
            .into());
        }
        if fulfillment.reserved {
            continue;
        }
        for (product_id, quantity) in
            ingredients(&fulfillment.recipe_json, fulfillment.quantity_milli)?
        {
            let key = (fulfillment.warehouse_id.clone(), product_id.clone());
            let available = availability.get(&key).copied().ok_or_else(|| {
                conflict(
                    "POS_STOCK_UNAVAILABLE",
                    "Insufficient product or ingredients for this item.",
                )
            })?;
            if available < quantity {
                return Err(conflict(
                    "POS_STOCK_UNAVAILABLE",
                    "Insufficient product or ingredients for this item.",
                )
                .into());
            }
            availability.insert(key, available - quantity);
            repository
                .reserve_stock(
                    &fulfillment.id,
                    &fulfillment.warehouse_id,
                    &product_id,
                    quantity,
                )
                .await?;
        }
        reserved_fulfillments.push(fulfillment.id.clone());
    }
    for fulfillment_id in reserved_fulfillments {
        repository
            .mark_fulfillment_reserved(&fulfillment_id)
            .await?;
    }
    let payment_id = repository.new_id().await?;
    if !valid_uuid(&payment_id) {
        return Err(source_invalid().into());
    }
    repository
        .insert_payment(&PosPaymentInsert {
            id: payment_id.clone(),
            sale_id: sale.id.clone(),
            company_id: actor.company_id.as_str().to_owned(),
            method: input.method.clone(),
            amount_minor: input.amount_minor,
            tendered_minor: input.tendered_minor,
            reference: input.reference.trim().to_owned(),
            created_by: actor.user_id.as_str().to_owned(),
            now,
        })
        .await?;
    if input.method != "transfer" {
        let accounts = payment_accounts(&settings)?;
        let account = if input.method == "cash" {
            accounts.cash.clone()
        } else {
            accounts.edc.clone()
        };
        let credit = if sale.invoice_id.is_some() {
            accounts.receivable.clone()
        } else {
            accounts.advance.clone()
        };
        let finance_payment_id = repository.new_id().await?;
        if !valid_uuid(&finance_payment_id) {
            return Err(source_invalid().into());
        }
        let source_key = format!("receipt:{payment_id}");
        let journal_id = format!("journal-{source_key}");
        finance
            .record_receipt(&PosReceiptRequest {
                pos_payment_id: payment_id.clone(),
                finance_payment_id: finance_payment_id.clone(),
                company_id: actor.company_id.clone(),
                party_id: sale.party_id.clone(),
                number: format!("POS-R-{payment_id}"),
                payment_date: sale.business_date.clone(),
                currency: sale.currency.clone(),
                amount_minor: input.amount_minor,
                account_id: account.clone(),
                journal_id: journal_id.clone(),
                actor_id: actor.user_id.clone(),
                now,
            })
            .await
            .map_err(R::Error::from)?;
        finance
            .post_journal(&PosJournalRequest {
                company_id: actor.company_id.clone(),
                branch_id: actor.branch_id.clone(),
                chart_id: accounts.chart_id,
                currency: sale.currency.clone(),
                business_date: sale.business_date.clone(),
                source_key,
                source_id: sale.id.clone(),
                actor_id: actor.user_id.clone(),
                lines: vec![
                    PosJournalLine {
                        account_id: account,
                        debit: input.amount_minor,
                        credit: 0,
                    },
                    PosJournalLine {
                        account_id: credit,
                        debit: 0,
                        credit: input.amount_minor,
                    },
                ],
                now,
            })
            .await
            .map_err(R::Error::from)?;
        repository.settle_payment(&payment_id, now).await?;
    }
    if !repository.bump_payment_pending(&sale.id, now).await? {
        return Err(version_conflict().into());
    }
    Ok(PosResult {
        id: payment_id,
        version: Some(sale.version + 1),
    })
}
