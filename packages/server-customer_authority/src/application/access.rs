//! Customer Authority customer-access use cases.
//!
//! Ports supply current order facts, the owning transaction, replay and the
//! committed readback. This layer never opens a transaction or reads HTTP.
use crate::domain::access::{
    AccessActor, AccessBindingRow, AccessChange, AccessEventRow, AccessOrder, AccessReadRow,
    AccessReceipt, AccessRequest, AccessSourceRow, ChangePlan, ReceiptSnapshot, RequestPlan,
    binding_changed, binding_not_found, change_identity, change_plan, idempotency_conflict,
    readback_invalid, request_id, request_identity, request_plan, source_not_found,
    validate_request_source,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type AccessFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn require_binding(
    row: Option<AccessBindingRow>,
    actor: &AccessActor,
) -> Result<AccessBindingRow, BusinessError> {
    let row = row.ok_or_else(binding_not_found)?;
    row.validate_source()?;
    if !actor.in_scope(
        &row.company_id,
        row.branch_id.as_deref(),
        row.department_id.as_deref(),
    ) {
        return Err(binding_not_found());
    }
    Ok(row)
}

fn require_scope(row: &AccessBindingRow, actor: &AccessActor) -> Result<(), BusinessError> {
    if !actor.in_scope(
        &row.company_id,
        row.branch_id.as_deref(),
        row.department_id.as_deref(),
    ) {
        return Err(binding_not_found());
    }
    Ok(())
}

fn receipt_value(
    actor: &AccessActor,
    id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<ReceiptSnapshot>,
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
    let parsed: AccessReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed.id != id
        || parsed.version < 1
        || !matches!(
            parsed.status.as_str(),
            "pending" | "approved" | "rejected" | "revoked"
        )
    {
        return Err(readback_invalid());
    }
    Ok(Some(parsed.to_wire()))
}

fn validate_events(rows: Vec<AccessEventRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > crate::domain::access::MAX_EVENT_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_ACCESS_SOURCE_INVALID",
            "The customer access history is not available.",
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
                "CUSTOMER_ACCESS_SOURCE_INVALID",
                "The customer access history is not available.",
            ));
        }
        previous = row.version;
        out.push(serde_json::json!({
            "version": row.version,
            "action": row.action,
            "actor_id": row.actor_id,
            "reason": row.reason,
            "created_at": row.created_at,
        }));
    }
    Ok(out)
}

fn validate_reads(rows: Vec<AccessReadRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > crate::domain::access::MAX_READ_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_ACCESS_SOURCE_INVALID",
            "The customer read audit is not available.",
        ));
    }
    rows.into_iter()
        .map(|row| {
            if row.binding_version < 1
                || row.user_id.is_empty()
                || row.read_at < 0
                || row.projection_version.is_empty()
                || row.projection_version.len() > 200
            {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "CUSTOMER_ACCESS_SOURCE_INVALID",
                    "The customer read audit is not available.",
                ));
            }
            Ok(serde_json::json!({
                "binding_version": row.binding_version,
                "user_id": row.user_id,
                "read_at": row.read_at,
                "projection_version": row.projection_version,
            }))
        })
        .collect()
}

fn validate_sources(rows: Vec<AccessSourceRow>) -> Result<Vec<Value>, BusinessError> {
    if rows.len() > crate::domain::access::MAX_SOURCE_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_ACCESS_SOURCE_INVALID",
            "The approved customer document list is not available.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    rows.into_iter()
        .map(|row| {
            if row.id.is_empty()
                || row.id.len() > 200
                || row.number.is_empty()
                || row.number.len() > 200
                || row.name.is_empty()
                || row.name.len() > 300
                || row.version < 1
                || !seen.insert(row.id.clone())
            {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "CUSTOMER_ACCESS_SOURCE_INVALID",
                    "The approved customer document list is not available.",
                ));
            }
            Ok(row.to_json())
        })
        .collect()
}

pub trait CustomerAccessCommandRepository {
    type Error: From<BusinessError>;

    fn digest<'a>(&'a mut self, value: &'a str) -> AccessFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> AccessFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
    fn order<'a>(
        &'a mut self,
        company_id: &'a str,
        order_id: &'a str,
    ) -> AccessFuture<'a, Option<AccessOrder>, Self::Error>;
    fn binding_for_update<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error>;
    fn reviewer_eligible<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        binding_id: &'a str,
    ) -> AccessFuture<'a, bool, Self::Error>;
    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> AccessFuture<'a, Value, Self::Error>;
    fn commit_change<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> AccessFuture<'a, Value, Self::Error>;
}

pub trait CustomerAccessReadRepository {
    type Error: From<BusinessError>;

    fn binding<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error>;
    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessBindingRow>, Self::Error>;
    fn sources<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessSourceRow>, Self::Error>;
    fn events<'a>(&'a mut self, id: &'a str) -> AccessFuture<'a, Vec<AccessEventRow>, Self::Error>;
    fn reads<'a>(&'a mut self, id: &'a str) -> AccessFuture<'a, Vec<AccessReadRow>, Self::Error>;
}

pub async fn request_access<R: CustomerAccessCommandRepository>(
    repository: &mut R,
    actor: &AccessActor,
    key: &CommandKey,
    input: &AccessRequest,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_sales(now)?;
    actor.require_read(now)?;
    let input = input.validate(now)?;
    let id = request_id(key.as_str());
    let identity = request_identity(actor, key.as_str(), &id, &input);
    let hash = repository.digest(&identity).await?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        &id,
        crate::domain::access::REQUEST_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let order = repository
        .order(actor.company_id.as_str(), &input.order_id)
        .await?;
    let order = validate_request_source(actor, &input, order)?;
    let plan = request_plan(actor, key.as_str(), &input, &order, now);
    let result = repository
        .commit_request(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn change_access<R: CustomerAccessCommandRepository>(
    repository: &mut R,
    actor: &AccessActor,
    key: &CommandKey,
    id: &str,
    input: &AccessChange,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_sales(now)?;
    actor.require_read(now)?;
    let input = input.validate()?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(binding_not_found().into());
    }
    let identity = change_identity(actor, key.as_str(), id, &input);
    let operation = input.action.operation();
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
    let binding = repository
        .binding_for_update(actor, id)
        .await?
        .ok_or_else(binding_not_found)?;
    require_scope(&binding, actor)?;
    if binding.order_id.is_empty() || binding.customer_id.is_empty() {
        return Err(binding_changed().into());
    }
    let order = repository
        .order(actor.company_id.as_str(), &binding.order_id)
        .await?
        .ok_or_else(source_not_found)?;
    if order.id != binding.order_id
        || order.company_id != binding.company_id
        || order.customer_id != binding.customer_id
        || order.branch_id != binding.branch_id
        || order.department_id != binding.department_id
    {
        return Err(source_not_found().into());
    }
    let plan = change_plan(actor, &binding, &order, &input, key.as_str(), now)?;
    if input.action.is_review() && !repository.reviewer_eligible(actor, id).await? {
        return Err(crate::domain::access::reviewer_unavailable().into());
    }
    let result = repository
        .commit_change(actor, key.as_str(), &plan, &hash)
        .await?;
    Ok(result)
}

pub async fn access_workspace<R: CustomerAccessReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    actor.require_sales(now)?;
    let rows = repository.workspace(actor).await?;
    if rows.len() > crate::domain::access::MAX_WORKSPACE_ROWS {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "CUSTOMER_ACCESS_SOURCE_INVALID",
            "The customer access workspace is not available.",
        )
        .into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        row.validate_source()?;
        if !actor.in_scope(
            &row.company_id,
            row.branch_id.as_deref(),
            row.department_id.as_deref(),
        ) {
            continue;
        }
        if !seen.insert(row.id.clone()) {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "CUSTOMER_ACCESS_SOURCE_INVALID",
                "The customer access workspace is not available.",
            )
            .into());
        }
        out.push(row.to_json(actor, now));
    }
    let sources = validate_sources(repository.sources(actor).await?)?;
    Ok(serde_json::json!({"rows": out, "sources": sources}))
}

pub async fn access_detail<R: CustomerAccessReadRepository>(
    repository: &mut R,
    actor: &AccessActor,
    id: &str,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    actor.require_sales(now)?;
    if RecordId::new(id.to_owned()).is_err() {
        return Err(binding_not_found().into());
    }
    let row = require_binding(repository.binding(actor, id).await?, actor)?;
    let events = validate_events(repository.events(id).await?)?;
    let reads = validate_reads(repository.reads(id).await?)?;
    Ok(serde_json::json!({
        "binding": row.to_json(actor, now),
        "events": events,
        "reads": reads,
    }))
}

/// The retained TypeScript reader exposes the same decision flags for Work.
pub fn work_item(row: &AccessBindingRow, actor: &AccessActor, now: i64) -> Value {
    let pending = row.status == "pending" && row.expires_at > now;
    let can_act = pending && row.can_review(actor);
    serde_json::json!({
        "id": row.id,
        "type": "customer_access",
        "title": format!("Customer access · {}", row.order_number),
        "detail": "Verify the recipient and authority evidence before sharing this source.",
        "href": format!("/customer-access/{}", row.id),
        "status": if row.expires_at <= now { "expired" } else { row.status.as_str() },
        "updatedAt": row.updated_at,
        "queue": if pending { if can_act { "todo" } else { "waiting" } } else { "completed" },
        "canAct": can_act,
        "responsibleQueue": "Independent sales manager",
        "nextAction": if pending { "Review or revoke the access request" } else { "Read retained access evidence" },
        "blocker": if pending && row.reviewers == 0 { Some("No eligible independent sales manager; administrator coverage is required") } else { None },
        "relationships": [if row.created_by == actor.user_id.as_str() { "requested" } else if can_act { "approval" } else { "team" }],
    })
}
