//! Finance-owned direct target facts only. Actual Inventory and Maintenance
//! ownership, personal reviewer policy and purchases remain mandatory outside.
use super::*;
use erpx_server_shared::public::RecordId;
use num_bigint::BigInt;

pub struct MaintenancePartsJournalFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub journal_id: RecordId,
    pub consumption_id: RecordId,
    pub creator_user_id: erpx_server_shared::public::UserId,
    pub currency: String,
    pub debit_total: String,
    pub credit_total: String,
}
pub struct CurrentMaintenancePartsJournalTarget {
    actor: FinanceActor,
    command: JournalReviewCommand,
    witness: JournalTransactionWitness,
    facts: MaintenancePartsJournalFacts,
    stable: Value,
}
impl CurrentMaintenancePartsJournalTarget {
    pub fn facts(&self) -> &MaintenancePartsJournalFacts {
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
pub async fn inspect_maintenance_parts_journal_target(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<Option<CurrentMaintenancePartsJournalTarget>, WorkflowError> {
    capture(c, actor, command).await
}
async fn capture(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<Option<CurrentMaintenancePartsJournalTarget>, WorkflowError> {
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
        Some(Some(kind)) if kind != "maintenance_parts" => return Ok(None),
        _ => {}
    }
    let Some(source) = image::load(c, actor, command, false)
        .await
        .map_err(to_workflow)?
    else {
        return Ok(None);
    };
    if source.journal.source_type != "maintenance_parts" {
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
    let facts = MaintenancePartsJournalFacts {
        holding_id: actor.holding_id.clone(),
        company_id: actor.company_id.clone(),
        journal_id: RecordId::new(&source.journal.id)?,
        consumption_id: RecordId::new(consumption)?,
        creator_user_id: erpx_server_shared::public::UserId::new(&source.journal.created_by)?,
        currency: source.journal.currency,
        debit_total: debit.to_string(),
        credit_total: credit.to_string(),
    };
    Ok(Some(CurrentMaintenancePartsJournalTarget {
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
        "The actual direct Maintenance parts source is unavailable.",
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
