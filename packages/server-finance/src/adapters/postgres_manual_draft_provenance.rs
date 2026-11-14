//! Closed, transaction-retained manual draft edition provenance.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use erpx_server_workflow::public::record_origin::{
    PgRawRecordOrigin, PgRecordOrigin, RawRecordOriginRequest, RecordOriginRequest,
    read_raw_record_origin, read_record_origin_with_limit, verify_raw_record_origin,
    verify_record_origin,
};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

fn denied() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MANUAL_DRAFT_PROVENANCE_INVALID",
        "The manual journal provenance is unavailable.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "JOURNAL_DETAIL_SOURCE_LIMIT",
        "The journal exceeds this bounded read.",
    )
    .into()
}
fn charge(total: &mut usize, additional: usize, limit_bytes: usize) -> Result<(), Error> {
    *total = total
        .checked_add(additional)
        .filter(|n| *n <= limit_bytes)
        .ok_or_else(limit)?;
    Ok(())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.get(key).and_then(Value::as_str).ok_or_else(denied)
}
fn number(v: &Value, key: &str) -> Result<i64, Error> {
    v.get(key).and_then(Value::as_i64).ok_or_else(denied)
}
fn terminal_shape(journal: &Value) -> Result<(), Error> {
    let timestamp = |name| -> Result<i64, Error> {
        let value = number(journal, name)?;
        crate::public::journal_detail::FinanceReadInteger::new(value).map_err(|_| denied())?;
        Ok(value)
    };
    timestamp("updated_at")?;
    let present_actor = |name| -> Result<(), Error> {
        UserId::new(text(journal, name)?).map_err(|_| denied())?;
        Ok(())
    };
    let null = |name| journal.get(name) == Some(&Value::Null);
    match text(journal, "status")? {
        "draft"
            if null("posted_by")
                && null("posted_at")
                && null("reversed_by")
                && null("reversed_at") =>
        {
            Ok(())
        }
        "posted" if null("reversed_by") && null("reversed_at") => {
            present_actor("posted_by")?;
            if timestamp("posted_at")? != timestamp("updated_at")? {
                return Err(denied());
            }
            Ok(())
        }
        "reversed" => {
            present_actor("posted_by")?;
            present_actor("reversed_by")?;
            timestamp("posted_at")?;
            if timestamp("reversed_at")? != timestamp("updated_at")? {
                return Err(denied());
            }
            Ok(())
        }
        _ => Err(denied()),
    }
}

/// Owner-only source fence, acquired before the first IAM snapshot.
pub async fn lock_manual_draft_provenance(c: &mut PgConnection, write: bool) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let sql = if write {
        "LOCK TABLE finance_manual_draft_editions,finance_manual_draft_receipt_links IN SHARE ROW EXCLUSIVE MODE"
    } else {
        "LOCK TABLE finance_manual_draft_editions,finance_manual_draft_receipt_links IN SHARE MODE"
    };
    sqlx::query(sql).execute(c).await?;
    Ok(())
}

/// Provenance is historical evidence only and never supplies present authority.
pub struct CurrentManualDraftProvenance {
    company: CompanyId,
    id: String,
    original_actor: String,
    chain: Vec<Value>,
    typed: Vec<PgRecordOrigin>,
    raw: Vec<PgRawRecordOrigin>,
    retained_bytes: usize,
    limit_bytes: usize,
}
impl CurrentManualDraftProvenance {
    pub fn original_actor(&self) -> &str {
        &self.original_actor
    }
    pub fn retained_text_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current_with_limit(c, self.limit_bytes).await
    }
    pub async fn require_current_with_limit(
        &self,
        c: &mut PgConnection,
        limit_bytes: usize,
    ) -> Result<(), Error> {
        if self.retained_bytes >= self.limit_bytes.min(limit_bytes) {
            return Err(limit());
        }
        for proof in &self.typed {
            verify_record_origin(c, proof).await?;
        }
        for proof in &self.raw {
            verify_raw_record_origin(c, proof).await?;
        }
        let fresh = inspect_manual_draft_provenance_with_limit(
            c,
            &self.company,
            &self.id,
            self.limit_bytes
                .min(limit_bytes)
                .saturating_sub(self.retained_bytes),
        )
        .await?;
        if fresh.original_actor != self.original_actor || fresh.chain != self.chain {
            return Err(denied());
        }
        Ok(())
    }
}

/// Read and validate the entire closed chain and the current editable image.
/// Unedited roots have no editions; edited roots must have a complete chain.
pub async fn inspect_manual_draft_provenance(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentManualDraftProvenance, Error> {
    inspect_manual_draft_provenance_with_limit(c, company, id, 4 * 1024 * 1024).await
}
pub async fn inspect_manual_draft_provenance_with_limit(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    limit_bytes: usize,
) -> Result<CurrentManualDraftProvenance, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    let key = id
        .strip_prefix("journal-")
        .filter(|s| !s.is_empty())
        .ok_or_else(denied)?;
    let header: Option<Value> = sqlx::query_scalar(
        "SELECT CASE WHEN octet_length(to_jsonb(j)::text)<=least(262144,$3) THEN to_jsonb(j) END FROM finance_journals j WHERE j.id=$1 AND j.company_id=$2 FOR SHARE",
    )
    .bind(id)
    .bind(company.as_str())
    .bind(i64::try_from(limit_bytes).unwrap_or(i64::MAX))
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(denied)?;
    let header = header.ok_or_else(limit)?;
    terminal_shape(&header)?;
    let mut held = 0usize;
    charge(&mut held, header.to_string().len(), limit_bytes)?;
    if text(&header, "source_type")? != "manual"
        || header["source_id"] != Value::Null
        || number(&header, "is_opening")? != 0
        || text(&header, "journal_number")? != format!("J-{key}")
    {
        return Err(denied());
    }
    let chain: Vec<Option<Value>> = sqlx::query_scalar("WITH source AS MATERIALIZED (SELECT COALESCE(e.journal_version,l.journal_version) version,jsonb_build_object('edition',to_jsonb(e),'link',to_jsonb(l),'actual_sha256',encode(sha256(convert_to(e.snapshot_json,'UTF8')),'hex')) value FROM finance_manual_draft_editions e FULL JOIN finance_manual_draft_receipt_links l ON l.journal_id=e.journal_id AND l.journal_version=e.journal_version WHERE e.journal_id=$1 OR l.journal_id=$1 ORDER BY COALESCE(e.journal_version,l.journal_version) LIMIT 10001), measured AS (SELECT value,version,octet_length(value::text) bytes,sum(octet_length(value::text)) OVER (ORDER BY version) size FROM source) SELECT CASE WHEN bytes<=262144 AND size<=least(2097152,$2) THEN value END FROM measured ORDER BY version")
        .bind(id).bind(i64::try_from(limit_bytes.saturating_sub(held)).unwrap_or(i64::MAX)).fetch_all(&mut *c).await?;
    if chain.len() > 10_000 || chain.len() == 1 {
        return Err(denied());
    }
    let chain = chain
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(limit)?;
    for row in &chain {
        charge(&mut held, row.to_string().len(), limit_bytes)?;
    }
    let latest = if chain.is_empty() {
        1
    } else {
        i64::try_from(chain.len()).map_err(|_| denied())?
    };
    let expected_version = match text(&header, "status")? {
        "draft" => latest,
        "posted" => latest + 1,
        "reversed" => latest + 2,
        _ => return Err(denied()),
    };
    if number(&header, "version")? != expected_version {
        return Err(denied());
    }
    let original_actor = if chain.is_empty() {
        text(&header, "created_by")?.to_owned()
    } else {
        text(&chain[0]["link"], "actor_id")?.to_owned()
    };
    let mut typed = Vec::new();
    let mut raw = Vec::new();
    if chain.is_empty() {
        let receipt_limit = limit_bytes.saturating_sub(held).saturating_sub(
            key.len() + id.len() + original_actor.len() + company.as_str().len() + 1024,
        );
        let proof = read_record_origin_with_limit(
            c,
            RecordOriginRequest {
                command_key: key.into(),
                operation: "finance.journal.create".into(),
                record_id: id.into(),
                creator_user_id: UserId::new(&original_actor).map_err(|_| denied())?,
                company_id: company.clone(),
            },
            receipt_limit,
        )
        .await?
        .ok_or_else(denied)?;
        charge(&mut held, proof.retained_text_bytes(), limit_bytes)?;
        if proof.result().as_str().len() > 16 * 1024 {
            return Err(limit());
        }
        let result: Value = serde_json::from_str(proof.result().as_str()).map_err(|_| denied())?;
        if result["id"] != id
            || result["number"] != format!("J-{key}")
            || result["status"] != "draft"
            || result["version"] != 1
        {
            return Err(denied());
        }
        typed.push(proof);
    } else {
        let mut previous_hash: Option<String> = None;
        for (i, row) in chain.iter().enumerate() {
            let e = &row["edition"];
            let l = &row["link"];
            let version = i64::try_from(i + 1).map_err(|_| denied())?;
            let hash = text(e, "snapshot_sha256")?;
            if number(e, "journal_version")? != version
                || number(l, "journal_version")? != version
                || text(e, "journal_id")? != id
                || text(l, "journal_id")? != id
                || text(e, "company_id")? != company.as_str()
                || text(l, "company_id")? != company.as_str()
                || text(l, "snapshot_sha256")? != hash
                || row["actual_sha256"] != hash
                || text(e, "prepared_by")? != text(l, "actor_id")?
            {
                return Err(denied());
            }
            if version == 1 {
                if l["previous_version"] != Value::Null
                    || l["previous_sha256"] != Value::Null
                    || text(l, "command_key")? != key
                {
                    return Err(denied());
                }
            } else if number(l, "previous_version")? != version - 1
                || Some(text(l, "previous_sha256")?) != previous_hash.as_deref()
            {
                return Err(denied());
            }
            let snapshot: Value =
                serde_json::from_str(text(e, "snapshot_json")?).map_err(|_| denied())?;
            let journal = &snapshot["journal"];
            terminal_shape(journal)?;
            if journal["id"] != id
                || journal["company_id"] != company.as_str()
                || number(journal, "version")? != version
                || journal["status"] != "draft"
                || journal["created_by"] != l["actor_id"]
            {
                return Err(denied());
            }
            for field in [
                "id",
                "company_id",
                "chart_id",
                "currency",
                "journal_number",
                "created_at",
                "source_type",
                "source_id",
                "is_opening",
            ] {
                if journal[field] != header[field] {
                    return Err(denied());
                }
            }
            if version == latest {
                let mut actual = header.clone();
                let mut expected = journal.clone();
                if text(&header, "status")? != "draft" {
                    for field in [
                        "status",
                        "version",
                        "updated_at",
                        "posted_by",
                        "posted_at",
                        "reversed_by",
                        "reversed_at",
                    ] {
                        actual.as_object_mut().ok_or_else(denied)?.remove(field);
                        expected.as_object_mut().ok_or_else(denied)?.remove(field);
                    }
                }
                if actual != expected {
                    return Err(denied());
                }
                let lines: Vec<Option<Value>> = sqlx::query_scalar("WITH source AS MATERIALIZED (SELECT to_jsonb(l) value,l.ordinal,l.id FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY l.ordinal,l.id LIMIT 10001 FOR SHARE), measured AS (SELECT value,ordinal,id,octet_length(value::text) bytes,sum(octet_length(value::text)) OVER (ORDER BY ordinal,id) size FROM source) SELECT CASE WHEN bytes<=262144 AND size<=least(2097152,$2) THEN value END FROM measured ORDER BY ordinal,id").bind(id).bind(i64::try_from(limit_bytes.saturating_sub(held)).unwrap_or(i64::MAX)).fetch_all(&mut *c).await?;
                if lines.len() > 10_000 {
                    return Err(denied());
                }
                let lines = lines
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(limit)?;
                for line in &lines {
                    charge(&mut held, line.to_string().len(), limit_bytes)?;
                }
                if snapshot["lines"] != Value::Array(lines) {
                    return Err(denied());
                }
            }
            let actor = text(l, "actor_id")?;
            let command_key = text(l, "command_key")?;
            let operation = if version == 1 {
                "finance.journal.create"
            } else {
                "finance.manual-draft.edit"
            };
            let receipt_limit = limit_bytes.saturating_sub(held) / 2;
            let receipt_limit = receipt_limit.saturating_sub(
                command_key.len()
                    + operation.len()
                    + id.len()
                    + actor.len()
                    + company.as_str().len()
                    + 1024,
            );
            let proof = read_record_origin_with_limit(
                c,
                RecordOriginRequest {
                    command_key: command_key.into(),
                    operation: operation.into(),
                    record_id: id.into(),
                    creator_user_id: UserId::new(actor).map_err(|_| denied())?,
                    company_id: company.clone(),
                },
                receipt_limit,
            )
            .await?
            .ok_or_else(denied)?;
            charge(
                &mut held,
                proof.retained_text_bytes().saturating_mul(2),
                limit_bytes,
            )?;
            if proof.result().as_str().len() > 16 * 1024 {
                return Err(limit());
            }
            let result: Value =
                serde_json::from_str(proof.result().as_str()).map_err(|_| denied())?;
            if result["id"] != id
                || result["status"] != "draft"
                || result["version"].as_i64() != Some(version)
                || (version == 1 && result["number"] != format!("J-{key}"))
            {
                return Err(denied());
            }
            let raw_proof = read_raw_record_origin(
                c,
                RawRecordOriginRequest {
                    command_key: command_key.into(),
                    operation: operation.into(),
                    record_id: id.into(),
                    creator_user_id: actor.into(),
                    company_id: company.clone(),
                },
            )
            .await?
            .ok_or_else(denied)?;
            if raw_proof.receipt_fingerprint() != text(l, "receipt_sha256")? {
                return Err(denied());
            }
            typed.push(proof);
            raw.push(raw_proof);
            previous_hash = Some(hash.into());
        }
    }
    Ok(CurrentManualDraftProvenance {
        company: company.clone(),
        id: id.into(),
        original_actor,
        chain,
        typed,
        raw,
        retained_bytes: held,
        limit_bytes,
    })
}
