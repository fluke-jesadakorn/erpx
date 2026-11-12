//! Native Finance revenue-obligation recognition.
//!
//! The command runs on the caller's serializable Holding connection. It owns
//! the obligation guard, the canonical Finance journal create+post through the
//! existing preparation/store ports, the obligation CAS and the Workflow
//! receipt/audit/outbox in one unit. It never commits and never constructs
//! authority; composition supplies the already authorized Finance actor.
use super::postgres_journal::PgJournalStore;
use super::postgres_journal_facts::{PgJournalPreparation, embedded_currency_rules};
use super::sales_invoice_create::{SalesInvoiceCommandActor, authority_json, fingerprint, receipt};
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
};
use crate::domain::company_money::FinanceCollation;
use crate::domain::journal::{JournalError, JournalLineInput, journal_minor};
use crate::domain::money::units_to_decimal;
use crate::domain::revenue::recognize_revenue_amount;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const OPERATION: &str = "finance.revenue.recognize";

/// Exact browser request shape retained from the previous Finance command.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevenueRecognitionInput {
    pub amount: String,
    pub period_id: String,
    pub entry_date: String,
}

/// Result shape returned to the existing workplace transport.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RevenueRecognitionResult {
    pub id: String,
    pub journal_id: String,
    pub recognized: String,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_REVENUE_RECOGNITION_UNAVAILABLE",
        "The revenue recognition could not be completed.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.encode_utf16().count() <= 200 && !value.chars().any(char::is_control)
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && time::Date::parse(
            value,
            &time::macros::format_description!("[year]-[month]-[day]"),
        )
        .is_ok()
}

fn validate_input(obligation_id: &str, input: &RevenueRecognitionInput) -> Result<(), Error> {
    if !valid_identifier(obligation_id) {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "OBLIGATION_NOT_FOUND",
            "Revenue obligation not found.",
        )
        .into());
    }
    if !valid_identifier(&input.period_id) || !valid_date(&input.entry_date) {
        return Err(invalid(
            "VALIDATION_FAILED",
            "Revenue recognition period and date are required.",
        ));
    }
    Ok(())
}

/// Current journal authority is bound to the exact draft built by this command.
struct BoundRevenueDraft {
    actor: FinanceActor,
    draft: JournalDraft,
}

impl JournalAuthority for BoundRevenueDraft {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != &self.actor || draft != &self.draft {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "REVENUE_RECOGNITION_BINDING_REQUIRED",
                message: "Use the current authorized revenue recognition.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn journal_failure(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The revenue recognition journal could not be prepared.",
        )
        .into(),
    }
}

async fn replay(
    c: &mut PgConnection,
    command: &CommandKey,
    request_hash: &str,
) -> Result<Option<RevenueRecognitionResult>, Error> {
    let saved = sqlx::query(
        "SELECT operation,request_hash,result_json FROM command_receipts \
         WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = saved else {
        return Ok(None);
    };
    let saved_operation: String = row.try_get("operation")?;
    let saved_hash: String = row.try_get("request_hash")?;
    if saved_operation != OPERATION || saved_hash != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.and_then(|value| serde_json::from_str::<RevenueRecognitionResult>(&value).ok())
        .map(Some)
        .ok_or_else(unavailable)
}

fn scope(actor: &SalesInvoiceCommandActor, finance: &FinanceActor) -> Result<(), Error> {
    if actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.company_id != finance.company_id.as_str()
        || actor.user_id != finance.user_id.as_str()
        || actor.assignment_id != finance.assignment_id.as_str()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FINANCE_SCOPE_REQUIRED",
            "Use the current Company Finance assignment.",
        )
        .into());
    }
    Ok(())
}

/// Recognize revenue against one obligation on the caller's open serializable
/// transaction. Lock order: command receipt, obligation row (`FOR UPDATE`),
/// active revenue account (`FOR SHARE`), then the canonical journal store's
/// period/chart/accounts (`FOR SHARE`) before inserting the posted journal.
/// The obligation CAS and receipt/audit/outbox complete the same unit.
pub async fn recognize_revenue<C: Clock + Sync>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    finance: &FinanceActor,
    clock: &C,
    command: &CommandKey,
    obligation_id: &str,
    input: &RevenueRecognitionInput,
) -> Result<RevenueRecognitionResult, Error> {
    serializable(c).await?;
    scope(actor, finance)?;
    validate_input(obligation_id, input)?;
    let amount_minor = journal_minor(&input.amount, 2).map_err(|error| {
        BusinessError::new(
            error.kind,
            error.code,
            "Recognition requires a valid monetary amount.",
        )
    })?;
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, OPERATION, obligation_id, &input_json);
    let authority = authority_json(actor);
    if let Some(previous) = replay(c, command, &request_hash).await? {
        return Ok(previous);
    }

    let obligation = sqlx::query(
        "SELECT o.id,o.description,o.allocated_minor,o.recognized_minor,\
                o.revenue_account_id,o.deferred_account_id,c.company_id,c.currency \
         FROM finance_revenue_obligations o \
         JOIN finance_revenue_contracts c ON c.id=o.contract_id \
         WHERE o.id=$1 AND c.company_id=$2 FOR UPDATE OF o",
    )
    .bind(obligation_id)
    .bind(finance.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "OBLIGATION_NOT_FOUND",
            "Revenue obligation not found.",
        )
    })?;
    let description: String = obligation.try_get("description")?;
    let allocated_minor: i64 = obligation.try_get("allocated_minor")?;
    let recognized_minor: i64 = obligation.try_get("recognized_minor")?;
    let revenue_account_id: String = obligation.try_get("revenue_account_id")?;
    let deferred_account_id: String = obligation.try_get("deferred_account_id")?;
    let currency: String = obligation.try_get("currency")?;
    let outcome = recognize_revenue_amount(allocated_minor, recognized_minor, amount_minor)?;

    let chart_id: String = sqlx::query_scalar(
        "SELECT chart_id FROM finance_accounts \
         WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE",
    )
    .bind(&revenue_account_id)
    .bind(finance.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| BusinessError::invalid("ACCOUNT_INVALID", "Revenue account is unavailable."))?;
    let amount = units_to_decimal(&num_bigint::BigInt::from(amount_minor), 2)
        .map_err(|_| invalid("INVALID_MONEY", "Recognition amount is invalid."))?;
    let draft = JournalDraft {
        chart_id,
        period_id: input.period_id.clone(),
        entry_date: input.entry_date.clone(),
        currency,
        fx_rate: None,
        is_opening: false,
        memo: format!("Revenue recognition {description}"),
        source_type: "revenue_recognition".into(),
        source_id: Some(obligation_id.to_owned()),
        lines: vec![
            JournalLineInput {
                account_id: deferred_account_id,
                description: None,
                debit: amount.clone(),
                credit: "0".into(),
                dimensions: None,
            },
            JournalLineInput {
                account_id: revenue_account_id,
                description: None,
                debit: "0".into(),
                credit: amount.clone(),
                dimensions: None,
            },
        ],
    };
    let mut preparation_authority = BoundRevenueDraft {
        actor: finance.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "CURRENCY_CATALOGUE_UNAVAILABLE",
            "Currency rules are unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid(
            "FINANCE_COLLATION_UNAVAILABLE",
            "Finance collation is unavailable.",
        )
    })?;
    let plan = {
        let mut ports = PgJournalPreparation::new(
            &mut *c,
            finance,
            &mut preparation_authority,
            clock,
            &rules,
            &collation,
        )
        .map_err(journal_failure)?;
        prepare_journal(&mut ports, finance, &draft, command, JournalStatus::Posted)
            .await
            .map_err(journal_failure)?
    };
    PgJournalStore::new(
        finance.holding_id.clone(),
        finance.company_id.clone(),
        &mut *c,
    )
    .persist(&plan)
    .await
    .map_err(journal_failure)?;

    let updated: Option<i64> = sqlx::query_scalar(
        "UPDATE finance_revenue_obligations o \
         SET recognized_minor=$1,status=$2 \
         FROM finance_revenue_contracts c \
         WHERE o.contract_id=c.id AND o.id=$3 AND c.company_id=$4 \
           AND o.recognized_minor=$5 AND o.allocated_minor=$6 \
         RETURNING o.recognized_minor",
    )
    .bind(outcome.recognized_minor)
    .bind(outcome.status)
    .bind(obligation_id)
    .bind(finance.company_id.as_str())
    .bind(recognized_minor)
    .bind(allocated_minor)
    .fetch_optional(&mut *c)
    .await?;
    if updated != Some(outcome.recognized_minor) {
        return Err(conflict(
            "REVENUE_OBLIGATION_CHANGED",
            "The revenue obligation changed. Reload and review again.",
        ));
    }

    let result = RevenueRecognitionResult {
        id: obligation_id.to_owned(),
        journal_id: plan.journal_id().to_owned(),
        recognized: amount,
    };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        OPERATION,
        &request_hash,
        obligation_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        clock.now().get(),
    )
    .await?;
    Ok(result)
}
