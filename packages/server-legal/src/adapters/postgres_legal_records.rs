//! PostgreSQL adapter for the native Legal record reads and lifecycle commands.
//!
//! The caller supplies one serializable Holding transaction, current session
//! proof, Company/branch/department selection and the identity/notification
//! ports. This adapter re-checks visibility, version, membership and enabled
//! state in that same unit.
use crate::{
    application::legal_records::{
        LegalRecordsError, LegalRecordsFuture, LegalRecordsPorts, legal_command, legal_detail,
        legal_register,
    },
    domain::legal_records::*,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

pub type PgLegalFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, LegalRecordsError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalHoldNotice {
    pub hold_id: String,
    pub item_id: String,
    pub user_id: String,
    pub notified_at: i64,
    pub company_id: String,
    pub item_version: i64,
}

/// Identity-owned same-person decision. Production must supply the current IAM
/// projection; an absent port keeps hold release fail-closed.
pub trait LegalIdentity: Send + Sync {
    fn same_person<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        left: &'a str,
        right: &'a str,
        company: &'a CompanyId,
    ) -> PgLegalFuture<'a, bool>;
}

/// Collaboration-owned notification effect inside the Legal transaction.
pub trait LegalHoldNotifier: Send + Sync {
    fn notify<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        notices: &'a [LegalHoldNotice],
    ) -> PgLegalFuture<'a, ()>;
}

pub struct PgLegalRecords<'c> {
    connection: &'c mut PgConnection,
    identity: Option<std::sync::Arc<dyn LegalIdentity>>,
    notifier: Option<std::sync::Arc<dyn LegalHoldNotifier>>,
    now: i64,
}

impl<'c> PgLegalRecords<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        identity: Option<std::sync::Arc<dyn LegalIdentity>>,
        notifier: Option<std::sync::Arc<dyn LegalHoldNotifier>>,
    ) -> Self {
        Self {
            connection,
            identity,
            notifier,
            now: now_millis(),
        }
    }

    pub fn with_now(
        connection: &'c mut PgConnection,
        identity: Option<std::sync::Arc<dyn LegalIdentity>>,
        notifier: Option<std::sync::Arc<dyn LegalHoldNotifier>>,
        now: i64,
    ) -> Self {
        Self {
            connection,
            identity,
            notifier,
            now,
        }
    }

    fn transaction(&self) -> Result<(), LegalRecordsError> {
        if !self.connection.is_in_transaction() {
            return Err(unavailable(
                "LEGAL_TRANSACTION_REQUIRED",
                "Legal requires its complete owned transaction.",
            )
            .into());
        }
        Ok(())
    }

    async fn enabled(&mut self, company: &str) -> Result<bool, LegalRecordsError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM legal_settings WHERE company_id=$1 AND enabled=true)",
        )
        .bind(company)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn editor(&mut self, item: &str, user: &str) -> Result<bool, LegalRecordsError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM legal_members WHERE item_id=$1 AND user_id=$2 AND access='edit')",
        )
        .bind(item)
        .bind(user)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn frozen(&mut self, item: &str) -> Result<bool, LegalRecordsError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$1 AND state IN ('executing','completed'))",
        )
        .bind(item)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn same_person(
        &mut self,
        left: &str,
        right: &str,
        company: &CompanyId,
    ) -> Result<bool, LegalRecordsError> {
        let Some(identity) = self.identity.as_ref() else {
            return Ok(true);
        };
        identity
            .same_person(&mut *self.connection, left, right, company)
            .await
    }

    async fn hold_sources(
        &mut self,
        actor: &LegalRecordsActor,
        item: &str,
    ) -> Result<Vec<LegalHoldSource>, LegalRecordsError> {
        let rows = sqlx::query(
            "SELECT id,state,created_by,release_requested_by FROM legal_holds \
             WHERE item_id=$1 ORDER BY created_at,id LIMIT $2",
        )
        .bind(item)
        .bind(HOLD_LIMIT as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        let mut holds = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            let state: String = row.try_get("state")?;
            let created_by: String = row.try_get("created_by")?;
            let release_requested_by: Option<String> = row.try_get("release_requested_by")?;
            let same_person_creator = self
                .same_person(actor.user_id.as_str(), &created_by, &actor.company_id)
                .await?;
            let same_person_release_requester = match release_requested_by.as_deref() {
                Some(user) => {
                    self.same_person(actor.user_id.as_str(), user, &actor.company_id)
                        .await?
                }
                None => false,
            };
            holds.push(LegalHoldSource {
                id,
                state,
                created_by,
                release_requested_by,
                same_person_creator,
                same_person_release_requester,
            });
        }
        Ok(holds)
    }

    async fn next_document_version(&mut self, item: &str) -> Result<i64, LegalRecordsError> {
        Ok(sqlx::query_scalar(
            "SELECT (coalesce(max(version),0)+1)::bigint FROM legal_documents WHERE item_id=$1",
        )
        .bind(item)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn receipt(
        &mut self,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<Value>, LegalRecordsError> {
        let row = sqlx::query(
            "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let saved: String = row.try_get("request_hash")?;
        if saved != request_hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        raw.and_then(|value| serde_json::from_str(&value).ok())
            .map(Some)
            .ok_or_else(|| {
                conflict(
                    "IDEMPOTENCY_CONFLICT",
                    "The retained result is unavailable.",
                )
                .into()
            })
    }

    async fn apply_effect(
        &mut self,
        actor: &LegalRecordsActor,
        plan: &LegalCommandPlan,
    ) -> Result<Option<String>, LegalRecordsError> {
        match &plan.effect {
            LegalCommandEffect::AddDocument {
                document_id,
                document_version,
                name,
                body,
                checksum,
                bytes,
            } => {
                sqlx::query(
                    "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) \
                     VALUES($1,$2,$3,$4,'text/plain',$5,$6,$7,$8,$9)",
                )
                .bind(document_id)
                .bind(&plan.item_id)
                .bind(i32_version(*document_version)?)
                .bind(name)
                .bind(checksum)
                .bind(bytes)
                .bind(body)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
            }
            LegalCommandEffect::CreateHold { hold_id, reason } => {
                sqlx::query(
                    "INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at) \
                     VALUES($1,$2,$3,'active',$4,$5)",
                )
                .bind(hold_id)
                .bind(&plan.item_id)
                .bind(reason)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query("INSERT INTO legal_hold_targets(hold_id,item_id) VALUES($1,$2)")
                    .bind(hold_id)
                    .bind(&plan.item_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            LegalCommandEffect::RequestHoldRelease { hold_id } => {
                let changed = sqlx::query(
                    "UPDATE legal_holds SET state='release_requested',release_requested_by=$1 \
                     WHERE id=$2 AND item_id=$3 AND state='active'",
                )
                .bind(actor.user_id.as_str())
                .bind(hold_id)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(hold_state().into());
                }
            }
            LegalCommandEffect::ReleaseHold { hold_id } => {
                let changed = sqlx::query(
                    "UPDATE legal_holds SET state='released',released_by=$1,released_at=$2 \
                     WHERE id=$3 AND item_id=$4 AND state='release_requested'",
                )
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .bind(hold_id)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(hold_state().into());
                }
            }
            LegalCommandEffect::Share {
                share_id,
                document_id,
                email,
                expires_at,
            } => {
                let belongs: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM legal_documents WHERE id=$1 AND item_id=$2)",
                )
                .bind(document_id)
                .bind(&plan.item_id)
                .fetch_one(&mut *self.connection)
                .await?;
                if !belongs {
                    return Err(invalid(
                        "LEGAL_DOCUMENT_REQUIRED",
                        "Select a document from this record.",
                    )
                    .into());
                }
                if *expires_at <= self.now + 60_000 || *expires_at > self.now + 90 * 86_400_000 {
                    return Err(invalid(
                        "LEGAL_EXPIRY_REQUIRED",
                        "Choose an external access expiry between one minute and 90 days.",
                    )
                    .into());
                }
                sqlx::query(
                    "INSERT INTO legal_shares(id,item_id,document_id,email,expires_at,created_by,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(share_id)
                .bind(&plan.item_id)
                .bind(document_id)
                .bind(email)
                .bind(expires_at)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO legal_invitation_jobs(id,share_id,holding_slug,state,created_at) \
                     VALUES($1,$2,$3,'queued',$4)",
                )
                .bind(format!("legal-invitation-{}", short_hash(share_id)))
                .bind(share_id)
                .bind(actor.holding_slug.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
            }
            LegalCommandEffect::RevokeShare { share_id } => {
                let changed = sqlx::query(
                    "UPDATE legal_shares SET revoked_at=$1,version=version+1 \
                     WHERE id=$2 AND item_id=$3 AND revoked_at IS NULL",
                )
                .bind(self.now)
                .bind(share_id)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_SHARE_STATE",
                        "This external share is no longer active.",
                    )
                    .into());
                }
            }
            LegalCommandEffect::SetMember { user_id, access } => {
                if access == "remove" {
                    sqlx::query("DELETE FROM legal_members WHERE item_id=$1 AND user_id=$2")
                        .bind(&plan.item_id)
                        .bind(user_id)
                        .execute(&mut *self.connection)
                        .await?;
                } else {
                    let eligible: bool = sqlx::query_scalar(
                        "SELECT legal_person_eligible($1,$2,$3,$4,NULL)",
                    )
                    .bind(actor.company_id.as_str())
                    .bind(user_id)
                    .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                    .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                    .fetch_one(&mut *self.connection)
                    .await?;
                    if !eligible {
                        return Err(BusinessError::new(
                            FailureKind::InvalidInput,
                            "LEGAL_PERSON_UNAVAILABLE",
                            "Choose an active person with access to this Company and record scope.",
                        )
                        .into());
                    }
                    sqlx::query(
                        "INSERT INTO legal_members(item_id,user_id,access) VALUES($1,$2,$3) \
                         ON CONFLICT(item_id,user_id) DO UPDATE SET access=excluded.access",
                    )
                    .bind(&plan.item_id)
                    .bind(user_id)
                    .bind(access)
                    .execute(&mut *self.connection)
                    .await?;
                }
            }
            LegalCommandEffect::ExecutedReview {
                review_id,
                executed_copy_id,
                decision,
                checks,
                reason,
            } => {
                let row = sqlx::query(
                    "SELECT e.state,e.imported_by,e.approved_cycle::bigint approved_cycle,e.approved_hash,e.executed_hash,\
                            i.kind,i.state item_state,\
                            d.created_by document_author,d.checksum document_checksum,\
                            p.created_by approved_author,p.checksum approved_checksum,p.locked \
                     FROM legal_executed_copies e \
                     JOIN legal_items i ON i.id=e.item_id AND i.company_id=$3 \
                     JOIN legal_documents d ON d.id=e.document_id \
                     JOIN legal_documents p ON p.id=e.approved_document_id \
                     WHERE e.id=$1 AND e.item_id=$2",
                )
                .bind(executed_copy_id)
                .bind(&plan.item_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::NotFound,
                        "LEGAL_NOT_FOUND",
                        "Executed copy unavailable in your current scope.",
                    )
                })?;
                let state: String = row.try_get("state")?;
                let imported_by: String = row.try_get("imported_by")?;
                let kind: String = row.try_get("kind")?;
                let item_state: String = row.try_get("item_state")?;
                let document_author: String = row.try_get("document_author")?;
                let approved_author: String = row.try_get("approved_author")?;
                let document_checksum: String = row.try_get("document_checksum")?;
                let approved_checksum: String = row.try_get("approved_checksum")?;
                let approved_hash: String = row.try_get("approved_hash")?;
                let executed_hash: String = row.try_get("executed_hash")?;
                let locked: bool = row.try_get("locked")?;
                let approved_cycle: i64 = row.try_get("approved_cycle")?;
                if state == "confirmed"
                    || kind != "contract"
                    || !matches!(item_state.as_str(), "approved" | "executed" | "active")
                    || !locked
                    || approved_checksum != approved_hash
                    || document_checksum != executed_hash
                {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_EXECUTED_STATE",
                        "Import a new revision to correct a confirmed copy.",
                    )
                    .into());
                }
                let current_cycle: Option<i64> = sqlx::query_scalar(
                    "SELECT cycle::bigint FROM legal_approvals WHERE item_id=$1 AND decision='approved' ORDER BY cycle DESC LIMIT 1",
                )
                .bind(&plan.item_id)
                .fetch_optional(&mut *self.connection)
                .await?;
                if current_cycle != Some(approved_cycle) {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_STALE",
                        "The approved version changed. Import against the current approval.",
                    )
                    .into());
                }
                let member: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM legal_members WHERE item_id=$1 AND user_id=$2)",
                )
                .bind(&plan.item_id)
                .bind(actor.user_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?;
                if !member {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "LEGAL_REVIEWER_UNAVAILABLE",
                        "The reviewer needs explicit contract membership.",
                    )
                    .into());
                }
                for author in [
                    imported_by.as_str(),
                    document_author.as_str(),
                    approved_author.as_str(),
                ] {
                    if self
                        .same_person(actor.user_id.as_str(), author, &actor.company_id)
                        .await?
                    {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "LEGAL_INDEPENDENT_REVIEW",
                            "An independent reviewer must inspect the signed original.",
                        )
                        .into());
                    }
                }
                let confirmed = decision == "confirm";
                sqlx::query(
                    "INSERT INTO legal_executed_reviews(id,executed_copy_id,item_version,decision,checks,reason,reviewer_id,created_at) \
                     VALUES($1,$2,$3,$4,$5::jsonb,$6,$7,$8)",
                )
                .bind(review_id)
                .bind(executed_copy_id)
                .bind(i32_version(plan.expected_version)?)
                .bind(decision)
                .bind(checks.to_string())
                .bind(reason)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                let changed = sqlx::query(
                    "UPDATE legal_executed_copies \
                     SET state=$1,reviewed_by=$2,reviewed_at=$3,version=version+1 \
                     WHERE id=$4 AND item_id=$5 AND state<>'confirmed'",
                )
                .bind(if confirmed { "confirmed" } else { "changes_required" })
                .bind(if confirmed {
                    Some(actor.user_id.as_str())
                } else {
                    None
                })
                .bind(if confirmed { Some(self.now) } else { None })
                .bind(executed_copy_id)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_EXECUTED_STATE",
                        "The executed copy changed. Reload before reviewing.",
                    )
                    .into());
                }
            }
            LegalCommandEffect::ExecutedImport {
                command_id,
                document_id,
                executed_copy_id,
                attachment_link_id,
                metadata,
            } => {
                let item = sqlx::query(
                    "SELECT kind,state FROM legal_items WHERE id=$1 AND company_id=$2 FOR UPDATE",
                )
                .bind(&plan.item_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(not_found)?;
                let kind: String = item.try_get("kind")?;
                let item_state: String = item.try_get("state")?;
                if kind != "contract" || !matches!(item_state.as_str(), "approved" | "executed" | "active") {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_EXECUTED_STATE",
                        "Import a signed original against an approved contract.",
                    )
                    .into());
                }
                let approved_document_id = metadata
                    .get("approvedDocumentId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("LEGAL_IMPORT_REQUIRED", "Choose the approved document version."))?;
                if metadata
                    .get("signedDate")
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.len() != 10)
                    || metadata
                        .get("parties")
                        .and_then(Value::as_array)
                        .is_none_or(|values| values.len() < 2 || values.len() > 30)
                    || metadata
                        .get("signers")
                        .and_then(Value::as_array)
                        .is_none_or(|values| values.is_empty() || values.len() > 30)
                    || metadata
                        .get("declaredPageCount")
                        .and_then(Value::as_i64)
                        .is_none_or(|value| !(1..=1000).contains(&value))
                {
                    return Err(invalid("LEGAL_IMPORT_REQUIRED", "Supply complete executed-copy signing details.").into());
                }
                let evidence = erpx_server_files::public::legal_import_evidence::inspect_legal_import_evidence(
                    &mut *self.connection,
                    &actor.company_id,
                    &plan.item_id,
                    attachment_link_id,
                )
                .await?;
                if evidence.checksum_sha256.len() != 64
                    || evidence.bytes < 1
                    || evidence.bytes > 10 * 1024 * 1024
                    || !matches!(evidence.media_type.as_str(), "application/pdf" | "image/png" | "image/jpeg")
                {
                    return Err(invalid("LEGAL_IMPORT_REQUIRED", "Import an executed copy as a PDF or scanned image.").into());
                }
                let approval = sqlx::query(
                    "SELECT cycle::bigint cycle,document_id,document_hash FROM legal_approvals                      WHERE item_id=$1 AND decision='approved' ORDER BY cycle DESC LIMIT 1",
                )
                .bind(&plan.item_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| BusinessError::new(FailureKind::Conflict, "LEGAL_APPROVAL_STALE", "The approved version changed. Import against the current approval."))?;
                let approved_cycle: i64 = approval.try_get("cycle")?;
                let current_document: String = approval.try_get("document_id")?;
                let approved_hash: String = approval.try_get("document_hash")?;
                let locked: bool = sqlx::query_scalar(
                    "SELECT locked FROM legal_documents WHERE id=$1 AND item_id=$2",
                )
                .bind(approved_document_id)
                .bind(&plan.item_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_IMPORT_REQUIRED", "Choose the approved document version."))?;
                if current_document != approved_document_id || !locked {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_STALE",
                        "Choose the current approved document.",
                    )
                    .into());
                }
                let next_version: i64 = sqlx::query_scalar(
                    "SELECT (COALESCE(MAX(version),0)+1)::bigint FROM legal_documents WHERE item_id=$1",
                )
                .bind(&plan.item_id)
                .fetch_one(&mut *self.connection)
                .await?;
                let name: String = evidence.original_name.chars().take(200).collect::<String>();
                if name.is_empty() {
                    return Err(invalid("LEGAL_IMPORT_REQUIRED", "The signed original needs a file name.").into());
                }
                sqlx::query(
                    "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,locked,created_by,created_at,attachment_link_id,attachment_source_id)                      VALUES($1,$2,$3,$4,$5,$6,$7,'',$8,false,$9,$10,$11,$12)",
                )
                .bind(document_id)
                .bind(&plan.item_id)
                .bind(i32_version(next_version)?)
                .bind(&name)
                .bind(&evidence.media_type)
                .bind(&evidence.checksum_sha256)
                .bind(evidence.bytes)
                .bind(&evidence.object_key)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .bind(attachment_link_id)
                .bind(&evidence.source_id)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO legal_executed_copies(id,item_id,document_id,approved_cycle,approved_document_id,approved_hash,executed_hash,metadata,state,imported_by,imported_at,version)                      VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,'pending_review',$9,$10,1)",
                )
                .bind(executed_copy_id)
                .bind(&plan.item_id)
                .bind(document_id)
                .bind(i32_version(approved_cycle)?)
                .bind(approved_document_id)
                .bind(&approved_hash)
                .bind(&evidence.checksum_sha256)
                .bind(metadata.to_string())
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                evidence
                    .bind(
                        &mut *self.connection,
                        &actor.company_id,
                        actor.user_id.as_str(),
                        command_id,
                        "legal_executed_copy",
                        executed_copy_id,
                        self.now,
                    )
                    .await?;
            }
            LegalCommandEffect::Archive { .. } => {}
            LegalCommandEffect::ApprovalOpen {
                instance_id,
                document_id,
                signer_ids,
                approver_ids,
                ..
            } => {
                let item = sqlx::query(
                    "SELECT kind,state,branch_id,department_id,created_by FROM legal_items                      WHERE id=$1 AND company_id=$2 FOR UPDATE",
                )
                .bind(&plan.item_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(not_found)?;
                let kind: String = item.try_get("kind")?;
                let item_state: String = item.try_get("state")?;
                let item_created_by: String = item.try_get("created_by")?;
                let branch_id: Option<String> = item.try_get("branch_id")?;
                let department_id: Option<String> = item.try_get("department_id")?;
                if kind != "contract" || !matches!(item_state.as_str(), "draft" | "review" | "approval") {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_STATE",
                        "Return this contract to draft before opening a new approval.",
                    )
                    .into());
                }
                let document = sqlx::query(
                    "SELECT checksum,created_by FROM legal_documents WHERE id=$1 AND item_id=$2",
                )
                .bind(document_id)
                .bind(&plan.item_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_DOCUMENT_REQUIRED", "Select a document from this contract."))?;
                let document_checksum: String = document.try_get("checksum")?;
                let document_author: String = document.try_get("created_by")?;
                let executed: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM legal_executed_copies WHERE document_id=$1)",
                )
                .bind(document_id)
                .fetch_one(&mut *self.connection)
                .await?;
                if executed {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_STATE",
                        "Imported executed copies are reviewed separately from draft approval.",
                    )
                    .into());
                }
                let last_author: Option<String> = sqlx::query_scalar(
                    "SELECT actor_id FROM legal_events WHERE item_id=$1 ORDER BY version DESC,created_at DESC LIMIT 1",
                )
                .bind(&plan.item_id)
                .fetch_optional(&mut *self.connection)
                .await?;
                for approver in approver_ids {
                    let eligible: bool = sqlx::query_scalar(
                        "SELECT legal_person_eligible($1,$2,$3,$4,'legal.approve') AND EXISTS(SELECT 1 FROM legal_members WHERE item_id=$5 AND user_id=$2)",
                    )
                    .bind(actor.company_id.as_str())
                    .bind(approver)
                    .bind(branch_id.as_deref())
                    .bind(department_id.as_deref())
                    .bind(&plan.item_id)
                    .fetch_one(&mut *self.connection)
                    .await?;
                    if !eligible {
                        return Err(BusinessError::new(
                            FailureKind::InvalidInput,
                            "LEGAL_REVIEWER_UNAVAILABLE",
                            "Reviewer needs explicit record membership and an active Legal approval role.",
                        )
                        .into());
                    }
                    for author in [actor.user_id.as_str(), item_created_by.as_str(), document_author.as_str()] {
                        if self
                            .same_person(approver, author, &actor.company_id)
                            .await?
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "LEGAL_INDEPENDENT_REVIEW",
                                "Content authors cannot approve their own work.",
                            )
                            .into());
                        }
                    }
                    if let Some(last) = last_author.as_deref() {
                        if self.same_person(approver, last, &actor.company_id).await? {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "LEGAL_INDEPENDENT_REVIEW",
                                "Content authors cannot approve their own work.",
                            )
                            .into());
                        }
                    }
                }
                for signer in signer_ids {
                    let access: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM legal_members WHERE item_id=$1 AND user_id=$2)                          OR EXISTS(SELECT 1 FROM legal_shares WHERE item_id=$1 AND document_id=$3 AND user_id=$2 AND revoked_at IS NULL AND expires_at>$4)",
                    )
                    .bind(&plan.item_id)
                    .bind(signer)
                    .bind(document_id)
                    .bind(self.now)
                    .fetch_one(&mut *self.connection)
                    .await?;
                    if !access {
                        return Err(BusinessError::new(
                            FailureKind::InvalidInput,
                            "LEGAL_SIGNER_UNAVAILABLE",
                            "Each signer must have explicit access to this version.",
                        )
                        .into());
                    }
                }
                let cycle: i64 = sqlx::query_scalar(
                    "SELECT (COALESCE(MAX(cycle),0)+1)::bigint FROM approval_instances WHERE record_type='legal' AND record_id=$1 AND phase='request'",
                )
                .bind(&plan.item_id)
                .fetch_one(&mut *self.connection)
                .await?;
                sqlx::query(
                    "UPDATE approval_instances SET state='superseded',version=version+1,updated_at=$1                      WHERE record_type='legal' AND record_id=$2 AND state IN ('pending','blocked')",
                )
                .bind(self.now)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,created_at,updated_at)                      VALUES($1,$2,$3,$4,'legal',$5,'request',$6,$7,NULL,'pending',$8,$8)",
                )
                .bind(instance_id)
                .bind(actor.company_id.as_str())
                .bind(branch_id.as_deref())
                .bind(department_id.as_deref())
                .bind(&plan.item_id)
                .bind(cycle)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                let snapshot = serde_json::to_string(&json!({
                    "documentId": document_id,
                    "checksum": document_checksum,
                    "signers": signer_ids,
                }))
                .map_err(|_| invalid("LEGAL_APPROVAL_INVALID", "The approval route is invalid."))?;
                for (ordinal, approver) in approver_ids.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,route_snapshot_json)                          VALUES($1,$2,$3,$4,'named',$5)",
                    )
                    .bind(format!("{instance_id}-{ordinal}"))
                    .bind(instance_id)
                    .bind(ordinal as i64)
                    .bind(approver)
                    .bind(&snapshot)
                    .execute(&mut *self.connection)
                    .await?;
                }
                sqlx::query(
                    "INSERT INTO legal_approval_routes(item_id,instance_id,document_id,document_hash,signers,source_version,created_by)                      VALUES($1,$2,$3,$4,$5::jsonb,$6,$7)                      ON CONFLICT(item_id) DO UPDATE SET instance_id=excluded.instance_id,document_id=excluded.document_id,document_hash=excluded.document_hash,signers=excluded.signers,source_version=excluded.source_version,created_by=excluded.created_by",
                )
                .bind(&plan.item_id)
                .bind(instance_id)
                .bind(document_id)
                .bind(&document_checksum)
                .bind(serde_json::to_string(signer_ids).map_err(|_| invalid("LEGAL_APPROVAL_INVALID", "The approval route is invalid."))?)
                .bind(i32_version(plan.next_version)?)
                .bind(actor.user_id.as_str())
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "UPDATE legal_documents SET locked=true WHERE id=$1 AND item_id=$2",
                )
                .bind(document_id)
                .bind(&plan.item_id)
                .execute(&mut *self.connection)
                .await?;
                if let Some(first) = approver_ids.first() {
                    sqlx::query(
                        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)                          VALUES($1,$2,$3,'legal',$4,$5,'Request approval needs your decision',$6) ON CONFLICT DO NOTHING",
                    )
                    .bind(format!("legal-approval-{instance_id}-{first}"))
                    .bind(actor.company_id.as_str())
                    .bind(first)
                    .bind(&plan.item_id)
                    .bind(plan.next_version)
                    .bind(self.now)
                    .execute(&mut *self.connection)
                    .await?;
                }
                return Ok(Some("approval".to_owned()));
            }
            LegalCommandEffect::ApprovalDecide {
                decision_id,
                command_id,
                decision,
                reason,
            } => {
                let instance = sqlx::query(
                    "SELECT id,state,current_step::bigint current_step,cycle::bigint cycle                      FROM approval_instances WHERE record_type='legal' AND record_id=$1 AND company_id=$2                      ORDER BY insertion_order DESC LIMIT 1 FOR UPDATE",
                )
                .bind(&plan.item_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_APPROVAL_ROUTE", "No active approval route."))?;
                let instance_id: String = instance.try_get("id")?;
                let state: String = instance.try_get("state")?;
                let current_step: i64 = instance.try_get("current_step")?;
                let cycle: i64 = instance.try_get("cycle")?;
                if state != "pending" {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_ROUTE",
                        "No active approval route.",
                    )
                    .into());
                }
                let step = sqlx::query(
                    "SELECT id,approver_user_id,state FROM approval_steps WHERE instance_id=$1 AND ordinal=$2 FOR UPDATE",
                )
                .bind(&instance_id)
                .bind(current_step)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_APPROVAL_ROUTE", "Approval step unavailable."))?;
                let step_id: String = step.try_get("id")?;
                let approver: String = step.try_get("approver_user_id")?;
                let step_state: String = step.try_get("state")?;
                if step_state != "pending"
                    || !self.same_person(actor.user_id.as_str(), &approver, &actor.company_id).await?
                {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "LEGAL_APPROVER",
                        "This decision is assigned to another reviewer.",
                    )
                    .into());
                }
                let route = sqlx::query(
                    "SELECT document_id,document_hash,signers::text signers FROM legal_approval_routes WHERE item_id=$1 AND instance_id=$2",
                )
                .bind(&plan.item_id)
                .bind(&instance_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_APPROVAL_ROUTE", "Approval snapshot unavailable."))?;
                let document_id: String = route.try_get("document_id")?;
                let document_hash: String = route.try_get("document_hash")?;
                let signers: String = route.try_get("signers")?;
                let authority = serde_json::to_string(&json!({
                    "role": actor.role,
                    "assignmentId": actor.assignment_id.as_str(),
                    "documentId": document_id,
                    "documentHash": document_hash,
                }))
                .map_err(|_| invalid("LEGAL_APPROVAL_INVALID", "The approval decision is invalid."))?;
                sqlx::query(
                    "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at,authority_json)                      VALUES($1,$2,$3,$4,$5,$6,$6,$7,$8,$9,$10,$11)",
                )
                .bind(decision_id)
                .bind(&instance_id)
                .bind(&step_id)
                .bind(decision)
                .bind(actor.user_id.as_str())
                .bind(&approver)
                .bind(reason)
                .bind(plan.expected_version)
                .bind(command_id)
                .bind(self.now)
                .bind(&authority)
                .execute(&mut *self.connection)
                .await?;
                let step_decision = if decision == "approve" { "approved" } else { "rejected" };
                sqlx::query("UPDATE approval_steps SET state=$1 WHERE id=$2")
                    .bind(step_decision)
                    .bind(&step_id)
                    .execute(&mut *self.connection)
                    .await?;
                if decision != "approve" {
                    sqlx::query(
                        "UPDATE approval_instances SET state=$1,version=version+1,updated_at=$2 WHERE id=$3",
                    )
                    .bind(if decision == "reject" { "rejected" } else { "cancelled" })
                    .bind(self.now)
                    .bind(&instance_id)
                    .execute(&mut *self.connection)
                    .await?;
                    return Ok(Some(if decision == "revise" { "draft" } else { "rejected" }.to_owned()));
                }
                let next: Option<String> = sqlx::query_scalar(
                    "SELECT approver_user_id FROM approval_steps WHERE instance_id=$1 AND ordinal=$2",
                )
                .bind(&instance_id)
                .bind(current_step + 1)
                .fetch_optional(&mut *self.connection)
                .await?;
                if let Some(next_approver) = next {
                    sqlx::query(
                        "UPDATE approval_instances SET current_step=current_step+1,version=version+1,updated_at=$1 WHERE id=$2",
                    )
                    .bind(self.now)
                    .bind(&instance_id)
                    .execute(&mut *self.connection)
                    .await?;
                    sqlx::query(
                        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)                          VALUES($1,$2,$3,'legal',$4,$5,'Request approval needs your decision',$6) ON CONFLICT DO NOTHING",
                    )
                    .bind(format!("legal-approval-{instance_id}-{next_approver}"))
                    .bind(actor.company_id.as_str())
                    .bind(&next_approver)
                    .bind(&plan.item_id)
                    .bind(plan.next_version)
                    .bind(self.now)
                    .execute(&mut *self.connection)
                    .await?;
                    return Ok(Some("approval".to_owned()));
                }
                sqlx::query(
                    "UPDATE approval_instances SET state='approved',version=version+1,updated_at=$1 WHERE id=$2",
                )
                .bind(self.now)
                .bind(&instance_id)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO legal_approvals(item_id,cycle,document_id,document_hash,signers,reviewer_id,decision,created_at)                      VALUES($1,$2,$3,$4,$5::jsonb,$6,'approved',$7) ON CONFLICT(item_id,cycle) DO NOTHING",
                )
                .bind(&plan.item_id)
                .bind(i32_version(cycle)?)
                .bind(document_id)
                .bind(document_hash)
                .bind(signers)
                .bind(actor.user_id.as_str())
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                return Ok(Some("approved".to_owned()));
            }
            LegalCommandEffect::ApprovalReroute {
                decision_id,
                command_id,
                approver_id,
                reason,
            } => {
                let instance = sqlx::query(
                    "SELECT id,state,current_step::bigint current_step FROM approval_instances                      WHERE record_type='legal' AND record_id=$1 AND company_id=$2 ORDER BY insertion_order DESC LIMIT 1 FOR UPDATE",
                )
                .bind(&plan.item_id)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_APPROVAL_ROUTE", "No active approval route."))?;
                let instance_id: String = instance.try_get("id")?;
                let state: String = instance.try_get("state")?;
                let current_step: i64 = instance.try_get("current_step")?;
                if state != "pending" {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "LEGAL_APPROVAL_ROUTE",
                        "No active approval route.",
                    )
                    .into());
                }
                let step = sqlx::query(
                    "SELECT id,approver_user_id FROM approval_steps WHERE instance_id=$1 AND ordinal=$2 FOR UPDATE",
                )
                .bind(&instance_id)
                .bind(current_step)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(|| invalid("LEGAL_APPROVAL_ROUTE", "Approval step unavailable."))?;
                let step_id: String = step.try_get("id")?;
                let previous: String = step.try_get("approver_user_id")?;
                let item_author: String = sqlx::query_scalar(
                    "SELECT created_by FROM legal_items WHERE id=$1",
                )
                .bind(&plan.item_id)
                .fetch_one(&mut *self.connection)
                .await?;
                let eligible: bool = sqlx::query_scalar(
                    "SELECT legal_person_eligible($1,$2,NULL,NULL,'legal.approve') AND EXISTS(SELECT 1 FROM legal_members WHERE item_id=$3 AND user_id=$2)",
                )
                .bind(actor.company_id.as_str())
                .bind(approver_id)
                .bind(&plan.item_id)
                .fetch_one(&mut *self.connection)
                .await?;
                if !eligible || self.same_person(approver_id, &item_author, &actor.company_id).await? {
                    return Err(BusinessError::new(
                        FailureKind::InvalidInput,
                        "LEGAL_REVIEWER_UNAVAILABLE",
                        "The replacement must be an independent eligible reviewer.",
                    )
                    .into());
                }
                sqlx::query("UPDATE approval_steps SET approver_user_id=$1 WHERE id=$2")
                    .bind(approver_id)
                    .bind(&step_id)
                    .execute(&mut *self.connection)
                    .await?;
                sqlx::query(
                    "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at)                      VALUES($1,$2,$3,'reroute',$4,$5,$6,$7,$8,$9,$10)",
                )
                .bind(decision_id)
                .bind(&instance_id)
                .bind(&step_id)
                .bind(actor.user_id.as_str())
                .bind(&previous)
                .bind(approver_id)
                .bind(reason)
                .bind(plan.expected_version)
                .bind(command_id)
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "UPDATE approval_instances SET version=version+1,updated_at=$1 WHERE id=$2",
                )
                .bind(self.now)
                .bind(&instance_id)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)                      VALUES($1,$2,$3,'legal',$4,$5,'Request approval needs your decision',$6) ON CONFLICT DO NOTHING",
                )
                .bind(format!("legal-approval-{instance_id}-{approver_id}"))
                .bind(actor.company_id.as_str())
                .bind(approver_id)
                .bind(&plan.item_id)
                .bind(plan.next_version)
                .bind(self.now)
                .execute(&mut *self.connection)
                .await?;
                return Ok(Some("approval".to_owned()));
            }
        }
        Ok(None)
    }

    async fn expand_and_notify(&mut self, company: &str) -> Result<(), LegalRecordsError> {
        sqlx::query(
            "WITH RECURSIVE targets(hold_id,item_id) AS ( \
               SELECT id,item_id FROM legal_holds WHERE state!='released' \
               UNION \
               SELECT t.hold_id,l.target_id FROM targets t JOIN legal_links l ON l.source_id=t.item_id \
               WHERE l.relation IN ('evidence','derived','amends','obligation') \
             ) INSERT INTO legal_hold_targets SELECT * FROM targets ON CONFLICT DO NOTHING",
        )
        .execute(&mut *self.connection)
        .await?;
        let rows = sqlx::query(
            "WITH added AS ( \
               INSERT INTO legal_hold_notices(hold_id,item_id,user_id,notified_at) \
               SELECT h.id,t.item_id,m.user_id,$2 \
               FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id \
               JOIN legal_members m ON m.item_id=t.item_id \
               WHERE h.state!='released' \
                 AND t.item_id IN (SELECT id FROM legal_items WHERE company_id=$1) \
               ON CONFLICT DO NOTHING RETURNING hold_id,item_id,user_id,notified_at \
             ) SELECT added.hold_id,added.item_id,added.user_id,added.notified_at,i.company_id,i.version::bigint item_version                FROM added JOIN legal_items i ON i.id=added.item_id ORDER BY added.hold_id,added.item_id,added.user_id",
        )
        .bind(company)
        .bind(self.now)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.is_empty() {
            return Ok(());
        }
        let notices = rows
            .into_iter()
            .map(|row| {
                Ok(LegalHoldNotice {
                    hold_id: row.try_get("hold_id")?,
                    item_id: row.try_get("item_id")?,
                    user_id: row.try_get("user_id")?,
                    notified_at: row.try_get("notified_at")?,
                    company_id: row.try_get("company_id")?,
                    item_version: row.try_get("item_version")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let Some(notifier) = self.notifier.as_ref() else {
            return Err(unavailable(
                "LEGAL_NOTIFIER_UNAVAILABLE",
                "A current Legal hold notifier is required before preserving content.",
            )
            .into());
        };
        notifier.notify(&mut *self.connection, &notices).await
    }

    async fn save_receipt(
        &mut self,
        actor: &LegalRecordsActor,
        key: &str,
        request_hash: &str,
        plan: &LegalCommandPlan,
        result: &Value,
    ) -> Result<(), LegalRecordsError> {
        let authority = serde_json::to_string(&json!({
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "role": actor.role,
            "userId": actor.user_id.as_str(),
            "assignmentId": actor.assignment_id.as_str(),
        }))
        .map_err(|_| {
            unavailable(
                "LEGAL_RECEIPT_UNAVAILABLE",
                "The Legal command receipt is unavailable.",
            )
        })?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(key)
        .bind(&plan.operation)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(&plan.item_id)
        .bind(result.to_string())
        .bind(self.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if inserted != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,'legal',$6,$7,$5,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
        )
        .bind(format!("legal-audit-{}", short_hash(key)))
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(&plan.operation)
        .bind(&plan.item_id)
        .bind(plan.next_version)
        .bind(self.now)
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::to_string(&json!({
            "id": plan.item_id,
            "state": plan.result.state,
            "version": plan.next_version,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
        }))
        .map_err(|_| {
            unavailable(
                "LEGAL_RECEIPT_UNAVAILABLE",
                "The Legal command outbox is unavailable.",
            )
        })?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,'legal',$4,$5,$6,$7)",
        )
        .bind(format!("legal-outbox-{}", short_hash(key)))
        .bind(key)
        .bind(&plan.operation)
        .bind(&plan.item_id)
        .bind(i32_version(plan.next_version)?)
        .bind(payload)
        .bind(self.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

fn i32_version(value: i64) -> Result<i32, LegalRecordsError> {
    i32::try_from(value).map_err(|_| {
        unavailable(
            "LEGAL_VERSION_INVALID",
            "Refresh the Legal record before saving.",
        )
        .into()
    })
}

fn short_hash(value: &str) -> String {
    sha256_hex(value.as_bytes())[..32].to_owned()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn hold_state() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_HOLD_STATE",
        "This Legal hold state changed. Refresh before saving.",
    )
}

impl LegalRecordsPorts for PgLegalRecords<'_> {
    fn register<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        query: &'a LegalRegisterQuery,
    ) -> LegalRecordsFuture<'a, LegalRegister> {
        Box::pin(async move {
            self.transaction()?;
            let kind = query.kind.as_deref();
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let search = query.search.as_str();
            let state = query.state.as_str();
            let user = actor.user_id.as_str();
            let role = actor.role.as_str();
            let limit = REGISTER_PAGE_SIZE;
            let offset = (query.page - 1) * REGISTER_PAGE_SIZE;
            let filter = "i.company_id=$1 \
                 AND ($2::text IS NULL OR i.kind=$2) \
                 AND ($3::text='' OR i.state=$3) \
                 AND ($4::text='' OR strpos(lower(i.title||' '||i.description),lower($4))>0 \
                   OR EXISTS(SELECT 1 FROM legal_documents d WHERE d.item_id=i.id AND strpos(lower(d.name||' '||d.body),lower($4))>0) \
                   OR EXISTS(SELECT 1 FROM legal_document_inspections x JOIN legal_documents d ON d.id=x.document_id WHERE d.item_id=i.id AND x.kind='ocr' AND strpos(lower(x.result_json::jsonb->>'text'),lower($4))>0)) \
                 AND ($5::text IS NULL OR i.branch_id=$5) \
                 AND ($6::text IS NULL OR i.department_id=$6) \
                 AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$7)) \
                 AND ($8::text<>'legal_requester' OR (i.kind='request' AND i.created_by=$7)) \
                 AND ($9::boolean=false OR i.owner_id=$10)";
            // The filter is a fixed literal; every value remains bound.
            let rows = sqlx::query(AssertSqlSafe(format!(
                "SELECT i.id,i.company_id,i.branch_id,i.department_id,i.kind,i.title,i.description,\
                 i.classification,i.country,i.jurisdiction,i.language,i.currency,i.timezone,\
                 i.priority,i.due_date,i.owner_id,i.created_by,i.state,i.version::bigint version,i.created_at,i.updated_at \
                 FROM legal_items i WHERE {filter} ORDER BY i.updated_at DESC,i.id LIMIT $11 OFFSET $12"
            )))
            .bind(actor.company_id.as_str())
            .bind(kind)
            .bind(state)
            .bind(search)
            .bind(branch)
            .bind(department)
            .bind(user)
            .bind(role)
            .bind(query.mine)
            .bind(user)
            .bind(limit)
            .bind(offset)
            .fetch_all(&mut *self.connection)
            .await?;
            let records = rows
                .iter()
                .map(record)
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT count(*) FROM legal_items i WHERE {filter}"
            )))
            .bind(actor.company_id.as_str())
            .bind(kind)
            .bind(state)
            .bind(search)
            .bind(branch)
            .bind(department)
            .bind(user)
            .bind(role)
            .bind(query.mine)
            .bind(user)
            .fetch_one(&mut *self.connection)
            .await?;
            let counts = sqlx::query(
                "SELECT i.state,count(*)::bigint count FROM legal_items i \
                 WHERE i.company_id=$1 \
                   AND ($2::text IS NULL OR i.branch_id=$2) \
                   AND ($3::text IS NULL OR i.department_id=$3) \
                   AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$4)) \
                   AND ($5::text<>'legal_requester' OR (i.kind='request' AND i.created_by=$4)) \
                 GROUP BY i.state ORDER BY i.state",
            )
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .bind(user)
            .bind(role)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalStateCount {
                    state: row.try_get("state")?,
                    count: row.try_get("count")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            Ok(LegalRegister {
                records,
                total,
                page: query.page,
                counts,
                enabled: self.enabled(actor.company_id.as_str()).await?,
                can_edit: actor.can("legal.edit"),
                can_hold: actor.can("legal.hold"),
                can_release_hold: actor.can("legal.hold.release"),
                can_share: actor.can("legal.share"),
            })
        })
    }

    fn detail<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        id: &'a str,
    ) -> LegalRecordsFuture<'a, LegalDetail> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT i.id,i.company_id,i.branch_id,i.department_id,i.kind,i.title,i.description,\
                 i.classification,i.country,i.jurisdiction,i.language,i.currency,i.timezone,\
                 i.priority,i.due_date,i.owner_id,i.created_by,i.state,i.version::bigint version,i.created_at,i.updated_at \
                 FROM legal_items i \
                 WHERE i.id=$1 AND i.company_id=$2 \
                   AND ($3::text IS NULL OR i.branch_id=$3) \
                   AND ($4::text IS NULL OR i.department_id=$4) \
                   AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$5)) \
                   AND ($6::text<>'legal_requester' OR (i.kind='request' AND i.created_by=$5))",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(actor.role.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(not_found)?;
            let mut item = record(&row)?;
            item.data = self.subtype(&item.kind, id).await?;
            let documents = sqlx::query(
                "SELECT id,item_id,version::bigint version,name,media_type,checksum,bytes,body,created_by,created_at,locked,object_key \
                 FROM legal_documents WHERE item_id=$1 ORDER BY version DESC LIMIT $2",
            )
            .bind(id)
            .bind(DOCUMENT_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .iter()
            .map(document)
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let events = sqlx::query(
                "SELECT id,item_id,action,actor_id,note,version::bigint version,created_at FROM legal_events \
                 WHERE item_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(id)
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .iter()
            .map(event)
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let members = sqlx::query(
                "SELECT user_id,access FROM legal_members WHERE item_id=$1 ORDER BY user_id",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalMember {
                    user_id: row.try_get("user_id")?,
                    access: row.try_get("access")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let holds = sqlx::query(
                "SELECT h.id,h.item_id,h.reason,h.state,source.company_id source_company,source.branch_id source_branch,source.department_id source_department,source.classification source_classification, \
                   EXISTS(SELECT 1 FROM legal_members sm WHERE sm.item_id=source.id AND sm.user_id=$2) source_member, \
                   (SELECT n.acknowledged_at FROM legal_hold_notices n WHERE n.hold_id=h.id AND n.item_id=t.item_id AND n.user_id=$2) acknowledged_at \
                 FROM legal_holds h JOIN legal_hold_targets t ON t.hold_id=h.id \
                 JOIN legal_items source ON source.id=h.item_id \
                 WHERE t.item_id=$1 AND h.state!='released' ORDER BY h.created_at,h.id LIMIT $3",
            )
            .bind(id)
            .bind(actor.user_id.as_str())
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| hold_row(&row, actor, id))
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let links = sqlx::query(
                "SELECT l.target_id,l.relation,t.title FROM legal_links l JOIN legal_items t ON t.id=l.target_id \
                 WHERE l.source_id=$1 AND t.company_id=$2 \
                   AND ($3::text IS NULL OR t.branch_id=$3) \
                   AND ($4::text IS NULL OR t.department_id=$4) \
                   AND (t.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=t.id AND lm.user_id=$5)) \
                 ORDER BY l.target_id,l.relation LIMIT $6",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalLinkView {
                    target_id: row.try_get("target_id")?,
                    relation: row.try_get("relation")?,
                    title: row.try_get("title")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let comments = sqlx::query(
                "SELECT id,body,visibility,created_by,created_at FROM legal_comments \
                 WHERE item_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(id)
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalCommentView {
                    id: row.try_get("id")?,
                    body: row.try_get("body")?,
                    visibility: row.try_get("visibility")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let envelopes = sqlx::query(
                "SELECT id,state,document_id,expires_at,consent FROM legal_envelopes \
                 WHERE item_id=$1 ORDER BY created_at DESC,id DESC LIMIT 20",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalEnvelopeView {
                    id: row.try_get("id")?,
                    state: row.try_get("state")?,
                    document_id: row.try_get("document_id")?,
                    expires_at: row.try_get("expires_at")?,
                    consent: row.try_get("consent")?,
                    signers: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let signers = sqlx::query(
                "SELECT s.envelope_id,s.user_id,s.ordinal::bigint ordinal,s.state,s.signed_at \
                 FROM legal_signers s JOIN legal_envelopes e ON e.id=s.envelope_id \
                 WHERE e.item_id=$1 ORDER BY s.envelope_id,s.ordinal",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            let mut by_envelope: BTreeMap<String, Vec<LegalSignerView>> = BTreeMap::new();
            for row in signers {
                by_envelope
                    .entry(row.try_get("envelope_id")?)
                    .or_default()
                    .push(LegalSignerView {
                        user_id: row.try_get("user_id")?,
                        ordinal: row.try_get("ordinal")?,
                        state: row.try_get("state")?,
                        signed_at: row.try_get("signed_at")?,
                    });
            }
            let envelopes = envelopes
                .into_iter()
                .map(|mut envelope| {
                    envelope.signers = by_envelope.remove(&envelope.id).unwrap_or_default();
                    envelope
                })
                .collect();
            let shares = if actor.can("legal.share") {
                sqlx::query(
                    "SELECT id,email,document_id,user_id,expires_at,revoked_at FROM legal_shares \
                     WHERE item_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
                )
                .bind(id)
                .bind(DETAIL_LIMIT as i64)
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(|row| {
                    Ok(LegalShareView {
                        id: row.try_get("id")?,
                        email: row.try_get("email")?,
                        document_id: row.try_get("document_id")?,
                        user_id: row.try_get("user_id")?,
                        expires_at: row.try_get("expires_at")?,
                        revoked_at: row.try_get("revoked_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?
            } else {
                Vec::new()
            };
            let submissions = sqlx::query(
                "SELECT u.id,u.name,u.checksum,u.created_by,u.created_at \
                 FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id \
                 WHERE s.item_id=$1 ORDER BY u.created_at DESC,u.id DESC LIMIT $2",
            )
            .bind(id)
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalSubmissionView {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    checksum: row.try_get("checksum")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let access = sqlx::query(
                "SELECT id,document_id,actor_id,action,created_at FROM legal_access_events \
                 WHERE item_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(id)
            .bind(DETAIL_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(LegalAccessEvent {
                    id: row.try_get("id")?,
                    document_id: row.try_get("document_id")?,
                    actor_id: row.try_get("actor_id")?,
                    action: row.try_get("action")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
            sqlx::query(
                "INSERT INTO legal_access_events(id,item_id,document_id,actor_id,action,created_at) \
                 VALUES('legal-access-'||gen_random_uuid()::text,$1,NULL,$2,'record.read',$3)",
            )
            .bind(id)
            .bind(actor.user_id.as_str())
            .bind(self.now)
            .execute(&mut *self.connection)
            .await?;
            Ok(LegalDetail {
                item,
                documents,
                events,
                members,
                holds,
                links,
                comments,
                envelopes,
                shares,
                submissions,
                access,
                can_edit: actor.can("legal.edit"),
                can_hold: actor.can("legal.hold"),
                can_release_hold: actor.can("legal.hold.release"),
                can_share: actor.can("legal.share"),
            })
        })
    }

    fn replay<'a>(
        &'a mut self,
        key: &'a str,
        request_hash: &'a str,
    ) -> LegalRecordsFuture<'a, Option<Value>> {
        Box::pin(async move {
            self.transaction()?;
            self.receipt(key, request_hash).await
        })
    }

    fn source<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        id: &'a str,
    ) -> LegalRecordsFuture<'a, LegalCommandSource> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT i.id,i.company_id,i.branch_id,i.department_id,i.kind,i.title,i.description,\
                 i.classification,i.country,i.jurisdiction,i.language,i.currency,i.timezone,\
                 i.priority,i.due_date,i.owner_id,i.created_by,i.state,i.version::bigint version,i.created_at,i.updated_at \
                 FROM legal_items i WHERE i.id=$1 AND i.company_id=$2 \
                   AND ($3::text IS NULL OR i.branch_id=$3) \
                   AND ($4::text IS NULL OR i.department_id=$4) \
                   AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$5)) \
                   AND ($6::text<>'legal_requester' OR (i.kind='request' AND i.created_by=$5)) \
                 FOR UPDATE",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(actor.role.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(not_found)?;
            let item = record(&row)?;
            let enabled = self.enabled(actor.company_id.as_str()).await?;
            let editor = self.editor(id, actor.user_id.as_str()).await?;
            let frozen = self.frozen(id).await?;
            let holds = self.hold_sources(actor, id).await?;
            let documents = sqlx::query_scalar(
                "SELECT id FROM legal_documents WHERE item_id=$1 ORDER BY version DESC LIMIT $2",
            )
            .bind(id)
            .bind(DOCUMENT_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            let next_document_version = self.next_document_version(id).await?;
            Ok(LegalCommandSource {
                item,
                enabled,
                editor,
                frozen,
                holds,
                documents,
                next_document_version,
            })
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a LegalRecordsActor,
        key: &'a str,
        request_hash: &'a str,
        plan: &'a LegalCommandPlan,
    ) -> LegalRecordsFuture<'a, Value> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("legal:{}", actor.company_id.as_str()))
                .execute(&mut *self.connection)
                .await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))")
                .execute(&mut *self.connection)
                .await?;
            if let Some(prior) = self.receipt(key, request_hash).await? {
                return Ok(prior);
            }
            let current: Option<(i64, String)> = sqlx::query_as(
                "SELECT version::bigint version,state FROM legal_items WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(&plan.item_id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some((version, _)) = current else {
                return Err(not_found().into());
            };
            if version != plan.expected_version {
                return Err(version_conflict().into());
            }
            let effect_state = self.apply_effect(actor, plan).await?;
            let new_state = effect_state.unwrap_or_else(|| plan.new_state.clone());
            let changed = sqlx::query(
                "UPDATE legal_items SET state=$1,version=version+1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND version=$5",
            )
            .bind(&new_state)
            .bind(self.now)
            .bind(&plan.item_id)
            .bind(actor.company_id.as_str())
            .bind(i32_version(plan.expected_version)?)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(version_conflict().into());
            }
            sqlx::query(
                "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("legal-event-{}", short_hash(key)))
            .bind(&plan.item_id)
            .bind(&plan.action)
            .bind(actor.user_id.as_str())
            .bind(&plan.note)
            .bind(i32_version(plan.next_version)?)
            .bind(self.now)
            .execute(&mut *self.connection)
            .await?;
            self.expand_and_notify(actor.company_id.as_str()).await?;
            let mut result_view = plan.result.clone();
            result_view.state = new_state;
            let result = serde_json::to_value(&result_view).map_err(|_| {
                unavailable(
                    "LEGAL_RESULT_UNAVAILABLE",
                    "The Legal command result is unavailable.",
                )
            })?;
            self.save_receipt(actor, key, request_hash, plan, &result)
                .await?;
            Ok(result)
        })
    }
}

impl PgLegalRecords<'_> {
    async fn subtype(&mut self, kind: &str, id: &str) -> Result<Value, LegalRecordsError> {
        let table = match kind {
            "request" => "legal_requests",
            "matter" => "legal_matters",
            "contract" => "legal_contracts",
            "obligation" => "legal_obligations",
            "compliance" => "legal_compliance",
            "license" => "legal_licenses",
            "corporate" => "legal_corporate",
            "ip" => "legal_ip_assets",
            "privacy" => "legal_privacy",
            "counsel" => "legal_counsel",
            "template" => "legal_templates",
            "rule" => "legal_rules",
            _ => {
                return Err(unavailable(
                    "LEGAL_SOURCE_INVALID",
                    "The current Legal source is invalid.",
                )
                .into());
            }
        };
        // The table name comes exclusively from the closed match above.
        let value: Option<Value> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT data FROM {table} WHERE item_id=$1"
        )))
        .bind(id)
        .fetch_optional(&mut *self.connection)
        .await?
        .flatten();
        Ok(value.unwrap_or_else(|| json!({})))
    }
}

fn record(row: &sqlx::postgres::PgRow) -> Result<LegalRecord, sqlx::Error> {
    Ok(LegalRecord {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        classification: row.try_get("classification")?,
        country: row.try_get("country")?,
        jurisdiction: row.try_get("jurisdiction")?,
        language: row.try_get("language")?,
        currency: row.try_get("currency")?,
        timezone: row.try_get("timezone")?,
        priority: row.try_get("priority")?,
        due_date: row.try_get("due_date")?,
        owner_id: row.try_get("owner_id")?,
        created_by: row.try_get("created_by")?,
        state: row.try_get("state")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        data: json!({}),
    })
}

fn document(row: &sqlx::postgres::PgRow) -> Result<LegalDocumentVersion, sqlx::Error> {
    Ok(LegalDocumentVersion {
        id: row.try_get("id")?,
        item_id: row.try_get("item_id")?,
        version: row.try_get("version")?,
        name: row.try_get("name")?,
        media_type: row.try_get("media_type")?,
        checksum: row.try_get("checksum")?,
        bytes: row.try_get("bytes")?,
        body: row.try_get("body")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        locked: row.try_get("locked")?,
        object_key: row.try_get("object_key")?,
    })
}

fn event(row: &sqlx::postgres::PgRow) -> Result<LegalEvent, sqlx::Error> {
    Ok(LegalEvent {
        id: row.try_get("id")?,
        item_id: row.try_get("item_id")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        note: row.try_get("note")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
    })
}

fn hold_row(
    row: &sqlx::postgres::PgRow,
    actor: &LegalRecordsActor,
    item: &str,
) -> Result<LegalHoldView, sqlx::Error> {
    let source_company: String = row.try_get("source_company")?;
    let source_branch: Option<String> = row.try_get("source_branch")?;
    let source_department: Option<String> = row.try_get("source_department")?;
    let source_classification: String = row.try_get("source_classification")?;
    let source_member: bool = row.try_get("source_member")?;
    let visible = source_company == actor.company_id.as_str()
        && actor
            .branch_id
            .as_ref()
            .is_none_or(|branch| source_branch.as_deref() == Some(branch.as_str()))
        && actor
            .department_id
            .as_ref()
            .is_none_or(|department| source_department.as_deref() == Some(department.as_str()))
        && (source_classification == "internal" || source_member);
    let item_id: String = row.try_get("item_id")?;
    Ok(LegalHoldView {
        id: row.try_get("id")?,
        reason: if visible {
            row.try_get("reason")?
        } else {
            "Protected by a related legal hold".into()
        },
        state: row.try_get("state")?,
        direct: item_id == item,
        acknowledged_at: row.try_get("acknowledged_at")?,
    })
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEGAL_NOT_FOUND",
        "Legal record is unavailable in your current scope.",
    )
}

fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_VERSION_CONFLICT",
        "This record changed. Refresh before saving.",
    )
}

pub async fn read_legal_register(
    pool: &DatabasePool,
    actor: &LegalRecordsActor,
    query: LegalRegisterQuery,
) -> Result<LegalRegister, LegalRecordsError> {
    let pool = pool.clone();
    let actor = actor.clone();
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let query = query.clone();
            Box::pin(async move {
                let mut ports = PgLegalRecords::new(connection, None, None);
                legal_register(&mut ports, &actor, query).await
            })
        })
        .await
}

pub async fn read_legal_detail(
    pool: &DatabasePool,
    actor: &LegalRecordsActor,
    id: &str,
) -> Result<LegalDetail, LegalRecordsError> {
    let pool = pool.clone();
    let actor = actor.clone();
    let id = id.to_owned();
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let id = id.clone();
            Box::pin(async move {
                let mut ports = PgLegalRecords::new(connection, None, None);
                legal_detail(&mut ports, &actor, &id).await
            })
        })
        .await
}

pub async fn execute_legal_command(
    pool: &DatabasePool,
    actor: &LegalRecordsActor,
    key: &CommandKey,
    request_hash: &str,
    raw: &Value,
    identity: std::sync::Arc<dyn LegalIdentity>,
    notifier: std::sync::Arc<dyn LegalHoldNotifier>,
) -> Result<Value, LegalRecordsError> {
    let pool = pool.clone();
    let actor = actor.clone();
    let key = key.clone();
    let request_hash = request_hash.to_owned();
    let raw = raw.clone();
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let key = key.clone();
            let request_hash = request_hash.clone();
            let raw = raw.clone();
            let identity = identity.clone();
            let notifier = notifier.clone();
            Box::pin(async move {
                let mut ports = PgLegalRecords::new(connection, Some(identity), Some(notifier));
                legal_command(&mut ports, &actor, &key, &request_hash, &raw).await
            })
        })
        .await
}

pub fn legal_records_business_error(error: LegalRecordsError) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "LEGAL_UNAVAILABLE",
            "The Legal service is unavailable.",
        ),
    }
}
