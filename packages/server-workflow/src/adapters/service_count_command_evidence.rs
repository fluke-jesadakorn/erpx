//! Planned Count ledger evidence. Values come from the trusted actor, request,
//! owner output and the exact clock/identifier sources used by Workflow.
use crate::{
    adapters::postgres_commands::{WorkflowOutput, WorkflowRequest},
    application::commands::{CommandActor, CommandEventScope, command_fingerprint},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, FailureKind, IdentifierSource, RecordId, TimestampMillis,
};
use sqlx::{Connection, PgConnection};
use std::sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_COUNT_COMMAND_EVIDENCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The planned Count command evidence changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// One genuine server-clock sample, shared by the ledger and planned witness.
#[derive(Clone, Copy)]
pub struct CountCommandClock(TimestampMillis);
impl CountCommandClock {
    pub fn sample(clock: &impl Clock) -> Result<Self, BusinessError> {
        let now = clock.now();
        if !(0..=9_007_199_254_740_991).contains(&now.get()) {
            return Err(BusinessError::invalid(
                "COMMAND_TIME_INVALID",
                "Use a valid command time.",
            ));
        }
        Ok(Self(now))
    }
}
impl Clock for CountCommandClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}
/// Records IDs emitted by the real caller-provided source, in Workflow's
/// documented audit-then-outbox order. No identifiers are fabricated here.
#[derive(Default)]
pub struct CountCommandGeneratedIds {
    audit: OnceLock<RecordId>,
    outbox: OnceLock<RecordId>,
    calls: AtomicUsize,
}
impl CountCommandGeneratedIds {
    pub fn wrap<'a, I: IdentifierSource>(
        &'a self,
        source: &'a mut I,
    ) -> CountCommandIdSource<'a, I> {
        CountCommandIdSource {
            generated: self,
            source,
        }
    }
}
pub struct CountCommandIdSource<'a, I> {
    generated: &'a CountCommandGeneratedIds,
    source: &'a mut I,
}
impl<I: IdentifierSource> IdentifierSource for CountCommandIdSource<'_, I> {
    fn next(&mut self) -> RecordId {
        let id = self.source.next();
        match self.generated.calls.fetch_add(1, Ordering::SeqCst) {
            0 => {
                let _ = self.generated.audit.set(id.clone());
            }
            1 => {
                let _ = self.generated.outbox.set(id.clone());
            }
            _ => {}
        }
        id
    }
}
#[derive(Clone)]
pub struct PlannedCountCommandEvidence {
    unit: Unit,
    key: String,
    operation: String,
    record: String,
    actor: String,
    assignment: String,
    authority: String,
    hash: String,
    result: String,
    version: i64,
    payload: String,
    aggregate: String,
    now: i64,
}
/// Plan before receipt insertion. Historical replay never calls this method.
pub async fn plan_count_command_evidence(
    c: &mut PgConnection,
    actor: &CommandActor,
    request: &WorkflowRequest,
    output: &WorkflowOutput,
    clock: CountCommandClock,
) -> Result<PlannedCountCommandEvidence, Error> {
    if !matches!(
        request.operation.as_str(),
        "inventory.count.create"
            | "inventory.count.update"
            | "inventory.count.submit"
            | "inventory.count.approve"
            | "inventory.count.post"
            | "inventory.count.reject"
            | "inventory.count.cancel"
            | "flow.assign"
    ) || request.key.is_empty()
        || request.record_id.is_empty()
        || !(1..=9_007_199_254_740_991).contains(&output.version)
    {
        return Err(invalid());
    }
    let tx = unit(c).await?;
    let hash = command_fingerprint(
        actor,
        &request.operation,
        &request.record_id,
        &request.input,
    )?;
    let authority = actor.receipt_authority()?.as_str().to_owned();
    let state = match output.result.property("state")? {
        Some(v) if v.as_str() != "null" => Some(v),
        _ => output.result.property("status")?,
    };
    let state = state
        .map(|v| v.string_utf16_units())
        .transpose()?
        .flatten()
        .map(|u| String::from_utf16(&u).map_err(|_| changed()))
        .transpose()?;
    let scope = output
        .event_scope
        .clone()
        .unwrap_or_else(|| CommandEventScope::from(actor));
    let payload = actor
        .outbox_payload(&request.record_id, output.version, state.as_deref(), &scope)?
        .as_str()
        .to_owned();
    Ok(PlannedCountCommandEvidence {
        unit: tx,
        key: request.key.clone(),
        operation: request.operation.clone(),
        record: request.record_id.clone(),
        actor: actor.user_id.as_str().into(),
        assignment: actor.assignment_id.as_str().into(),
        authority,
        hash,
        result: output.result.as_str().into(),
        version: output.version,
        payload,
        aggregate: request
            .operation
            .split('.')
            .take(2)
            .collect::<Vec<_>>()
            .join("_"),
        now: clock.now().get(),
    })
}
#[derive(Clone, PartialEq, Eq)]
struct Images {
    receipt: String,
    audit: String,
    outbox: String,
}
pub struct RetainedCountCommandEvidence {
    plan: PlannedCountCommandEvidence,
    audit: String,
    outbox: String,
    images: Images,
}
impl PlannedCountCommandEvidence {
    /// Check all planned values and exact row counts before retaining any image.
    pub async fn retain(
        &self,
        c: &mut PgConnection,
        ids: &CountCommandGeneratedIds,
    ) -> Result<RetainedCountCommandEvidence, Error> {
        if ids.calls.load(Ordering::SeqCst) != 2 {
            return Err(changed());
        }
        let audit = ids.audit.get().ok_or_else(changed)?.as_str().to_owned();
        let outbox = ids.outbox.get().ok_or_else(changed)?.as_str().to_owned();
        let images = self.capture(c, &audit, &outbox).await?;
        Ok(RetainedCountCommandEvidence {
            plan: self.clone(),
            audit,
            outbox,
            images,
        })
    }
    async fn capture(
        &self,
        c: &mut PgConnection,
        audit: &str,
        outbox: &str,
    ) -> Result<Images, Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let receipt = serde_json::json!({"command_id":self.key,"operation":self.operation,"actor_id":self.actor,"request_hash":self.hash,"record_id":self.record,"result_json":self.result,"created_at":self.now,"authority_json":self.authority});
        let audit_expected = serde_json::json!({"id":audit,"command_id":self.key,"actor_id":self.actor,"assignment_id":self.assignment,"action":self.operation,"record_type":self.aggregate,"record_id":self.record,"record_version":self.version,"purpose":self.operation,"created_at":self.now,"authority_json":self.authority});
        let outbox_expected = serde_json::json!({"id":outbox,"command_id":self.key,"event_type":self.operation,"aggregate_type":self.aggregate,"aggregate_id":self.record,"aggregate_version":self.version,"payload_json":self.payload,"created_at":self.now,"published_at":null,"claim_until":null,"attempts":0,"processed_at":null,"next_attempt_at":0,"lease_token":null,"last_error":null,"dead_letter_at":null});
        let r:Option<String>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 AND to_jsonb(r)=$2::jsonb THEN to_jsonb(r)::text END FROM command_receipts r WHERE command_id=$1").bind(&self.key).bind(receipt).fetch_optional(&mut *c).await?.flatten();
        let a:Vec<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=2097152 AND to_jsonb(a)=$2::jsonb THEN to_jsonb(a)::text END FROM audit_events a WHERE command_id=$1 ORDER BY id LIMIT 2").bind(&self.key).bind(audit_expected).fetch_all(&mut *c).await?;
        let o:Vec<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(o)::text)<=2097152 AND to_jsonb(o)=$2::jsonb THEN to_jsonb(o)::text END FROM outbox_events o WHERE command_id=$1 ORDER BY id LIMIT 2").bind(&self.key).bind(outbox_expected).fetch_all(c).await?;
        if a.len() != 1 || o.len() != 1 {
            return Err(changed());
        }
        Ok(Images {
            receipt: r.ok_or_else(changed)?,
            audit: a.into_iter().next().flatten().ok_or_else(changed)?,
            outbox: o.into_iter().next().flatten().ok_or_else(changed)?,
        })
    }
}
impl RetainedCountCommandEvidence {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.plan.capture(c, &self.audit, &self.outbox).await? != self.images {
            return Err(changed());
        }
        Ok(())
    }
}

/// Validate immutable historical command evidence without consulting the current
/// Flow route or preparing new business effects. Worker-owned outbox delivery
/// fields may advance after the original command commits.
pub async fn verify_historical_count_command(
    c: &mut PgConnection,
    actor: &CommandActor,
    request: &WorkflowRequest,
    branch: &str,
    posted_journal: Option<&str>,
) -> Result<(), Error> {
    use crate::application::commands::CommandField;
    if request.operation == "inventory.count.update" {
        return verify_historical_count_revision(c, actor, request).await;
    }
    let _ = unit(c).await?;
    let input: serde_json::Value =
        serde_json::from_str(request.input.as_str()).map_err(|_| changed())?;
    let expected = input["expectedVersion"]
        .as_i64()
        .filter(|v| *v >= 0 && *v < 9_007_199_254_740_991)
        .ok_or_else(changed)?;
    let (status, event_action, version, result_version, reason, step, decision) =
        match request.operation.as_str() {
            "flow.assign" => (
                None,
                "assign",
                input["recordVersion"]
                    .as_i64()
                    .filter(|v| *v > 0)
                    .ok_or_else(changed)?,
                if expected == 0 { 2 } else { expected + 1 },
                input["reason"].as_str().ok_or_else(changed)?,
                input["stepKey"].as_str(),
                false,
            ),
            operation => {
                let (action, state, review) = match operation {
                    "inventory.count.submit" => ("submit", "submitted", false),
                    "inventory.count.approve" => ("approve", "approved", true),
                    "inventory.count.post" => ("post", "posted", true),
                    "inventory.count.reject" => ("reject", "rejected", true),
                    "inventory.count.cancel" => ("cancel", "cancelled", false),
                    _ => return Err(invalid()),
                };
                if expected == 0 {
                    return Err(changed());
                }
                (
                    Some(state),
                    action,
                    expected + 1,
                    expected + 1,
                    input["reason"].as_str().unwrap_or(""),
                    None,
                    review,
                )
            }
        };
    let result = if let Some(status) = status {
        serde_json::json!({"id":request.record_id,"status":status,"version":result_version,"journalId":if event_action == "post" { posted_journal } else { None }})
    } else {
        serde_json::json!({"id":request.record_id,"version":result_version})
    };
    let scope = if event_action == "assign" {
        CommandEventScope {
            company_id: actor.company_id.clone(),
            branch_id: CommandField::Value(branch.into()),
            department_id: CommandField::Null,
        }
    } else {
        CommandEventScope::from(actor)
    };
    let payload = actor.outbox_payload(&request.record_id, version, status, &scope)?;
    let authority = actor.receipt_authority()?;
    let hash = command_fingerprint(
        actor,
        &request.operation,
        &request.record_id,
        &request.input,
    )?;
    let aggregate = if event_action == "assign" {
        "flow_assign"
    } else {
        "inventory_count"
    };
    let ok: bool = sqlx::query_scalar(r#"
      SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=$1
        AND octet_length(to_jsonb(r)::text)<=2097152 AND r.operation=$2 AND r.record_id=$3
        AND r.actor_id=$4 AND r.request_hash=$5 AND (r.authority_json::jsonb - 'iam')=($6::text::jsonb - 'iam')
        AND r.result_json::jsonb=$7::jsonb AND r.created_at BETWEEN 0 AND 9007199254740991
        AND (SELECT count(*) FROM audit_events a WHERE a.command_id=$1)=1
        AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=$1 AND octet_length(to_jsonb(a)::text)<=2097152
          AND a.actor_id=$4 AND a.assignment_id=$8 AND a.action=$2 AND a.record_type=$9
          AND a.record_id=$3 AND a.record_version=$10 AND a.purpose=$2 AND a.authority_json=r.authority_json AND a.created_at=r.created_at)
        AND (SELECT count(*) FROM outbox_events o WHERE o.command_id=$1)=1
        AND EXISTS(SELECT 1 FROM outbox_events o WHERE o.command_id=$1 AND octet_length(to_jsonb(o)::text)<=2097152
          AND o.event_type=$2 AND o.aggregate_type=$9 AND o.aggregate_id=$3 AND o.aggregate_version=$10
          AND o.payload_json::jsonb=jsonb_set($11::text::jsonb,'{authority}',r.authority_json::jsonb - ARRAY['companyId','branchId','departmentId','scopeKind','scopeNodeId']) AND o.created_at=r.created_at)
        AND (SELECT count(*) FROM flow_events f WHERE f.command_id=$1)=1
        AND EXISTS(SELECT 1 FROM flow_events f WHERE f.command_id=$1 AND octet_length(to_jsonb(f)::text)<=2097152
          AND f.company_id=$12 AND f.kind='stock_count' AND f.record_id=$3 AND f.action=$13
          AND f.created_at BETWEEN 0 AND 9007199254740991 AND f.actor_user_id=$4 AND (CASE WHEN $13='assign' THEN left(f.reason,length($14)+3)=$14||' → ' AND length(f.reason)>length($14)+3 ELSE f.reason=$14 END) AND f.record_version=$10
          AND CASE $13
            WHEN 'assign' THEN f.step_key=$15
            WHEN 'submit' THEN f.step_key='draft'
            WHEN 'cancel' THEN f.step_key='draft'
            WHEN 'approve' THEN f.step_key='submitted'
            WHEN 'post' THEN f.step_key='approved'
            WHEN 'reject' THEN f.step_key IN ('submitted','approved')
            ELSE false END
          AND f.next_step_key IS NOT DISTINCT FROM $16::text
          AND ($13 NOT IN ('assign','post','reject','cancel') OR f.next_user_id IS NULL))
        AND (SELECT count(*) FROM decision_evidence d WHERE d.command_id=$1)=CASE WHEN $17 THEN 1 ELSE 0 END
        AND (NOT $17 OR EXISTS(SELECT 1 FROM decision_evidence d WHERE d.command_id=$1 AND octet_length(to_jsonb(d)::text)<=2097152
          AND d.record_id=$3 AND d.action=$2 AND d.actor_id=$4 AND d.assignment_id=$8
          AND d.created_at BETWEEN 0 AND 9007199254740991 AND d.reviewed_version=$10-1 AND d.authority_json=r.authority_json AND d.input_digest ~ '^[0-9a-f]{64}$')))
    "#).bind(&request.key).bind(&request.operation).bind(&request.record_id).bind(actor.user_id.as_str()).bind(hash)
        .bind(authority.as_str()).bind(result).bind(actor.assignment_id.as_str()).bind(aggregate).bind(version)
        .bind(payload.as_str()).bind(actor.company_id.as_str()).bind(event_action).bind(reason).bind(step).bind(status).bind(decision)
        .fetch_one(c).await?;
    if !ok {
        return Err(changed());
    }
    Ok(())
}

/// Generic draft revision has ordinary command evidence, but no decision or
/// Flow event. Never infer its historical output from the current Count status.
async fn verify_historical_count_revision(
    c: &mut PgConnection,
    actor: &CommandActor,
    request: &WorkflowRequest,
) -> Result<(), Error> {
    let _ = unit(c).await?;
    // Inspect only this normalized scalar: valid unknown/stripped UTF-16 data
    // must not be reparsed through serde_json's stricter string representation.
    let expected = request
        .input
        .property("expectedVersion")?
        .and_then(|v| v.as_str().parse::<i64>().ok())
        .filter(|v| *v > 0 && *v < 9_007_199_254_740_991)
        .ok_or_else(changed)?;
    let version = expected + 1;
    let result = serde_json::json!({"id":request.record_id,"status":"draft","version":version});
    let scope = CommandEventScope::from(actor);
    let payload = actor.outbox_payload(&request.record_id, version, Some("draft"), &scope)?;
    let authority = actor.receipt_authority()?;
    let hash = command_fingerprint(
        actor,
        &request.operation,
        &request.record_id,
        &request.input,
    )?;
    let ok: bool = sqlx::query_scalar(r#"
      SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=$1
        AND octet_length(to_jsonb(r)::text)<=2097152 AND r.operation='inventory.count.update' AND r.record_id=$2
        AND r.actor_id=$3 AND r.request_hash=$4 AND (r.authority_json::jsonb - 'iam')=($5::text::jsonb - 'iam')
        AND r.result_json::jsonb=$6::jsonb AND r.created_at BETWEEN 0 AND 9007199254740991
        AND (SELECT count(*) FROM audit_events a WHERE a.command_id=$1)=1
        AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=$1 AND octet_length(to_jsonb(a)::text)<=2097152
          AND a.actor_id=$3 AND a.assignment_id=$7 AND a.action='inventory.count.update' AND a.record_type='inventory_count'
          AND a.record_id=$2 AND a.record_version=$8 AND a.purpose='inventory.count.update' AND a.authority_json=r.authority_json AND a.created_at=r.created_at)
        AND (SELECT count(*) FROM outbox_events o WHERE o.command_id=$1)=1
        AND EXISTS(SELECT 1 FROM outbox_events o WHERE o.command_id=$1 AND octet_length(to_jsonb(o)::text)<=2097152
          AND o.event_type='inventory.count.update' AND o.aggregate_type='inventory_count' AND o.aggregate_id=$2 AND o.aggregate_version=$8
          AND o.payload_json::jsonb=jsonb_set($9::text::jsonb,'{authority}',r.authority_json::jsonb - ARRAY['companyId','branchId','departmentId','scopeKind','scopeNodeId']) AND o.created_at=r.created_at)
        AND NOT EXISTS(SELECT 1 FROM flow_events f WHERE f.command_id=$1)
        AND NOT EXISTS(SELECT 1 FROM decision_evidence d WHERE d.command_id=$1))
    "#).bind(&request.key).bind(&request.record_id).bind(actor.user_id.as_str()).bind(hash)
        .bind(authority.as_str()).bind(result).bind(actor.assignment_id.as_str()).bind(version)
        .bind(payload.as_str()).fetch_one(c).await?;
    if !ok {
        return Err(changed());
    }
    Ok(())
}
