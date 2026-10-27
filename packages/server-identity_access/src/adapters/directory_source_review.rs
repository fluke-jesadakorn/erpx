//! Actual Identity review/receipt storage. Stored payloads remain data; current
//! normal IAM context, source CAS and original proposer authority are mandatory.
use super::{
    authorized_work::HoldingWorkSourcePreparation,
    directory_source_admin::{PgDirectorySourceHoldingPolicy, PreparedDirectorySourceChange},
    permission_catalog::permission_catalog,
    postgres_effective_authority::current_authority_facts,
    work_admission::work_module_catalog,
};
use crate::{
    application::effective_authority::authorize_effective_work,
    domain::{
        directory_source_review::*,
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
        work_context::{ExpectedWorkSelection, WorkContextRequest, WorkScopeKind},
    },
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone)]
pub struct DirectorySourceReviewPreparation;
impl DirectorySourceReviewPreparation {
    pub(super) async fn locks(c: &mut PgConnection) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_TRANSACTION_REQUIRED").into());
        }
        sqlx::query("LOCK TABLE iam_reviews,iam_receipts,iam_audit IN SHARE ROW EXCLUSIVE MODE")
            .execute(c)
            .await?;
        Ok(())
    }
}
impl HoldingWorkSourcePreparation for DirectorySourceReviewPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(Self::locks(c))
    }
}
pub struct StoredDirectorySourceReview {
    id: RecordId,
    payload: StoredSourceProposal,
    status: String,
    version: i64,
    created_by: String,
    created_person: String,
}
impl StoredDirectorySourceReview {
    pub fn require_pending(&self, version: i64, now: i64) -> Result<(), BusinessError> {
        self.payload.require_live(now)?;
        if self.status != "pending" || self.version != version {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED"));
        }
        Ok(())
    }
    pub fn connection_id(&self) -> Result<RecordId, BusinessError> {
        RecordId::new(&self.payload.binding.connection_id)
    }
    pub fn source_request(
        &self,
    ) -> Result<
        crate::application::directory_source_admin::DirectorySourceChangeRequest,
        BusinessError,
    > {
        use crate::domain::directory_provenance::{Denial, SourceRevision};
        crate::application::directory_source_admin::DirectorySourceChangeRequest::new(
            self.connection_id()?,
            self.payload.binding.key_id.clone(),
            self.payload.binding.connection_version,
            self.payload
                .binding
                .expected_revision
                .map(SourceRevision::checked)
                .transpose()
                .map_err(Denial::business)?,
        )
    }
}
pub struct DirectorySourceReviewStore;
fn require_company_scope(
    current: &AuthorizedWorkContext,
    permission: &str,
    now: i64,
) -> Result<(), BusinessError> {
    current.require_permission(permission)?;
    let a = current.selection().assignment();
    if a.scope_kind != WorkScopeKind::Holding
        || a.scope_node_id.is_some()
        || now >= current.expires_at()
    {
        return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED"));
    }
    Ok(())
}
async fn require_admin(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    permission: &str,
    now: i64,
) -> Result<(), Error> {
    require_company_scope(current, permission, now)?;
    let proof = current
        .directory_administration
        .as_ref()
        .ok_or_else(|| review_error("DIRECTORY_SOURCE_ADMIN_PROOF_REQUIRED"))?;
    if proof.holding_id != current.holding_id().as_str()
        || proof.company_id != current.selection().workspace().company_id.as_str()
        || proof.user_id != current.user_id().as_str()
        || proof.session_id != current.session_id().as_str()
        || now >= proof.deadline
        || proof.unit != unit(c).await?
    {
        return Err(review_error("DIRECTORY_SOURCE_ADMIN_PROOF_REQUIRED").into());
    }
    Ok(())
}
fn binding(
    change: &PreparedDirectorySourceChange,
) -> Result<DirectorySourceBinding, BusinessError> {
    DirectorySourceBinding::new(
        change
            .holding_id()
            .ok_or_else(|| review_error("DIRECTORY_SOURCE_PLATFORM_UNAVAILABLE"))?,
        change.company_id(),
        change.connection_id(),
        change.expected_revision().map(|r| r.get()),
        change.proposed_source(),
    )
}
fn pins(current: &AuthorizedWorkContext) -> ProposerPins {
    let s = current.selection();
    let a = s.assignment();
    ProposerPins {
        user_id: current.user_id().as_str().into(),
        person_id: current.person_id().map(|p| p.as_str().into()),
        assignment_id: a.id.as_str().into(),
        assignment_version: a.version,
        role: a.role.clone(),
        role_revision: s.role_revision(),
        identity_version: current.identity_version(),
        session_id: current.session_id().as_str().into(),
        selection_revision: s.revision(),
        branch_slug: s.workspace().branch_slug.clone(),
        department_slug: s.workspace().department_slug.clone(),
        expires_at: current.expires_at(),
    }
}
fn digest(value: &impl Serialize) -> Result<String, BusinessError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value)
            .map_err(|_| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
    )))
}
#[derive(Serialize)]
struct Command<'a, T> {
    actor: &'a str,
    assignment: &'a str,
    company: &'a str,
    operation: &'a str,
    payload: T,
}
fn apply_digest(
    current: &AuthorizedWorkContext,
    id: &RecordId,
    version: i64,
) -> Result<String, BusinessError> {
    digest(&Command {
        actor: current.user_id().as_str(),
        assignment: current.selection().assignment().id.as_str(),
        company: current.selection().workspace().company_id.as_str(),
        operation: "directory.source-review.apply",
        payload: (id.as_str(), version),
    })
}
fn proposal_digest(
    current: &AuthorizedWorkContext,
    request: &crate::application::directory_source_admin::DirectorySourceChangeRequest,
    reason: &str,
) -> Result<String, BusinessError> {
    digest(&Command {
        actor: current.user_id().as_str(),
        assignment: current.selection().assignment().id.as_str(),
        company: current.selection().workspace().company_id.as_str(),
        operation: "directory.source-review.propose",
        payload: (
            request.connection_id().as_str(),
            request.key_id(),
            request.expected_connection_version(),
            request.expected_source_revision().map(|r| r.get()),
            reason,
        ),
    })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(review_error("DIRECTORY_SOURCE_REVIEW_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(review_error("DIRECTORY_SOURCE_REVIEW_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn existing_receipt(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &CommandKey,
    hash: &str,
) -> Result<Option<DirectorySourceReviewResult>, Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,CASE WHEN octet_length(actor_id)<=200 THEN actor_id END actor_id,CASE WHEN octet_length(request_hash)=64 THEN request_hash END request_hash,CASE WHEN octet_length(result::text)<=16384 THEN result END result FROM iam_receipts WHERE id=$1 FOR UPDATE").bind(key.as_str()).fetch_optional(c).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.try_get::<String, _>("company_id")?
        != current.selection().workspace().company_id.as_str()
        || row.try_get::<String, _>("actor_id")? != current.user_id().as_str()
        || row.try_get::<String, _>("request_hash")? != hash
    {
        return Err(review_error("IDEMPOTENCY_CONFLICT").into());
    }
    let value: Option<serde_json::Value> = row.try_get("result")?;
    Ok(Some(
        serde_json::from_value(
            value.ok_or_else(|| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
        )
        .map_err(|_| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
    ))
}
impl DirectorySourceReviewStore {
    pub async fn replay_proposal(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        request: &crate::application::directory_source_admin::DirectorySourceChangeRequest,
        key: &CommandKey,
        reason: &str,
        now: i64,
    ) -> Result<Option<DirectorySourceReviewResult>, Error> {
        require_admin(c, current, "iam.directory.manage", now).await?;
        if current
            .directory_administration
            .as_ref()
            .map(|p| p.connection_id.as_str())
            != Some(request.connection_id().as_str())
        {
            return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
        }
        existing_receipt(c, current, key, &proposal_digest(current, request, reason)?).await
    }
    pub async fn read(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        id: &RecordId,
        now: i64,
    ) -> Result<StoredDirectorySourceReview, Error> {
        if !current.has_permission("iam.directory.manage") && !current.has_permission("iam.review")
        {
            current.require_permission("iam.review")?;
        }
        require_admin(
            c,
            current,
            if current.has_permission("iam.review") {
                "iam.review"
            } else {
                "iam.directory.manage"
            },
            now,
        )
        .await?;
        let row=sqlx::query("SELECT CASE WHEN octet_length(kind)<=80 THEN kind END kind,CASE WHEN octet_length(payload::text)<=16384 THEN payload END payload,CASE WHEN octet_length(created_by)<=200 THEN created_by END created_by,CASE WHEN octet_length(created_person)<=200 THEN created_person END created_person,CASE WHEN octet_length(reason)<=4096 THEN reason END reason,created_at,status,version::bigint version FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR UPDATE").bind(id.as_str()).bind(current.selection().workspace().company_id.as_str()).fetch_optional(c).await?.ok_or_else(||review_error("REVIEW_NOT_FOUND"))?;
        if row.try_get::<String, _>("kind")? != DIRECTORY_SOURCE_REVIEW_KIND {
            return Err(review_error("REVIEW_UNSUPPORTED").into());
        }
        let payload: StoredSourceProposal = serde_json::from_value(
            row.try_get::<Option<serde_json::Value>, _>("payload")?
                .ok_or_else(|| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
        )
        .map_err(|_| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?;
        if payload.binding.holding_id != current.holding_id().as_str()
            || payload.binding.company_id != current.selection().workspace().company_id.as_str()
            || current
                .directory_administration
                .as_ref()
                .map(|p| p.connection_id.as_str())
                != Some(payload.binding.connection_id.as_str())
        {
            return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
        }
        if row.try_get::<Option<&str>, _>("created_by")? != Some(payload.proposer.user_id.as_str())
            || row.try_get::<Option<&str>, _>("created_person")?
                != Some(
                    payload
                        .proposer
                        .person_id
                        .as_deref()
                        .unwrap_or(&payload.proposer.user_id),
                )
            || row.try_get::<Option<&str>, _>("reason")? != Some(payload.reason.as_str())
            || row.try_get::<i64, _>("created_at")? != payload.created_at
        {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
        }
        Ok(StoredDirectorySourceReview {
            id: id.clone(),
            payload,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            created_by: row
                .try_get::<Option<String>, _>("created_by")?
                .ok_or_else(|| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
            created_person: row
                .try_get::<Option<String>, _>("created_person")?
                .ok_or_else(|| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?,
        })
    }
    pub async fn propose(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        change: &PreparedDirectorySourceChange,
        key: &CommandKey,
        reason: &str,
        now: i64,
    ) -> Result<DirectorySourceReviewResult, Error> {
        require_admin(c, current, "iam.directory.manage", now).await?;
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err(review_error("DIRECTORY_SOURCE_REASON_REQUIRED").into());
        }
        let selected = binding(change)?;
        if selected.company_id != current.selection().workspace().company_id.as_str()
            || selected.holding_id != current.holding_id().as_str()
            || current
                .directory_administration
                .as_ref()
                .map(|p| p.connection_id.as_str())
                != Some(selected.connection_id.as_str())
        {
            return Err(review_error("DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED").into());
        }
        let source_request =
            crate::application::directory_source_admin::DirectorySourceChangeRequest::new(
                RecordId::new(change.connection_id())?,
                selected.key_id.clone(),
                selected.connection_version,
                change.expected_revision(),
            )?;
        let hash = proposal_digest(current, &source_request, reason)?;
        if let Some(result) = existing_receipt(c, current, key, &hash).await? {
            return Ok(result);
        }
        let id = format!(
            "directory-source-{}",
            hex::encode(Sha256::digest(key.as_str().as_bytes()))
        );
        let proposer = pins(current);
        let payload = StoredSourceProposal {
            format_version: 1,
            binding: selected,
            proposer,
            reason: reason.into(),
            created_at: now,
            expires_at: now
                .saturating_add(DIRECTORY_SOURCE_REVIEW_MS)
                .min(current.expires_at()),
        };
        payload.require_live(now)?;
        let result = DirectorySourceReviewResult {
            id: id.clone(),
            status: "pending".into(),
            version: 1,
            source_revision: payload.binding.revision,
        };
        let value = serde_json::to_value(&payload)
            .map_err(|_| review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?;
        let person = payload
            .proposer
            .person_id
            .as_deref()
            .unwrap_or(&payload.proposer.user_id);
        sqlx::query("INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason) VALUES($1,$2,$3,$4,$5,$6,$7,'pending',1,$8)").bind(&id).bind(&payload.binding.company_id).bind(DIRECTORY_SOURCE_REVIEW_KIND).bind(&value).bind(&payload.proposer.user_id).bind(person).bind(now).bind(reason).execute(&mut*c).await?;
        record_receipt(c, current, key, &hash, &result, now).await?;
        audit(
            c,
            current,
            &format!("{id}-proposed"),
            "directory.source-review.propose",
            &id,
            &value,
            now,
        )
        .await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        let actual = Self::read(c, current, &RecordId::new(&id)?, now).await?;
        if actual.status != "pending"
            || actual.version != 1
            || actual.created_by != payload.proposer.user_id
            || actual.created_person != person
            || digest(&actual.payload)? != digest(&payload)?
        {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
        }
        if existing_receipt(c, current, key, &hash).await? != Some(result.clone()) {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
        }
        require_receipt_image(c, current, key, &hash, &result, now).await?;
        require_audit_image(
            c,
            current,
            &format!("{id}-proposed"),
            "directory.source-review.propose",
            &id,
            &value,
            now,
        )
        .await?;
        Ok(result)
    }
    /// Current normal IAM and same-Control MFA have already run, including on
    /// replay. A saved result is history only; it never performs source writes.
    pub async fn replay(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        id: &RecordId,
        version: i64,
        key: &CommandKey,
        now: i64,
    ) -> Result<Option<DirectorySourceReviewResult>, Error> {
        require_admin(c, current, "iam.review", now).await?;
        let result =
            existing_receipt(c, current, key, &apply_digest(current, id, version)?).await?;
        if let Some(result) = &result {
            let review = Self::read(c, current, id, now).await?;
            if result.id != id.as_str()
                || result.status != "approved"
                || result.version
                    != version
                        .checked_add(1)
                        .ok_or_else(|| review_error("DIRECTORY_SOURCE_REVIEW_CHANGED"))?
                || result.source_revision != review.payload.binding.revision
                || review.status != "approved"
                || review.version != result.version
            {
                return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
            }
        }
        Ok(result)
    }
    pub async fn retain_apply<'a>(
        c: &mut PgConnection,
        current: &'a AuthorizedWorkContext,
        change: &PreparedDirectorySourceChange,
        id: &RecordId,
        version: i64,
        key: &CommandKey,
        now: i64,
    ) -> Result<CurrentDirectorySourceApproval<'a>, Error> {
        require_admin(c, current, "iam.review", now).await?;
        let review = Self::read(c, current, id, now).await?;
        review.payload.require_live(now)?;
        if review.status != "pending"
            || review.version != version
            || binding(change)? != review.payload.binding
        {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
        }
        if review.created_by != review.payload.proposer.user_id
            || review.created_person
                != review
                    .payload
                    .proposer
                    .person_id
                    .as_deref()
                    .unwrap_or(&review.payload.proposer.user_id)
        {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
        }
        if current.same_person_user(&UserId::new(&review.created_by)?)
            || current.person_id().map(|p| p.as_str()) == Some(review.created_person.as_str())
            || sqlx::query_scalar::<_, bool>("SELECT COALESCE(iam_same_person($1,$2,$3),false)")
                .bind(&review.created_by)
                .bind(current.user_id().as_str())
                .bind(&review.payload.binding.company_id)
                .fetch_one(&mut *c)
                .await?
        {
            return Err(review_error("INDEPENDENT_REVIEW_REQUIRED").into());
        }
        require_proposer(c, &review.payload, now).await?;
        Ok(CurrentDirectorySourceApproval {
            current,
            review,
            unit: unit(c).await?,
            key: key.clone(),
            hash: apply_digest(current, id, version)?,
            deadline: current.expires_at(),
        })
    }
}
async fn require_proposer(
    c: &mut PgConnection,
    p: &StoredSourceProposal,
    now: i64,
) -> Result<(), Error> {
    p.require_live(now)?;
    let a = &p.proposer;
    let request = WorkContextRequest {
        holding_id: HoldingId::new(&p.binding.holding_id)?,
        company_id: CompanyId::new(&p.binding.company_id)?,
        branch_slug: a.branch_slug.clone(),
        department_slug: a.department_slug.clone(),
        session_id: RecordId::new(&a.session_id)?,
        user_id: UserId::new(&a.user_id)?,
        session_expires_at: a.expires_at,
        expected: Some(ExpectedWorkSelection {
            revision: a.selection_revision,
            assignment_id: AssignmentId::new(&a.assignment_id)?,
            assignment_version: a.assignment_version,
        }),
        mutation: true,
    };
    let facts = current_authority_facts(c, &request, now).await?;
    let current = authorize_effective_work(
        &request,
        &WorkAuthorizationRequest {
            permission: "iam.directory.manage".into(),
            module: None,
            destination: "iam/connections".into(),
            api: true,
        },
        facts,
        permission_catalog()?,
        work_module_catalog()?,
        now,
    )?;
    require_company_scope(&current, "iam.directory.manage", now)?;
    if current.selection().assignment().role != a.role
        || current.selection().role_revision() != a.role_revision
        || current.identity_version() != a.identity_version
        || current.person_id().map(|p| p.as_str()) != a.person_id.as_deref()
    {
        return Err(review_error("PROPOSER_AUTHORITY_REVOKED").into());
    }
    Ok(())
}
async fn record_receipt(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &CommandKey,
    hash: &str,
    result: &DirectorySourceReviewResult,
    now: i64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(key.as_str()).bind(current.selection().workspace().company_id.as_str()).bind(current.user_id().as_str()).bind(hash).bind(serde_json::to_value(result).map_err(|_|review_error("DIRECTORY_SOURCE_REVIEW_UNAVAILABLE"))?).bind(now).execute(c).await?;
    Ok(())
}
async fn audit(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    action: &str,
    target: &str,
    value: &serde_json::Value,
    now: i64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(current.selection().workspace().company_id.as_str()).bind(current.user_id().as_str()).bind(current.selection().assignment().id.as_str()).bind(action).bind(target).bind(value).bind(now).execute(c).await?;
    Ok(())
}
async fn require_receipt_image(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &CommandKey,
    hash: &str,
    result: &DirectorySourceReviewResult,
    now: i64,
) -> Result<(), Error> {
    let actual:Option<serde_json::Value>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=16384 THEN to_jsonb(r) END FROM iam_receipts r WHERE id=$1 FOR SHARE").bind(key.as_str()).fetch_one(c).await?;
    let expected = serde_json::json!({"id":key.as_str(),"company_id":current.selection().workspace().company_id.as_str(),"actor_id":current.user_id().as_str(),"request_hash":hash,"result":result,"created_at":now});
    if actual.as_ref() != Some(&expected) {
        return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
    }
    Ok(())
}
async fn require_audit_image(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    action: &str,
    target: &str,
    value: &serde_json::Value,
    now: i64,
) -> Result<(), Error> {
    let actual:Option<serde_json::Value>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=16384 THEN to_jsonb(a) END FROM iam_audit a WHERE id=$1 FOR SHARE").bind(id).fetch_one(c).await?;
    let expected = serde_json::json!({"id":id,"company_id":current.selection().workspace().company_id.as_str(),"actor_id":current.user_id().as_str(),"assignment_id":current.selection().assignment().id.as_str(),"action":action,"target_id":target,"evidence":value,"created_at":now});
    if actual.as_ref() != Some(&expected) {
        return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
    }
    Ok(())
}
/// Borrowed actual current review, never deserializable or constructible from a
/// stored payload. Finalization belongs to the same source Holding transaction.
pub struct CurrentDirectorySourceApproval<'a> {
    current: &'a AuthorizedWorkContext,
    review: StoredDirectorySourceReview,
    unit: Unit,
    key: CommandKey,
    hash: String,
    deadline: i64,
}
impl CurrentDirectorySourceApproval<'_> {
    pub(super) fn deadline(&self) -> i64 {
        self.deadline.min(self.review.payload.expires_at)
    }
    pub fn result(&self) -> DirectorySourceReviewResult {
        DirectorySourceReviewResult {
            id: self.review.id.as_str().into(),
            status: "approved".into(),
            version: self.review.version + 1,
            source_revision: self.review.payload.binding.revision,
        }
    }
    pub(super) fn require_candidate(
        &self,
        change: &PreparedDirectorySourceChange,
    ) -> Result<(), BusinessError> {
        if binding(change)? != self.review.payload.binding {
            return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED"));
        }
        Ok(())
    }
}
impl PgDirectorySourceHoldingPolicy for CurrentDirectorySourceApproval<'_> {
    fn audit_actor(&self) -> &str {
        self.current.user_id().as_str()
    }
    fn audit_assignment(&self) -> &str {
        self.current.selection().assignment().id.as_str()
    }
    fn audit_reason(&self) -> &str {
        &self.review.payload.reason
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if now >= self.deadline || unit(c).await? != self.unit {
                return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
            }
            require_admin(c, self.current, "iam.review", now).await?;
            require_proposer(c, &self.review.payload, now).await?;
            let actual =
                DirectorySourceReviewStore::read(c, self.current, &self.review.id, now).await?;
            if actual.status != "pending"
                || actual.version != self.review.version
                || digest(&actual.payload)? != digest(&self.review.payload)?
            {
                return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
            }
            let n=sqlx::query("UPDATE iam_reviews SET status='approved',version=version+1,reviewed_by=$1,reviewed_at=$2 WHERE id=$3 AND company_id=$4 AND status='pending' AND version=$5").bind(self.current.user_id().as_str()).bind(now).bind(self.review.id.as_str()).bind(&self.review.payload.binding.company_id).bind(self.review.version).execute(&mut*c).await?.rows_affected();
            if n != 1 {
                return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
            }
            let result = self.result();
            record_receipt(c, self.current, &self.key, &self.hash, &result, now).await?;
            let evidence = serde_json::json!({"binding":self.review.payload.binding,"proposedBy":self.review.created_by,"result":result});
            audit(
                c,
                self.current,
                &format!("{}-approved", self.review.id.as_str()),
                "directory.source-review.apply",
                self.review.id.as_str(),
                &evidence,
                now,
            )
            .await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(&mut *c)
                .await?;
            let after =
                DirectorySourceReviewStore::read(c, self.current, &self.review.id, now).await?;
            let approved:Option<(String,i64)>=sqlx::query_as("SELECT reviewed_by,reviewed_at FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR SHARE").bind(self.review.id.as_str()).bind(&self.review.payload.binding.company_id).fetch_optional(&mut*c).await?;
            if after.status != "approved"
                || after.version != self.review.version + 1
                || digest(&after.payload)? != digest(&self.review.payload)?
                || approved != Some((self.current.user_id().as_str().into(), now))
                || existing_receipt(c, self.current, &self.key, &self.hash).await?
                    != Some(result.clone())
            {
                return Err(review_error("DIRECTORY_SOURCE_REVIEW_CHANGED").into());
            }
            require_receipt_image(c, self.current, &self.key, &self.hash, &result, now).await?;
            require_audit_image(
                c,
                self.current,
                &format!("{}-approved", self.review.id.as_str()),
                "directory.source-review.apply",
                self.review.id.as_str(),
                &evidence,
                now,
            )
            .await?;
            require_proposer(c, &self.review.payload, now).await?;
            Ok(())
        })
    }
}
