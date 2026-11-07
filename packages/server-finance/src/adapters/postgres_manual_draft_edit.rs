//! Manual draft replacement in the caller-owned serializable Holding unit.
use crate::adapters::postgres_journal_facts::current_transaction_witness;
use crate::adapters::postgres_manual_draft_provenance::{
    CurrentManualDraftProvenance, inspect_manual_draft_provenance,
    inspect_manual_draft_provenance_with_limit,
};
use crate::application::journal::{
    FinanceActor, JournalDraft, JournalPreparation, JournalStatus, JournalTransactionWitness,
    ManualJournalEditBasis,
};
use crate::application::manual_draft_edit::check_prepared_edit;
use crate::domain::journal::JournalError;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use erpx_server_workflow::public::record_origin::{
    RawRecordOriginRequest, RecordOriginRequest, read_raw_record_origin, read_record_origin,
};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MANUAL_DRAFT_CHANGED",
        "The manual draft changed. Reload and review.",
    )
    .into()
}
fn journal_error(e: JournalError) -> Error {
    BusinessError::new(
        e.kind,
        e.code,
        "The current Finance journal could not be edited.",
    )
    .into()
}
fn witness_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => journal_error(e),
    }
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v.get(k).and_then(Value::as_str).ok_or_else(changed)
}
fn integer(v: &Value, k: &str) -> Result<i64, Error> {
    v.get(k).and_then(Value::as_i64).ok_or_else(changed)
}
async fn image(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    budget: usize,
) -> Result<(Value, Vec<Value>), Error> {
    let journal: Option<Value> = sqlx::query_scalar(
        "SELECT CASE WHEN octet_length(to_jsonb(j)::text)<=least(262144,$3) THEN to_jsonb(j) END FROM finance_journals j WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(company)
    .bind(i64::try_from(budget).unwrap_or(i64::MAX))
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(changed)?;
    let journal = journal.ok_or_else(source_limit)?;
    let available = (2 * 1024 * 1024_usize)
        .min(budget)
        .checked_sub(journal.to_string().len())
        .ok_or_else(source_limit)?;
    let lines: Vec<Option<Value>> = sqlx::query_scalar("WITH source AS MATERIALIZED (SELECT to_jsonb(l) value,l.ordinal,l.id FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY l.ordinal,l.id LIMIT 10001 FOR UPDATE), measured AS (SELECT value,ordinal,id,octet_length(value::text) bytes,sum(octet_length(value::text)) OVER (ORDER BY ordinal,id) total FROM source) SELECT CASE WHEN bytes<=262144 AND total<=$2 THEN value END FROM measured ORDER BY ordinal,id").bind(id).bind(i64::try_from(available).map_err(|_| source_limit())?).fetch_all(c).await?;
    if lines.len() > 10_000 {
        return Err(source_limit());
    }
    let lines = lines
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(source_limit)?;
    Ok((journal, lines))
}
fn source_limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "JOURNAL_DETAIL_SOURCE_LIMIT",
        "The journal exceeds this bounded read.",
    )
    .into()
}
fn image_size(journal: &Value, lines: &[Value]) -> usize {
    lines.iter().fold(journal.to_string().len(), |n, row| {
        n.saturating_add(row.to_string().len())
    })
}
async fn snapshot(
    c: &mut PgConnection,
    journal: &Value,
    lines: &[Value],
) -> Result<(String, String), Error> {
    let value = serde_json::to_string(&json!({"journal":journal,"lines":lines}))
        .map_err(|_| PersistenceError::new("MANUAL_DRAFT_SNAPSHOT_ENCODING"))?;
    let hash = sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
        .bind(&value)
        .fetch_one(c)
        .await?;
    Ok((value, hash))
}
/// Captured source and provenance do not carry present authorization.
pub struct CapturedManualDraftEdit {
    actor: FinanceActor,
    id: String,
    draft: JournalDraft,
    expected: i64,
    reason: String,
    unit: JournalTransactionWitness,
    before: Value,
    before_lines: Vec<Value>,
    provenance: CurrentManualDraftProvenance,
}
/// Retained after-image is bound to the same unit and must be completed after ledger effects.
pub struct AppliedManualDraftEdit {
    actor: FinanceActor,
    id: String,
    version: i64,
    preparation_id: String,
    unit: JournalTransactionWitness,
    after: Value,
    after_lines: Vec<Value>,
    before_hash: String,
    after_hash: String,
    anchor_actor: Option<String>,
    anchor_fingerprint: Option<String>,
}
pub async fn capture_manual_draft_edit(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    draft: &JournalDraft,
    expected_version: i64,
    reason: &str,
) -> Result<CapturedManualDraftEdit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    let provenance = inspect_manual_draft_provenance(c, &actor.company_id, id).await?;
    let (before, before_lines) = image(
        c,
        actor.company_id.as_str(),
        id,
        (4 * 1024 * 1024_usize).saturating_sub(provenance.retained_text_bytes()),
    )
    .await?;
    if provenance
        .retained_text_bytes()
        .saturating_add(image_size(&before, &before_lines))
        > 4 * 1024 * 1024
    {
        return Err(source_limit());
    }
    let basis = ManualJournalEditBasis {
        status: text(&before, "status")?,
        source_type: text(&before, "source_type")?,
        source_id: before["source_id"].as_str(),
        is_opening: integer(&before, "is_opening")? != 0,
        version: integer(&before, "version")?,
        chart_id: text(&before, "chart_id")?,
        currency: text(&before, "currency")?,
    };
    crate::application::journal::validate_manual_journal_edit(&basis, draft, expected_version)
        .map_err(journal_error)?;
    let unit = current_transaction_witness(c)
        .await
        .map_err(witness_error)?;
    Ok(CapturedManualDraftEdit {
        actor: actor.clone(),
        id: id.into(),
        draft: draft.clone(),
        expected: expected_version,
        reason: reason.into(),
        unit,
        before,
        before_lines,
        provenance,
    })
}
pub async fn apply_manual_draft_edit(
    c: &mut PgConnection,
    captured: CapturedManualDraftEdit,
    plan: &JournalPreparation,
) -> Result<AppliedManualDraftEdit, Error> {
    if !c.is_in_transaction()
        || current_transaction_witness(c)
            .await
            .map_err(witness_error)?
            != captured.unit
        || plan.transaction_witness.as_ref() != Some(&captured.unit)
    {
        return Err(changed());
    }
    captured
        .provenance
        .require_current_with_limit(
            c,
            (4 * 1024 * 1024_usize)
                .saturating_sub(image_size(&captured.before, &captured.before_lines)),
        )
        .await?;
    let anchor_fingerprint = if captured.expected == 1 {
        let key = captured
            .id
            .strip_prefix("journal-")
            .filter(|key| !key.is_empty())
            .ok_or_else(changed)?;
        let origin = read_raw_record_origin(
            c,
            RawRecordOriginRequest {
                command_key: key.into(),
                operation: "finance.journal.create".into(),
                record_id: captured.id.clone(),
                creator_user_id: captured.provenance.original_actor().into(),
                company_id: captured.actor.company_id.clone(),
            },
        )
        .await?
        .ok_or_else(changed)?;
        Some(origin.receipt_fingerprint().to_owned())
    } else {
        None
    };
    let used = captured
        .provenance
        .retained_text_bytes()
        .saturating_add(image_size(&captured.before, &captured.before_lines))
        .saturating_add(anchor_fingerprint.as_ref().map_or(0, String::len));
    let (before, lines) = image(
        c,
        captured.actor.company_id.as_str(),
        &captured.id,
        (4 * 1024 * 1024_usize).saturating_sub(used),
    )
    .await?;
    if captured
        .provenance
        .retained_text_bytes()
        .saturating_add(image_size(&captured.before, &captured.before_lines))
        .saturating_add(image_size(&before, &lines))
        > 4 * 1024 * 1024
    {
        return Err(source_limit());
    }
    if before != captured.before
        || lines != captured.before_lines
        || !matches!(plan.result.status, JournalStatus::Draft)
    {
        return Err(changed());
    }
    let basis = ManualJournalEditBasis {
        status: text(&before, "status")?,
        source_type: text(&before, "source_type")?,
        source_id: before["source_id"].as_str(),
        is_opening: integer(&before, "is_opening")? != 0,
        version: integer(&before, "version")?,
        chart_id: text(&before, "chart_id")?,
        currency: text(&before, "currency")?,
    };
    let version = check_prepared_edit(
        &captured.actor,
        &captured.draft,
        &basis,
        captured.expected,
        plan,
    )
    .map_err(journal_error)?;
    let (before_snapshot, computed_before_hash) = snapshot(c, &before, &lines).await?;
    let before_hash = if captured.expected == 1 {
        computed_before_hash
    } else {
        sqlx::query_scalar::<_,String>("SELECT snapshot_sha256 FROM finance_manual_draft_editions WHERE company_id=$1 AND journal_id=$2 AND journal_version=$3 FOR SHARE").bind(captured.actor.company_id.as_str()).bind(&captured.id).bind(captured.expected).fetch_optional(&mut *c).await?.ok_or_else(changed)?
    };
    let anchor_actor =
        (captured.expected == 1).then(|| captured.provenance.original_actor().to_owned());
    if let Some(actor) = &anchor_actor {
        let n=sqlx::query("INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES($1,$2,$3,1,$4,$5,'Retained original draft before the first edit',$6,$7)").bind(format!("{}:v1",captured.id)).bind(captured.actor.company_id.as_str()).bind(&captured.id).bind(&before_snapshot).bind(&before_hash).bind(actor).bind(plan.now.get()).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed());
        }
    }
    let count=sqlx::query("UPDATE finance_journals SET period_id=$1,entry_date=$2,memo=$3,fx_rate_micros=$4,created_by=$5,version=$6,updated_at=$7 WHERE id=$8 AND company_id=$9 AND version=$10 AND status='draft' AND source_type='manual'")
  .bind(&captured.draft.period_id).bind(&captured.draft.entry_date).bind(&captured.draft.memo).bind(plan.fx).bind(captured.actor.user_id.as_str()).bind(version).bind(plan.now.get()).bind(&captured.id).bind(captured.actor.company_id.as_str()).bind(captured.expected).execute(&mut *c).await?.rows_affected();
    if count != 1 {
        return Err(changed());
    }
    sqlx::query("DELETE FROM finance_journal_lines WHERE journal_id=$1")
        .bind(&captured.id)
        .execute(&mut *c)
        .await?;
    let mut expected_lines = Vec::with_capacity(plan.amounts.lines.len());
    for (i, line) in plan.amounts.lines.iter().enumerate() {
        let dim = serde_json::to_string(&line.dimensions.clone().unwrap_or_default())
            .map_err(|_| PersistenceError::new("JOURNAL_DIMENSION_ENCODING_FAILED"))?;
        let ordinal = i64::try_from(i).map_err(|_| PersistenceError::new("JOURNAL_LINE_LIMIT"))?;
        expected_lines.push(json!({"id":format!("{}:v{}:{i}",captured.id,version),"journal_id":captured.id,"account_id":line.account_id,"description":line.description.as_deref().unwrap_or(""),"debit_minor":line.debit,"credit_minor":line.credit,"base_debit_minor":line.base_debit,"base_credit_minor":line.base_credit,"dimension_json":dim,"ordinal":ordinal}));
        sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(format!("{}:v{}:{i}",captured.id,version)).bind(&captured.id).bind(&line.account_id).bind(line.description.as_deref().unwrap_or("")).bind(line.debit).bind(line.credit).bind(line.base_debit).bind(line.base_credit).bind(dim).bind(ordinal).execute(&mut *c).await?;
    }
    let used = used
        .saturating_add(image_size(&before, &lines))
        .saturating_add(before_snapshot.len())
        .saturating_add(
            expected_lines
                .iter()
                .map(|v| v.to_string().len())
                .sum::<usize>(),
        );
    let (after, after_lines) = image(
        c,
        captured.actor.company_id.as_str(),
        &captured.id,
        (4 * 1024 * 1024_usize).saturating_sub(used),
    )
    .await?;
    let mut expected_after = before.clone();
    expected_after["period_id"] = captured.draft.period_id.clone().into();
    expected_after["entry_date"] = captured.draft.entry_date.clone().into();
    expected_after["memo"] = captured.draft.memo.clone().into();
    expected_after["fx_rate_micros"] = plan.fx.into();
    expected_after["created_by"] = captured.actor.user_id.as_str().into();
    expected_after["version"] = version.into();
    expected_after["updated_at"] = plan.now.get().into();
    if after != expected_after || after_lines != expected_lines {
        return Err(changed());
    }
    let (after_snapshot, after_hash) = snapshot(c, &after, &after_lines).await?;
    if used
        .saturating_add(image_size(&after, &after_lines))
        .saturating_add(after_snapshot.len())
        > 4 * 1024 * 1024
    {
        return Err(source_limit());
    }
    sqlx::query("INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
 .bind(format!("{}:v{}",captured.id,version)).bind(captured.actor.company_id.as_str()).bind(&captured.id).bind(version).bind(&after_snapshot).bind(&after_hash).bind(&captured.reason).bind(captured.actor.user_id.as_str()).bind(plan.now.get()).execute(&mut *c).await?;
    Ok(AppliedManualDraftEdit {
        actor: captured.actor,
        id: captured.id,
        version,
        preparation_id: plan.journal_id.clone(),
        unit: captured.unit,
        after,
        after_lines,
        before_hash,
        after_hash,
        anchor_actor,
        anchor_fingerprint,
    })
}
async fn receipt_fingerprint(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    key: &str,
    user: &str,
    operation: &str,
    version: i64,
) -> Result<String, Error> {
    let user_id = erpx_server_shared::public::UserId::new(user).map_err(|_| changed())?;
    let typed = read_record_origin(
        c,
        RecordOriginRequest {
            command_key: key.into(),
            operation: operation.into(),
            record_id: id.into(),
            creator_user_id: user_id,
            company_id: actor.company_id.clone(),
        },
    )
    .await?
    .ok_or_else(changed)?;
    let result: Value = serde_json::from_str(typed.result().as_str()).map_err(|_| changed())?;
    if result["id"] != id
        || result["status"] != "draft"
        || result["version"].as_i64() != Some(version)
        || version == 1 && result["number"] != format!("J-{key}")
    {
        return Err(changed());
    }
    let raw = read_raw_record_origin(
        c,
        RawRecordOriginRequest {
            command_key: key.into(),
            operation: operation.into(),
            record_id: id.into(),
            creator_user_id: user.into(),
            company_id: actor.company_id.clone(),
        },
    )
    .await?
    .ok_or_else(changed)?;
    Ok(raw.receipt_fingerprint().to_owned())
}
#[expect(
    clippy::too_many_arguments,
    reason = "The immutable link is checked against each exact source and receipt field."
)]
async fn link(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    version: i64,
    key: &str,
    user: &str,
    hash: &str,
    previous: Option<(i64, &str)>,
    receipt: &str,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO finance_manual_draft_receipt_links(journal_id,journal_version,company_id,command_key,actor_id,snapshot_sha256,previous_version,previous_sha256,receipt_sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(journal_id,journal_version) DO NOTHING")
 .bind(id).bind(version).bind(actor.company_id.as_str()).bind(key).bind(user).bind(hash).bind(previous.map(|p|p.0)).bind(previous.map(|p|p.1)).bind(receipt).execute(&mut *c).await?;
    let actual:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(l) FROM finance_manual_draft_receipt_links l WHERE journal_id=$1 AND journal_version=$2 FOR SHARE").bind(id).bind(version).fetch_optional(c).await?;
    let expected = json!({"journal_id":id,"journal_version":version,"company_id":actor.company_id.as_str(),"command_key":key,"actor_id":user,"snapshot_sha256":hash,"previous_version":previous.map(|p|p.0),"previous_sha256":previous.map(|p|p.1),"receipt_sha256":receipt});
    if actual.as_ref() != Some(&expected) {
        return Err(changed());
    }
    Ok(())
}
/// Called after the real Workflow receipt, audit and outbox, within the same unit.
/// Repeated retain verifies existing immutable links and never commits independently.
pub async fn retain_manual_draft_edit(
    c: &mut PgConnection,
    applied: &AppliedManualDraftEdit,
    key: &CommandKey,
) -> Result<CurrentManualDraftProvenance, Error> {
    if !c.is_in_transaction()
        || current_transaction_witness(c)
            .await
            .map_err(witness_error)?
            != applied.unit
    {
        return Err(changed());
    }
    let held = image_size(&applied.after, &applied.after_lines)
        .saturating_add(applied.anchor_fingerprint.as_ref().map_or(0, String::len));
    let (journal, lines) = image(
        c,
        applied.actor.company_id.as_str(),
        &applied.id,
        (4 * 1024 * 1024_usize).saturating_sub(held),
    )
    .await?;
    if applied.preparation_id != format!("journal-{}", key.as_str())
        || journal != applied.after
        || lines != applied.after_lines
    {
        return Err(changed());
    }
    if let Some(original) = &applied.anchor_actor {
        let original_key = applied
            .id
            .strip_prefix("journal-")
            .filter(|s| !s.is_empty())
            .ok_or_else(changed)?;
        let raw = receipt_fingerprint(
            c,
            &applied.actor,
            &applied.id,
            original_key,
            original,
            "finance.journal.create",
            1,
        )
        .await?;
        if applied.anchor_fingerprint.as_deref() != Some(raw.as_str()) {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMAND_ORIGIN_CHANGED",
                "Record provenance changed during the command.",
            )
            .into());
        }
        link(
            c,
            &applied.actor,
            &applied.id,
            1,
            original_key,
            original,
            &applied.before_hash,
            None,
            &raw,
        )
        .await?;
    }
    let editor = applied.actor.user_id.as_str();
    let raw = receipt_fingerprint(
        c,
        &applied.actor,
        &applied.id,
        key.as_str(),
        editor,
        "finance.manual-draft.edit",
        applied.version,
    )
    .await?;
    link(
        c,
        &applied.actor,
        &applied.id,
        applied.version,
        key.as_str(),
        editor,
        &applied.after_hash,
        Some((applied.version - 1, &applied.before_hash)),
        &raw,
    )
    .await?;
    let held = held.saturating_add(image_size(&journal, &lines));
    inspect_manual_draft_provenance_with_limit(
        c,
        &applied.actor.company_id,
        &applied.id,
        (4 * 1024 * 1024_usize).saturating_sub(held),
    )
    .await
}
