//! POS owner journal posting on the Finance unit. POS proves its own register
//! session command authority; this adapter rechecks the exact captured draft
//! and writes the canonical Finance journal inside the same serializable
//! transaction. A caller-supplied journal id is derived from the POS source key
//! so its deferred posting cannot silently change identity.
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
};
use crate::domain::journal::{JournalError, JournalLineInput};
use crate::domain::money::units_to_decimal;
use crate::public::{FinanceCollation, PgJournalPreparation, PgJournalStore, embedded_currency_rules};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosJournalLine {
    pub account_id: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

/// Exact POS-owned posting facts. The POS command supplies them from its own
/// retained settings/session read; they are not provider or request authority.
#[derive(Clone, Debug)]
pub struct PosJournalPosting {
    pub journal_id: String,
    pub actor: FinanceActor,
    pub chart_id: String,
    pub currency: String,
    pub business_date: String,
    pub source_key: String,
    pub source_id: String,
    pub memo: String,
    pub lines: Vec<PosJournalLine>,
}

struct BoundAuthority {
    actor: FinanceActor,
    draft: JournalDraft,
}

impl JournalAuthority for BoundAuthority {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.description == b.description
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "POS_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized POS posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn journal_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => {
            BusinessError::new(error.kind, error.code, "The POS Finance posting is unavailable.").into()
        }
    }
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

/// Post the POS journal on the caller's open Finance transaction. The entry
/// date must fall in an open period; the canonical preparation revalidates the
/// chart, accounts, currency and functional FX evidence. Never commits.
pub async fn post_pos_journal<C: Clock + Sync>(
    c: &mut PgConnection,
    clock: &C,
    posting: &PosJournalPosting,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "POS_FINANCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    let period_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2 ORDER BY start_date DESC, id LIMIT 1",
    )
    .bind(posting.actor.company_id.as_str())
    .bind(&posting.business_date)
    .fetch_optional(&mut *c)
    .await?;
    let Some(period_id) = period_id else {
        return Err(invalid(
            "POS_PERIOD_CLOSED",
            "An open accounting period is required for this date.",
        ));
    };
    let lines = posting
        .lines
        .iter()
        .map(|line| {
            Ok(JournalLineInput {
                account_id: line.account_id.clone(),
                description: None,
                debit: units_to_decimal(&BigInt::from(line.debit_minor), 2)
                    .map_err(|_| invalid("POS_FINANCE_LINE_INVALID", "The POS posting amount is invalid."))?,
                credit: units_to_decimal(&BigInt::from(line.credit_minor), 2)
                    .map_err(|_| invalid("POS_FINANCE_LINE_INVALID", "The POS posting amount is invalid."))?,
                dimensions: None,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let draft = JournalDraft {
        chart_id: posting.chart_id.clone(),
        period_id,
        entry_date: posting.business_date.clone(),
        currency: posting.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo: posting.memo.clone(),
        source_type: "pos".into(),
        source_id: Some(posting.source_id.clone()),
        lines,
    };
    let key = CommandKey::new(posting.source_key.clone()).map_err(|_| {
        invalid(
            "POS_FINANCE_SOURCE_KEY_INVALID",
            "The POS posting source key is invalid.",
        )
    })?;
    let mut authority = BoundAuthority {
        actor: posting.actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "POS_FINANCE_CURRENCY_UNAVAILABLE",
            "The Finance currency rules are unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid(
            "POS_FINANCE_COLLATION_UNAVAILABLE",
            "The Finance collation is unavailable.",
        )
    })?;
    let plan = {
        let mut ports = PgJournalPreparation::new(
            c,
            &posting.actor,
            &mut authority,
            clock,
            &rules,
            &collation,
        )
        .map_err(journal_error)?;
        prepare_journal(&mut ports, &posting.actor, &draft, &key, JournalStatus::Posted)
            .await
            .map_err(journal_error)?
    };
    if plan.journal_id() != posting.journal_id {
        return Err(invalid(
            "POS_FINANCE_JOURNAL_ID_CHANGED",
            "The POS journal identity changed before posting.",
        ));
    }
    PgJournalStore::new(
        posting.actor.holding_id.clone(),
        posting.actor.company_id.clone(),
        c,
    )
    .persist(&plan)
    .await
    .map_err(journal_error)?;
    let persisted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND company_id=$2 AND status='posted')",
    )
    .bind(plan.journal_id())
    .bind(posting.actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    if !persisted {
        return Err(invalid(
            "POS_FINANCE_JOURNAL_UNAVAILABLE",
            "The POS journal was not persisted.",
        ));
    }
    Ok(plan.journal_id().to_owned())
}
