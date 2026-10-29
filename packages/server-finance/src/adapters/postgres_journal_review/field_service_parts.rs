//! Finance-owned direct target facts only. Actual Inventory and FieldService
//! ownership, personal reviewer policy and purchases remain mandatory outside.
use super::*;
use num_bigint::BigInt;

pub struct FieldServicePartsJournalFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub journal_id: String,
    pub consumption_id: String,
    pub creator_user_id: String,
    pub currency: String,
    pub debit_total: String,
    pub credit_total: String,
}
pub struct CurrentFieldServicePartsJournalTarget {
    actor: FinanceActor,
    command: JournalReviewCommand,
    witness: JournalTransactionWitness,
    facts: FieldServicePartsJournalFacts,
    stable: Value,
}
impl CurrentFieldServicePartsJournalTarget {
    pub fn facts(&self) -> &FieldServicePartsJournalFacts {
        &self.facts
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), WorkflowError> {
        if current_transaction_witness(c).await.map_err(to_workflow)? != self.witness {
            return Err(changed_source());
        }
        let fresh = capture(c, &self.actor, &self.command)
            .await?
            .ok_or_else(changed_source)?;
        if fresh.stable != self.stable
            || fresh.facts.debit_total != self.facts.debit_total
            || fresh.facts.credit_total != self.facts.credit_total
        {
            return Err(changed_source());
        }
        Ok(())
    }
}
pub async fn inspect_field_service_parts_journal_target(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<Option<CurrentFieldServicePartsJournalTarget>, WorkflowError> {
    capture(c, actor, command).await
}
async fn capture(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<Option<CurrentFieldServicePartsJournalTarget>, WorkflowError> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    let kind:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(source_type)<=32 THEN source_type END FROM finance_journals WHERE id=$1 AND company_id=$2 FOR SHARE")
        .bind(command.journal_id.as_str()).bind(actor.company_id.as_str()).fetch_optional(&mut*c).await?;
    match kind {
        None => return Ok(None),
        Some(None) => return Err(unsupported()),
        Some(Some(kind)) if kind != "field_service_parts" => return Ok(None),
        _ => {}
    }
    let Some(source) = image::load(c, actor, command, false)
        .await
        .map_err(to_workflow)?
    else {
        return Ok(None);
    };
    if source.journal.source_type != "field_service_parts" {
        return Ok(None);
    }
    if source.before.chain.len() != 1
        || !image::configuration(&source.before, "secondaryOwners")
            .map_err(to_workflow)?
            .is_empty()
    {
        return Err(unsupported());
    }
    let secondary: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_book_postings WHERE secondary_journal_id=$1)",
    )
    .bind(&source.journal.id)
    .fetch_one(&mut *c)
    .await?;
    if secondary {
        return Err(unsupported());
    }
    let mut stable = source
        .before
        .journals
        .iter()
        .find(|h| h["id"] == source.journal.id)
        .ok_or_else(unsupported)?
        .clone();
    if stable["company_id"] != actor.company_id.as_str()
        || stable["is_opening"] != 0
        || source.before.chain[0]["cycle"] != false
    {
        return Err(unsupported());
    }
    let consumption = source
        .journal
        .source_id
        .as_deref()
        .ok_or_else(unsupported)?;
    let mut debit = BigInt::from(0);
    let mut credit = BigInt::from(0);
    for line in source
        .before
        .lines
        .iter()
        .filter(|line| line["journal_id"] == source.journal.id)
    {
        debit += image::integer(line, "debit_minor").map_err(to_workflow)?;
        credit += image::integer(line, "credit_minor").map_err(to_workflow)?;
    }
    // Original Field Service ownership additionally requires the immutable
    // operational stock_cost in the same currency/amount. This does not choose
    // a current FX quote or inherit the issuer's operational permissions.
    if debit != credit {
        return Err(unsupported());
    }
    let original_cost:Option<bool>=sqlx::query_scalar("SELECT true FROM money_transaction_snapshots m WHERE m.company_id=$1 AND m.source_type='operational_inventory_consumption' AND m.source_id=$2 AND m.amount_kind='stock_cost' AND m.currency=$3 AND m.amount_units=$4::numeric FOR SHARE OF m")
        .bind(actor.company_id.as_str()).bind(consumption).bind(&source.journal.currency).bind(debit.to_string()).fetch_optional(&mut*c).await?;
    if original_cost != Some(true) {
        return Err(unsupported());
    }
    for field in [
        "status",
        "version",
        "updated_at",
        "posted_by",
        "posted_at",
        "reversed_by",
        "reversed_at",
    ] {
        stable
            .as_object_mut()
            .ok_or_else(unsupported)?
            .remove(field);
    }
    let facts = FieldServicePartsJournalFacts {
        holding_id: actor.holding_id.clone(),
        company_id: actor.company_id.clone(),
        journal_id: source.journal.id.clone(),
        consumption_id: consumption.to_owned(),
        creator_user_id: source.journal.created_by.clone(),
        currency: source.journal.currency,
        debit_total: debit.to_string(),
        credit_total: credit.to_string(),
    };
    Ok(Some(CurrentFieldServicePartsJournalTarget {
        actor: source.actor,
        command: source.command,
        witness: source.witness,
        facts,
        stable,
    }))
}
fn unsupported() -> WorkflowError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_JOURNAL_SOURCE_UNSUPPORTED",
        "The actual direct FieldService parts source is unavailable.",
    )
    .into()
}
fn changed_source() -> WorkflowError {
    BusinessError::new(
        FailureKind::Conflict,
        "FINANCE_JOURNAL_SOURCE_CHANGED",
        "The retained Finance parts target changed.",
    )
    .into()
}
