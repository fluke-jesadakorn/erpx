//! Customer Authority customer identity-change use cases.
//!
//! Ports supply the current customer master, the owning transaction, replay and
//! the committed readback. This layer never opens a transaction or reads HTTP.
use crate::domain::access::AccessActor;
use crate::domain::identity::{
    ChangePlan, CustomerSnapshot, IdentityChange, IdentityChangeRow, IdentityCustomer,
    IdentityEventRow, IdentityRequest, MAX_CANDIDATES, MAX_HISTORY_ROWS, MAX_WORKSPACE_ROWS,
    REQUEST_OPERATION, RequestPlan, change_identity, change_not_found, change_plan,
    customer_changed, customer_not_found,
    idempotency_conflict, independent_approver_required, readback_invalid, request_id,
    request_identity, request_plan, resubmit_forbidden, reviewer_unavailable, unchanged,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type IdentityFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn require_company_scope(actor: &AccessActor) -> Result<(), BusinessError> {
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "CUSTOMER_COMPANY_REVIEW_REQUIRED",
            "Use a Company assignment for identity changes.",
        ));
    }
    Ok(())
}

fn receipt_value(
    actor: &AccessActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<crate::domain::access::ReceiptSnapshot>,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
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
    if parsed.id != id
        || parsed.version < 1
        || !matches!(
            parsed.status.as_str(),
            "pending" | "applied" | "rejected"
        )
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn validate_events(rows: Vec<IdentityEventRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > MAX_HISTORY_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_IDENTITY_SOURCE_INVALID",
            "The customer identity history is not available.",
        ));
    }
    let mut previous = 0;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.id.is_empty()
            || row.change_id.is_empty()
            || row.version <= previous
            || row.version > 1_000_000
            || row.action.trim().is_empty()
            || row.action.len() > 40
            || row.actor_id.is_empty()
            || row.reason.trim().is_empty()
            || row.customer_version < 1
            || row.created_at < 0
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "CUSTOMER_IDENTITY_SOURCE_INVALID",
                "The customer identity history is not available.",
            ));
        }
        previous = row.version;
        out.push(row.to_json());
    }
    Ok(out)
}

pub trait IdentityCommandRepository {
    type Error: From<BusinessError>;

    fn digest<'a>(&'a mut self, value: &'a str) -> IdentityFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> IdentityFuture<'a, Option<crate::domain::access::ReceiptSnapshot>, Self::Error>;
    fn source_customer<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_id: &'a str,
    ) -> IdentityFuture<'a, Option<CustomerSnapshot>, Self::Error>;
    fn customer_version<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_id: &'a str,
    ) -> IdentityFuture<'a, Option<i64>, Self::Error>;
    fn change_for_update<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> IdentityFuture<'a, Option<IdentityChangeRow>, Self::Error>;
    fn reviewer_eligible<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> IdentityFuture<'a, bool, Self::Error>;
    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> IdentityFuture<'a, Value, Self::Error>;
    fn commit_change<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> IdentityFuture<'a, Value, Self::Error>;
}

pub trait IdentityReadRepository {
    type Error: From<BusinessError>;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> IdentityFuture<'a, Vec<IdentityChangeRow>, Self::Error>;
    fn customers<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> IdentityFuture<'a, Vec<IdentityCustomer>, Self::Error>;
    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> IdentityFuture<'a, Vec<IdentityEventRow>, Self::Error>;
}

pub async fn request_customer_identity_change<R: IdentityCommandRepository>(
    repository: &mut R,
    actor: &AccessActor,
    key: &CommandKey,
    input: &IdentityRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_sales(now)?;
    actor.require_read(now)?;
    require_company_scope(actor)?;
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
        REQUEST_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let source = repository
        .source_customer(actor.company_id.as_str(), &input.customer_id)
        .await?
        .ok_or_else(customer_not_found)?;
    if source.version != input.expected_customer_version {
        return Err(customer_changed().into());
    }
    if source.identity == input.proposed {
        return Err(unchanged().into());
    }
    let plan = request_plan(actor, key.as_str(), &input, &source.identity, now);
    let result = repository
        .commit_request(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn change_customer_identity<R: IdentityCommandRepository>(
    repository: &mut R,
    actor: &AccessActor,
    key: &CommandKey,
    id: &str,
    input: &IdentityChange,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_sales(now)?;
    actor.require_read(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(change_not_found().into());
    }
    let input = input.validate()?;
    let operation = input.action.operation();
    let identity = change_identity(actor, key.as_str(), id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        id,
        &operation,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let row = repository
        .change_for_update(actor, id)
        .await?
        .ok_or_else(change_not_found)?;
    if row.company_id != actor.company_id.as_str() {
        return Err(change_not_found().into());
    }
    row.validate_source()?;
    if input.action.is_resubmit() {
        if row.created_by != actor.user_id.as_str() && !actor.can_approve() {
            return Err(resubmit_forbidden().into());
        }
    } else {
        actor.require_approve(now)?;
        if row.created_by == actor.user_id.as_str() {
            return Err(independent_approver_required().into());
        }
        if !repository.reviewer_eligible(actor, id).await? {
            return Err(reviewer_unavailable().into());
        }
    }
    let plan = change_plan(actor, &row, key.as_str(), &input, now)?;
    let current = repository
        .customer_version(actor.company_id.as_str(), &plan.customer_id)
        .await?
        .ok_or_else(customer_changed)?;
    if current != plan.expected_customer_version {
        return Err(customer_changed().into());
    }
    let result = repository
        .commit_change(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn identity_workspace<R: IdentityReadRepository>(
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
            "CUSTOMER_IDENTITY_SOURCE_INVALID",
            "The customer identity workspace is not available.",
        )
        .into());
    }
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if row.company_id != actor.company_id.as_str() {
            continue;
        }
        out.push(row.to_json(actor));
    }
    let customers = repository.customers(actor).await?;
    if customers.len() > MAX_CANDIDATES {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_IDENTITY_SOURCE_INVALID",
            "The customer identity workspace is not available.",
        )
        .into());
    }
    let customers_json: Vec<Value> = customers.iter().map(IdentityCustomer::to_json).collect();
    let can_create = actor.branch_id.is_none() && actor.department_id.is_none();
    Ok(serde_json::json!({"rows": out, "customers": customers_json, "canCreate": can_create}))
}

pub async fn identity_detail<R: IdentityReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    actor.require_sales(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(change_not_found().into());
    }
    let rows = repository.workspace(actor).await?;
    let row = rows
        .into_iter()
        .find(|r| r.id == id && r.company_id == actor.company_id.as_str())
        .ok_or_else(change_not_found)?;
    row.validate_source()?;
    let events = validate_events(repository.history(id).await?)?;
    Ok(serde_json::json!({
        "row": row.to_json(actor),
        "history": events,
    }))
}

/// The retained TypeScript reader exposes the same review flags for Work.
pub async fn identity_work<R: IdentityReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    now: i64,
) -> Result<Vec<Value>, R::Error> {
    if !actor.has("commerce.sales") || !actor.has("commerce.read") {
        return Ok(Vec::new());
    }
    actor.require_read(now)?;
    actor.require_sales(now)?;
    let rows = repository.workspace(actor).await?;
    Ok(rows
        .into_iter()
        .filter(|row| row.company_id == actor.company_id.as_str())
        .map(|row| row.work_item(actor))
        .collect())
}
