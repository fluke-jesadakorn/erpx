//! Native POS register-session, bill, void and refund commands.
//!
//! The repository owns POS tables only. Finance journals, periods, parties,
//! Commerce/Inventory stock and PMS room-charge facts are explicit ports; no
//! cross-owner SQL is constructed here.
#![allow(clippy::too_many_arguments)]
use crate::domain::commands::{
    CashApproveInput, PosActor, PosCashMovement, PosCommand, PosCommandInput, PosFulfillment,
    PosJournalLine, PosJournalRequest, PosLine, PosOperation, PosReceipt, PosRefund, PosRegister,
    PosResult, PosSale, PosSettings, PosShift, RefundRejectInput, RefundRequestInput,
    SaleCreateInput, SaleVersionInput, ShiftCloseInput, ShiftOpenInput, ShiftReviewInput,
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
fn same_person(actor: &PosActor, user: &str) -> bool {
    actor.user_id.as_str() == user
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
    if same_person(actor, &shift.opened_by)
        || shift
            .closed_by
            .as_deref()
            .is_some_and(|user| same_person(actor, user))
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
    if movement.approved_by.is_some() || same_person(actor, &movement.created_by) {
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
    if refund.state != "requested" || same_person(actor, &refund.requested_by) {
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
