//! Planned Field intake ledger evidence. Values come from the trusted actor, request,
//! owner output and the exact clock/identifier sources used by Workflow.
use crate::{
    adapters::postgres_commands::{SourceOrderedWorkflowRequest, WorkflowOutput, WorkflowRequest},
    application::commands::{CommandActor, CommandEventScope, command_fingerprint_source_ordered},
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
    PersistenceError::new("FIELD_INTAKE_COMMAND_EVIDENCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The planned attachment command evidence changed.",
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
pub struct FieldIntakeCommandClock(TimestampMillis);
impl FieldIntakeCommandClock {
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
impl Clock for FieldIntakeCommandClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}
/// Records IDs emitted by the real caller-provided source, in Workflow's
/// documented audit-then-outbox order. No identifiers are fabricated here.
pub struct FieldIntakeCommandGeneratedIds {
    audit: OnceLock<RecordId>,
    outbox: OnceLock<RecordId>,
    calls: AtomicUsize,
}
impl FieldIntakeCommandGeneratedIds {
    pub fn reserve<I: IdentifierSource>(source: &mut I) -> Self {
        let audit = OnceLock::new();
        let outbox = OnceLock::new();
        let _ = audit.set(source.next());
        let _ = outbox.set(source.next());
        Self {
            audit,
            outbox,
            calls: AtomicUsize::new(0),
        }
    }
    pub fn wrap<'a, I: IdentifierSource>(
        &'a self,
        source: &'a mut I,
    ) -> FieldIntakeCommandIdSource<'a, I> {
        FieldIntakeCommandIdSource {
            generated: self,
            source,
        }
    }
}
pub struct FieldIntakeCommandIdSource<'a, I> {
    generated: &'a FieldIntakeCommandGeneratedIds,
    source: &'a mut I,
}
impl<I: IdentifierSource> IdentifierSource for FieldIntakeCommandIdSource<'_, I> {
    fn next(&mut self) -> RecordId {
        match self.generated.calls.fetch_add(1, Ordering::SeqCst) {
            0 => self
                .generated
                .audit
                .get()
                .expect("reserved audit id")
                .clone(),
            1 => self
                .generated
                .outbox
                .get()
                .expect("reserved outbox id")
                .clone(),
            _ => self.source.next(),
        }
    }
}
#[derive(Clone)]
pub struct PlannedFieldIntakeCommandEvidence {
    unit: Unit,
    key: String,
    operation: String,
    record: String,
    actor: String,
    company: String,
    assignment: String,
    authority: String,
    hash: String,
    result: String,
    version: i64,
    payload: String,
    aggregate: String,
    now: i64,
    audit_id: String,
    outbox_id: String,
}
/// Plan before receipt insertion. Historical replay never calls this method.
pub async fn plan_field_intake_command_evidence(
    c: &mut PgConnection,
    actor: &CommandActor,
    request: &WorkflowRequest,
    source: &SourceOrderedWorkflowRequest,
    output: &WorkflowOutput,
    clock: FieldIntakeCommandClock,
    ids: &FieldIntakeCommandGeneratedIds,
) -> Result<PlannedFieldIntakeCommandEvidence, Error> {
    if request.operation != "attachment.set.create"
        || source.operation != request.operation
        || source.key != request.key
        || source.record_id.physical_text() != request.record_id
        || source.input.as_str() != request.input.as_str()
        || request.key.is_empty()
        || request.record_id.is_empty()
        || !(1..=9_007_199_254_740_991).contains(&output.version)
    {
        return Err(invalid());
    }
    let tx = unit(c).await?;
    let hash = command_fingerprint_source_ordered(
        actor,
        &request.operation,
        &source.record_id,
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
        .outbox_payload_source_ordered(&source.record_id, output.version, state.as_deref(), &scope)?
        .as_str()
        .to_owned();
    Ok(PlannedFieldIntakeCommandEvidence {
        unit: tx,
        key: request.key.clone(),
        operation: request.operation.clone(),
        record: request.record_id.clone(),
        actor: actor.user_id.as_str().into(),
        company: actor.company_id.as_str().into(),
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
        audit_id: ids.audit.get().ok_or_else(changed)?.as_str().into(),
        outbox_id: ids.outbox.get().ok_or_else(changed)?.as_str().into(),
    })
}
#[derive(Clone, PartialEq, Eq)]
struct Images {
    receipt: String,
    audit: String,
    outbox: String,
}
pub struct RetainedFieldIntakeCommandEvidence {
    plan: PlannedFieldIntakeCommandEvidence,
    audit: String,
    outbox: String,
    images: Images,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldIntakeReceiptAnchor {
    pub receipt_hash: String,
    pub audit_id: String,
    pub outbox_id: String,
}
pub struct FieldIntakeOriginFacts<'a> {
    pub key: &'a str,
    pub company: &'a str,
    pub actor: &'a str,
    pub target: &'a str,
    pub set_id: &'a str,
    pub version: i64,
    pub policy_version: i64,
}
impl PlannedFieldIntakeCommandEvidence {
    /// Verify this Workflow-owned, same-unit planned receipt before a Files origin is appended.
    pub async fn verify_field_intake_origin(
        &self,
        c: &mut PgConnection,
        facts: FieldIntakeOriginFacts<'_>,
    ) -> Result<FieldIntakeReceiptAnchor, Error> {
        let FieldIntakeOriginFacts {
            key,
            company,
            actor,
            target,
            set_id,
            version,
            policy_version,
        } = facts;
        if unit(c).await? != self.unit
            || self.key != key
            || self.company != company
            || self.actor != actor
            || self.record != target
            || self.version != version
        {
            return Err(changed());
        }
        let result: serde_json::Value =
            serde_json::from_str(&self.result).map_err(|_| changed())?;
        if result
            != serde_json::json!({"setId":set_id,"expectedTargetVersion":version,"policyVersion":policy_version})
        {
            return Err(changed());
        }
        let expected = serde_json::json!({"command_id":self.key,"operation":self.operation,"actor_id":self.actor,"request_hash":self.hash,"record_id":self.record,"result_json":self.result,"created_at":self.now,"authority_json":self.authority});
        let receipt_hash: Option<String>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(r)::text)<=2097152 AND to_jsonb(r)=$2::jsonb THEN encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex') END FROM command_receipts r WHERE r.command_id=$1").bind(&self.key).bind(expected).fetch_optional(c).await?.flatten();
        Ok(FieldIntakeReceiptAnchor {
            receipt_hash: receipt_hash.ok_or_else(changed)?,
            audit_id: self.audit_id.clone(),
            outbox_id: self.outbox_id.clone(),
        })
    }
    /// Check all planned values and exact row counts before retaining any image.
    pub async fn retain(
        &self,
        c: &mut PgConnection,
        ids: &FieldIntakeCommandGeneratedIds,
    ) -> Result<RetainedFieldIntakeCommandEvidence, Error> {
        if ids.calls.load(Ordering::SeqCst) != 2 {
            return Err(changed());
        }
        let audit = ids.audit.get().ok_or_else(changed)?.as_str().to_owned();
        let outbox = ids.outbox.get().ok_or_else(changed)?.as_str().to_owned();
        if audit != self.audit_id || outbox != self.outbox_id {
            return Err(changed());
        }
        let images = self.capture(c, &audit, &outbox).await?;
        Ok(RetainedFieldIntakeCommandEvidence {
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
impl RetainedFieldIntakeCommandEvidence {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.plan.capture(c, &self.audit, &self.outbox).await? != self.images {
            return Err(changed());
        }
        Ok(())
    }
}

/// Verify immutable native replay evidence; worker-owned delivery fields may advance.
pub async fn verify_historical_field_intake_command(
    c: &mut PgConnection,
    actor: &CommandActor,
    request: &WorkflowRequest,
    source: &SourceOrderedWorkflowRequest,
    output: &WorkflowOutput,
    anchor: &FieldIntakeReceiptAnchor,
) -> Result<(), Error> {
    let _ = unit(c).await?;
    if request.operation != "attachment.set.create"
        || source.operation != request.operation
        || source.key != request.key
        || source.record_id.physical_text() != request.record_id
        || source.input.as_str() != request.input.as_str()
    {
        return Err(invalid());
    }
    // The origin's immutable receipt digest authenticates the saved authority.
    // It is used only to verify the historical fingerprint, never to grant work.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("workflow.command:{}", request.key))
        .execute(&mut *c)
        .await?;
    let raw: Option<String> = sqlx::query_scalar("SELECT CASE WHEN octet_length(r.authority_json)<=2097152 AND encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex')=$2 AND r.actor_id=$3 AND r.operation=$4 AND r.record_id=$5 THEN r.authority_json END FROM command_receipts r WHERE r.command_id=$1 FOR SHARE").bind(&request.key).bind(&anchor.receipt_hash).bind(actor.user_id.as_str()).bind(&request.operation).bind(&request.record_id).fetch_optional(&mut *c).await?.flatten();
    let value: serde_json::Value =
        serde_json::from_str(&raw.ok_or_else(changed)?).map_err(|_| changed())?;
    if value["companyId"].as_str() != Some(actor.company_id.as_str()) {
        return Err(changed());
    }
    use crate::application::commands::{CommandField, WorkAuthority};
    let text = |name: &str| {
        value[name]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(changed)
    };
    let field = |name: &str| -> Result<CommandField<String>, Error> {
        match value.get(name) {
            Some(serde_json::Value::Null) => Ok(CommandField::Null),
            Some(serde_json::Value::String(s)) => Ok(CommandField::Value(s.clone())),
            None => Ok(CommandField::Omitted),
            _ => Err(changed()),
        }
    };
    let number = |name: &str| {
        value[name]
            .as_i64()
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(changed)
    };
    let mut historical = actor.clone();
    historical.assignment_id = erpx_server_shared::public::AssignmentId::new(text("assignmentId")?)
        .map_err(|_| changed())?;
    historical.role = text("role")?;
    historical.branch_id = field("branchId")?;
    historical.department_id = field("departmentId")?;
    historical.authority = CommandField::Value(WorkAuthority {
        session_id: text("sessionId")?,
        revision: number("revision")?,
        assignment_version: number("assignmentVersion")?,
    });
    historical.assignment_scope_kind = text("scopeKind")?;
    historical.assignment_scope_node_id = match value.get("scopeNodeId") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        _ => return Err(changed()),
    };
    historical.iam = match value.get("iam") {
        None => CommandField::Omitted,
        Some(serde_json::Value::Null) => CommandField::Null,
        Some(v) => CommandField::Value(
            crate::domain::command_json::CommandJson::from_serializable(v)?,
        ),
    };
    historical.validate()?;
    let hash = command_fingerprint_source_ordered(
        &historical,
        &request.operation,
        &source.record_id,
        &request.input,
    )?;
    let authority = historical.receipt_authority()?;
    let scope = output
        .event_scope
        .clone()
        .unwrap_or_else(|| CommandEventScope::from(&historical));
    let payload = historical.outbox_payload_source_ordered(
        &source.record_id,
        output.version,
        None,
        &scope,
    )?;
    let ok: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.command_id=$1 AND octet_length(to_jsonb(r)::text)<=2097152 AND encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex')=$11 AND r.operation=$2 AND r.record_id=$3 AND r.actor_id=$4 AND r.request_hash=$5 AND (r.authority_json::jsonb-'iam')=($6::text::jsonb-'iam') AND r.result_json::jsonb=$7::text::jsonb AND r.created_at BETWEEN 0 AND 9007199254740991 AND (SELECT count(*) FROM audit_events a WHERE a.command_id=$1)=1 AND EXISTS(SELECT 1 FROM audit_events a WHERE a.command_id=$1 AND octet_length(to_jsonb(a)::text)<=2097152 AND a.id=$12 AND a.actor_id=$4 AND a.assignment_id=$8 AND a.action=$2 AND a.record_type='attachment_set' AND a.record_id=$3 AND a.record_version=$9 AND a.purpose=$2 AND a.authority_json=r.authority_json AND a.created_at=r.created_at) AND (SELECT count(*) FROM outbox_events o WHERE o.command_id=$1)=1 AND EXISTS(SELECT 1 FROM outbox_events o WHERE o.command_id=$1 AND octet_length(to_jsonb(o)::text)<=2097152 AND o.id=$13 AND o.event_type=$2 AND o.aggregate_type='attachment_set' AND o.aggregate_id=$3 AND o.aggregate_version=$9 AND o.payload_json::jsonb=jsonb_set($10::text::jsonb,'{authority}',r.authority_json::jsonb-ARRAY['companyId','branchId','departmentId','scopeKind','scopeNodeId']) AND o.created_at=r.created_at))").bind(&request.key).bind(&request.operation).bind(&request.record_id).bind(actor.user_id.as_str()).bind(hash).bind(authority.as_str()).bind(output.result.as_str()).bind(historical.assignment_id.as_str()).bind(output.version).bind(payload.as_str()).bind(&anchor.receipt_hash).bind(&anchor.audit_id).bind(&anchor.outbox_id).fetch_one(c).await?;
    if !ok {
        return Err(changed());
    }
    Ok(())
}
