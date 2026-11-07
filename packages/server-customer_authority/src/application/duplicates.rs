//! Customer Authority duplicate-review use cases.
use crate::domain::access::AccessActor;
use crate::domain::duplicates::{
    DuplicateReviewCustomer, DuplicateReviewEventRow, DuplicateReviewRow, DuplicateReviewRequest,
    MAX_HISTORY_ROWS, MAX_WORKSPACE_ROWS, REQUEST_OPERATION, RequestPlan,
    idempotency_conflict, readback_invalid, request_id, request_identity,
    request_plan, review_not_found, source_not_found,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type DuplicateAccessFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn require_binding_scope(row: &DuplicateReviewRow, actor: &AccessActor) -> Result<(), BusinessError> {
    if row.company_id != actor.company_id.as_str() {
        return Err(review_not_found());
    }
    Ok(())
}

fn receipt_value(
    actor: &AccessActor,
    id: &str,
    request_hash: &str,
    receipt: Option<crate::domain::access::ReceiptSnapshot>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != REQUEST_OPERATION
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["companyId"].as_str() != Some(actor.company_id.as_str())
        || authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
    {
        return Err(idempotency_conflict());
    }
    let parsed: crate::domain::access::AccessReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id || parsed.version < 1 {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn validate_events(rows: Vec<DuplicateReviewEventRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > MAX_HISTORY_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_DUPLICATE_SOURCE_INVALID",
            "The customer duplicate history is not available.",
        ));
    }
    let mut previous = 0;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.version <= previous
            || row.version > 1_000_000
            || row.action.trim().is_empty()
            || row.action.len() > 200
            || row.actor_id.is_empty()
            || row.reason.trim().is_empty()
            || row.created_at < 0
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "CUSTOMER_DUPLICATE_SOURCE_INVALID",
                "The customer duplicate history is not available.",
            ));
        }
        previous = row.version;
        out.push(serde_json::json!({
            "version": row.version,
            "action": row.action,
            "actor_id": row.actor_id,
            "reason": row.reason,
            "canonical_customer_id": row.canonical_customer_id,
            "created_at": row.created_at,
        }));
    }
    Ok(out)
}

pub trait DuplicateReviewCommandRepository {
    type Error: From<BusinessError>;

    fn digest<'a>(&'a mut self, value: &'a str) -> DuplicateAccessFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> DuplicateAccessFuture<'a, Option<crate::domain::access::ReceiptSnapshot>, Self::Error>;
    fn customers_active<'a>(
        &'a mut self,
        company_id: &'a str,
        left: &'a str,
        right: &'a str,
    ) -> DuplicateAccessFuture<'a, i64, Self::Error>;
    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> DuplicateAccessFuture<'a, Value, Self::Error>;
}

pub trait DuplicateReviewReadRepository {
    type Error: From<BusinessError>;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewRow>, Self::Error>;
    fn customers<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewCustomer>, Self::Error>;
    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewEventRow>, Self::Error>;
}

pub async fn request_duplicate_review<R: DuplicateReviewCommandRepository>(
    repository: &mut R,
    actor: &AccessActor,
    key: &CommandKey,
    input: &DuplicateReviewRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_sales(now)?;
    actor.require_read(now)?;
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "CUSTOMER_COMPANY_REVIEW_REQUIRED",
            "Customer duplicate review requires Company scope.",
        )
        .into());
    }
    let input = input.validate()?;
    let id = request_id(key.as_str());
    let identity = request_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let (left, right) = input.sorted_pair();
    let count = repository
        .customers_active(actor.company_id.as_str(), &left, &right)
        .await?;
    if count != 2 {
        return Err(source_not_found().into());
    }
    let plan = request_plan(actor, key.as_str(), &input, now);
    let result = repository
        .commit_request(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn duplicate_review_workspace<R: DuplicateReviewReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    actor.require_sales(now)?;
    let rows = repository.workspace(actor).await?;
    if rows.len() > MAX_WORKSPACE_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_DUPLICATE_SOURCE_INVALID",
            "The customer duplicate review workspace is not available.",
        )
        .into());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if row.company_id != actor.company_id.as_str() {
            continue;
        }
        if !seen.insert(row.id.clone()) {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "CUSTOMER_DUPLICATE_SOURCE_INVALID",
                "The customer duplicate review workspace is not available.",
            )
            .into());
        }
        out.push(row.to_json(actor, now));
    }
    let customers = repository.customers(actor).await?;
    let customers_json: Vec<Value> = customers.iter().map(|c| c.to_json()).collect();
    let can_create = actor.branch_id.is_none() && actor.department_id.is_none();
    Ok(serde_json::json!({"rows": out, "customers": customers_json, "canCreate": can_create}))
}

pub async fn duplicate_review_detail<R: DuplicateReviewReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    actor.require_sales(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(review_not_found().into());
    }
    let rows = repository.workspace(actor).await?;
    let row = rows
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(review_not_found)?;
    require_binding_scope(&row, actor)?;
    row.validate_source()?;
    let events = validate_events(repository.history(id).await?)?;
    Ok(serde_json::json!({
        "row": row.to_json(actor, now),
        "history": events,
    }))
}
