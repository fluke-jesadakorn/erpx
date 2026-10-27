//! Closed Finance-owned source facts for current external policy composition.
//! Historical provenance is not current IAM, release or purchase authority.
use super::*;
use erpx_server_shared::public::UserId;
use erpx_server_workflow::public::record_origin::{
    PgRecordOrigin, RecordOriginRequest, lock_record_origin_sources, read_record_origin,
    verify_record_origin,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalReviewRootKind {
    Manual,
    OpeningImport,
}

/// Opaque, non-cloneable same-transaction source consistency proof. Its getters
/// provide owner facts only. Current sealed Identity, Commercial and readiness
/// policy remain mandatory, as does the journal adapter's exact after-image.
pub struct CurrentJournalReviewScope {
    actor: FinanceActor,
    command: JournalReviewCommand,
    journal: ReviewJournal,
    witness: JournalTransactionWitness,
    stable_headers: Vec<Value>,
    root_id: String,
    root_kind: JournalReviewRootKind,
    origins: Vec<PgRecordOrigin>,
}

impl CurrentJournalReviewScope {
    pub fn holding_id(&self) -> &HoldingId {
        &self.actor.holding_id
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.actor.company_id
    }
    pub fn journal_id(&self) -> &str {
        &self.journal.id
    }
    pub fn creator_user_id(&self) -> &str {
        &self.journal.created_by
    }
    pub fn journal_version(&self) -> i64 {
        self.journal.version
    }
    pub fn journal_status(&self) -> &str {
        &self.journal.status
    }
    pub fn root_journal_id(&self) -> &str {
        &self.root_id
    }
    pub fn root_kind(&self) -> JournalReviewRootKind {
        self.root_kind
    }

    /// Preserve the first source identity after canonical post/reversal and on
    /// exact receipt replay. Status/version and their canonical timestamps are
    /// deliberately not source identity; the real writer separately validates
    /// their exact expected transitions and every line/Money/book after-image.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), WorkflowError> {
        require_unit(c).await?;
        if current_transaction_witness(c).await.map_err(to_workflow)? != self.witness {
            return Err(source_changed());
        }
        for origin in &self.origins {
            verify_record_origin(c, origin).await?;
        }
        let fresh = image::load(c, &self.actor, &self.command, false)
            .await
            .map_err(to_workflow)?
            .ok_or_else(source_changed)?;
        let (headers, kind) = closed_identity(c, &fresh).await?;
        if headers != self.stable_headers || kind != self.root_kind {
            return Err(source_changed());
        }
        Ok(())
    }
}

/// Utility locks only: composition must call before the first Holding snapshot,
/// together with its IAM/Commercial/release source locks. No source data is read
/// and no transaction is begun/committed here. Later positive receipt tuple locks
/// reject an old SERIALIZABLE receipt image rather than accepting stale facts.
pub async fn lock_journal_review_sources(c: &mut PgConnection) -> Result<(), WorkflowError> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    lock_record_origin_sources(c).await?;
    image::lock_sources(c).await.map_err(to_workflow)
}

pub async fn inspect_journal_review_source(
    c: &mut PgConnection,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<CurrentJournalReviewScope, WorkflowError> {
    require_unit(c).await?;
    let source = image::load(c, actor, command, false)
        .await
        .map_err(to_workflow)?
        .ok_or_else(unsupported)?;
    let (stable_headers, root_kind) = closed_identity(c, &source).await?;
    let root_id = stable_headers
        .last()
        .and_then(|h| h["id"].as_str())
        .ok_or_else(unsupported)?
        .to_owned();
    let mut origins = Vec::with_capacity(stable_headers.len());
    let mut origin_text_bytes = 0usize;
    for (index, header) in stable_headers.iter().enumerate() {
        let id = string(header, "id")?;
        let root = index + 1 == stable_headers.len();
        let prefix = if root { "journal-" } else { "reversal-" };
        let key = id
            .strip_prefix(prefix)
            .filter(|k| !k.is_empty())
            .ok_or_else(unsupported)?;
        let creator = UserId::new(string(header, "created_by")?).map_err(|_| unsupported())?;
        let parent = if root {
            id
        } else {
            string(header, "source_id")?
        };
        let operation = if root {
            "finance.journal.create"
        } else {
            "finance.journal.reverse"
        };
        let origin = read_record_origin(
            c,
            RecordOriginRequest {
                command_key: key.to_owned(),
                operation: operation.into(),
                record_id: parent.into(),
                creator_user_id: creator,
                company_id: actor.company_id.clone(),
            },
        )
        .await?
        .ok_or_else(unsupported)?;
        origin_text_bytes = origin_text_bytes
            .checked_add(origin.retained_text_bytes())
            .filter(|bytes| *bytes <= 32 * 1024 * 1024)
            .ok_or_else(origin_limit)?;
        if origin.result().as_str().len() > 16 * 1024 {
            return Err(origin_limit());
        }
        let result: Value =
            serde_json::from_str(origin.result().as_str()).map_err(|_| unsupported())?;
        if root {
            if header["journal_number"] != format!("J-{key}")
                || result["id"] != id
                || result["number"] != format!("J-{key}")
                || result["status"] != "draft"
                || result["version"].as_i64() != Some(1)
            {
                return Err(unsupported());
            }
        } else if header["journal_number"] != format!("REV-{key}")
            || result["reversalJournalId"] != id
            || result["reversedJournalId"] != parent
        {
            return Err(unsupported());
        }
        origins.push(origin);
    }
    Ok(CurrentJournalReviewScope {
        actor: source.actor,
        command: source.command,
        journal: source.journal,
        witness: source.witness,
        stable_headers,
        root_id,
        root_kind,
        origins,
    })
}

async fn require_unit(c: &mut PgConnection) -> Result<(), WorkflowError> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

async fn closed_identity(
    c: &mut PgConnection,
    source: &PgJournalReviewSource,
) -> Result<(Vec<Value>, JournalReviewRootKind), WorkflowError> {
    let chain = &source.before.chain;
    if chain.is_empty()
        || chain.len() > 101
        || chain.iter().any(|r| r["cycle"] != false)
        || !image::configuration(&source.before, "secondaryOwners")
            .map_err(to_workflow)?
            .is_empty()
    {
        return Err(unsupported());
    }
    let ids: Vec<_> = chain
        .iter()
        .map(|r| string(r, "id").map(str::to_owned))
        .collect::<Result<_, _>>()?;
    // The relationship, including an inconsistent foreign-Company link, wins
    // over a manual/opening label. Keep this absence predicate under the same
    // canonical utility fence as all projection writers.
    let secondary: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_book_postings WHERE secondary_journal_id=ANY($1))",
    )
    .bind(&ids)
    .fetch_one(&mut *c)
    .await?;
    if secondary {
        return Err(unsupported());
    }
    let mut headers = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let h = source
            .before
            .journals
            .iter()
            .find(|h| h["id"] == *id)
            .ok_or_else(unsupported)?;
        if h["company_id"] != source.actor.company_id.as_str()
            || h["chart_id"] != source.journal.chart_id
            || h["currency"] != source.journal.currency
            || !image::configuration(&source.before, "charts")
                .map_err(to_workflow)?
                .iter()
                .any(|chart| chart["id"] == h["chart_id"] && chart["company_id"] == h["company_id"])
        {
            return Err(unsupported());
        }
        if index + 1 < ids.len()
            && (h["source_type"] != "reversal"
                || h["source_id"] != ids[index + 1]
                || h["is_opening"] != 0)
        {
            return Err(unsupported());
        }
        let mut stable = h.clone();
        let object = stable.as_object_mut().ok_or_else(unsupported)?;
        for key in [
            "status",
            "version",
            "updated_at",
            "posted_by",
            "posted_at",
            "reversed_by",
            "reversed_at",
        ] {
            object.remove(key);
        }
        headers.push(stable);
    }
    let root = headers.last().ok_or_else(unsupported)?;
    let kind = if root["source_type"] == "manual"
        && root["source_id"].is_null()
        && root["is_opening"] == 0
    {
        JournalReviewRootKind::Manual
    } else if ids.len() == 1
        && root["source_type"] == "opening_import"
        && root["source_id"] == root["chart_id"]
        && root["is_opening"] == 1
    {
        JournalReviewRootKind::OpeningImport
    } else {
        return Err(unsupported());
    };
    Ok((headers, kind))
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, WorkflowError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(unsupported)
}
fn unsupported() -> WorkflowError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_JOURNAL_SOURCE_UNSUPPORTED",
        "This source requires its proved owning Finance workflow and current primary book.",
    )
    .into()
}
fn source_changed() -> WorkflowError {
    BusinessError::new(
        FailureKind::Conflict,
        "FINANCE_JOURNAL_SOURCE_CHANGED",
        "The retained Finance source identity changed in this owner transaction.",
    )
    .into()
}
fn origin_limit() -> WorkflowError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_JOURNAL_ORIGIN_LIMIT",
        "The protected Finance origin chain exceeds this bounded review unit.",
    )
    .into()
}
